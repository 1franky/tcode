//! Formateador EXTERNO por lenguaje (`lenguajes.formateador` en la
//! config): un proceso que recibe el texto por stdin y devuelve el
//! formateado por stdout (`rustfmt --emit stdout`, `black -q -`,
//! `prettier --stdin-filepath {archivo}`...). Módulo aparte del LSP a
//! propósito: no sabe nada de sesiones ni de `textDocument/formatting`;
//! `formatear_antes_de_guardar` (en `main.rs`) solo decide cuál de los
//! dos usar — si hay un formateador externo configurado, este tiene
//! prioridad y el LSP no se consulta.
//!
//! Nunca bloquea el guardado: cualquier falla (no se pudo lanzar, código
//! de salida distinto de 0, salida vacía o no UTF-8, tardó más de
//! `TIEMPO_MAXIMO`) devuelve el motivo para la barra de estado y el
//! archivo se guarda tal cual. El resultado se aplica como ediciones
//! mínimas (`tcode_core::ediciones_minimas`) en UN solo paso de deshacer,
//! así el cursor y los pliegues se quedan sobre el mismo código.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tcode_config::ComandoLsp;
use tcode_core::{Editor, Modo};
use tokio::io::AsyncWriteExt;

/// Cuánto se espera al formateador antes de matarlo y guardar sin
/// formatear. Algo más que el LSP (~2 s): acá cada guardado lanza un
/// proceso nuevo, y algunos (prettier vía node) tardan en arrancar.
pub const TIEMPO_MAXIMO: Duration = Duration::from_secs(3);

/// Marcador de los argumentos que se reemplaza por la ruta absoluta del
/// archivo (para formateadores que eligen reglas según la extensión o
/// buscan su config desde ahí, como `prettier --stdin-filepath`).
const MARCADOR_ARCHIVO: &str = "{archivo}";

