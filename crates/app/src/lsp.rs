//! Gestión del ciclo de vida del cliente LSP activo (PLAN.md §2, §11 M2):
//! cuándo lanzarlo, el handshake `initialize`/`initialized`, mantener al
//! servidor al tanto de los cambios del archivo (`didOpen`/`didChange`), y
//! convertir sus notificaciones de diagnósticos en algo que `tcode-ui`
//! pueda dibujar.
//!
//! Simplificación deliberada de esta primera pieza: un solo cliente LSP
//! activo a la vez, asociado al panel activo — no hay todavía un cliente
//! por lenguaje corriendo en paralelo para cada split abierto. Cambiar de
//! panel a un archivo de otro lenguaje relanza el cliente.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingClientCapabilities,
    DocumentFormattingParams, FormattingOptions, InitializeParams, InitializedParams, TextDocumentClientCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
};
use serde_json::json;
use tcode_config::Config;
use tcode_lsp::{Cliente, EdicionTexto, MensajeEntrante};
use tcode_syntax::Lenguaje;
use tcode_ui::Layout as PanelLayout;

/// Handshake en curso o completo con el servidor LSP activo.
enum Fase {
    /// Se envió `initialize` con este id, se espera su respuesta.
    Iniciando { id_initialize: i64 },
    /// `initialize`/`initialized`/`didOpen` completos: se pueden mandar
    /// `didChange` y se procesan diagnósticos entrantes.
    Listo { version: i32 },
}

struct SesionLsp {
    cliente: Cliente,
    lenguaje: Lenguaje,
    /// Comando + argumentos + variables de entorno con los que se lanzó
    /// esta sesión — `comando_efectivo` en el momento del lanzamiento.
    /// Se guarda para que `actualizar_para_archivo` note un cambio de
    /// configuración (usuario edita el comando personalizado desde el
    /// panel de administración, variables de entorno incluidas) aunque
    /// el lenguaje no haya cambiado, y relance.
    comando_usado: (String, Vec<String>, BTreeMap<String, String>),
    fase: Fase,
    uri: Uri,
    ultimo_texto_enviado: String,
    /// Si el servidor anunció `documentFormattingProvider` al responder
    /// `initialize` (BACKLOG.md P2 #5) — `false` hasta entonces, y para
    /// siempre en servidores que no formatean (pyright). Se mira ANTES de
    /// mandar `textDocument/formatting`, para no esperar en vano una
    /// respuesta que va a ser un error "método no soportado".
    soporta_formateo: bool,
}

/// Cuánto se espera, como mucho, la respuesta a `textDocument/formatting`
/// antes de guardar igual sin formatear (BACKLOG.md P2 #5: formatear
/// nunca puede bloquear el guardado). Mientras tanto la UI no se redibuja
/// (ver `EstadoLsp::pedir_formateo`), así que tiene que ser corto; 2 s
/// alcanza de sobra para rust-analyzer + rustfmt (unos 100-300 ms en un
/// archivo normal, medido en tmux) incluso con el primer `rustfmt` en
/// frío.
const TIMEOUT_FORMATEO: Duration = Duration::from_secs(2);

/// Comando + argumentos + variables de entorno a usar para lanzar el LSP
/// de `lenguaje` (PLAN.md §5.3): lo que configuró el usuario a mano en
/// la sección "Lenguajes / LSP", si hay algo, o si no el comando por
/// defecto de `tcode_lsp::comando_para` (que puede no haber ninguno, como
/// para todos los lenguajes salvo Python por ahora) sin ninguna variable
/// de entorno — los defaults embebidos nunca las necesitan.
pub fn comando_efectivo(lenguaje: Lenguaje, config: &Config) -> Option<(String, Vec<String>, BTreeMap<String, String>)> {
    if let Some(personalizado) = config.lenguajes.comando_configurado(lenguaje.id()) {
        return Some((personalizado.comando.clone(), personalizado.argumentos.clone(), personalizado.env.clone()));
    }
    tcode_lsp::comando_para(lenguaje)
        .map(|(comando, args)| (comando.to_string(), args.iter().map(|a| a.to_string()).collect(), BTreeMap::new()))
}

/// Estado LSP de la aplicación: como mucho una sesión activa (ver nota de
/// simplificación arriba).
#[derive(Default)]
pub struct EstadoLsp {
    sesion: Option<SesionLsp>,
}

