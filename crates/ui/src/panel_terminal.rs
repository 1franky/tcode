//! Panel de la terminal integrada (BACKLOG.md P3 #26): una franja abajo
//! del área de edición con un título (la shell, y si tiene el foco) y la
//! pantalla que interpretó `vt100` celda por celda — colores (16, 256 y
//! RGB), negrita, itálica, subrayado, video inverso y caracteres anchos.
//! La sesión en sí (la shell, la PTY) es `tcode_terminal`; acá solo se
//! dibuja.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear};
use ratatui::Frame;

use crate::Paleta;

fn color(c: vt100::Color) -> Option<Color> {
    match c {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) => Some(Color::Indexed(i)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

/// Dibuja el panel en `area`. Devuelve el área interior (donde va la
/// pantalla: su tamaño es el que tiene que tener la terminal) y, si
/// `enfocada`, dónde va el cursor.
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    pantalla: &vt100::Screen,
    titulo: &str,
    enfocada: bool,
    paleta: &Paleta,
) -> (Rect, Option<(u16, u16)>) {
    frame.render_widget(Clear, area);
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let estilo_titulo = if enfocada { estilo_base.add_modifier(Modifier::BOLD) } else { estilo_base.fg(paleta.numero_linea) };
    let bloque = Block::default()
        .borders(Borders::TOP)
        .border_set(crate::BORDE_ASCII)
        .border_style(estilo_base.fg(paleta.numero_linea))
        .title(ratatui::text::Span::styled(format!(" {titulo} "), estilo_titulo))
        .style(estilo_base);
    let interior = bloque.inner(area);
    frame.render_widget(bloque, area);

    let (filas, columnas) = pantalla.size();
    let buffer = frame.buffer_mut();
    for fila in 0..filas.min(interior.height) {
        for columna in 0..columnas.min(interior.width) {
            let Some(celda) = pantalla.cell(fila, columna) else { continue };
            if celda.is_wide_continuation() {
                continue;
            }
            let (mut fg, mut bg) = (color(celda.fgcolor()).unwrap_or(paleta.texto), color(celda.bgcolor()).unwrap_or(paleta.fondo));
            if celda.inverse() {
                std::mem::swap(&mut fg, &mut bg);
            }
            let mut estilo = Style::default().fg(fg).bg(bg);
            if celda.bold() {
                estilo = estilo.add_modifier(Modifier::BOLD);
            }
            if celda.italic() {
                estilo = estilo.add_modifier(Modifier::ITALIC);
            }
            if celda.underline() {
                estilo = estilo.add_modifier(Modifier::UNDERLINED);
            }
            let contenido = celda.contents();
            let simbolo = if contenido.is_empty() { " " } else { contenido.as_str() };
            if let Some(destino) = buffer.cell_mut((interior.x + columna, interior.y + fila)) {
                destino.set_symbol(simbolo).set_style(estilo);
            }
        }
    }

    // Sin cursor si la shell lo ocultó o si se está mirando el historial.
    let cursor = (enfocada && !pantalla.hide_cursor() && pantalla.scrollback() == 0).then(|| {
        let (fila, columna) = pantalla.cursor_position();
        (interior.x + columna.min(interior.width.saturating_sub(1)), interior.y + fila.min(interior.height.saturating_sub(1)))
    });
    if let Some(posicion) = cursor {
        frame.set_cursor_position(posicion);
    }
    (interior, cursor)
}
