//! Renderizado en terminal de `tcode` con `ratatui`. Observa el estado del
//! `core` para dibujarlo, coloreado según el [`Tema`](tcode_config::Tema)
//! activo y el resaltado de sintaxis de `tcode-syntax`; no modifica el
//! `core` (ver PLAN.md §3).

mod paleta;
mod statusbar;
mod vista_codigo;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use tcode_core::Editor;
use tcode_syntax::Resaltador;

pub use paleta::Paleta;

/// Estado propio de la UI que no pertenece al `core` — por ahora, solo el
/// desplazamiento vertical del viewport.
#[derive(Default)]
pub struct EstadoUi {
    scroll_vertical: usize,
}

/// Dibuja un frame completo: vista de código arriba, statusbar de una línea
/// abajo (PLAN.md §1).
#[allow(clippy::too_many_arguments)]
pub fn dibujar(
    frame: &mut Frame,
    editor: &Editor,
    estado: &mut EstadoUi,
    ruta_mostrada: &str,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
) {
    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    vista_codigo::dibujar(frame, partes[0], editor, estado, paleta, resaltador, ruta_mostrada);
    statusbar::dibujar(frame, partes[1], editor, ruta_mostrada, paleta);
}
