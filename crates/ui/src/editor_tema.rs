use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use tcode_config::{
    analizar_color_hex, campos_color, formatear_color_hex, hsl_a_rgb, ComponenteHsl, EstadoEditorTema, ModoEdicion,
    PALETA_PREDEFINIDA,
};

use crate::Paleta;

/// Dibuja el editor visual de tema (`Ctrl+K Ctrl+P`, PLAN.md §7) a
/// pantalla completa — igual que el panel de administración, no es un
/// overlay flotante: mientras está activo es lo único que se dibuja. Los
/// tres métodos de entrada de color reemplazan el área central entera
/// (lista de campos / paleta predefinida / ajuste HSL), nunca se
/// combinan.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoEditorTema, paleta: &Paleta) {
    frame.render_widget(Clear, area_total);
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    frame.render_widget(Paragraph::new("").style(estilo_base), area_total);

    let filas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area_total);

    match estado.modo() {
        ModoEdicion::Paleta(_) => dibujar_paleta(frame, filas[0], estado, paleta, estilo_base),
        ModoEdicion::Hsl { .. } => dibujar_hsl(frame, filas[0], estado, paleta, estilo_base),
        ModoEdicion::Ninguno | ModoEdicion::Hex(_) => dibujar_lista(frame, filas[0], estado, paleta, estilo_base),
    }
    dibujar_mensaje(frame, filas[1], estado, paleta);
    dibujar_pie(frame, filas[2], estado, paleta);
}

/// Una fila por campo de color de `tcode_config::campos_color()`: un
/// "chip" (██) con el color actual, la etiqueta, y el valor hex — o, si
/// es la fila seleccionada y se está editando por hex, el texto que se
/// va escribiendo en vez del valor guardado.
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

            let (texto_valor, estilo_valor) = match (seleccionado, estado.modo()) {
                (true, ModoEdicion::Hex(buffer)) => (format!("#{buffer}▏"), estilo_fila.fg(paleta.busqueda_actual)),
                _ => (hex_guardado.unwrap_or_else(|| "(sin definir)".to_string()), estilo_fila),
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

/// Método "paleta predefinida" (PLAN.md §7): reemplaza la lista de
/// campos por la lista de `PALETA_PREDEFINIDA`, cada una con su propio
/// chip de color — igual estilo visual que `dibujar_lista`, para que se
/// sienta como el mismo picker con otra fuente de colores.
fn dibujar_paleta(frame: &mut Frame, area: Rect, estado: &EstadoEditorTema, paleta: &Paleta, estilo_base: Style) {
    let ModoEdicion::Paleta(seleccion) = estado.modo() else { return };
    let seleccion = *seleccion;

    let items: Vec<ListItem> = PALETA_PREDEFINIDA
        .iter()
        .enumerate()
        .map(|(idx, (nombre, hex))| {
            let seleccionado = idx == seleccion;
            let estilo_fila = if seleccionado { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            let (r, g, b) = analizar_color_hex(hex).unwrap_or((0, 0, 0));
            let spans = vec![
                Span::styled("██ ", estilo_fila.fg(Color::Rgb(r, g, b))),
                Span::styled(format!("{nombre:<16}"), estilo_fila),
                Span::styled(*hex, estilo_fila),
            ];
            ListItem::new(Line::from(spans)).style(estilo_fila)
        })
        .collect();

    let mut estado_lista = ListState::default();
    estado_lista.select(Some(seleccion));

    let lista = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Elegir de la paleta predefinida ").style(estilo_base));
    frame.render_stateful_widget(lista, area, &mut estado_lista);
}

/// Método "ajustar HSL con flechas" (PLAN.md §7): un swatch grande con
/// el color resultante arriba, y las tres filas de matiz/saturación/
/// luminosidad debajo — la enfocada (`←`/`→` cambia cuál) resaltada, con
/// el mismo color que usa la búsqueda para su coincidencia actual.
fn dibujar_hsl(frame: &mut Frame, area: Rect, estado: &EstadoEditorTema, paleta: &Paleta, estilo_base: Style) {
    let ModoEdicion::Hsl { h, s, l, foco, .. } = estado.modo() else { return };
    let (h, s, l, foco) = (*h, *s, *l, *foco);
    let (r, g, b) = hsl_a_rgb(h, s, l);
    let hex = formatear_color_hex(r, g, b);

    let filas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(1), Constraint::Length(1), Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    // El texto del swatch necesita contraste contra el color de fondo
    // que está mostrando, no contra el fondo del editor — blanco o
    // negro según qué tan clara sea la luminosidad resultante.
    let texto_swatch = if l > 55 { Color::Black } else { Color::White };
    let swatch = Paragraph::new(format!(" Color resultante: {hex} "))
        .style(Style::default().bg(Color::Rgb(r, g, b)).fg(texto_swatch))
        .block(Block::default().borders(Borders::ALL).title(" Ajustar HSL con flechas ").style(estilo_base));
    frame.render_widget(swatch, filas[0]);

    let fila_componente = |nombre: &str, valor: String, es_foco: bool| {
        let estilo = if es_foco { estilo_base.bg(paleta.linea_actual).fg(paleta.busqueda_actual) } else { estilo_base };
        Paragraph::new(format!(" {nombre:<13}{valor}")).style(estilo)
    };
    frame.render_widget(fila_componente("Matiz", format!("{h}°"), foco == ComponenteHsl::Matiz), filas[1]);
    frame.render_widget(fila_componente("Saturación", format!("{s}%"), foco == ComponenteHsl::Saturacion), filas[2]);
    frame.render_widget(fila_componente("Luminosidad", format!("{l}%"), foco == ComponenteHsl::Luminosidad), filas[3]);
    frame.render_widget(Paragraph::new("").style(estilo_base), filas[4]);
}

fn dibujar_mensaje(frame: &mut Frame, area: Rect, estado: &EstadoEditorTema, paleta: &Paleta) {
    let estilo = Style::default().bg(paleta.fondo).fg(paleta.diagnostico_info);
    let texto = estado.mensaje().unwrap_or("");
    frame.render_widget(Paragraph::new(format!(" {texto}")).style(estilo), area);
}

fn dibujar_pie(frame: &mut Frame, area: Rect, estado: &EstadoEditorTema, paleta: &Paleta) {
    let texto = match estado.modo() {
        ModoEdicion::Hex(_) => "Escribí el color en hex (sin #) · Enter aplica y guarda · Esc cancela",
        ModoEdicion::Paleta(_) => "↑↓ elegir color · Enter aplica y guarda · Esc cancela",
        ModoEdicion::Hsl { .. } => "←→ elegir matiz/saturación/luminosidad · ↑↓ ajustar · Enter guarda · Esc cancela",
        ModoEdicion::Ninguno => "↑↓ moverse · Enter hex · P paleta predefinida · H ajustar HSL · Esc cerrar",
    };
    let estilo = Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto);
    frame.render_widget(Paragraph::new(texto).style(estilo), area);
}
