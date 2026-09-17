use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::BufReader;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

use crate::protocolo::{escribir_mensaje, leer_mensaje};

/// Un mensaje que llega desde el servidor LSP, ya distinguido entre
/// notificación (sin id, como `textDocument/publishDiagnostics`) y
/// respuesta a un request que mandamos nosotros.
#[derive(Debug)]
pub enum MensajeEntrante {
    Notificacion { metodo: String, params: Value },
    Respuesta { id: i64, resultado: std::result::Result<Value, Value> },
}

/// Cliente LSP conectado a un proceso de lenguaje server externo, hablado
/// por stdio (PLAN.md §2: "cliente + procesos externos"). Todo asíncrono:
/// enviar (`peticion`/`notificacion`) no espera la respuesta ahí mismo,
/// esta llega más tarde por `receptor` — quien usa el cliente decide cómo
/// correlacionar el id devuelto por `peticion` con la respuesta.
pub struct Cliente {
    proceso: Child,
    stdin: ChildStdin,
    siguiente_id: i64,
    pub receptor: mpsc::UnboundedReceiver<MensajeEntrante>,
}

impl Cliente {
    /// Lanza `comando` como proceso hijo y arranca la tarea de fondo que
    /// lee su stdout continuamente, reenviando cada mensaje por
    /// `receptor`. El proceso se mata solo si el `Cliente` se dropea sin
    /// pasar por `cerrar` (`kill_on_drop`).
    pub async fn lanzar(comando: &str, args: &[&str]) -> Result<Self> {
        let mut proceso = Command::new(comando)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("no se pudo lanzar '{comando}': ¿está instalado y en el PATH?"))?;

        let stdin = proceso.stdin.take().context("el proceso no expuso stdin")?;
        let stdout = proceso.stdout.take().context("el proceso no expuso stdout")?;

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(leer_en_bucle(BufReader::new(stdout), tx));

