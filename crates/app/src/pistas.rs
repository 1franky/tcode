//! Inlay hints (BACKLOG.md P2 #23) del lado de `app`: cuándo pedirlos y
//! qué hacer con la respuesta. Se piden para el documento activo cuando
//! sus hints son de otro texto y el texto actual se quedó quieto
//! [`PAUSA`] (lo vigila el tick del bucle), para todo el archivo de una
//! vez. La respuesta se aplica solo si el texto sigue
//! siendo el que se le mandó al servidor; si no, se descarta y el
//! próximo tick vuelve a pedir. Dibujarlos (y correr el cursor y los
//! clics) es cosa de `tcode_ui::pistas` y `vista_codigo`.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use lsp_types::Position;
use tcode_ui::pistas::{Pista, PistasInlay};
use tcode_ui::Layout as PanelLayout;

use crate::lsp::TipoPedido;
use crate::{sincronizar_lsp, EstadoApp};

/// Cuánto tiene que quedarse quieto el texto antes de pedir hints.
const PAUSA: Duration = Duration::from_millis(300);
/// Un pedido sin respuesta después de esto se da por perdido (si no, un
/// servidor que no contesta dejaría de recibir pedidos para siempre).
const VENCIMIENTO: Duration = Duration::from_secs(5);

/// Qué documento y con qué texto (`ruta mostrada`, `Buffer::revision`).
type Clave = (String, u64);

#[derive(Default)]
pub struct EstadoPistas {
    /// El texto del documento activo que hace falta pedir, y desde
    /// cuándo está así.
    visto: Option<(Clave, Instant)>,
    /// Ya pasó la pausa: pedir en la próxima vuelta del bucle (que es
    /// async; el tick no).
    pub por_pedir: bool,
    /// Pedido en vuelo, y cuándo se mandó.
    pedido: Option<(Clave, Instant)>,
}

/// Si el documento activo necesita hints nuevos: prendidos, sin ajuste de
/// línea, con un servidor que los ofrece y con los que tiene calculados
/// para otro texto.
fn clave_pendiente(layout: &PanelLayout, estado: &EstadoApp) -> Option<Clave> {
    let config = &estado.config.editor;
    if !config.inlay_hints || config.ajuste_linea {
        return None;
    }
    let panel = layout.panel_activo();
    let revision = panel.editor.buffer().revision();
    if panel.pistas.revision == revision {
        return None;
    }
    estado.lsp.capacidades(&panel.ruta_mostrada).filter(|c| c.pistas_inlay)?;
    Some((panel.ruta_mostrada.clone(), revision))
}

impl EstadoPistas {
    fn esperando(&self) -> bool {
        self.pedido.as_ref().is_some_and(|(_, cuando)| cuando.elapsed() < VENCIMIENTO)
    }

    pub fn necesita_tick(&self, layout: &PanelLayout, estado: &EstadoApp) -> bool {
        estado.lsp.refresco_pistas_pendiente()
            || (!self.esperando() && !self.por_pedir && clave_pendiente(layout, estado).is_some())
    }
}

/// Un tick: si el documento activo necesita hints y su texto se quedó
/// quieto, deja el pedido para la próxima vuelta del bucle.
pub fn tick(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    // `workspace/inlayHint/refresh`: los hints de todos los documentos
    // pasan a ser "de otro texto" (se siguen viendo mientras tanto, salvo
    // en la línea del cursor) y se vuelven a pedir.
    if estado.lsp.tomar_refresco_pistas() {
        for panel in layout.paneles_mut() {
            panel.pistas.revision = 0;
        }
        estado.pistas.pedido = None;
    }
    let clave = clave_pendiente(layout, estado);
    let pistas = &mut estado.pistas;
    match (clave, &pistas.visto) {
        (None, _) => pistas.visto = None,
        (Some(clave), Some((vista, desde))) if *vista == clave => {
            if desde.elapsed() >= PAUSA && !pistas.esperando() {
                pistas.por_pedir = true;
            }
        }
        (Some(clave), _) => pistas.visto = Some((clave, Instant::now())),
    }
}

/// Manda el pedido (async: antes sincroniza el texto con el servidor).
pub async fn enviar(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    estado.pistas.por_pedir = false;
    sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
    let Some(clave) = clave_pendiente(layout, estado) else { return };
    // El rango termina en el último carácter que existe: rust-analyzer
    // rechaza un fin más allá del archivo ("Invalid offset").
    let buffer = layout.editor_activo().buffer();
    let ultima = buffer.num_lineas().saturating_sub(1);
    let largo: usize = buffer.linea_texto(ultima).chars().map(char::len_utf16).sum();
    let rango = json!({ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": ultima, "character": largo } } });
    if estado.lsp.pedir(TipoPedido::PistasInlay, &clave.0, Position::default(), rango).await.is_ok() {
        estado.pistas.pedido = Some((clave, Instant::now()));
    }
}

/// Llegó la respuesta (o un error, `None`): se aplica si el documento
/// activo sigue teniendo el texto que se le mandó.
pub fn respuesta(layout: &mut PanelLayout, estado: &mut EstadoApp, valor: Option<&Value>) {
    let Some(((ruta, revision), _)) = estado.pistas.pedido.take() else { return };
    let (Some(valor), panel) = (valor, layout.panel_activo_mut()) else { return };
    let buffer = panel.editor.buffer();
    if panel.ruta_mostrada != ruta || buffer.revision() != revision {
        return;
    }
    let num_lineas = buffer.num_lineas();
    let pistas = tcode_lsp::parsear_pistas(valor)
        .into_iter()
        .filter(|p| (p.linea as usize) < num_lineas)
        .map(|p| {
            let linea = p.linea as usize;
            let mut unidades = 0;
            let columna = buffer
                .linea_texto(linea)
                .chars()
                .take_while(|c| {
                    let dentro = unidades < p.caracter;
                    unidades += c.len_utf16() as u32;
                    dentro
                })
                .count();
            Pista { linea, columna, texto: p.texto }
        })
        .collect();
    panel.pistas = PistasInlay::nuevas(revision, num_lineas, pistas);
}

/// Apaga los hints de todos los documentos (al apagar la opción).
pub fn limpiar(layout: &mut PanelLayout) {
    for panel in layout.paneles_mut() {
        panel.pistas = PistasInlay::default();
    }
}