impl EstadoLsp {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Al cambiar de archivo activo (abrir uno nuevo, cambiar de panel) O
    /// al habilitar/deshabilitar el LSP de un lenguaje desde el panel de
    /// administración (sección "Lenguajes / LSP", PLAN.md §5.3, sin
    /// cambiar de archivo): si el lenguaje EFECTIVO (`None` si está
    /// deshabilitado en `config`, aunque el archivo sí tenga ese
    /// lenguaje) es distinto del que ya está corriendo, cierra la sesión
    /// vieja y arranca una nueva. Si el nuevo archivo no tiene lenguaje
    /// con LSP (o está deshabilitado, o el server configurado no se pudo
    /// lanzar — no está instalado), simplemente no queda sesión activa —
    /// el editor sigue funcionando igual, sin LSP.
    pub async fn actualizar_para_archivo(&mut self, ruta: &str, contenido: &str, config: &Config) {
        let lenguaje = Lenguaje::detectar_por_extension(ruta);
        let lenguaje_efectivo = lenguaje.filter(|l| config.lenguajes.lsp_habilitado(l.id()));
        let comando_efectivo_actual = lenguaje_efectivo.and_then(|l| comando_efectivo(l, config));

        let necesita_relanzar = match (&self.sesion, lenguaje_efectivo) {
            // Mismo lenguaje: relanza si además cambió el comando
            // configurado (edición en vivo desde el panel de
            // administración), no solo si cambió el lenguaje.
            (Some(sesion), Some(l)) => {
                sesion.lenguaje != l || comando_efectivo_actual.as_ref() != Some(&sesion.comando_usado)
            }
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };
        if !necesita_relanzar {
            return;
        }

        if let Some(sesion) = self.sesion.take() {
            // `matar`, no `cerrar`: esto corre en el camino síncrono de
            // cada tecla (`sincronizar_lsp`, `app/main.rs`) — el
            // protocolo de cierre educado completo puede tardar hasta un
            // segundo si el servidor viejo no responde `shutdown` rápido,
            // y se sentiría como que `tcode` se traba al cambiar de
            // archivo. El cierre prolijo se reserva para cuando de
            // verdad no hay apuro: salir de `tcode` (`EstadoLsp::cerrar`,
            // más abajo).
            sesion.cliente.matar().await;
        }

        let Some(lenguaje) = lenguaje_efectivo else { return };
        let Some((comando, args, env)) = comando_efectivo_actual else { return };
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        let env_ref: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let Ok(uri) = uri_de_archivo(Path::new(ruta)) else { return };
        let Ok(mut cliente) = Cliente::lanzar(&comando, &args_ref, &env_ref).await else { return };

        // Lo único que se declara explícitamente es `formatting` (sin
        // registro dinámico: `tcode` no responde `client/
        // registerCapability`), para que un servidor que decide qué
        // anunciar según lo que soporta el cliente anuncie
        // `documentFormattingProvider` de forma estática.
        let capabilities = ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                formatting: Some(DocumentFormattingClientCapabilities { dynamic_registration: Some(false) }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            capabilities,
            ..Default::default()
        };
        let Ok(id_initialize) = cliente.peticion("initialize", params).await else { return };

        self.sesion = Some(SesionLsp {
            cliente,
            lenguaje,
            comando_usado: (comando, args, env),
            fase: Fase::Iniciando { id_initialize },
            uri,
            ultimo_texto_enviado: contenido.to_string(),
            soporta_formateo: false,
        });
    }

    /// Espera el siguiente mensaje del servidor LSP activo. Nunca resuelve
    /// si no hay sesión — pensado para usarse dentro de un `tokio::select!`
    /// junto al stream de teclado, sin bloquearlo cuando no hay LSP.
    pub async fn siguiente_mensaje(&mut self) -> Option<MensajeEntrante> {
        match &mut self.sesion {
            Some(sesion) => sesion.cliente.receptor.recv().await,
            None => std::future::pending().await,
        }
    }

