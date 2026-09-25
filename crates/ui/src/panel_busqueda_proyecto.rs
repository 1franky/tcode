use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use tcode_fs::{CampoProyecto, EstadoBusquedaProyecto};

use crate::Paleta;

/// Una fila de la lista de resultados: el encabezado de un archivo, o
/// una coincidencia (con su índice global, el de
/// `EstadoBusquedaProyecto::seleccion`).
enum Fila {
    Archivo(usize),
    Coincidencia(usize, usize, usize),
}

/// Dibuja la vista "Buscar en el proyecto" (`Ctrl+Shift+F`/`Ctrl+K B`,
/// BACKLOG.md P1 #16): casi a pantalla completa — la lista de resultados
/// necesita alto y ancho, a diferencia de los overlays de "escribir para
/// buscar" (`overlay::dibujar`). Arriba los tres campos (consulta,
/// reemplazo, filtro de rutas) y una línea de estado (contador, opciones,
/// error o confirmación de "reemplazar todo"); abajo los resultados
/// agrupados por archivo, con el número de línea y la coincidencia
/// resaltada.
///
/// Solo se arman las filas visibles: con miles de coincidencias, armar
/// un `ListItem` por cada una en cada frame sería trabajo tirado. La
/// ventana se calcula sin estado (la seleccionada queda en la última fila
/// visible si no entra desde arriba), igual de estable que el
/// `ListState` fresco por frame de `overlay::dibujar`.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoBusquedaProyecto, paleta: &Paleta) {
    let margen_x = (area_total.width / 20).max(1);
    let margen_y = (area_total.height / 20).max(1);
    if area_total.width <= 2 * margen_x + 20 || area_total.height <= 2 * margen_y + 8 {
        return;
    }
    let area = Rect {
        x: area_total.x + margen_x,
        y: area_total.y + margen_y,
        width: area_total.width - 2 * margen_x,
        height: area_total.height - 2 * margen_y,
    };
    frame.render_widget(Clear, area);

    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(if estado.confirmando() { 7 } else { 6 }), Constraint::Min(1)])
        .split(area);

    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let estilo_activo = estilo_base.add_modifier(Modifier::BOLD);
    let estilo_tenue = estilo_base.fg(paleta.numero_linea);

    // Campos: mismo ancho de etiqueta para que los tres textos queden
    // alineados.
    let campos = [
        (CampoProyecto::Consulta, "Buscar:     ", estado.consulta()),
        (CampoProyecto::Reemplazo, "Reemplazar: ", estado.reemplazo()),
        (CampoProyecto::Filtro, "Archivos:   ", estado.filtro()),
    ];
    let mut lineas: Vec<Line> = campos
        .iter()
        .map(|(campo, etiqueta, texto)| {
            let estilo = if estado.campo() == *campo { estilo_activo } else { estilo_base };
            let mut spans = vec![Span::styled(*etiqueta, estilo), Span::styled(texto.to_string(), estilo)];
            if texto.is_empty() && *campo == CampoProyecto::Filtro {
                spans.push(Span::styled("(globs separados por coma; !glob excluye)", estilo_tenue));
            }
            Line::from(spans)
        })
        .collect();
    lineas.extend(lineas_estado(estado, paleta, estilo_base));

    let cabecera = Paragraph::new(lineas).style(estilo_base).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(crate::BORDE_ASCII)
            .title(" Buscar en el proyecto ")
            .style(estilo_base),
    );
    frame.render_widget(cabecera, partes[0]);

    let bloque_lista = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_set(crate::BORDE_ASCII)
        .title_bottom(" Enter abrir | Tab campo | Alt+R/C/W | Alt+Enter reemplazar todo | Esc ")
        .style(estilo_base);
    let interior = bloque_lista.inner(partes[1]);
    frame.render_widget(bloque_lista, partes[1]);
    frame.render_widget(Paragraph::new(filas_visibles(estado, interior.height as usize, paleta, estilo_base)), interior);

    // Cursor real de la terminal al final del campo activo.
    let (fila, texto) = match estado.campo() {
        CampoProyecto::Consulta => (0, estado.consulta()),
        CampoProyecto::Reemplazo => (1, estado.reemplazo()),
        CampoProyecto::Filtro => (2, estado.filtro()),
    };
    let columna = (partes[0].x + 1 + 12 + texto.chars().count() as u16).min(partes[0].right().saturating_sub(2));
    frame.set_cursor_position((columna, partes[0].y + 1 + fila));
}

