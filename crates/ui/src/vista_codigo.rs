use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_core::{Coincidencia, Editor};
use tcode_lsp::{DiagnosticoSimple, Severidad};
use tcode_syntax::{Lenguaje, Resaltador, Token};

use crate::{EstadoUi, Paleta};

/// Dibuja el contenido del archivo (coloreado por tree-sitter si la
/// extensión corresponde a uno de los lenguajes de M1, PLAN.md §11),
/// resalta la(s) línea(s) con cursor, subraya las líneas con diagnósticos
/// LSP (M2, PLAN.md §2: "diagnósticos inline"), resalta el fondo de las
/// coincidencias de búsqueda activas (`Ctrl+F`/`Ctrl+H`, PLAN.md §4) y
/// las selecciones de multi-cursor (`Ctrl+D`/`Ctrl+Shift+L`, PLAN.md §11
/// M3) — cada cursor adicional (más allá del principal) se marca con un
/// carácter en video invertido, porque solo puede haber un cursor REAL de
/// la terminal a la vez. `mostrar_cursor` posiciona ese cursor real en el
/// principal — solo debe ser `true` para el panel activo cuando hay
/// varios (`Ctrl+\`, PLAN.md §4).
#[allow(clippy::too_many_arguments)]
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    estado: &mut EstadoUi,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
    ruta: &str,
    mostrar_cursor: bool,
    diagnosticos: &[DiagnosticoSimple],
    coincidencias_busqueda: &[Coincidencia],
    indice_coincidencia_actual: Option<usize>,
) {
    let alto_visible = area.height as usize;
    let ancho_visible = area.width as usize;
    let cursor = editor.cursor();
    let cursores = editor.cursores();
    ajustar_scroll(estado, cursor.linea, alto_visible);

    let lineas = editor.buffer().lineas_texto();
    let tokens = calcular_tokens(editor, resaltador, ruta);
    let lineas_con_cursor: Vec<usize> = cursores.iter().map(|c| c.cursor.linea).collect();

    let visibles: Vec<Line> = lineas
        .iter()
        .enumerate()
        .skip(estado.scroll_vertical)
        .take(alto_visible)
        .map(|(idx, linea)| {
            let inicio_byte = editor.buffer().inicio_byte_linea(idx);
            let fin_byte = inicio_byte + linea.len();
            let mut spans = spans_de_linea(
                linea,
                inicio_byte,
                &tokens,
                paleta,
                coincidencias_busqueda,
                indice_coincidencia_actual,
            );

            for c in cursores {
                if !c.tiene_seleccion() {
                    continue;
                }
                let a = editor.buffer().offset_byte(c.ancla.linea, c.ancla.columna);
                let b = editor.buffer().offset_byte(c.cursor.linea, c.cursor.columna);
                let (inicio_sel, fin_sel) = (a.min(b), a.max(b));
                if fin_sel <= inicio_byte || inicio_sel >= fin_byte {
                    continue;
                }
                let inicio_local = inicio_sel.max(inicio_byte) - inicio_byte;
                let fin_local = fin_sel.min(fin_byte) - inicio_byte;
                spans = transformar_rango(spans, inicio_local, fin_local, |estilo| {
                    if estilo.bg.is_none() {
                        estilo.bg(paleta.seleccion)
                    } else {
                        estilo
                    }
                });
            }

            // Marcador de los cursores adicionales (todo menos el
            // principal, índice 0, que usa el cursor real de la terminal
            // — ver doc de esta función).
            for c in cursores.iter().skip(1) {
                if c.cursor.linea != idx {
                    continue;
                }
                let offset_local = editor.buffer().offset_byte(c.cursor.linea, c.cursor.columna) - inicio_byte;
                if offset_local >= linea.len() {
                    // Al final de la línea no hay carácter que invertir:
                    // se agrega un espacio de relleno marcado en su lugar.
                    spans.push(Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)));
                    continue;
                }
                let ancho_char = linea[offset_local..].chars().next().map(char::len_utf8).unwrap_or(1);
                spans = transformar_rango(spans, offset_local, offset_local + ancho_char, |estilo| {
                    estilo.add_modifier(Modifier::REVERSED)
                });
            }

            if lineas_con_cursor.contains(&idx) {
                // Se añade un span final de relleno para que el resaltado
                // de la línea actual cubra todo el ancho, no solo el texto.
                let ocupado = linea.len();
                if ancho_visible > ocupado {
                    spans.push(Span::raw(" ".repeat(ancho_visible - ocupado)));
                }
                for span in &mut spans {
                    // No pisa el fondo si el span ya tiene uno propio (una
                    // coincidencia de búsqueda o una selección): esa señal
                    // debe seguir siendo visible aunque el cursor esté en
                    // esa línea.
                    if span.style.bg.is_none() {
                        span.style = span.style.bg(paleta.linea_actual);
                    }
                }
            }
            if let Some(severidad) = severidad_mas_grave_en_linea(diagnosticos, idx) {
                let color = color_severidad(paleta, severidad);
                for span in &mut spans {
                    // Subraya sin tocar el color del texto (preserva el
                    // resaltado de sintaxis): `underline_color` separa el
                    // color del subrayado del color del texto.
                    span.style = span.style.add_modifier(Modifier::UNDERLINED).underline_color(color);
                }
            }
            Line::from(spans)
        })
        .collect();

    frame.render_widget(
        Paragraph::new(visibles).style(Style::default().bg(paleta.fondo).fg(paleta.texto)),
        area,
    );

    if mostrar_cursor {
        let columna = area.x + cursor.columna as u16;
        let fila = area.y + (cursor.linea - estado.scroll_vertical) as u16;
        frame.set_cursor_position((columna, fila));
    }
}

