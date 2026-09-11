use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_core::Editor;
use tcode_syntax::{Lenguaje, Resaltador, Token};

use crate::{EstadoUi, Paleta};

/// Dibuja el contenido del archivo (coloreado por tree-sitter si la
/// extensión corresponde a uno de los lenguajes de M1, PLAN.md §11),
/// resalta la línea del cursor y posiciona el cursor real de la terminal
/// sobre él.
#[allow(clippy::too_many_arguments)]
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    estado: &mut EstadoUi,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
    ruta: &str,
) {
    let alto_visible = area.height as usize;
    let ancho_visible = area.width as usize;
    let cursor = editor.cursor();
    ajustar_scroll(estado, cursor.linea, alto_visible);

    let lineas = editor.buffer().lineas_texto();
    let tokens = calcular_tokens(editor, resaltador, ruta);

    let visibles: Vec<Line> = lineas
        .iter()
        .enumerate()
        .skip(estado.scroll_vertical)
        .take(alto_visible)
        .map(|(idx, linea)| {
            let inicio_byte = editor.buffer().inicio_byte_linea(idx);
            let mut spans = spans_de_linea(linea, inicio_byte, &tokens, paleta);
            if idx == cursor.linea {
                // Se añade un span final de relleno para que el resaltado
                // de la línea actual cubra todo el ancho, no solo el texto.
                let ocupado = linea.len();
                if ancho_visible > ocupado {
                    spans.push(Span::raw(" ".repeat(ancho_visible - ocupado)));
                }
                for span in &mut spans {
                    span.style = span.style.bg(paleta.linea_actual);
                }
            }
            Line::from(spans)
        })
        .collect();

    frame.render_widget(
        Paragraph::new(visibles).style(Style::default().bg(paleta.fondo).fg(paleta.texto)),
        area,
    );

    let columna = area.x + cursor.columna as u16;
    let fila = area.y + (cursor.linea - estado.scroll_vertical) as u16;
    frame.set_cursor_position((columna, fila));
}

/// Resalta el archivo completo si su extensión corresponde a uno de los 5
/// lenguajes de M1; si no, o si el parseo falla, se sigue mostrando el
/// texto sin colorear (nunca rompe el render).
fn calcular_tokens(editor: &Editor, resaltador: &mut Resaltador, ruta: &str) -> Vec<Token> {
    let Some(lenguaje) = Lenguaje::detectar_por_extension(ruta) else {
        return Vec::new();
    };
    let fuente = editor.buffer().a_texto();
    resaltador.resaltar(lenguaje, &fuente).unwrap_or_default()
}

/// Construye los spans coloreados de una línea a partir de los tokens del
/// archivo completo (en offsets de byte), recortados al rango de esta
/// línea. Los tokens llegan sin solapes y en orden, así que basta un barrido
/// lineal.
fn spans_de_linea<'a>(texto_linea: &'a str, inicio_byte_linea: usize, tokens: &[Token], paleta: &Paleta) -> Vec<Span<'a>> {
    if tokens.is_empty() {
        return vec![Span::raw(texto_linea)];
    }

    let fin_byte_linea = inicio_byte_linea + texto_linea.len();
    let mut spans = Vec::new();
    let mut cursor = inicio_byte_linea;

    for token in tokens {
        if token.fin <= inicio_byte_linea || token.inicio >= fin_byte_linea {
            continue;
        }
        let inicio = token.inicio.max(inicio_byte_linea);
        let fin = token.fin.min(fin_byte_linea);

        if inicio > cursor {
            spans.push(Span::raw(&texto_linea[(cursor - inicio_byte_linea)..(inicio - inicio_byte_linea)]));
        }
        spans.push(Span::styled(
            &texto_linea[(inicio - inicio_byte_linea)..(fin - inicio_byte_linea)],
            paleta.estilo_sintaxis(token.nombre),
        ));
        cursor = fin;
    }

    if cursor < fin_byte_linea {
        spans.push(Span::raw(&texto_linea[(cursor - inicio_byte_linea)..]));
    }

    spans
}

/// Desplazamiento vertical automático: mantiene la línea del cursor siempre
/// visible dentro del área disponible. Es un concern puro de renderizado,
/// no vive en el `core`.
fn ajustar_scroll(estado: &mut EstadoUi, linea_cursor: usize, alto_visible: usize) {
    if alto_visible == 0 {
        return;
    }
    if linea_cursor < estado.scroll_vertical {
        estado.scroll_vertical = linea_cursor;
    } else if linea_cursor >= estado.scroll_vertical + alto_visible {
        estado.scroll_vertical = linea_cursor - alto_visible + 1;
    }
}
