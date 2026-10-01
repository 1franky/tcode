use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::Paleta;

const ANCHO: u16 = 64;

/// Pregunta al arrancar si recuperar los cambios sin guardar de una sesión
/// que se cerró de golpe (BACKLOG.md P2 #21): recuadro centrado con qué
/// archivos hay y las teclas. `archivos` ya viene con lo que se muestra
/// de cada uno; si son muchos se cortan con "y N más".
pub fn dibujar(frame: &mut Frame, area_total: Rect, archivos: &[String], paleta: &Paleta) {
    const MAX_ARCHIVOS: usize = 8;
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let estilo_titulo = estilo_base.fg(paleta.diagnostico_advertencia);
    let mut contenido = vec![
        Line::styled("tcode se cerró sin terminar y quedaron cambios sin guardar en:", estilo_titulo),
        Line::raw(""),
    ];
    // Una línea por archivo: lo que no entra se corta por la izquierda,
    // que el nombre del archivo (al final) es lo que importa.
    let disponible = (ANCHO.min(area_total.width) as usize).saturating_sub(4);
    for archivo in archivos.iter().take(MAX_ARCHIVOS) {
        let largo = archivo.chars().count();
        let visible = if largo > disponible && disponible > 3 {
            let resto: String = archivo.chars().skip(largo - (disponible - 3)).collect();
            format!("...{resto}")
        } else {
            archivo.clone()
        };
        contenido.push(Line::styled(format!("  {visible}"), estilo_base));
    }
    if archivos.len() > MAX_ARCHIVOS {
        contenido.push(Line::styled(format!("  y {} más", archivos.len() - MAX_ARCHIVOS), estilo_base));
    }
    contenido.push(Line::raw(""));
    contenido.push(Line::styled("Enter recupera (quedan sin guardar, Ctrl+Z vuelve al disco)", estilo_base));
    contenido.push(Line::styled("d descarta · Esc decide después (se vuelve a preguntar)", estilo_base));

    let ancho = ANCHO.min(area_total.width);
    let alto = (contenido.len() as u16 + 2).min(area_total.height);
    if ancho < 20 || alto < 5 {
        return;
    }
    let area = Rect {
        x: area_total.x + (area_total.width - ancho) / 2,
        y: area_total.y + (area_total.height - alto) / 2,
        width: ancho,
        height: alto,
    };
    frame.render_widget(Clear, area);
    let widget = Paragraph::new(contenido).style(estilo_base).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(crate::BORDE_ASCII)
            .title(" Recuperar cambios ")
            .style(estilo_base),
    );
    frame.render_widget(widget, area);
}
