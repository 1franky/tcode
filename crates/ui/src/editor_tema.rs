use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use tcode_config::{analizar_color_hex, campos_color, EstadoEditorTema};

use crate::Paleta;

/// Dibuja el editor visual de tema (`Ctrl+K Ctrl+P`, PLAN.md §7) a
/// pantalla completa — igual que el panel de administración, no es un
/// overlay flotante: mientras está activo es lo único que se dibuja.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoEditorTema, paleta: &Paleta) {
    frame.render_widget(Clear, area_total);
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    frame.render_widget(Paragraph::new("").style(estilo_base), area_total);

    let filas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area_total);

    dibujar_lista(frame, filas[0], estado, paleta, estilo_base);
    dibujar_mensaje(frame, filas[1], estado, paleta);
    dibujar_pie(frame, filas[2], estado, paleta);
}

/// Una fila por campo de color de `tcode_config::campos_color()`: un
/// "chip" (██) con el color actual, la etiqueta, y el valor hex — o, si
/// es la fila seleccionada y se está editando, el texto que se va
/// escribiendo en vez del valor guardado.
fn dibujar_lista(frame: &mut Frame, area: Rect, estado: &EstadoEditorTema, paleta: &Paleta, estilo_base: Style) {
    let campos = campos_color();
    let items: Vec<ListItem> = campos
        .iter()
        .enumerate()
        .map(|(idx, campo)| {
            let seleccionado = idx == estado.campo();
            let estilo_fila = if seleccionado { estilo_base.bg(paleta.linea_actual) } else { estilo_base };

            let hex_guardado = campo.valor_actual(estado.tema());
            let color_chip = hex_guardado
                .as_deref()
                .and_then(|h| analizar_color_hex(h).ok())
                .map(|(r, g, b)| Color::Rgb(r, g, b))
                .unwrap_or(estilo_fila.bg.unwrap_or(paleta.fondo));

            let (texto_valor, estilo_valor) = if seleccionado && estado.editando() {
                (format!("#{}▏", estado.buffer_hex()), estilo_fila.fg(paleta.busqueda_actual))
            } else {
                (hex_guardado.unwrap_or_else(|| "(sin definir)".to_string()), estilo_fila)
            };

            let spans = vec![
                Span::styled("██ ", estilo_fila.fg(color_chip)),
                Span::styled(format!("{:<32}", campo.etiqueta), estilo_fila),
                Span::styled(texto_valor, estilo_valor),
            ];
            ListItem::new(Line::from(spans)).style(estilo_fila)
        })
        .collect();

    let mut estado_lista = ListState::default();
    estado_lista.select(Some(estado.campo()));

    let lista = List::new(items).block(
        Block::default().borders(Borders::ALL).title(" Editor visual de tema — colores por código hex ").style(estilo_base),
    );
    frame.render_stateful_widget(lista, area, &mut estado_lista);
}

fn dibujar_mensaje(frame: &mut Frame, area: Rect, estado: &EstadoEditorTema, paleta: &Paleta) {
    let estilo = Style::default().bg(paleta.fondo).fg(paleta.diagnostico_info);
    let texto = estado.mensaje().unwrap_or("");
    frame.render_widget(Paragraph::new(format!(" {texto}")).style(estilo), area);
}

fn dibujar_pie(frame: &mut Frame, area: Rect, estado: &EstadoEditorTema, paleta: &Paleta) {
    let texto = if estado.editando() {
        "Escribí el color en hex (sin #) · Enter aplica y guarda · Esc cancela"
    } else {
        "↑↓ moverse · Enter editar color (hex) · Esc cerrar"
    };
    let estilo = Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto);
    frame.render_widget(Paragraph::new(texto).style(estilo), area);
}