    /// Procesa un mensaje ya recibido: completa el handshake si era la
    /// respuesta a `initialize`, o actualiza los diagnósticos del panel
    /// activo si era una notificación `publishDiagnostics` del archivo
    /// actualmente activo.
    pub async fn procesar_mensaje(&mut self, mensaje: MensajeEntrante, layout: &mut PanelLayout) {
        let Some(sesion) = &mut self.sesion else { return };

        match mensaje {
            MensajeEntrante::Respuesta { id, resultado } => {
                if let Fase::Iniciando { id_initialize } = sesion.fase {
                    if let (true, Ok(resultado)) = (id == id_initialize, &resultado) {
                        sesion.soporta_formateo = tcode_lsp::soporta_formateo(resultado);
                        let _ = sesion.cliente.notificacion("initialized", InitializedParams {}).await;
                        let _ = sesion
                            .cliente
                            .notificacion(
                                "textDocument/didOpen",
                                DidOpenTextDocumentParams {
                                    text_document: TextDocumentItem {
                                        uri: sesion.uri.clone(),
                                        language_id: sesion.lenguaje.id().to_string(),
                                        version: 1,
                                        text: sesion.ultimo_texto_enviado.clone(),
                                    },
                                },
                            )
                            .await;
                        sesion.fase = Fase::Listo { version: 1 };
                    }
                }
            }
            MensajeEntrante::Notificacion { metodo, params } => {
                if metodo == "textDocument/publishDiagnostics" {
                    // El texto tal como lo tiene `tcode` ahora mismo — hace
                    // falta para la conversión UTF-16 → carácter de las
                    // columnas del diagnóstico (`parsear_diagnosticos`).
                    if let Ok((uri, diagnosticos)) =
                        tcode_lsp::parsear_diagnosticos(&params, &sesion.ultimo_texto_enviado)
                    {
                        if uri == sesion.uri.as_str() {
                            layout.establecer_diagnosticos_activo(diagnosticos);
                        }
                    }
                }
            }
        }
    }