/// Línea(s) de estado de la cabecera, por prioridad: confirmación de
/// "reemplazar todo" pendiente (dos líneas: la pregunta y qué pasa con
/// abiertos y cerrados — sin contarlos: la UI solo sabe qué estaba
/// abierto al buscar, no ahora), error del patrón o del filtro, aviso de
/// una sola vez (resultado del reemplazo), o el contador con las
/// opciones activas (mismas marcas que la barra de `Ctrl+F`).
fn lineas_estado(estado: &EstadoBusquedaProyecto, paleta: &Paleta, base: Style) -> Vec<Line<'static>> {
    let estilo_alerta = base.fg(paleta.diagnostico_advertencia).add_modifier(Modifier::BOLD);
    if estado.confirmando() {
        let pregunta = format!(
            "Reemplazar {} coincidencias en {} archivos por \"{}\"? y confirma, cualquier otra tecla cancela",
            estado.total(),
            estado.archivos().len(),
            estado.reemplazo(),
        );
        return vec![
            Line::styled(pregunta, estilo_alerta),
            Line::styled(
                "Abiertos en pestañas: cambia el buffer (sin guardar, Ctrl+Z deshace). Cerrados: se escriben a disco.",
                base.fg(paleta.diagnostico_advertencia),
            ),
        ];
    }
    if let Some(error) = estado.error() {
        return vec![Line::styled(error.to_string(), base.fg(paleta.diagnostico_error))];
    }
    if let Some(aviso) = estado.aviso() {
        return vec![Line::styled(aviso.to_string(), estilo_alerta)];
    }

    let mut texto = if estado.consulta().is_empty() {
        "Escribí para buscar".to_string()
    } else {
        format!("{} coincidencias en {} archivos", estado.total(), estado.archivos().len())
    };
    if estado.buscando() {
        texto.push_str(" (buscando...)");
    } else if let Some(resumen) = estado.resumen() {
        if resumen.truncado {
            texto.push_str(&format!(" (tope de {}: afiná la búsqueda)", tcode_fs::TOPE_COINCIDENCIAS));
        } else if resumen.cancelado {
            texto.push_str(" (búsqueda interrumpida)");
        } else {
            texto.push_str(&format!(" ({} archivos revisados)", resumen.archivos_revisados));
        }
    }
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
    if !flags.is_empty() {
        texto.push_str("  ");
        texto.push_str(&flags);
    }
    vec![Line::styled(texto, base)]
}

/// Las `alto` filas de resultados que se ven, con la seleccionada adentro.
fn filas_visibles<'a>(estado: &'a EstadoBusquedaProyecto, alto: usize, paleta: &Paleta, base: Style) -> Vec<Line<'a>> {
    if alto == 0 {
        return Vec::new();
    }
    let mut filas = Vec::new();
    let mut global = 0;
    let mut fila_seleccionada = 0;
    for (i, archivo) in estado.archivos().iter().enumerate() {
        filas.push(Fila::Archivo(i));
        for j in 0..archivo.coincidencias.len() {
            if global == estado.seleccion() {
                fila_seleccionada = filas.len();
            }
            filas.push(Fila::Coincidencia(i, j, global));
            global += 1;
        }
    }
    let desde = if fila_seleccionada < alto { 0 } else { fila_seleccionada + 1 - alto };
    // Encabezado "pegajoso": si la ventana arranca a mitad de un archivo,
    // la primera fila muestra de qué archivo son (la de abajo es otra
    // coincidencia del mismo o el encabezado del siguiente, así que no
    // se pierde nada que haga falta ver) — salvo que sea la seleccionada.
    let mut visibles: Vec<&Fila> = filas.iter().skip(desde).take(alto).collect();
    let encabezado;
    if let Some(Fila::Coincidencia(i, _, _)) = visibles.first() {
        if desde != fila_seleccionada {
            encabezado = Fila::Archivo(*i);
            visibles[0] = &encabezado;
        }
    }

    visibles
        .into_iter()
        .map(|fila| match *fila {
            Fila::Archivo(i) => {
                let archivo = &estado.archivos()[i];
                let mut spans = vec![
                    Span::styled(archivo.ruta_mostrada.clone(), base.add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" ({})", archivo.coincidencias.len()), base.fg(paleta.numero_linea)),
                ];
                if archivo.desde_buffer {
                    spans.push(Span::styled(" [abierto]", base.fg(paleta.numero_linea)));
                }
                Line::from(spans)
            }
            Fila::Coincidencia(i, j, global) => {
                let c = &estado.archivos()[i].coincidencias[j];
                let estilo = if global == estado.seleccion() { base.bg(paleta.linea_actual) } else { base };
                let r = c.resaltado.clone();
                Line::from(vec![
                    Span::styled(format!("{:>6}: ", c.linea + 1), estilo.fg(paleta.numero_linea)),
                    Span::styled(&c.fragmento[..r.start], estilo),
                    Span::styled(&c.fragmento[r.clone()], estilo.bg(paleta.busqueda_actual)),
                    Span::styled(&c.fragmento[r.end..], estilo),
                ])
                .style(estilo)
            }
        })
        .collect()
}
