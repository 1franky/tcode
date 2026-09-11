//! Renderizado en terminal de `tcode` con `ratatui`. Observa el estado del
//! `core` para dibujarlo, coloreado según el [`Tema`](tcode_config::Tema)
//! activo y el resaltado de sintaxis de `tcode-syntax`; no modifica el
//! `core` (ver PLAN.md §3).

mod overlay;
mod paleta;
mod paneles;
mod panel_archivos;
mod panel_buscador;
mod panel_paleta;
mod statusbar;
mod vista_codigo;

use ratatui::layout::{Constraint, Direction, Layout as LayoutRatatui};
use ratatui::Frame;

use tcode_commands::EstadoPaleta;
use tcode_fs::{BuscadorArchivos, Explorador};
use tcode_syntax::Resaltador;

pub use paleta::Paleta;
pub use paneles::{DireccionSplit, Layout, PanelEditor};

/// Ancho fijo (en columnas) del panel lateral del explorador cuando está
/// visible.
const ANCHO_PANEL_LATERAL: u16 = 30;

/// Estado propio de la UI que no pertenece al `core` — por ahora, solo el
/// desplazamiento vertical del viewport (uno por [`PanelEditor`]).
#[derive(Default)]
pub struct EstadoUi {
    scroll_vertical: usize,
}

/// Dibuja un frame completo: panel lateral del explorador (si está
/// visible, `Ctrl+B`) a la izquierda, el árbol de paneles de edición
/// (`Ctrl+\`/`Ctrl+K Ctrl+\`, PLAN.md §4) a la derecha, y la paleta de
/// comandos (`Ctrl+Shift+P`/`F1`) o el buscador de archivos (`Ctrl+P`)
/// encima de todo cuando alguno de los dos está abierto (nunca los dos a
/// la vez).
pub fn dibujar(
    frame: &mut Frame,
    layout: &mut Layout,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
    explorador: &Explorador,
    paleta_comandos: &EstadoPaleta,
    buscador_archivos: &BuscadorArchivos,
) {
    let area_total = frame.area();

    let area_principal = if explorador.visible() {
        let partes = LayoutRatatui::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(ANCHO_PANEL_LATERAL), Constraint::Min(1)])
            .split(area_total);
        panel_archivos::dibujar(frame, partes[0], explorador, paleta);
        partes[1]
    } else {
        area_total
    };

    layout.dibujar(frame, area_principal, paleta, resaltador);

    if paleta_comandos.activa() {
        panel_paleta::dibujar(frame, area_total, paleta_comandos, paleta);
    } else if buscador_archivos.activo() {
        panel_buscador::dibujar(frame, area_total, buscador_archivos, paleta);
    }
}
