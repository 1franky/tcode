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

use std::path::Path;

use anyhow::Result;
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, InitializedParams,
    TextDocumentContentChangeEvent, TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
};
use tcode_lsp::{Cliente, MensajeEntrante};
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
    fase: Fase,
    uri: Uri,
    ultimo_texto_enviado: String,
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

    /// Al cambiar de archivo activo (abrir uno nuevo, cambiar de panel):
    /// si el lenguaje detectado tiene un LSP configurado y es distinto
    /// del que ya está corriendo, cierra la sesión vieja y arranca una
    /// nueva. Si el nuevo archivo no tiene lenguaje con LSP, o el server
    /// configurado no se pudo lanzar (no está instalado), simplemente no
    /// queda sesión activa — el editor sigue funcionando igual, sin LSP.
    pub async fn actualizar_para_archivo(&mut self, ruta: &str, contenido: &str) {
        let lenguaje = Lenguaje::detectar_por_extension(ruta);

        let necesita_relanzar = match (&self.sesion, lenguaje) {
            (Some(sesion), Some(l)) => sesion.lenguaje != l,
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };
        if !necesita_relanzar {
            return;
        }

        if let Some(sesion) = self.sesion.take() {
            sesion.cliente.cerrar().await;
        }

        let Some(lenguaje) = lenguaje else { return };
        let Some((comando, args)) = tcode_lsp::comando_para(lenguaje) else { return };
        let Ok(uri) = uri_de_archivo(Path::new(ruta)) else { return };
        let Ok(mut cliente) = Cliente::lanzar(comando, args).await else { return };

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let Ok(id_initialize) = cliente.peticion("initialize", params).await else { return };

        self.sesion = Some(SesionLsp {
            cliente,
            lenguaje,
            fase: Fase::Iniciando { id_initialize },
            uri,
            ultimo_texto_enviado: contenido.to_string(),
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
                    if id == id_initialize && resultado.is_ok() {
                        let _ = sesion.cliente.notificacion("initialized", InitializedParams {}).await;
                        let _ = sesion
                            .cliente
                            .notificacion(
                                "textDocument/didOpen",
                                DidOpenTextDocumentParams {
                                    text_document: TextDocumentItem {
                                        uri: sesion.uri.clone(),
                                        language_id: id_lenguaje_lsp(sesion.lenguaje).to_string(),
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
                    if let Ok((uri, diagnosticos)) = tcode_lsp::parsear_diagnosticos(&params) {
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

    /// Cierra la sesión LSP activa, si hay una (al salir de tcode).
    pub async fn cerrar(self) {
        if let Some(sesion) = self.sesion {
            sesion.cliente.cerrar().await;
        }
    }
}

/// Convierte una ruta de archivo a un URI `file://` válido para LSP.
/// Escapado mínimo (solo espacios): suficiente para las rutas típicas de
/// un proyecto; un percent-encoding completo según RFC 3986 es una mejora
/// pendiente para nombres de archivo con caracteres más exóticos.
fn uri_de_archivo(ruta: &Path) -> Result<Uri> {
    let absoluta = if ruta.is_absolute() { ruta.to_path_buf() } else { std::env::current_dir()?.join(ruta) };
    let texto = absoluta.display().to_string().replace(' ', "%20");
    format!("file://{texto}").parse::<Uri>().map_err(|e| anyhow::anyhow!("ruta no convertible a URI: {e}"))
}

/// `languageId` que exige LSP en `TextDocumentItem` — identifica el
/// lenguaje ante el servidor, distinto del nombre que muestra la
/// statusbar.
fn id_lenguaje_lsp(lenguaje: Lenguaje) -> &'static str {
    match lenguaje {
        Lenguaje::Python => "python",
        Lenguaje::Rust => "rust",
        Lenguaje::JavaScript => "javascript",
        Lenguaje::Go => "go",
        Lenguaje::Markdown => "markdown",
    }
}