    /// Si el contenido del archivo activo cambió desde el último envío,
    /// notifica `textDocument/didChange` con el texto completo (full
    /// sync — más simple y suficientemente rápido para M2; la
    /// sincronización incremental queda como optimización futura).
    pub async fn sincronizar_contenido(&mut self, texto_actual: &str) {
        let Some(sesion) = &mut self.sesion else { return };
        let Fase::Listo { version } = &mut sesion.fase else { return };
        if texto_actual == sesion.ultimo_texto_enviado {
            return;
        }

        *version += 1;
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri: sesion.uri.clone(), version: *version },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: texto_actual.to_string(),
            }],
        };
        if sesion.cliente.notificacion("textDocument/didChange", params).await.is_ok() {
            sesion.ultimo_texto_enviado = texto_actual.to_string();
        }
    }

    /// Pide `textDocument/formatting` para el archivo `ruta` (cuyo texto
    /// actual es `texto`) y espera la respuesta, como mucho
    /// [`TIMEOUT_FORMATEO`] — lo usa el guardado con "formatear al
    /// guardar" prendido (`guardar_archivo_activo`, `app/main.rs`,
    /// BACKLOG.md P2 #5). Devuelve las ediciones ya traducidas a offsets
    /// de bytes sobre `texto`, o el motivo (texto corto, para la barra de
    /// estado) por el que no se formateó: no hay sesión, todavía está
    /// iniciando, el servidor no anuncia `documentFormattingProvider`,
    /// la sesión está abierta sobre otro archivo, respondió con error, o
    /// no respondió a tiempo. Nunca falla de otra forma: quien llama
    /// guarda igual en cualquiera de esos casos.
    ///
    /// Quien llama tiene que haber sincronizado el texto antes
    /// (`sincronizar_lsp`): las posiciones de la respuesta se refieren al
    /// documento que tiene el SERVIDOR, así que si no coincide con `texto`
    /// no se pide nada (aplicarlas sobre otro texto rompería el archivo).
    ///
    /// La espera es un bucle propio sobre el mismo canal que lee el
    /// `tokio::select!` de `ejecutar`, no un segundo lector: la respuesta
    /// se reconoce por su id, y cualquier otro mensaje que llegue
    /// mientras tanto (diagnósticos, otra respuesta) se procesa ahí mismo
    /// con `procesar_mensaje`, igual que lo habría hecho el bucle
    /// principal — no se pierde nada. Si se vence el tiempo se manda
    /// `$/cancelRequest`; si la respuesta llega igual más tarde, el bucle
    /// principal la recibe con un id que nadie espera y la ignora.
    pub async fn pedir_formateo(
        &mut self,
        ruta: &str,
        texto: &str,
        opciones: FormattingOptions,
        layout: &mut PanelLayout,
    ) -> std::result::Result<Vec<EdicionTexto>, &'static str> {
        let Some(sesion) = &mut self.sesion else { return Err("sin LSP activo") };
        let Fase::Listo { .. } = sesion.fase else { return Err("el LSP todavía está iniciando") };
        if !sesion.soporta_formateo {
            return Err("el LSP no soporta formatear");
        }
        let Ok(uri) = uri_de_archivo(Path::new(ruta)) else { return Err("sin LSP activo") };
        if uri.as_str() != sesion.uri.as_str() {
            return Err("el LSP está abierto sobre otro archivo");
        }
        if sesion.ultimo_texto_enviado != texto {
            return Err("el LSP no tiene el texto al día");
        }

        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            options: opciones,
            work_done_progress_params: Default::default(),
        };
        let Ok(id_formateo) = sesion.cliente.peticion("textDocument/formatting", params).await else {
            return Err("no se pudo hablar con el LSP");
        };

        let limite = tokio::time::Instant::now() + TIMEOUT_FORMATEO;
        loop {
            let Some(sesion) = &mut self.sesion else { return Err("sin LSP activo") };
            match tokio::time::timeout_at(limite, sesion.cliente.receptor.recv()).await {
                Err(_) => {
                    let _ = sesion.cliente.notificacion("$/cancelRequest", json!({ "id": id_formateo })).await;
                    return Err("el LSP tardó demasiado en formatear");
                }
                Ok(None) => return Err("el LSP se cerró"),
                Ok(Some(MensajeEntrante::Respuesta { id, resultado })) if id == id_formateo => {
                    return match resultado {
                        Ok(valor) => {
                            tcode_lsp::parsear_ediciones_formateo(&valor, texto).map_err(|_| "respuesta de formato inválida")
                        }
                        Err(_) => Err("el LSP devolvió un error al formatear"),
                    };
                }
                Ok(Some(otro)) => self.procesar_mensaje(otro, layout).await,
            }
        }
    }

    /// El lenguaje de la sesión LSP activa, si hay una — usado por la
    /// sección "Lenguajes / LSP" del panel de administración para saber
    /// a cuál de sus filas corresponde el estado en vivo (PLAN.md §5.3:
    /// "ver estado conectado/error").
    pub fn lenguaje_activo(&self) -> Option<Lenguaje> {
        self.sesion.as_ref().map(|s| s.lenguaje)
    }

    /// Líneas de stderr acumuladas por la sesión activa, de la más
    /// vieja a la más nueva — vacío si no hay sesión, o si la hay pero
    /// nunca escribió nada (PLAN.md §5.3, "ver logs"; `Ctrl+K R`,
    /// `crates/app/src/main.rs`), más el total de líneas recibidas por la
    /// sesión (`Cliente::logs_con_total`) — lo que usa el visor en vivo
    /// (`EstadoLogsLsp::actualizar`, BACKLOG.md P1 #2) para saber cuántas
    /// son nuevas. `(vacío, 0)` sin sesión.
    pub fn logs_con_total(&self) -> (Vec<String>, u64) {
        self.sesion.as_ref().map(|s| s.cliente.logs_con_total()).unwrap_or_default()
    }

    /// Texto legible en español del estado de la sesión activa —
    /// `None` si no hay ninguna (el panel muestra "Inactivo" en ese
    /// caso, decidido ahí en vez de acá para no acoplar este módulo a
    /// cómo se ve la fila).
    pub fn estado_texto(&self) -> Option<&'static str> {
        self.sesion.as_ref().map(|s| match s.fase {
            Fase::Iniciando { .. } => "Iniciando…",
            Fase::Listo { .. } => "Conectado",
        })
    }

    /// Cierra la sesión LSP activa, si hay una (al salir de tcode).
    pub async fn cerrar(self) {
        if let Some(sesion) = self.sesion {
            sesion.cliente.cerrar().await;
        }
    }
}

