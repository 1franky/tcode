use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::Paleta;

/// Dibuja un overlay de "escribir para buscar" genérico — la paleta de
/// comandos (`Ctrl+Shift+P`/`F1`), el buscador de archivos (`Ctrl+P`,
/// PLAN.md §4), el selector de temas y el visor de logs de LSP comparten
/// exactamente esta forma: recuadro centrado con un campo de consulta
/// arriba y una lista de resultados abajo, letras coincidentes en
/// negrita y la fila seleccionada resaltada con el color de línea actual
/// del tema.
///
/// `seleccion` maneja el scroll-follow vía `ListState` (BACKLOG.md P1,
/// "scroll-follow real en overlays"): antes se armaba un `List` sin
/// estado, así que siempre se dibujaba desde la fila 0 recortada a lo
/// que entraba en pantalla — con más resultados que alto disponible,
/// bajar la selección la dejaba resaltando una fila invisible. Un
/// `ListState` fresco por frame (mismo criterio que `TableState` en
/// `vista_csv::dibujar`, que tampoco persiste el suyo entre frames)
/// alcanza: `ratatui` recalcula el offset necesario para que la
/// seleccionada quede visible a partir de `selected`, sin que haga falta
/// guardar nada de un frame al siguiente. Si `seleccion` no cae dentro de
/// `filas` (el visor de logs pasa `usize::MAX` a propósito, porque no
/// hay ninguna acción que confirmar sobre una línea de log) no se marca
/// ninguna fila como seleccionada — mismo comportamiento que antes para
/// ese caso, sin forzar scroll a un índice que no existe.
pub fn dibujar(
    frame: &mut Frame,
    area_total: Rect,
    titulo: &str,
    consulta: &str,
    filas: &[(String, Vec<usize>)],
    seleccion: usize,
    paleta: &Paleta,
) {
    let area = area_centrada(area_total, 60, 60);
    frame.render_widget(Clear, area);

    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);

    let campo = Paragraph::new(Line::from(format!("> {consulta}")))
        .block(Block::default().borders(Borders::ALL).border_set(crate::BORDE_ASCII).title(format!(" {titulo} ")))
        .style(estilo_base);
    frame.render_widget(campo, partes[0]);

    let items: Vec<ListItem> = filas
        .iter()
        .enumerate()
        .map(|(idx, (texto, posiciones))| {
            let estilo_fila = if idx == seleccion { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            let spans: Vec<Span> = texto
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    let estilo =
                        if posiciones.contains(&i) { estilo_fila.add_modifier(Modifier::BOLD) } else { estilo_fila };
                    Span::styled(c.to_string(), estilo)
                })
                .collect();
            ListItem::new(Line::from(spans)).style(estilo_fila)
        })
        .collect();

    let lista = List::new(items).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_set(crate::BORDE_ASCII)
            .style(estilo_base),
    );

    let mut estado_lista = ListState::default();
    if seleccion < filas.len() {
        estado_lista.select(Some(seleccion));
    }
    frame.render_stateful_widget(lista, partes[1], &mut estado_lista);
}

/// Recorta `area` a un rectángulo centrado que ocupa `porcentaje_ancho`% x
/// `porcentaje_alto`% del total. `pub(crate)` porque `panel_guardar_como`
/// también la usa para su prompt centrado, más chico que este overlay de
/// "escribir para buscar" pero con la misma idea de recuadro en el medio.
pub(crate) fn area_centrada(area: Rect, porcentaje_ancho: u16, porcentaje_alto: u16) -> Rect {
    let margen_vertical = (100 - porcentaje_alto) / 2;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(margen_vertical),
            Constraint::Percentage(porcentaje_alto),
            Constraint::Percentage(margen_vertical),
        ])
        .split(area);

    let margen_horizontal = (100 - porcentaje_ancho) / 2;
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(margen_horizontal),
            Constraint::Percentage(porcentaje_ancho),
            Constraint::Percentage(margen_horizontal),
        ])
        .split(vertical[1])[1]
}
