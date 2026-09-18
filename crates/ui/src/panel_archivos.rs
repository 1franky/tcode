use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use tcode_fs::Explorador;

use crate::Paleta;

/// Panel lateral del explorador de archivos (`Ctrl+B`, PLAN.md §4/§11 M1):
/// árbol navegable con indentación por profundidad y la fila seleccionada
/// resaltada. Los íconos de carpeta son ASCII a propósito (`v`/`>`, no
/// `▾`/`▸`): esos caracteres geométricos tienen ancho "ambiguo" en
/// Unicode (1 o 2 columnas según la fuente/configuración regional de
/// cada terminal) — un sospechoso concreto de un bug de desalineación
/// permanente reportado en Windows Terminal, que coincide en que
/// aparecía específicamente en este panel y no se autocorregía sola
/// (ver la memoria del proyecto). ASCII puro nunca tiene ese problema.
///
/// En modo "salto rápido" (`Ctrl+K J`, `Explorador::modo_salto`) cada
/// fila suma una columna de 2 caracteres a la izquierda de todo (antes de
/// la indentación) con su etiqueta de una tecla — tipearla abre ese
/// archivo o expande esa carpeta directamente, sin navegar con las
/// flechas (útil con muchos archivos visibles a la vez, en vez de
/// contarlos uno por uno). No hay scroll-follow todavía en este panel
/// (una fila seleccionada más allá del alto visible simplemente no se
/// ve): las etiquetas comparten esa misma limitación — solo tienen
/// sentido, y solo se deberían usar, sobre filas realmente visibles en
/// pantalla.
pub fn dibujar(frame: &mut Frame, area: Rect, explorador: &Explorador, paleta: &Paleta) {
    let seleccion = explorador.seleccion();
    let modo_salto = explorador.modo_salto();

    let items: Vec<ListItem> = explorador
        .lista_visible()
        .into_iter()
        .enumerate()
        .map(|(idx, (profundidad, nodo))| {
            let indentacion = "  ".repeat(profundidad);
            let icono = if nodo.es_carpeta {
                if nodo.expandida {
                    "v "
                } else {
                    "> "
                }
            } else {
                "  "
            };
            let estilo = if idx == seleccion {
                Style::default().bg(paleta.linea_actual).fg(paleta.texto)
            } else {
                Style::default().fg(paleta.texto)
            };

            // "Salto rápido" (`Ctrl+K J`): una columna fija de 2 caracteres
            // a la izquierda de todo (antes de la indentación, como un
            // gutter de números de línea) con la etiqueta de esa fila —
            // en blanco si a esa fila no le tocó ninguna (se acabó el
            // alfabeto, PLAN.md, ver `Explorador::etiqueta_para_fila`) o
            // si el modo no está activo.
            if modo_salto {
                let (gutter, estilo_gutter) = match Explorador::etiqueta_para_fila(idx) {
                    // Fondo llamativo (mismo que resalta la coincidencia
                    // de búsqueda activa) para que la etiqueta se distinga
                    // de un vistazo del resto del árbol.
                    Some(etiqueta) => {
                        (format!("{etiqueta} "), Style::default().bg(paleta.busqueda_actual).fg(paleta.fondo).add_modifier(Modifier::BOLD))
                    }
                    // Sin etiqueta (se acabó el alfabeto): 2 espacios en
                    // blanco con el estilo normal de la fila, no el
                    // resaltado — nada que mirar acá.
                    None => ("  ".to_string(), estilo),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(gutter, estilo_gutter),
                    Span::styled(format!("{indentacion}{icono}{}", nodo.nombre), estilo),
                ]))
            } else {
                ListItem::new(Line::from(format!("{indentacion}{icono}{}", nodo.nombre))).style(estilo)
            }
        })
        .collect();

    let bloque = Block::default()
        .borders(Borders::RIGHT)
        .border_set(crate::BORDE_ASCII)
        .title(explorador.nombre_raiz().to_string())
        .style(Style::default().bg(paleta.fondo).fg(paleta.texto));

    frame.render_widget(List::new(items).block(bloque), area);
}
