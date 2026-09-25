//! Popups y listas de las funciones de LSP más allá de diagnósticos
//! (BACKLOG.md P1 #17): el completado y el hover, que aparecen pegados
//! al cursor; la lista de ubicaciones (definiciones, referencias), que
//! reusa el overlay de la paleta; y el prompt de renombrar. `app` los
//! dibuja encima de todo lo demás (después de `crate::dibujar`), así este
//! módulo no suma parámetros a esa función.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use tcode_commands::EstadoListaUbicaciones;
use tcode_lsp::EstadoCompletado;

use crate::{overlay, Paleta};

/// Filas de items visibles a la vez en el popup de completado.
const FILAS_COMPLETADO: usize = 10;
/// Ancho máximo de la etiqueta y del detalle (en caracteres) en una fila
/// del completado: lo que sobra se corta con `...`.
const ANCHO_ETIQUETA: usize = 40;
const ANCHO_DETALLE: usize = 30;
/// Ancho máximo del popup de hover.
const ANCHO_HOVER: usize = 80;

/// Rectángulo de `ancho` x `alto` pegado al cursor: debajo de la fila del
/// cursor si entra, si no arriba; corrido a la izquierda si se sale por la
/// derecha. `None` si la pantalla es demasiado chica.
fn area_junto_al_cursor(area_total: Rect, cursor: (u16, u16), ancho: u16, alto: u16) -> Option<Rect> {
    let ancho = ancho.min(area_total.width);
    if ancho < 10 || area_total.height < 3 {
        return None;
    }
    let (columna, fila) = cursor;
    let debajo = area_total.bottom().saturating_sub(fila + 1);
    let arriba = fila.saturating_sub(area_total.y);
    let (y, alto) = if debajo >= alto || debajo >= arriba {
        (fila + 1, alto.min(debajo))
    } else {
        let alto = alto.min(arriba);
        (fila - alto, alto)
    };
    if alto < 3 {
        return None;
    }
    let x = columna.min(area_total.right().saturating_sub(ancho)).max(area_total.x);
    Some(Rect { x, y, width: ancho, height: alto })
}

fn recortar(texto: &str, maximo: usize) -> String {
    if texto.chars().count() <= maximo {
        texto.to_string()
    } else {
        format!("{}...", texto.chars().take(maximo.saturating_sub(3)).collect::<String>())
    }
}

/// Popup de completado bajo el cursor: una fila por item (etiqueta, con
/// las letras que coinciden con lo escrito en negrita; tipo; detalle),
/// la seleccionada con el fondo de la línea actual, y la selección
/// siempre a la vista (se desplaza de a una ventana de
/// [`FILAS_COMPLETADO`]).
pub fn dibujar_completado(
    frame: &mut Frame,
    area_total: Rect,
    cursor: (u16, u16),
    estado: &EstadoCompletado,
    paleta: &Paleta,
) {
    let visibles = estado.visibles();
    if !estado.activo() || visibles.is_empty() {
        return;
    }
    let desde = estado.seleccion().saturating_sub(FILAS_COMPLETADO - 1);
    let ventana = &visibles[desde..(desde + FILAS_COMPLETADO).min(visibles.len())];

    let filas: Vec<(String, &[usize], String)> = ventana
        .iter()
        .map(|v| {
            let item = estado.item(v.indice);
            let mut resto = item.tipo.to_string();
            if let Some(detalle) = &item.detalle {
                resto = format!("{resto} {}", recortar(detalle, ANCHO_DETALLE));
            }
            (recortar(&item.etiqueta, ANCHO_ETIQUETA), v.posiciones.as_slice(), resto)
        })
        .collect();
    let ancho_etiqueta = filas.iter().map(|(e, _, _)| e.chars().count()).max().unwrap_or(0);
    let ancho_resto = filas.iter().map(|(_, _, r)| r.chars().count()).max().unwrap_or(0);
    let ancho = (ancho_etiqueta + 2 + ancho_resto + 2) as u16;
    let alto = filas.len() as u16 + 2;
    let Some(area) = area_junto_al_cursor(area_total, cursor, ancho, alto) else { return };

    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let lineas: Vec<Line> = filas
        .iter()
        .enumerate()
        .map(|(i, (etiqueta, posiciones, resto))| {
            let estilo = if desde + i == estado.seleccion() { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            let mut spans: Vec<Span> = etiqueta
                .chars()
                .enumerate()
                .map(|(j, c)| {
                    let estilo = if posiciones.contains(&j) { estilo.add_modifier(Modifier::BOLD) } else { estilo };
                    Span::styled(c.to_string(), estilo)
                })
                .collect();
            let relleno = ancho_etiqueta - etiqueta.chars().count() + 2;
            spans.push(Span::styled(" ".repeat(relleno), estilo));
            spans.push(Span::styled(resto.clone(), estilo.fg(paleta.numero_linea)));
            Line::from(spans).style(estilo)
        })
        .collect();
    let titulo = format!(" {}/{} ", estado.seleccion() + 1, visibles.len());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lineas).style(estilo_base).block(
            Block::default().borders(Borders::ALL).border_set(crate::BORDE_ASCII).title(titulo).style(estilo_base),
        ),
        area,
    );
}

