use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use tcode_core::{CampoBusqueda, EstadoBusqueda};

use crate::Paleta;

/// Ancho fijo (en columnas) de la barra de búsqueda/reemplazo, recortado
/// si el área de edición es más angosta.
const ANCHO: u16 = 46;

/// Dibuja la barra flotante de búsqueda/reemplazo (`Ctrl+F`/`Ctrl+H`,
/// PLAN.md §4) en la esquina superior derecha del área de edición, al
/// estilo VSCode — a diferencia de la paleta de comandos o el buscador de
/// archivos (`overlay::dibujar`), no es un recuadro centrado: no debe
/// tapar el código mientras se busca en él.
pub fn dibujar(frame: &mut Frame, area_editor: Rect, estado: &EstadoBusqueda, paleta: &Paleta) {
    if !estado.activa() {
        return;
    }

    let alto = if estado.modo_reemplazar() { 5 } else { 4 };
    let ancho = ANCHO.min(area_editor.width);
    if area_editor.height < alto || ancho < 10 {
        return;
    }

    let area = Rect {
        x: area_editor.x + area_editor.width - ancho,
        y: area_editor.y,
        width: ancho,
        height: alto,
    };
    frame.render_widget(Clear, area);

    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let estilo_campo_activo = estilo_base.add_modifier(Modifier::BOLD);

    let mut lineas = vec![Line::from(Span::styled(
        format!("Buscar: {}", estado.consulta()),
        campo_estilo(estado, CampoBusqueda::Consulta, estilo_base, estilo_campo_activo),
    ))];

    if estado.modo_reemplazar() {
        lineas.push(Line::from(Span::styled(
            format!("Reemplazar: {}", estado.reemplazo()),
            campo_estilo(estado, CampoBusqueda::Reemplazo, estilo_base, estilo_campo_activo),
        )));
    }

    lineas.push(Line::from(Span::styled(linea_estado(estado), estilo_base)));

    let titulo = if estado.modo_reemplazar() { " Reemplazar (Ctrl+H) " } else { " Buscar (Ctrl+F) " };
    let widget = Paragraph::new(lineas)
        .style(estilo_base)
        .block(Block::default().borders(Borders::ALL).border_set(crate::BORDE_ASCII).title(titulo));
    frame.render_widget(widget, area);

    // Posiciona el cursor real de la terminal al final del texto del
    // campo activo (`paneles::dibujar_panel` no lo pone en el código
    // mientras esta barra está abierta).
    let (prefijo, texto) = match estado.campo_activo() {
        CampoBusqueda::Consulta => ("Buscar: ", estado.consulta()),
        CampoBusqueda::Reemplazo => ("Reemplazar: ", estado.reemplazo()),
    };
    let fila = area.y + 1 + u16::from(estado.campo_activo() == CampoBusqueda::Reemplazo);
    let columna = area.x + 1 + (prefijo.len() + texto.chars().count()) as u16;
    frame.set_cursor_position((columna, fila));
}

fn campo_estilo(estado: &EstadoBusqueda, campo: CampoBusqueda, base: Style, activo: Style) -> Style {
    if estado.campo_activo() == campo {
        activo
    } else {
        base
    }
}

/// Línea inferior: error del patrón (si `regex` está activado y es
/// inválido) o el contador de coincidencias más las opciones activas
/// (`Alt+R` regex, `Alt+C` mayúsculas, `Alt+W` palabra completa).
fn linea_estado(estado: &EstadoBusqueda) -> String {
    if let Some(error) = estado.error() {
        return error.to_string();
    }

    let contador = match (estado.coincidencias().len(), estado.indice_actual()) {
        (0, _) => "sin coincidencias".to_string(),
        (n, Some(i)) => format!("{}/{n}", i + 1),
        (n, None) => format!("0/{n}"),
    };

    let opciones = estado.opciones();
    let mut flags = String::new();
    if opciones.regex {
        flags.push_str("[.*]");
    }
    if opciones.sensible_mayusculas {
        flags.push_str("[Aa]");
    }
    if opciones.palabra_completa {
        flags.push_str("[ab]");
    }

    if flags.is_empty() {
        contador
    } else {
        format!("{contador}  {flags}")
    }
}