/// El diagnóstico más grave (error > advertencia > información >
/// sugerencia) que cubre la línea `idx_linea`, si hay alguno.
fn severidad_mas_grave_en_linea(diagnosticos: &[DiagnosticoSimple], idx_linea: usize) -> Option<Severidad> {
    diagnosticos
        .iter()
        .filter(|d| (d.linea_inicio as usize) <= idx_linea && idx_linea <= (d.linea_fin as usize))
        .map(|d| d.severidad)
        .min_by_key(|severidad| match severidad {
            Severidad::Error => 0,
            Severidad::Advertencia => 1,
            Severidad::Informacion => 2,
            Severidad::Sugerencia => 3,
        })
}

fn color_severidad(paleta: &Paleta, severidad: Severidad) -> ratatui::style::Color {
    match severidad {
        Severidad::Error => paleta.diagnostico_error,
        Severidad::Advertencia => paleta.diagnostico_advertencia,
        Severidad::Informacion => paleta.diagnostico_info,
        Severidad::Sugerencia => paleta.diagnostico_sugerencia,
    }
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
/// archivo completo (en offsets de byte) y de las coincidencias de
/// búsqueda activas (mismo tipo de offset, ver `tcode_core::busqueda`),
/// recortados ambos al rango de esta línea. Como un rango de búsqueda
/// puede caer a mitad de un token de sintaxis (o viceversa), se calculan
/// los puntos de corte combinados de ambas fuentes y se resuelve el
/// estilo (color de texto + fondo de coincidencia) por cada segmento
/// resultante.
fn spans_de_linea<'a>(
    texto_linea: &'a str,
    inicio_byte_linea: usize,
    tokens: &[Token],
    paleta: &Paleta,
    coincidencias: &[Coincidencia],
    indice_coincidencia_actual: Option<usize>,
) -> Vec<Span<'a>> {
    let fin_byte_linea = inicio_byte_linea + texto_linea.len();

    let tokens_en_linea: Vec<&Token> =
        tokens.iter().filter(|t| t.fin > inicio_byte_linea && t.inicio < fin_byte_linea).collect();

    // Rangos de coincidencia recortados a offsets LOCALES (relativos al
    // inicio de esta línea), con si cada una es la coincidencia actual.
    let coincidencias_en_linea: Vec<(usize, usize, bool)> = coincidencias
        .iter()
        .enumerate()
        .filter(|(_, c)| c.fin > inicio_byte_linea && c.inicio < fin_byte_linea)
        .map(|(i, c)| {
            let inicio = c.inicio.max(inicio_byte_linea) - inicio_byte_linea;
            let fin = c.fin.min(fin_byte_linea) - inicio_byte_linea;
            (inicio, fin, Some(i) == indice_coincidencia_actual)
        })
        .collect();

    if tokens_en_linea.is_empty() && coincidencias_en_linea.is_empty() {
        return vec![Span::raw(texto_linea)];
    }

    let mut puntos: Vec<usize> = vec![0, texto_linea.len()];
    for t in &tokens_en_linea {
        puntos.push(t.inicio.max(inicio_byte_linea) - inicio_byte_linea);
        puntos.push(t.fin.min(fin_byte_linea) - inicio_byte_linea);
    }
    for (inicio, fin, _) in &coincidencias_en_linea {
        puntos.push(*inicio);
        puntos.push(*fin);
    }
    puntos.sort_unstable();
    puntos.dedup();

    let mut spans = Vec::new();
    for ventana in puntos.windows(2) {
        let (inicio, fin) = (ventana[0], ventana[1]);
        if inicio >= fin {
            continue;
        }

        let offset_abs = inicio_byte_linea + inicio;
        let mut estilo = tokens_en_linea
            .iter()
            .find(|t| t.inicio <= offset_abs && offset_abs < t.fin)
            .map(|t| paleta.estilo_sintaxis(t.nombre))
            .unwrap_or_default();

        if let Some((_, _, actual)) =
            coincidencias_en_linea.iter().find(|(ci, cf, _)| inicio >= *ci && fin <= *cf)
        {
            let color = if *actual { paleta.busqueda_actual } else { paleta.busqueda_otras };
            estilo = estilo.bg(color);
        }

        spans.push(Span::styled(&texto_linea[inicio..fin], estilo));
    }

    spans
}

