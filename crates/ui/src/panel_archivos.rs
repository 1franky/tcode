use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use tcode_fs::Explorador;

use crate::Paleta;

/// Panel lateral del explorador de archivos (`Ctrl+B`, PLAN.md §4/§11 M1):
/// árbol navegable con indentación por profundidad y la fila seleccionada
/// resaltada. Los íconos de carpeta son ASCII a propósito (`v`/`>`, no
/// `▾`/`▸`): esos caracteres geométricos tienen ancho "ambiguo" en
/// Unicode (1 o 2 columnas según la fuente/configuración regional de
/// cada terminal) — un sospechoso concreto de un bug de desalineación
/// permanente reportado en Windows Terminal, que coincide en que
/// aparecía específicamente en este panel y no se autocorregía sola
/// (ver la memoria del proyecto). ASCII puro nunca tiene ese problema.
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
                    "v "
                } else {
                    "> "
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
        .border_set(crate::BORDE_ASCII)
        .title(explorador.nombre_raiz().to_string())
        .style(Style::default().bg(paleta.fondo).fg(paleta.texto));

    frame.render_widget(List::new(items).block(bloque), area);
}
