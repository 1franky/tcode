use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use tcode_core::EstadoGuardarComo;

use crate::Paleta;

/// Ancho fijo (en columnas) del prompt, recortado si la pantalla es más
/// angosta — mismo criterio que `panel_busqueda::ANCHO`.
const ANCHO: u16 = 50;
/// 2 líneas de contenido (ruta + ayuda/error) más borde arriba y abajo.
const ALTO: u16 = 4;

/// Dibuja el prompt "Guardar como" (`Ctrl+Shift+S`/`Ctrl+K S`, o `Ctrl+S`
/// sobre un buffer sin ruta) centrado sobre lo que hubiera detrás — un
/// recuadro angosto de una sola línea de campo, sin la lista de
/// resultados que sí tienen la paleta de comandos y el buscador de
/// archivos (`overlay::dibujar`): acá no hay nada que listar, solo
/// escribir una ruta.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoGuardarComo, paleta: &Paleta) {
    if !estado.activa() {
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
    let texto_ayuda = estado.error().unwrap_or("Enter guarda · Esc cancela");

    let contenido = vec![
        Line::styled(estado.ruta().to_string(), estilo_base),
        Line::styled(texto_ayuda.to_string(), estilo_ayuda),
    ];
    let widget = Paragraph::new(contenido)
        .style(estilo_base)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(crate::BORDE_ASCII)
                .title(" Guardar como ")
                .style(estilo_base),
        );
    frame.render_widget(widget, area);

    // Cursor real de la terminal al final de la ruta escrita.
    let columna = area.x + 1 + estado.ruta().chars().count() as u16;
    frame.set_cursor_position((columna, area.y + 1));
}