/// Corre `comando` con `texto` por stdin y devuelve su stdout (con los
/// `\r\n` normalizados a `\n`: el buffer de `tcode` nunca tiene `\r`, ver
/// `tcode_core::Eol`), o el motivo por el que no se pudo formatear. El
/// directorio de trabajo es la carpeta del archivo, para que el
/// formateador encuentre su config de proyecto (`rustfmt.toml`,
/// `pyproject.toml`...). El proceso se mata si se pasa de `limite`.
pub async fn formatear_texto(comando: &ComandoLsp, archivo: &Path, texto: &str, limite: Duration) -> Result<String, String> {
    let archivo = std::path::absolute(archivo).unwrap_or_else(|_| archivo.to_path_buf());
    let argumentos: Vec<String> =
        comando.argumentos.iter().map(|a| a.replace(MARCADOR_ARCHIVO, &archivo.to_string_lossy())).collect();

    let mut proceso = tokio::process::Command::new(&comando.comando);
    proceso
        .args(&argumentos)
        .envs(&comando.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Si se vence el tiempo, soltar el `Child` lo mata.
        .kill_on_drop(true);
    if let Some(dir) = archivo.parent().filter(|d| d.is_dir()) {
        proceso.current_dir(dir);
    }
    // En los avisos alcanza con el nombre del binario: una ruta absoluta
    // larga se comería la barra de estado antes de llegar al motivo.
    let nombre = nombre_corto(&comando.comando);
    let mut hijo = proceso.spawn().map_err(|e| format!("no se pudo lanzar '{nombre}': {e}"))?;

    // Escribir stdin en paralelo con la lectura de stdout: con un archivo
    // grande, escribir todo primero puede trabarse con el formateador
    // esperando que alguien lea lo que ya escupió.
    let mut entrada = hijo.stdin.take();
    let contenido = texto.as_bytes().to_vec();
    let escritura = tokio::spawn(async move {
        if let Some(entrada) = entrada.as_mut() {
            // Un formateador que cierra stdin antes de leerlo todo
            // devuelve su propio error: no hace falta reportar este.
            let _ = entrada.write_all(&contenido).await;
            let _ = entrada.shutdown().await;
        }
    });

    let salida = match tokio::time::timeout(limite, hijo.wait_with_output()).await {
        Ok(Ok(salida)) => salida,
        Ok(Err(e)) => return Err(format!("'{nombre}' falló: {e}")),
        Err(_) => {
            escritura.abort();
            return Err(format!("'{nombre}' no respondió en {} s", limite.as_secs_f32()));
        }
    };
    let _ = escritura.await;

    if !salida.status.success() {
        let codigo = salida.status.code().map_or_else(|| "sin código".to_string(), |c| format!("código {c}"));
        let stderr = String::from_utf8_lossy(&salida.stderr);
        return Err(match primera_linea(&stderr) {
            Some(linea) => format!("'{nombre}' falló ({codigo}): {linea}"),
            None => format!("'{nombre}' falló ({codigo})"),
        });
    }
    let formateado = String::from_utf8(salida.stdout).map_err(|_| format!("'{nombre}' devolvió texto que no es UTF-8"))?;
    // Salida vacía con entrada no vacía: casi seguro un formateador mal
    // configurado (que escribió el resultado a otro lado, o falló con
    // código 0). Aplicarla borraría el archivo entero.
    if formateado.is_empty() && !texto.trim().is_empty() {
        return Err(format!("'{nombre}' no devolvió nada por stdout"));
    }
    Ok(formateado.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Nombre del archivo de `comando` (`/opt/x/bin/black` → `black`).
fn nombre_corto(comando: &str) -> &str {
    Path::new(comando).file_name().and_then(|n| n.to_str()).unwrap_or(comando)
}

/// Primera línea no vacía de `texto`, recortada para que entre en la
/// barra de estado.
fn primera_linea(texto: &str) -> Option<String> {
    const MAXIMO: usize = 120;
    let linea = texto.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(if linea.chars().count() > MAXIMO { format!("{}...", linea.chars().take(MAXIMO).collect::<String>()) } else { linea.to_string() })
}

/// Formatea el texto de `editor` con `comando` y aplica el resultado como
/// ediciones mínimas en UN paso de deshacer. Devuelve el mensaje para la
/// barra de estado: `None` si ya estaba formateado (nada que avisar).
pub async fn formatear_editor(editor: &mut Editor, archivo: &Path, comando: &ComandoLsp) -> Option<String> {
    let texto = editor.buffer().a_texto();
    match formatear_texto(comando, archivo, &texto, TIEMPO_MAXIMO).await {
        Ok(formateado) => {
            let ediciones = tcode_core::ediciones_minimas(&texto, &formateado);
            if !editor.aplicar_ediciones(&ediciones) {
                return None;
            }
            // En modo Normal (VIM) el cursor no puede quedar después del
            // último carácter de la línea: se vuelve a recortar.
            if editor.modo() == Modo::Normal {
                editor.entrar_modo_normal();
            }
            Some(format!("Formateado al guardar ({})", nombre_corto(&comando.comando)))
        }
        Err(motivo) => Some(format!("Sin formatear: {motivo}")),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sh(script: &str) -> ComandoLsp {
        ComandoLsp { comando: "sh".to_string(), argumentos: vec!["-c".to_string(), script.to_string()], ..Default::default() }
    }

    fn archivo() -> PathBuf {
        std::env::temp_dir().join("tcode-formateador-prueba.txt")
    }

    #[tokio::test]
    async fn devuelve_el_stdout_del_formateador() {
        let resultado = formatear_texto(&sh("sed s/hola/HOLA/"), &archivo(), "hola ñandú 😀\n", TIEMPO_MAXIMO).await;
        assert_eq!(resultado.unwrap(), "HOLA ñandú 😀\n");
    }

    #[tokio::test]
    async fn normaliza_crlf_de_la_salida() {
        let resultado = formatear_texto(&sh("printf 'a\\r\\nb\\r\\n'"), &archivo(), "x", TIEMPO_MAXIMO).await;
        assert_eq!(resultado.unwrap(), "a\nb\n");
    }

    #[tokio::test]
    async fn codigo_de_salida_distinto_de_cero_da_la_primera_linea_de_stderr() {
        let comando = sh("cat > /dev/null; echo '' >&2; echo 'error: sintaxis en línea 3' >&2; echo otra >&2; exit 2");
        let motivo = formatear_texto(&comando, &archivo(), "x\n", TIEMPO_MAXIMO).await.unwrap_err();
        assert_eq!(motivo, "'sh' falló (código 2): error: sintaxis en línea 3");
    }

    #[tokio::test]
    async fn se_mata_si_tarda_demasiado() {
        let inicio = std::time::Instant::now();
        let motivo = formatear_texto(&sh("sleep 5"), &archivo(), "x\n", Duration::from_millis(200)).await.unwrap_err();
        assert!(motivo.contains("no respondió"), "{motivo}");
        assert!(inicio.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn comando_inexistente_no_hace_fallar_nada() {
        let comando = ComandoLsp { comando: "tcode-no-existe-este-formateador".to_string(), ..Default::default() };
        let motivo = formatear_texto(&comando, &archivo(), "x\n", TIEMPO_MAXIMO).await.unwrap_err();
        assert!(motivo.starts_with("no se pudo lanzar"), "{motivo}");
    }

    #[tokio::test]
    async fn salida_vacia_no_borra_el_archivo() {
        let motivo = formatear_texto(&sh("cat > /dev/null"), &archivo(), "algo\n", TIEMPO_MAXIMO).await.unwrap_err();
        assert!(motivo.contains("no devolvió nada"));
    }

    #[tokio::test]
    async fn reemplaza_el_marcador_de_archivo_por_la_ruta_absoluta() {
        let comando = ComandoLsp {
            comando: "sh".to_string(),
            argumentos: vec!["-c".to_string(), "cat > /dev/null; echo \"$0\"".to_string(), "{archivo}".to_string()],
            ..Default::default()
        };
        let resultado = formatear_texto(&comando, Path::new("relativo.rs"), "x", TIEMPO_MAXIMO).await.unwrap();
        let esperado = std::env::current_dir().unwrap().join("relativo.rs");
        assert_eq!(resultado.trim_end(), esperado.to_string_lossy());
    }

    #[tokio::test]
    async fn entrada_grande_no_se_traba() {
        let texto: String = (0..200_000).map(|i| format!("linea {i}\n")).collect();
        let resultado = formatear_texto(&sh("cat"), &archivo(), &texto, TIEMPO_MAXIMO).await.unwrap();
        assert_eq!(resultado.len(), texto.len());
    }

    #[tokio::test]
    async fn formatear_editor_aplica_un_solo_paso_de_deshacer_y_conserva_el_cursor() {
        let mut editor = Editor::nuevo();
        editor.insertar_texto("a=1\nb=2\n");
        // Cursor sobre la "b".
        let offset = editor.buffer().offset_byte(1, 0);
        editor.mover_cursor_a_byte(offset);
        let comando = sh("sed 's/=/ = /'");
        let mensaje = formatear_editor(&mut editor, &archivo(), &comando).await.unwrap();
        assert!(mensaje.starts_with("Formateado"), "{mensaje}");
        assert_eq!(editor.buffer().a_texto(), "a = 1\nb = 2\n");
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (1, 0));
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "a=1\nb=2\n");
    }

    #[tokio::test]
    async fn formatear_editor_con_falla_deja_el_texto_igual_y_avisa() {
        let mut editor = Editor::nuevo();
        editor.insertar_texto("a=1\n");
        let mensaje = formatear_editor(&mut editor, &archivo(), &sh("echo roto >&2; exit 1")).await.unwrap();
        assert_eq!(mensaje, "Sin formatear: 'sh' falló (código 1): roto");
        assert_eq!(editor.buffer().a_texto(), "a=1\n");
    }

    #[tokio::test]
    async fn ya_formateado_no_avisa_nada() {
        let mut editor = Editor::nuevo();
        editor.insertar_texto("a = 1\n");
        assert!(formatear_editor(&mut editor, &archivo(), &sh("cat")).await.is_none());
    }
}