        Ok(Self { proceso, stdin, siguiente_id: 1, receptor: rx })
    }

    /// Envía un request identificado; la respuesta llega por `receptor`
    /// como `MensajeEntrante::Respuesta` con este mismo id.
    pub async fn peticion(&mut self, metodo: &str, params: impl Serialize) -> Result<i64> {
        let id = self.siguiente_id;
        self.siguiente_id += 1;
        let mensaje = json!({ "jsonrpc": "2.0", "id": id, "method": metodo, "params": params });
        escribir_mensaje(&mut self.stdin, &mensaje).await?;
        Ok(id)
    }

    pub async fn notificacion(&mut self, metodo: &str, params: impl Serialize) -> Result<()> {
        let mensaje = json!({ "jsonrpc": "2.0", "method": metodo, "params": params });
        escribir_mensaje(&mut self.stdin, &mensaje).await
    }

    /// Cierra la sesión siguiendo el protocolo que pide la spec de LSP:
    /// un request `shutdown` (esperando su respuesta antes de seguir,
    /// correlacionada por id) seguido de una notificación `exit` — le da
    /// al servidor la chance de liberar sus propios recursos (archivos
    /// temporales, procesos hijos propios, caches en disco) en vez de
    /// matarlo en seco a mitad de lo que estuviera haciendo. Nunca cuelga
    /// el cierre de `tcode` esperando a un servidor que no coopera: todo
    /// el intento "educado" tiene un límite de tiempo total
    /// ([`TIMEOUT_CIERRE_EDUCADO`]), y si no terminó para entonces (no
    /// respondió `shutdown`, o no salió solo tras `exit`) se lo mata
    /// igual.
    pub async fn cerrar(mut self) {
        let salio_solo = tokio::time::timeout(TIMEOUT_CIERRE_EDUCADO, self.intentar_cierre_educado())
            .await
            .unwrap_or(false);
        if !salio_solo {
            let _ = self.proceso.kill().await;
        }
    }

    /// Mata el proceso de inmediato, sin el protocolo de cierre educado
    /// de `cerrar` — para cuando hace falta relanzar la sesión ya mismo
    /// (cambió el lenguaje del archivo activo, o el comando configurado
    /// para el mismo lenguaje, PLAN.md §5.3) y esperar hasta un segundo a
    /// que el servidor viejo responda `shutdown` se sentiría como que
    /// `tcode` se traba al cambiar de archivo. El servidor no tiene
    /// margen para liberar sus propios recursos con prolijidad en este
    /// camino — trade-off aceptado a cambio de que cambiar de archivo
    /// siga sintiéndose instantáneo; `cerrar` (con el protocolo completo)
    /// sigue siendo lo que se usa al salir de `tcode`, sin apuro.
    pub async fn matar(mut self) {
        let _ = self.proceso.kill().await;
    }

    /// La parte "sin límite de tiempo propio" del cierre educado — quien
    /// llama (`cerrar`) es responsable de acotarla, porque un servidor
    /// que nunca responde `shutdown` dejaría este `await` colgado para
    /// siempre. Devuelve `true` solo si el proceso llegó a salir solo.
    async fn intentar_cierre_educado(&mut self) -> bool {
        let Ok(id_shutdown) = self.peticion("shutdown", Value::Null).await else { return false };

        loop {
            match self.receptor.recv().await {
                Some(MensajeEntrante::Respuesta { id, .. }) if id == id_shutdown => break,
                // Cualquier otro mensaje mientras se espera (una
                // notificación de diagnósticos que ya estaba en vuelo,
                // por ejemplo) se descarta sin problema — la sesión se
                // está cerrando, a nadie le importa ya.
                Some(_) => continue,
                // El canal se cerró: el proceso murió por su cuenta
                // mientras esperábamos, no hay nada más que "cerrar".
                None => return false,
            }
        }

        if self.notificacion("exit", Value::Null).await.is_err() {
            return false;
        }
        self.proceso.wait().await.is_ok()
    }
}

/// Cuánto se espera, en total, a que un servidor responda `shutdown` y
/// después termine solo tras `exit` antes de matarlo de todos modos. Un
/// segundo alcanza de sobra para cualquier servidor real (la respuesta a
/// `shutdown` no hace ningún trabajo pesado según la spec) sin demorar
/// perceptiblemente el cierre de `tcode` si el servidor no coopera.
const TIMEOUT_CIERRE_EDUCADO: Duration = Duration::from_secs(1);

async fn leer_en_bucle(mut reader: BufReader<ChildStdout>, tx: mpsc::UnboundedSender<MensajeEntrante>) {
    loop {
        let valor = match leer_mensaje(&mut reader).await {
            Ok(v) => v,
            Err(_) => break, // el proceso murió o el stream se rompió
        };

        let mensaje = if let Some(id) = valor.get("id").and_then(Value::as_i64) {
            match valor.get("error") {
                Some(error) => MensajeEntrante::Respuesta { id, resultado: Err(error.clone()) },
                None => {
                    MensajeEntrante::Respuesta { id, resultado: Ok(valor.get("result").cloned().unwrap_or(Value::Null)) }
                }
            }
        } else if let Some(metodo) = valor.get("method").and_then(Value::as_str) {
            MensajeEntrante::Notificacion {
                metodo: metodo.to_string(),
                params: valor.get("params").cloned().unwrap_or(Value::Null),
            }
        } else {
            continue;
        };

        if tx.send(mensaje).is_err() {
            break;
        }
    }
}

