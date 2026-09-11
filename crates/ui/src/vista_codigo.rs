use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_core::Editor;

use crate::{EstadoUi, Paleta};

/// Dibuja el contenido del archivo, resalta la línea del cursor con el
/// color del tema activo y posiciona el cursor real de la terminal
/// (parpadeante) sobre él. Sin resaltado de sintaxis todavía — tree-sitter
/// llega en otra pieza de M1 (PLAN.md §11).
pub fn dibujar(frame: &mut Frame, area: Rect, editor: &Editor, estado: &mut EstadoUi, paleta: &Paleta) {
    let alto_visible = area.height as usize;
    let ancho_visible = area.width as usize;
    let cursor = editor.cursor();
    ajustar_scroll(estado, cursor.linea, alto_visible);

    let lineas = editor.buffer().lineas_texto();
    let visibles: Vec<Line> = lineas
        .iter()
        .enumerate()
        .skip(estado.scroll_vertical)
        .take(alto_visible)
        .map(|(idx, linea)| {
            if idx == cursor.linea {
                // Se rellena con espacios para que el resaltado de la línea
                // actual cubra todo el ancho, no solo el texto.
                let texto = format!("{linea:<ancho_visible$}");
                Line::from(texto).style(Style::default().bg(paleta.linea_actual).fg(paleta.texto))
            } else {
                Line::from(linea.as_str())
            }
        })
        .collect();

    frame.render_widget(
        Paragraph::new(visibles).style(Style::default().bg(paleta.fondo).fg(paleta.texto)),
        area,
    );

    let columna = area.x + cursor.columna as u16;
    let fila = area.y + (cursor.linea - estado.scroll_vertical) as u16;
    frame.set_cursor_position((columna, fila));
}

/// Desplazamiento vertical automático: mantiene la línea del cursor siempre
/// visible dentro del área disponible. Es un concern puro de renderizado,
/// no vive en el `core`.
fn ajustar_scroll(estado: &mut EstadoUi, linea_cursor: usize, alto_visible: usize) {
    if alto_visible == 0 {
        return;
    }
    if linea_cursor < estado.scroll_vertical {
        estado.scroll_vertical = linea_cursor;
    } else if linea_cursor >= estado.scroll_vertical + alto_visible {
        estado.scroll_vertical = linea_cursor - alto_visible + 1;
    }
}
