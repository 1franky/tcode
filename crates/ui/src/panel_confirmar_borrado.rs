use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use tcode_fs::EstadoConfirmarBorrado;

use crate::Paleta;

/// Más angosto que `panel_guardar_como`/`panel_prompt_explorador` — no
/// hay ningún campo que escribir, solo el mensaje y la ayuda.
const ANCHO: u16 = 46;
const ALTO: u16 = 4;

/// Dibuja la confirmación de borrado (`Delete` con el explorador
/// enfocado, BACKLOG.md P0 "explorador de solo lectura") — acción
/// destructiva e irreversible, nunca se ejecuta directo desde la tecla.
/// Mismo recuadro centrado que el resto de los prompts del explorador,
/// pero sin campo editable: `y` confirma, cualquier otra tecla (incluido
/// `Enter`, a propósito — no hay "opción por defecto" para borrar algo)
/// cancela.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoConfirmarBorrado, paleta: &Paleta) {
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
    let estilo_mensaje = estilo_base.fg(paleta.diagnostico_error);
    let tipo = if estado.es_carpeta() { "la carpeta" } else { "el archivo" };

    let contenido = vec![
        Line::styled(format!("¿Borrar {tipo} '{}'?", estado.nombre()), estilo_mensaje),
        Line::styled("y confirma · cualquier otra tecla cancela", estilo_base),
    ];
    let widget = Paragraph::new(contenido).style(estilo_base).block(
        Block::default().borders(Borders::ALL).border_set(crate::BORDE_ASCII).title(" Confirmar borrado ").style(estilo_base),
    );
    frame.render_widget(widget, area);
}
