//! Renderizado en terminal de `tcode` con `ratatui`. Observa el estado del
//! `core` para dibujarlo, coloreado según el [`Tema`](tcode_config::Tema)
//! activo y el resaltado de sintaxis de `tcode-syntax`; no modifica el
//! `core` (ver PLAN.md §3).

mod paleta;
mod panel_archivos;
mod statusbar;
mod vista_codigo;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use tcode_core::Editor;
use tcode_fs::Explorador;
use tcode_syntax::Resaltador;

pub use paleta::Paleta;

/// Ancho fijo (en columnas) del panel lateral del explorador cuando está
/// visible.
const ANCHO_PANEL_LATERAL: u16 = 30;

/// Estado propio de la UI que no pertenece al `core` — por ahora, solo el
/// desplazamiento vertical del viewport.
#[derive(Default)]
pub struct EstadoUi {
    scroll_vertical: usize,
}

/// Dibuja un frame completo: panel lateral del explorador (si está
/// visible, `Ctrl+B`) a la izquierda, vista de código y statusbar a la
/// derecha (PLAN.md §1).
#[allow(clippy::too_many_arguments)]
pub fn dibujar(
    frame: &mut Frame,
    editor: &Editor,
    estado: &mut EstadoUi,
    ruta_mostrada: &str,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
    explorador: &Explorador,
) {
    let area_total = frame.area();

    let area_principal = if explorador.visible() {
        let partes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(ANCHO_PANEL_LATERAL), Constraint::Min(1)])
            .split(area_total);
        panel_archivos::dibujar(frame, partes[0], explorador, paleta);
        partes[1]
    } else {
        area_total
    };

    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area_principal);

    vista_codigo::dibujar(frame, partes[0], editor, estado, paleta, resaltador, ruta_mostrada);
    statusbar::dibujar(frame, partes[1], editor, ruta_mostrada, paleta);
}
