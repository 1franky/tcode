use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use tcode_fs::EstadoPromptExplorador;

use crate::Paleta;

/// Mismo tamaño que `panel_guardar_como` — es exactamente el mismo tipo
/// de recuadro (campo de una línea + ayuda/error), reutilizado acá para
/// "Nuevo archivo"/"Nueva carpeta"/"Renombrar" del explorador en vez del
/// buffer activo.
const ANCHO: u16 = 50;
const ALTO: u16 = 4;

/// Dibuja el prompt de texto del explorador (`Ctrl+K N`/`Ctrl+K C`/
/// `Ctrl+K M`, BACKLOG.md P0 "explorador de solo lectura") — mismo
/// recuadro centrado que "Guardar como", con el título según
/// `ModoPromptExplorador::titulo`.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoPromptExplorador, paleta: &Paleta) {
    let Some(modo) = estado.modo() else { return };

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
    let texto_ayuda = estado.error().unwrap_or("Enter confirma · Esc cancela");

    let contenido =
        vec![Line::styled(estado.texto().to_string(), estilo_base), Line::styled(texto_ayuda.to_string(), estilo_ayuda)];
    let widget = Paragraph::new(contenido).style(estilo_base).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(crate::BORDE_ASCII)
            .title(format!(" {} ", modo.titulo()))
            .style(estilo_base),
    );
    frame.render_widget(widget, area);

    let columna = area.x + 1 + estado.texto().chars().count() as u16;
    frame.set_cursor_position((columna, area.y + 1));
}
