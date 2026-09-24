//! Renderizado en terminal de `tcode` con `ratatui`. Observa el estado del
//! `core` para dibujarlo, coloreado según el [`Tema`](tcode_config::Tema)
//! activo y el resaltado de sintaxis de `tcode-syntax`; no modifica el
//! `core` (ver PLAN.md §3).

mod breadcrumbs;
mod editor_tema;
mod overlay;
mod paleta;
mod paneles;
mod panel_admin;
mod panel_archivos;
mod panel_buscador;
mod panel_busqueda;
mod panel_confirmar_borrado;
mod panel_guardar_como;
mod panel_logs_lsp;
mod panel_paleta;
mod panel_prompt_explorador;
mod panel_selector_tema;
mod statusbar;
mod vista_codigo;
mod vista_csv;
mod vista_markdown;

use ratatui::layout::{Constraint, Direction, Layout as LayoutRatatui};
use ratatui::Frame;

use tcode_commands::EstadoPaleta;
use tcode_config::{Config, ConfigInterfaz, ConfigProyecto, EstadoEditorTema, EstadoPanelAdmin, EstadoSelectorTema};
use tcode_core::{EstadoBusqueda, EstadoGuardarComo};
use tcode_fs::{BuscadorArchivos, EstadoConfirmarBorrado, EstadoPromptExplorador, Explorador};
use tcode_keymap::Keymap;
use tcode_lsp::EstadoLogsLsp;
use tcode_syntax::Resaltador;

pub use paleta::Paleta;
pub use paneles::{DireccionSplit, Layout, ModoCsv, ModoMarkdown, PanelEditor};
pub use panel_admin::FilaLenguajeLsp;
pub use vista_csv::ancho_columna as ancho_columna_csv;

/// Ancho fijo (en columnas) del panel lateral del explorador cuando está
/// visible.
const ANCHO_PANEL_LATERAL: u16 = 30;

/// Conjunto de caracteres de borde ASCII (`+`/`-`/`|`), en vez del
/// `border::PLAIN` por defecto de `ratatui` (`┌┐└┘│─`) que usa todo
/// `Block` con bordes si no se le pasa `.border_set(...)` explícito.
/// Esos caracteres de box-drawing tienen ancho "ambiguo" en Unicode —
/// sospechosos de un bug de desalineación persistente reportado en
/// Windows Terminal (ver el resto de reemplazos ASCII de esta misma
/// pieza: `panel_archivos`, `statusbar`, `vista_markdown`). Como TODOS
/// los paneles con recuadro de la app pasan por `Block`, este único
/// conjunto compartido cubre la superficie más grande de una sola vez.
pub(crate) const BORDE_ASCII: ratatui::symbols::border::Set = ratatui::symbols::border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

/// Estado propio de la UI que no pertenece al `core` (uno por
/// [`PanelEditor`]): un único desplazamiento de scroll, reinterpretado
/// según qué vista esté activa en ese panel — fila lógica de scroll
/// vertical para `vista_codigo`/`vista_markdown` (ya se compartía entre
/// esas dos), o columna de scroll horizontal para `vista_csv` en modo
/// `ModoCsv::Tabla`. Un mismo panel nunca tiene dos de esas vistas
/// activas a la vez (`ModoCsv`/`ModoMarkdown` son mutuamente
/// excluyentes), así que un solo campo alcanza sin pisarse — evita hacer
/// crecer `PanelEditor` (y con él, la variante más grande de `Panel`) por
/// cada vista nueva que necesite recordar un desplazamiento.
#[derive(Default)]
pub struct EstadoUi {
    scroll: usize,
    /// Solo para `vista_codigo` con ajuste de línea activo: con el ajuste,
    /// `scroll` es la línea LÓGICA de arriba de todo y esto la sub-fila
    /// de esa línea desde la que se empieza a ver (una línea larga puede
    /// quedar cortada por el borde de arriba). Guardar el scroll relativo
    /// a una línea, en vez de como índice de fila visual sobre todo el
    /// archivo, es lo que evita partir en filas el archivo entero en cada
    /// frame (BACKLOG.md P1 #14, `vista_codigo::ajustar_scroll_con_ajuste`).
    /// Sin ajuste vale siempre 0.
    subfila_scroll: usize,
    /// Partes de la ruta ya resueltas para el breadcrumb del panel
    /// (BACKLOG.md P3 #10, ver `breadcrumbs::CacheRuta`).
    breadcrumbs: breadcrumbs::CacheRuta,
}