/// Post-procesa `spans` (ya construidos por `spans_de_linea`) aplicando
/// `transformar` al estilo del tramo `[inicio_local, fin_local)`
/// (offsets de bytes relativos al inicio de la línea) — partiendo los
/// spans que caen a mitad del rango. Usado para pintar selecciones y
/// marcar cursores secundarios (multi-cursor, PLAN.md §11 M3) sin
/// duplicar la lógica de partir spans en cada caso.
fn transformar_rango<'a>(
    spans: Vec<Span<'a>>,
    inicio_local: usize,
    fin_local: usize,
    transformar: impl Fn(Style) -> Style,
) -> Vec<Span<'a>> {
    if inicio_local >= fin_local {
        return spans;
    }

    let mut resultado = Vec::with_capacity(spans.len() + 2);
    let mut cursor = 0usize;
    for span in spans {
        let len = span.content.len();
        let (inicio_span, fin_span) = (cursor, cursor + len);
        cursor = fin_span;

        if len == 0 || fin_span <= inicio_local || inicio_span >= fin_local {
            resultado.push(span);
            continue;
        }

        let corte_izq = inicio_local.saturating_sub(inicio_span).min(len);
        let corte_der = fin_local.saturating_sub(inicio_span).min(len);
        let estilo_original = span.style;
        let contenido = span.content.into_owned();

        if corte_izq > 0 {
            resultado.push(Span::styled(contenido[..corte_izq].to_string(), estilo_original));
        }
        resultado.push(Span::styled(contenido[corte_izq..corte_der].to_string(), transformar(estilo_original)));
        if corte_der < len {
            resultado.push(Span::styled(contenido[corte_der..].to_string(), estilo_original));
        }
    }
    resultado
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

#[cfg(test)]
mod tests {
    use super::*;

    fn texto(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn transformar_rango_parte_un_span_a_la_mitad() {
        let spans = vec![Span::raw("hola mundo")];
        let resultado = transformar_rango(spans, 5, 10, |e| e.bg(ratatui::style::Color::Red));
        assert_eq!(texto(&resultado), "hola mundo");
        // "hola " (sin tocar), "mundo" (con el fondo nuevo).
        assert_eq!(resultado.len(), 2);
        assert_eq!(resultado[0].content.as_ref(), "hola ");
        assert!(resultado[0].style.bg.is_none());
        assert_eq!(resultado[1].content.as_ref(), "mundo");
        assert_eq!(resultado[1].style.bg, Some(ratatui::style::Color::Red));
    }

    #[test]
    fn transformar_rango_no_pisa_un_fondo_que_ya_estaba_si_la_transformacion_lo_respeta() {
        let spans = vec![Span::styled("hola", Style::default().bg(ratatui::style::Color::Blue))];
        let resultado =
            transformar_rango(spans, 0, 4, |e| if e.bg.is_none() { e.bg(ratatui::style::Color::Red) } else { e });
        assert_eq!(resultado[0].style.bg, Some(ratatui::style::Color::Blue));
    }

    #[test]
    fn transformar_rango_atraviesa_varios_spans() {
        let spans = vec![Span::raw("ab"), Span::raw("cd"), Span::raw("ef")];
        // Rango [1,5): mitad de "ab", todo "cd", mitad de "ef" no — de
        // hecho [1,5) cubre 'b','c','d','e' (offsets 1..5 sobre "abcdef").
        let resultado = transformar_rango(spans, 1, 5, |e| e.add_modifier(Modifier::BOLD));
        assert_eq!(texto(&resultado), "abcdef");
        let en_negrita: String = resultado
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::BOLD))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(en_negrita, "bcde");
    }

    #[test]
    fn transformar_rango_vacio_no_cambia_nada() {
        let spans = vec![Span::raw("hola")];
        let resultado = transformar_rango(spans.clone(), 2, 2, |e| e.bg(ratatui::style::Color::Red));
        assert_eq!(texto(&resultado), texto(&spans));
        assert!(resultado[0].style.bg.is_none());
    }
}
