use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use tcode_fs::Explorador;

use crate::Paleta;

/// Panel lateral del explorador de archivos (`Ctrl+B`, PLAN.md §4/§11 M1):
/// árbol navegable con indentación por profundidad y la fila seleccionada
/// resaltada.
pub fn dibujar(frame: &mut Frame, area: Rect, explorador: &Explorador, paleta: &Paleta) {
    let seleccion = explorador.seleccion();

    let items: Vec<ListItem> = explorador
        .lista_visible()
        .into_iter()
        .enumerate()
        .map(|(idx, (profundidad, nodo))| {
            let indentacion = "  ".repeat(profundidad);
            let icono = if nodo.es_carpeta {
                if nodo.expandida {
                    "▾ "
                } else {
                    "▸ "
                }
            } else {
                "  "
            };
            let estilo = if idx == seleccion {
                Style::default().bg(paleta.linea_actual).fg(paleta.texto)
            } else {
                Style::default().fg(paleta.texto)
            };
            ListItem::new(Line::from(format!("{indentacion}{icono}{}", nodo.nombre))).style(estilo)
        })
        .collect();

    let bloque = Block::default()
        .borders(Borders::RIGHT)
        .title(explorador.nombre_raiz().to_string())
        .style(Style::default().bg(paleta.fondo).fg(paleta.texto));

    frame.render_widget(List::new(items).block(bloque), area);
}
