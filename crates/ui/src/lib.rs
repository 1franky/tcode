//! Renderizado en terminal de `tcode` con `ratatui`. Observa el estado del
//! `core` para dibujarlo, coloreado según el [`Tema`](tcode_config::Tema)
//! activo y el resaltado de sintaxis de `tcode-syntax`; no modifica el
//! `core` (ver PLAN.md §3).

mod editor_tema;
mod overlay;
mod paleta;
mod paneles;
mod panel_admin;
mod panel_archivos;
mod panel_buscador;
mod panel_busqueda;
mod panel_guardar_como;
mod panel_paleta;
mod panel_selector_tema;
mod statusbar;
mod vista_codigo;
mod vista_csv;
mod vista_markdown;

use ratatui::layout::{Constraint, Direction, Layout as LayoutRatatui};
use ratatui::Frame;

use tcode_commands::EstadoPaleta;
use tcode_config::{Config, EstadoEditorTema, EstadoPanelAdmin, EstadoSelectorTema};
use tcode_core::{EstadoBusqueda, EstadoGuardarComo};
use tcode_fs::{BuscadorArchivos, Explorador};
use tcode_keymap::Keymap;
use tcode_syntax::Resaltador;

pub use paleta::Paleta;
pub use paneles::{DireccionSplit, Layout, ModoCsv, ModoMarkdown, PanelEditor};
pub use panel_admin::FilaLenguajeLsp;

/// Ancho fijo (en columnas) del panel lateral del explorador cuando está
/// visible.
const ANCHO_PANEL_LATERAL: u16 = 30;

/// Estado propio de la UI que no pertenece al `core` — por ahora, solo el
/// desplazamiento vertical del viewport (uno por [`PanelEditor`]).
#[derive(Default)]
pub struct EstadoUi {
    scroll_vertical: usize,
}

/// Dibuja un frame completo. El panel de administración (`Ctrl+,`,
/// PLAN.md §5) es una vista aparte, a pantalla completa — mientras está
/// activo, es lo único que se dibuja (ni editor ni explorador se ven
/// detrás, a diferencia de los overlays flotantes de abajo). En caso
/// contrario: panel lateral del explorador (si está visible, `Ctrl+B`) a
/// la izquierda, el árbol de paneles de edición (`Ctrl+\`/`Ctrl+K
/// Ctrl+\`, PLAN.md §4) a la derecha, la barra de búsqueda/reemplazo
/// (`Ctrl+F`/`Ctrl+H`) flotando en la esquina superior derecha del área
/// de edición si está abierta, y la paleta de comandos
/// (`Ctrl+Shift+P`/`F1`), el buscador de archivos (`Ctrl+P`), el
/// selector de temas (`Ctrl+K Ctrl+T`) o el prompt de "Guardar como"
/// (`Ctrl+Shift+S`) encima de todo cuando alguno de los cuatro está
/// abierto (son mutuamente excluyentes — nunca dos a la vez).
#[allow(clippy::too_many_arguments)]
pub fn dibujar(
    frame: &mut Frame,
    layout: &mut Layout,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
    explorador: &Explorador,
    paleta_comandos: &EstadoPaleta,
    buscador_archivos: &BuscadorArchivos,
    estado_busqueda: &EstadoBusqueda,
    guardar_como: &EstadoGuardarComo,
    selector_tema: &EstadoSelectorTema,
    panel_admin_estado: &EstadoPanelAdmin,
    config: &Config,
    keymap: &Keymap,
    filas_lenguajes: &[FilaLenguajeLsp],
    editor_tema: &EstadoEditorTema,
) {
    let area_total = frame.area();

    if editor_tema.activo() {
        editor_tema::dibujar(frame, area_total, editor_tema, paleta);
        return;
    }

    if panel_admin_estado.activo() {
        panel_admin::dibujar(frame, area_total, panel_admin_estado, config, keymap, filas_lenguajes, paleta);
        return;
    }

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

    layout.dibujar(
        frame,
        area_principal,
        paleta,
        resaltador,
        estado_busqueda,
        config.editor.numeros_de_linea,
        config.editor.ajuste_linea,
        &config.interfaz,
    );
    panel_busqueda::dibujar(frame, area_principal, estado_busqueda, paleta);

    if paleta_comandos.activa() {
        panel_paleta::dibujar(frame, area_total, paleta_comandos, paleta);
    } else if buscador_archivos.activo() {
        panel_buscador::dibujar(frame, area_total, buscador_archivos, paleta);
    } else if selector_tema.activa() {
        panel_selector_tema::dibujar(frame, area_total, selector_tema, paleta);
    } else if guardar_como.activa() {
        panel_guardar_como::dibujar(frame, area_total, guardar_como, paleta);
    }
}
