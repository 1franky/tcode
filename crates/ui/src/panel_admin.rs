use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use tcode_config::{CampoEditor, Config, EstadoPanelAdmin, FocoPanelAdmin, Seccion};

use crate::Paleta;

/// Ancho fijo de la barra lateral de secciones (PLAN.md §5).
const ANCHO_BARRA: u16 = 30;

/// Dibuja el panel de administración (`Ctrl+,`) a pantalla completa: es
/// una vista más del sistema, no un overlay flotante sobre el editor
/// (PLAN.md §5) — quien llama (`tcode_ui::dibujar`) no dibuja nada más
/// del editor mientras este panel está activo.
pub fn dibujar(frame: &mut Frame, area_total: Rect, panel: &EstadoPanelAdmin, config: &Config, paleta: &Paleta) {
    frame.render_widget(Clear, area_total);
    frame.render_widget(Paragraph::new("").style(Style::default().bg(paleta.fondo)), area_total);

    let columnas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(ANCHO_BARRA), Constraint::Min(1)])
        .split(area_total);

    let filas_derecha = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(columnas[1]);

    dibujar_barra(frame, columnas[0], panel, paleta);
    match panel.foco() {
        FocoPanelAdmin::Busqueda => dibujar_busqueda(frame, filas_derecha[0], panel, paleta),
        _ => dibujar_central(frame, filas_derecha[0], panel, config, paleta),
    }
    dibujar_pie(frame, filas_derecha[1], panel, paleta);
}

/// Barra lateral con las 5 secciones de PLAN.md §5 (todas visibles desde
/// ya, aunque algunas todavía solo muestren un aviso "próximamente" en el
/// área central — ver `Seccion::implementada`).
fn dibujar_barra(frame: &mut Frame, area: Rect, panel: &EstadoPanelAdmin, paleta: &Paleta) {
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let items: Vec<ListItem> = Seccion::TODAS
        .iter()
        .enumerate()
        .map(|(idx, seccion)| {
            let seleccionada = idx == panel.indice_seccion();
            let marca = if seleccionada && panel.foco() == FocoPanelAdmin::Barra { "▸ " } else { "  " };
            let sufijo = if seccion.implementada() { "" } else { " (próximamente)" };
            let estilo = if seleccionada { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            ListItem::new(Line::from(Span::styled(format!("{marca}{}{sufijo}", seccion.nombre()), estilo)))
                .style(estilo)
        })
        .collect();
    let lista = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Administración ").style(estilo_base));
    frame.render_widget(lista, area);
}

/// Área central: filas editables de la sección actual si ya tiene
/// contenido real (solo "Editor" por ahora), o el aviso de qué va a
/// traer si todavía no lo tiene.
fn dibujar_central(frame: &mut Frame, area: Rect, panel: &EstadoPanelAdmin, config: &Config, paleta: &Paleta) {
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let seccion = panel.seccion_actual();
    let bloque = Block::default().borders(Borders::ALL).title(format!(" {} ", seccion.nombre())).style(estilo_base);

    if !seccion.implementada() {
        let parrafo =
            Paragraph::new(seccion.resumen_pendiente()).wrap(Wrap { trim: true }).block(bloque).style(estilo_base);
        frame.render_widget(parrafo, area);
        return;
    }

    let items: Vec<ListItem> = CampoEditor::TODOS
        .iter()
        .enumerate()
        .map(|(idx, campo)| {
            let seleccionado = idx == panel.campo() && panel.foco() == FocoPanelAdmin::Central;
            let estilo = if seleccionado { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            let mut texto = format!("{:<34}{}", campo.nombre(), campo.valor_actual(config));
            if let Some(nota) = campo.nota() {
                texto.push_str(&format!("   ({nota})"));
            }
            ListItem::new(Line::from(Span::styled(texto, estilo))).style(estilo)
        })
        .collect();
    frame.render_widget(List::new(items).block(bloque), area);
}

/// Búsqueda global de opciones (`Ctrl+F` dentro del panel, PLAN.md §5):
/// campo de consulta arriba, resultados abajo con las letras coincidentes
/// en negrita — mismo lenguaje visual que la paleta de comandos y el
/// buscador de archivos (`crate::overlay`), aunque dibujado directo en
/// vez de reusar ese módulo porque aquí no hay que centrar un recuadro
/// flotante: ya vive dentro del área central de este panel.
fn dibujar_busqueda(frame: &mut Frame, area: Rect, panel: &EstadoPanelAdmin, paleta: &Paleta) {
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let campo = Paragraph::new(format!("> {}", panel.busqueda()))
        .block(Block::default().borders(Borders::ALL).title(" Buscar opción "))
        .style(estilo_base);
    frame.render_widget(campo, partes[0]);

    let resultados = panel.resultados_busqueda();
    let items: Vec<ListItem> = resultados
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            let estilo_fila = if idx == panel.campo() { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            let mut spans: Vec<Span> = r
                .nombre
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    let estilo =
                        if r.posiciones.contains(&i) { estilo_fila.add_modifier(Modifier::BOLD) } else { estilo_fila };
                    Span::styled(c.to_string(), estilo)
                })
                .collect();
            spans.push(Span::styled(format!("  — {}", Seccion::TODAS[r.seccion].nombre()), estilo_fila));
            ListItem::new(Line::from(spans)).style(estilo_fila)
        })
        .collect();
    frame.render_widget(List::new(items).block(Block::default().borders(Borders::ALL).style(estilo_base)), partes[1]);
}

/// Barra inferior con el contexto de teclas disponible (PLAN.md §5),
/// distinto según dónde está el foco ahora mismo.
fn dibujar_pie(frame: &mut Frame, area: Rect, panel: &EstadoPanelAdmin, paleta: &Paleta) {
    let texto = match panel.foco() {
        FocoPanelAdmin::Barra => "↑↓ moverse · Enter/→ entrar a la sección · Ctrl+F buscar · Esc cerrar panel",
        FocoPanelAdmin::Central => {
            "↑↓ moverse · Enter/←→ cambiar valor · Tab volver a secciones · Ctrl+F buscar · Esc volver"
        }
        FocoPanelAdmin::Busqueda => "↑↓ moverse · Enter ir a la opción · Esc cancelar búsqueda",
    };
    let estilo = Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto);
    frame.render_widget(Paragraph::new(texto).style(estilo), area);
}
