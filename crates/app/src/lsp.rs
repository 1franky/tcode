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
use tcode_config::Config;
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
    /// Comando + argumentos con los que se lanzó esta sesión —
    /// `comando_efectivo` en el momento del lanzamiento. Se guarda para
    /// que `actualizar_para_archivo` note un cambio de configuración
    /// (usuario edita el comando personalizado desde el panel de
    /// administración) aunque el lenguaje no haya cambiado, y relance.
    comando_usado: (String, Vec<String>),
    fase: Fase,
    uri: Uri,
    ultimo_texto_enviado: String,
}

/// Comando + argumentos a usar para lanzar el LSP de `lenguaje`: el que
/// configuró el usuario a mano en la sección "Lenguajes / LSP" (PLAN.md
/// §5.3), si hay uno, o si no el que trae `tcode_lsp::comando_para` por
/// defecto (que puede no haber ninguno, como para todos los lenguajes
/// salvo Python por ahora).
pub fn comando_efectivo(lenguaje: Lenguaje, config: &Config) -> Option<(String, Vec<String>)> {
    if let Some(personalizado) = config.lenguajes.comando_configurado(lenguaje.id()) {
        return Some((personalizado.comando.clone(), personalizado.argumentos.clone()));
    }
    tcode_lsp::comando_para(lenguaje)
        .map(|(comando, args)| (comando.to_string(), args.iter().map(|a| a.to_string()).collect()))
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
        let Some((comando, args)) = comando_efectivo_actual else { return };
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        let Ok(uri) = uri_de_archivo(Path::new(ruta)) else { return };
        let Ok(mut cliente) = Cliente::lanzar(&comando, &args_ref).await else { return };

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let Ok(id_initialize) = cliente.peticion("initialize", params).await else { return };

        self.sesion = Some(SesionLsp {
            cliente,
            lenguaje,
            comando_usado: (comando, args),
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

    /// El lenguaje de la sesión LSP activa, si hay una — usado por la
    /// sección "Lenguajes / LSP" del panel de administración para saber
    /// a cuál de sus filas corresponde el estado en vivo (PLAN.md §5.3:
    /// "ver estado conectado/error").
    pub fn lenguaje_activo(&self) -> Option<Lenguaje> {
        self.sesion.as_ref().map(|s| s.lenguaje)
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

/// Convierte una ruta de archivo a un URI `file://` válido para LSP.
/// Escapado mínimo (solo espacios): suficiente para las rutas típicas de
/// un proyecto; un percent-encoding completo según RFC 3986 es una mejora
/// pendiente para nombres de archivo con caracteres más exóticos.
fn uri_de_archivo(ruta: &Path) -> Result<Uri> {
    let absoluta = if ruta.is_absolute() { ruta.to_path_buf() } else { std::env::current_dir()?.join(ruta) };
    let texto = absoluta.display().to_string().replace(' ', "%20");
    format!("file://{texto}").parse::<Uri>().map_err(|e| anyhow::anyhow!("ruta no convertible a URI: {e}"))
}