/// Convierte una ruta de archivo a un URI `file://` válido para LSP
/// (RFC 8089), con percent-encoding RFC 3986 completo — antes solo se
/// escapaban los espacios, así que rutas con `#`, `?`, tildes u otros
/// caracteres fuera de ASCII imprimible quedaban truncadas o mal
/// interpretadas por el servidor (todo lo que sigue a un `#`/`?` sin
/// escapar se interpreta como fragmento/query del URI, no como parte de
/// la ruta). Sigue sin cubrir archivos cuyo nombre no sea UTF-8 válido
/// (poco común, y no hay forma simple/portable de acceder a los bytes
/// crudos del nombre sin código específico por SO) — `Path::display`
/// ya reemplaza esos bytes por `�` antes de que esta función los vea.
fn uri_de_archivo(ruta: &Path) -> Result<Uri> {
    let absoluta = if ruta.is_absolute() { ruta.to_path_buf() } else { std::env::current_dir()?.join(ruta) };
    let texto = codificar_ruta_para_uri(&absoluta.display().to_string(), cfg!(windows));
    format!("file://{texto}").parse::<Uri>().map_err(|e| anyhow::anyhow!("ruta no convertible a URI: {e}"))
}

/// Percent-encoding RFC 3986 de una ruta absoluta ya como texto, para
/// concatenar después de `"file://"`. `es_windows` decide la
/// normalización previa (parámetro en vez de `cfg!(windows)` acá adentro
/// para que los tests puedan ejercitar el camino de Windows sin
/// necesitar correr en Windows de verdad):
///
/// - Windows separa carpetas con `\`, no `/` — se convierten antes de
///   codificar (una vez convertida, una barra invertida ya no
///   existe como para que el paso de abajo la toque).
/// - Una ruta absoluta de Windows empieza con la letra de unidad
///   (`C:\...`), no con `/` — RFC 8089 pide anteponerle una barra más
///   para que el URI completo quede `file:///C:/...` (tres barras en
///   total: dos de la autoridad vacía, más el separador inicial del
///   path).
///
/// Después de esa normalización, cualquier byte fuera del conjunto
/// "unreserved" de la RFC (`ALPHA` / `DIGIT` / `-` / `.` / `_` / `~`) se
/// escapa como `%XX` en mayúsculas, salvo `/` que se preserva como
/// separador — incluye los dos puntos de la letra de unidad de Windows
/// (`C:` → `C%3A`), que es justo como lo hace VS Code.
fn codificar_ruta_para_uri(ruta: &str, es_windows: bool) -> String {
    let normalizada = if es_windows { ruta.replace('\\', "/") } else { ruta.to_string() };
    let con_barra_inicial = if es_windows && !normalizada.starts_with('/') {
        format!("/{normalizada}")
    } else {
        normalizada
    };

    con_barra_inicial
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests_uri {
    use super::*;

    #[test]
    fn ruta_unix_simple_no_cambia() {
        assert_eq!(codificar_ruta_para_uri("/home/user/archivo.rs", false), "/home/user/archivo.rs");
    }

    #[test]
    fn espacios_se_codifican() {
        assert_eq!(codificar_ruta_para_uri("/home/user/mi archivo.rs", false), "/home/user/mi%20archivo.rs");
    }

    #[test]
    fn caracteres_especiales_de_uri_se_codifican() {
        // "#" y "?" sin escapar romperían el URI (se interpretarían
        // como el inicio del fragmento/query) — el caso que justifica
        // esta pieza.
        assert_eq!(codificar_ruta_para_uri("/tmp/nota#1.md", false), "/tmp/nota%231.md");
        assert_eq!(codificar_ruta_para_uri("/tmp/¿qué?.txt", false), "/tmp/%C2%BFqu%C3%A9%3F.txt");
    }

    #[test]
    fn puntos_guion_y_guion_bajo_no_se_codifican() {
        assert_eq!(codificar_ruta_para_uri("/tmp/mi-archivo_v2.0.tar.gz", false), "/tmp/mi-archivo_v2.0.tar.gz");
    }

    #[test]
    fn ruta_windows_convierte_barras_y_antepone_barra_inicial() {
        assert_eq!(codificar_ruta_para_uri(r"C:\Users\nombre\archivo.rs", true), "/C%3A/Users/nombre/archivo.rs");
    }

    #[test]
    fn ruta_windows_con_espacios_y_letra_de_unidad_minuscula() {
        assert_eq!(
            codificar_ruta_para_uri(r"c:\Program Files\proyecto\main.rs", true),
            "/c%3A/Program%20Files/proyecto/main.rs"
        );
    }

    #[test]
    fn uri_de_archivo_produce_tres_barras_tras_el_esquema_en_windows() {
        let texto = format!("file://{}", codificar_ruta_para_uri(r"C:\Users\a.rs", true));
        assert_eq!(texto, "file:///C%3A/Users/a.rs");
        assert!(texto.parse::<Uri>().is_ok(), "el URI resultante debe ser válido");
    }
}