/// Comando y argumentos para lanzar el LSP server de un lenguaje, si
/// `tcode` conoce uno (PLAN.md §6). `None` si no hay soporte configurado
/// todavía — el resto de los 13 lenguajes objetivo se suman
/// incrementalmente; el editor sigue funcionando igual sin LSP para esos
/// (y el usuario puede fijar el suyo a mano desde el panel de
/// administración, sección "Lenguajes / LSP", aunque no haya uno acá).
pub fn comando_para(lenguaje: tcode_syntax::Lenguaje) -> Option<(&'static str, &'static [&'static str])> {
    match lenguaje {
        tcode_syntax::Lenguaje::Python => Some(("pyright-langserver", &["--stdio"])),
        tcode_syntax::Lenguaje::TypeScript => Some(("typescript-language-server", &["--stdio"])),
        // clangd sirve tanto a C como a C++ (PLAN.md §6, fila "C/C++") y
        // habla por stdio sin argumentos adicionales.
        tcode_syntax::Lenguaje::C | tcode_syntax::Lenguaje::Cpp => Some(("clangd", &[])),
        tcode_syntax::Lenguaje::Ruby => Some(("solargraph", &["stdio"])),
        tcode_syntax::Lenguaje::Php => Some(("intelephense", &["--stdio"])),
        // Habla LSP por stdio sin flags ni argumentos adicionales, igual
        // que clangd/pyright — no necesita saber de antemano nada del
        // proyecto para arrancar (a diferencia de jdtls/omnisharp).
        tcode_syntax::Lenguaje::Kotlin => Some(("kotlin-language-server", &[])),
        // El paquete `vscode-langservers-extracted` (PLAN.md §6, fila
        // "HTML/CSS") instala un binario separado para cada uno, ambos
        // hablando LSP por `--stdio` sin argumentos extra.
        tcode_syntax::Lenguaje::Html => Some(("vscode-html-language-server", &["--stdio"])),
        tcode_syntax::Lenguaje::Css => Some(("vscode-css-language-server", &["--stdio"])),
        tcode_syntax::Lenguaje::Sql => Some(("sqls", &[])),
        // `jdtls` (Java) necesita un directorio de datos de workspace
        // como argumento (`-data <dir>`) para funcionar bien, y
        // `omnisharp` (C#) necesita el `.sln`/directorio del proyecto —
        // ninguno tiene un valor razonable que fijar acá sin saber el
        // proyecto del usuario, así que quedan sin comando por defecto
        // (configurables a mano, como cualquier lenguaje sin uno).
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comando_para_python_es_pyright() {
        let (comando, args) = comando_para(tcode_syntax::Lenguaje::Python).unwrap();
        assert_eq!(comando, "pyright-langserver");
        assert_eq!(args, &["--stdio"]);
    }

    #[test]
    fn comando_para_rust_todavia_no_existe() {
        assert!(comando_para(tcode_syntax::Lenguaje::Rust).is_none());
    }

    #[test]
    fn comando_para_typescript_es_typescript_language_server() {
        let (comando, args) = comando_para(tcode_syntax::Lenguaje::TypeScript).unwrap();
        assert_eq!(comando, "typescript-language-server");
        assert_eq!(args, &["--stdio"]);
    }

    #[test]
    fn comando_para_c_y_cpp_es_clangd() {
        assert_eq!(comando_para(tcode_syntax::Lenguaje::C).unwrap().0, "clangd");
        assert_eq!(comando_para(tcode_syntax::Lenguaje::Cpp).unwrap().0, "clangd");
    }

    #[test]
    fn comando_para_java_todavia_no_tiene_default() {
        assert!(comando_para(tcode_syntax::Lenguaje::Java).is_none());
    }

    #[test]
    fn comando_para_ruby_es_solargraph() {
        let (comando, args) = comando_para(tcode_syntax::Lenguaje::Ruby).unwrap();
        assert_eq!(comando, "solargraph");
        assert_eq!(args, &["stdio"]);
    }

    #[test]
    fn comando_para_php_es_intelephense() {
        let (comando, args) = comando_para(tcode_syntax::Lenguaje::Php).unwrap();
        assert_eq!(comando, "intelephense");
        assert_eq!(args, &["--stdio"]);
    }

    #[test]
    fn comando_para_kotlin_es_kotlin_language_server() {
        assert_eq!(comando_para(tcode_syntax::Lenguaje::Kotlin).unwrap().0, "kotlin-language-server");
    }

    #[test]
    fn comando_para_csharp_todavia_no_tiene_default() {
        assert!(comando_para(tcode_syntax::Lenguaje::CSharp).is_none());
    }

    #[test]
    fn comando_para_html_y_css_es_vscode_langservers_extracted() {
        let (comando, args) = comando_para(tcode_syntax::Lenguaje::Html).unwrap();
        assert_eq!(comando, "vscode-html-language-server");
        assert_eq!(args, &["--stdio"]);

        let (comando, args) = comando_para(tcode_syntax::Lenguaje::Css).unwrap();
        assert_eq!(comando, "vscode-css-language-server");
        assert_eq!(args, &["--stdio"]);
    }

    #[test]
    fn comando_para_sql_es_sqls() {
        assert_eq!(comando_para(tcode_syntax::Lenguaje::Sql).unwrap().0, "sqls");
    }

    /// Verifica el ciclo de vida completo contra un proceso real y
    /// simple (`cat`, que devuelve por stdout exactamente lo que recibe
    /// por stdin): confirma que `lanzar` conecta los pipes correctamente
    /// y que un mensaje enviado con `notificacion` efectivamente vuelve
    /// por `receptor` con el framing bien interpretado en ambos sentidos.
    /// No depende de tener un LSP server real instalado. Usa `cat`, que
    /// es específico de Unix — se salta en Windows.
    #[cfg(unix)]
    #[tokio::test]
    async fn lanzar_y_recibir_via_un_proceso_eco() {
        let mut cliente = Cliente::lanzar("cat", &[]).await.expect("cat debería existir en cualquier Unix");
        cliente.notificacion("prueba/eco", json!({ "hola": "mundo" })).await.unwrap();

        let mensaje = cliente.receptor.recv().await.expect("cat debería hacer eco del mensaje");
        match mensaje {
            MensajeEntrante::Notificacion { metodo, params } => {
                assert_eq!(metodo, "prueba/eco");
                assert_eq!(params, json!({ "hola": "mundo" }));
            }
            MensajeEntrante::Respuesta { .. } => panic!("se esperaba una notificación, no una respuesta"),
        }

        // `matar`, no `cerrar`: este test prueba el framing del eco, no
        // el protocolo de cierre (ver `cerrar_no_se_cuelga_aunque_el_
        // proceso_no_salga_solo` más abajo) — usar `cerrar` acá lo
        // haría tardar `TIMEOUT_CIERRE_EDUCADO` entero sin necesidad,
        // porque `cat` nunca sale solo tras `exit`.
        cliente.matar().await;
    }

    /// `cerrar` manda `shutdown`+`exit` antes de matar el proceso —
    /// `cat` no es un LSP real (no interpreta `exit`, nunca termina
    /// solo), así que ejercita justo el camino "el servidor no
    /// coopera": confirma que `cerrar` nunca se cuelga esperando para
    /// siempre a que salga solo, sino que lo mata al vencer
    /// `TIMEOUT_CIERRE_EDUCADO`. Sin el límite externo de este test, un
    /// bug que deje `cerrar` esperando indefinidamente trabaría el test
    /// entero en vez de fallarlo con un mensaje claro.
    #[cfg(unix)]
    #[tokio::test]
    async fn cerrar_no_se_cuelga_aunque_el_proceso_no_salga_solo() {
        let cliente = Cliente::lanzar("cat", &[]).await.expect("cat debería existir en cualquier Unix");
        tokio::time::timeout(TIMEOUT_CIERRE_EDUCADO + Duration::from_millis(500), cliente.cerrar())
            .await
            .expect("cerrar() no debería tardar más que su propio timeout interno");
    }
}