/// Modo zen (`Ctrl+K Z`, BACKLOG.md P3 #12): punto ÚNICO que decide qué
/// "cromo" alrededor del código se dibuja en este frame. Es un booleano de
/// sesión (vive en `app`, no en la config): en zen se apaga todo lo que no
/// es código SIN tocar los toggles de config — al salir, cada barra vuelve
/// a lo que diga su config de siempre. Hoy cubre el explorador
/// (`explorador_visible`) y la statusbar (`interfaz.mostrar_statusbar`);
/// cualquier barra nueva alrededor del código (pestañas, breadcrumbs...)
/// tiene que preguntar acá — agregar su `mostrar_*` a `interfaz` o su
/// propio booleano a este struct — en vez de leer `config` directo.
struct Cromo<'a> {
    explorador_visible: bool,
    interfaz: std::borrow::Cow<'a, ConfigInterfaz>,
}

impl<'a> Cromo<'a> {
    fn nuevo(modo_zen: bool, explorador_visible: bool, interfaz: &'a ConfigInterfaz) -> Self {
        if !modo_zen {
            return Self { explorador_visible, interfaz: std::borrow::Cow::Borrowed(interfaz) };
        }
        let interfaz = ConfigInterfaz { mostrar_statusbar: false, ..interfaz.clone() };
        Self { explorador_visible: false, interfaz: std::borrow::Cow::Owned(interfaz) }
    }
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
/// selector de temas (`Ctrl+K Ctrl+T`), el prompt de "Guardar como"
/// (`Ctrl+Shift+S`), el visor de logs del LSP activo (`Ctrl+K R`), el
/// prompt de texto del explorador (`Ctrl+K N`/`Ctrl+K C`/`Ctrl+K M` —
/// nuevo archivo/carpeta/renombrar) o su confirmación de borrado
/// (`Delete`) encima de todo cuando alguno de los siete está abierto (son
/// mutuamente excluyentes — nunca dos a la vez). `modo_zen` esconde el
/// explorador y las barras (ver [`Cromo`]); los overlays se siguen
/// dibujando igual, sobre el área completa.
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
    config_global: &Config,
    config_proyecto: Option<&ConfigProyecto>,
    keymap: &Keymap,
    filas_lenguajes: &[FilaLenguajeLsp],
    editor_tema: &EstadoEditorTema,
    logs_lsp: &EstadoLogsLsp,
    prompt_explorador: &EstadoPromptExplorador,
    confirmar_borrado: &EstadoConfirmarBorrado,
    modo_zen: bool,
) {
    let area_total = frame.area();

    if editor_tema.activo() {
        editor_tema::dibujar(frame, area_total, editor_tema, paleta);
        return;
    }

    if panel_admin_estado.activo() {
        let capas = panel_admin::CapasPanel { efectiva: config, global: config_global, proyecto: config_proyecto };
        panel_admin::dibujar(frame, area_total, panel_admin_estado, &capas, keymap, filas_lenguajes, paleta);
        return;
    }

    let cromo = Cromo::nuevo(modo_zen, explorador.visible(), &config.interfaz);

    let area_principal = if cromo.explorador_visible {
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
        config.editor.columna_regla,
        config.editor.indicadores_git,
        &cromo.interfaz,
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
    } else if logs_lsp.activo() {
        panel_logs_lsp::dibujar(frame, area_total, logs_lsp, paleta);
    } else if prompt_explorador.activo() {
        panel_prompt_explorador::dibujar(frame, area_total, prompt_explorador, paleta);
    } else if confirmar_borrado.activo() {
        panel_confirmar_borrado::dibujar(frame, area_total, confirmar_borrado, paleta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuera_de_zen_el_cromo_respeta_la_config_tal_cual() {
        let interfaz = ConfigInterfaz::default();
        let cromo = Cromo::nuevo(false, true, &interfaz);
        assert!(cromo.explorador_visible);
        assert!(cromo.interfaz.mostrar_statusbar);

        let sin_barra = ConfigInterfaz { mostrar_statusbar: false, ..ConfigInterfaz::default() };
        let cromo = Cromo::nuevo(false, false, &sin_barra);
        assert!(!cromo.explorador_visible);
        assert!(!cromo.interfaz.mostrar_statusbar);
    }

    #[test]
    fn en_zen_se_oculta_todo_sin_tocar_el_resto_de_la_interfaz() {
        let interfaz = ConfigInterfaz { statusbar_eol: false, ..ConfigInterfaz::default() };
        let cromo = Cromo::nuevo(true, true, &interfaz);
        assert!(!cromo.explorador_visible);
        assert!(!cromo.interfaz.mostrar_statusbar);
        // El resto se copia igual — y la config original no se toca.
        assert!(!cromo.interfaz.statusbar_eol);
        assert!(interfaz.mostrar_statusbar);
    }
}
