use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use tcode_core::EstadoIrALinea;

use crate::Paleta;

/// Mismo tamaño y forma que `panel_guardar_como`: una línea de campo más
/// una de ayuda/error, con borde.
const ANCHO: u16 = 40;
const ALTO: u16 = 4;

/// Dibuja el prompt "Ir a línea" (`Ctrl+G`, BACKLOG.md P0 #19) centrado
/// sobre lo que hubiera detrás, con el rango válido (`1-num_lineas`) en el
/// título. Va aparte de `tcode_ui::dibujar` (la llama `app` después, como
/// `panel_linea_vim`) para no sumarle otro parámetro más.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoIrALinea, num_lineas: usize, paleta: &Paleta) {
    if !estado.activo() {
        return;
    }
    let ancho = ANCHO.min(area_total.width);
    let alto = ALTO.min(area_total.height);
    if ancho < 10 || alto < 3 {
        return;
    }
    let area = Rect {
        x: area_total.x + (area_total.width - ancho) / 2,
        y: area_total.y + (area_total.height - alto) / 2,
        width: ancho,
        height: alto,
    };
    frame.render_widget(Clear, area);

    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let estilo_ayuda = if estado.error().is_some() { estilo_base.fg(paleta.diagnostico_error) } else { estilo_base };
    let texto_ayuda = estado.error().unwrap_or("n o n:col · Enter salta · Esc cancela");

    let contenido = vec![
        Line::styled(estado.texto().to_string(), estilo_base),
        Line::styled(texto_ayuda.to_string(), estilo_ayuda),
    ];
    let widget = Paragraph::new(contenido).style(estilo_base).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(crate::BORDE_ASCII)
            .title(format!(" Ir a línea (1-{}) ", num_lineas.max(1)))
            .style(estilo_base),
    );
    frame.render_widget(widget, area);

    let columna = area.x + 1 + estado.texto().chars().count() as u16;
    frame.set_cursor_position((columna.min(area.right().saturating_sub(2)), area.y + 1));
}