/// Popup de hover bajo el cursor: el texto (ya plano, ver
/// `tcode_lsp::texto_hover`) con cada línea cortada a [`ANCHO_HOVER`].
pub fn dibujar_hover(frame: &mut Frame, area_total: Rect, cursor: (u16, u16), texto: &str, paleta: &Paleta) {
    let lineas: Vec<String> = texto.lines().map(|l| recortar(&l.replace('\t', "    "), ANCHO_HOVER)).collect();
    let ancho = lineas.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16 + 2;
    let alto = lineas.len() as u16 + 2;
    let Some(area) = area_junto_al_cursor(area_total, cursor, ancho.max(12), alto) else { return };
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lineas.into_iter().map(Line::from).collect::<Vec<_>>()).style(estilo_base).block(
            Block::default().borders(Borders::ALL).border_set(crate::BORDE_ASCII).style(estilo_base),
        ),
        area,
    );
}

/// Lista de ubicaciones (varias definiciones, o las referencias):
/// el mismo overlay de "escribir para filtrar" que el selector de
/// símbolos.
pub fn dibujar_lista_ubicaciones(frame: &mut Frame, area_total: Rect, lista: &EstadoListaUbicaciones, paleta: &Paleta) {
    if !lista.activo() {
        return;
    }
    let filas: Vec<(String, Vec<usize>)> =
        lista.resultados().map(|(entrada, posiciones)| (entrada.etiqueta.clone(), posiciones.to_vec())).collect();
    overlay::dibujar(frame, area_total, lista.titulo(), lista.consulta(), &filas, lista.seleccion(), paleta);
}

/// Prompt de una línea con el nombre nuevo para "Renombrar símbolo",
/// centrado como el de "Guardar como".
pub fn dibujar_prompt_renombrar(frame: &mut Frame, area_total: Rect, nombre: &str, paleta: &Paleta) {
    let ancho = 50.min(area_total.width);
    let alto = 4.min(area_total.height);
    if ancho < 10 || alto < 3 {
        return;
    }
    let area = Rect {
        x: area_total.x + (area_total.width - ancho) / 2,
        y: area_total.y + (area_total.height - alto) / 2,
        width: ancho,
        height: alto,
    };
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(nombre.to_string(), estilo_base),
            Line::styled("Enter renombra, Esc cancela".to_string(), estilo_base),
        ])
        .style(estilo_base)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(crate::BORDE_ASCII)
                .title(" Renombrar símbolo ")
                .style(estilo_base),
        ),
        area,
    );
    let columna = area.x + 1 + nombre.chars().count() as u16;
    frame.set_cursor_position((columna.min(area.right().saturating_sub(2)), area.y + 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_popup_va_debajo_si_entra_y_arriba_si_no() {
        let pantalla = Rect { x: 0, y: 0, width: 80, height: 24 };
        assert_eq!(area_junto_al_cursor(pantalla, (5, 3), 20, 6), Some(Rect { x: 5, y: 4, width: 20, height: 6 }));
        let arriba = area_junto_al_cursor(pantalla, (5, 21), 20, 6).unwrap();
        assert_eq!((arriba.y, arriba.height), (15, 6));
    }

    #[test]
    fn el_popup_no_se_sale_por_la_derecha() {
        let pantalla = Rect { x: 0, y: 0, width: 80, height: 24 };
        assert_eq!(area_junto_al_cursor(pantalla, (75, 3), 20, 6).unwrap().x, 60);
    }

    #[test]
    fn recortar_agrega_puntos_suspensivos() {
        assert_eq!(recortar("abcdef", 10), "abcdef");
        assert_eq!(recortar("abcdefghijkl", 8), "abcde...");
    }
}
