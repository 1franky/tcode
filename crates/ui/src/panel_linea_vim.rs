use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use tcode_core::EstadoLineaComando;

use crate::Paleta;

/// Dibuja la línea de comandos `:` del modo VIM en la última fila de la
/// pantalla (encima de la barra de estado de abajo, como en VIM), sin
/// recuadro: `:` + lo escrito, con el cursor real de la terminal al
/// final. No dibuja nada si no está abierta. Va aparte de
/// `tcode_ui::dibujar` (la llama `app` después) para no sumarle otro
/// parámetro más.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoLineaComando, paleta: &Paleta) {
    if !estado.activa() || area_total.height == 0 || area_total.width < 2 {
        return;
    }
    let area = Rect { x: area_total.x, y: area_total.bottom() - 1, width: area_total.width, height: 1 };
    frame.render_widget(Clear, area);
    let texto = format!(":{}", estado.texto());
    // Si no entra, se muestra el final (lo que se está escribiendo).
    let visibles = area.width as usize - 1;
    let largo = texto.chars().count();
    let mostrado: String = texto.chars().skip(largo.saturating_sub(visibles)).collect();
    let columna = mostrado.chars().count() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(mostrado)).style(Style::default().bg(paleta.fondo).fg(paleta.texto)),
        area,
    );
    frame.set_cursor_position((area.x + columna, area.y));
}
