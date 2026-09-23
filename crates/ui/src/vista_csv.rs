use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Text;
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use tcode_core::{EstadoCsv, TablaCsv};

use crate::{EstadoUi, Paleta};

/// Ancho de columna (en columnas de terminal) recortado al contenido más
/// largo de esa columna, entre estos dos límites — sin esto una celda
/// enorme haría inútil el resto de la tabla, y una vacía se vería como
/// una franja invisible.
const ANCHO_MIN_COLUMNA: usize = 4;
const ANCHO_MAX_COLUMNA: usize = 30;

/// Separación entre columnas que usa `ratatui::widgets::Table` por
/// defecto (`column_spacing`), nunca cambiada acá — hace falta para
/// calcular tanto la ventana de columnas visibles como la posición en
/// pantalla de la celda seleccionada.
const ESPACIADO_COLUMNAS: usize = 1;

/// Dibuja la tabla CSV/TSV (PLAN.md §9, `Ctrl+K T`): fila de encabezado
/// congelada (`tabla.filas[0]`, siempre visible arriba, nunca se
/// desplaza verticalmente), la celda seleccionada resaltada y, si se está
/// editando una, su texto en construcción en vez del valor guardado.
/// `mostrar_cursor` sigue la misma convención que `vista_codigo`/
/// `panel_busqueda`: solo debe ser `true` para el panel activo, para no
/// pelear por el único cursor real de la terminal cuando hay varios
/// paneles (`Ctrl+\`).
///
/// El desplazamiento vertical que mantiene la fila seleccionada visible
/// lo calcula `ratatui` (vía `TableState`); el horizontal (columnas) no
/// tiene equivalente nativo en `ratatui::widgets::Table` — sin archivos
/// con muchas columnas, la suma de anchos supera el ancho de la terminal
/// y el widget encoge todas las columnas proporcionalmente hasta dejarlas
/// ilegibles (1-2 caracteres cada una). En vez de eso, acá se elige un
/// subconjunto contiguo de columnas que sí entra en `area.width` y que
/// incluye la columna seleccionada (`estado.columna()`), igual de
/// espíritu que `vista_codigo::ajustar_scroll` pero para columnas de
/// ancho variable en vez de filas de altura uniforme — de ahí que el
/// desplazamiento (`estado_ui.scroll`) reutilice el mismo campo de
/// `EstadoUi` donde vive el scroll vertical del código (un `PanelEditor`
/// nunca está en los dos modos a la vez, así que no se pisan).
///
/// Con un filtro activo (BACKLOG.md P2 #9) solo se dibujan las filas
/// visibles (`EstadoCsv::filas_visibles`; `estado.fila()` es un índice
/// en esa lista) y la última línea del área muestra una barra con el
/// filtro vigente — o, mientras se escribe uno (`Ctrl+K /`), el prompt.
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    tabla: &TablaCsv,
    estado: &EstadoCsv,
    estado_ui: &mut EstadoUi,
    paleta: &Paleta,
    mostrar_cursor: bool,
) {
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);

    if tabla.filas.is_empty() {
        frame.render_widget(Paragraph::new("(archivo CSV/TSV vacío)").style(estilo_base), area);
        return;
    }

    let visibles = estado.filas_visibles(tabla);
    let area = dibujar_barra_filtro(frame, area, tabla, estado, visibles.len(), paleta, mostrar_cursor);

    let num_columnas = tabla.num_columnas();
    let anchos: Vec<usize> = (0..num_columnas).map(|col| ancho_columna(tabla, estado, col)).collect();

    ajustar_scroll_horizontal(&mut estado_ui.scroll, &anchos, estado.columna(), area.width as usize);
    let primera_col = estado_ui.scroll;
    let ultima_col = columna_final_visible(&anchos, primera_col, area.width as usize);
    let anchos_visibles = &anchos[primera_col..ultima_col];
    let constraints: Vec<Constraint> = anchos_visibles.iter().map(|a| Constraint::Length(*a as u16)).collect();

    let fila_a_row = |indice_fila: usize, celdas: &[String], es_encabezado: bool| -> Row<'static> {
        let celdas_estilizadas: Vec<Cell> = (primera_col..ultima_col)
            .map(|col| {
                let es_seleccionada = indice_fila == estado.fila() && col == estado.columna();
                let texto = if es_seleccionada && estado.editando() {
                    estado.edicion().unwrap_or_default().to_string()
                } else {
                    celdas.get(col).cloned().unwrap_or_default()
                };

                let mut estilo = estilo_base;
                if es_encabezado {
                    estilo = estilo.add_modifier(Modifier::BOLD);
                }
                if es_seleccionada {
                    estilo = estilo.bg(paleta.linea_actual);
                    if estado.editando() {
                        estilo = estilo.add_modifier(Modifier::UNDERLINED);
                    }
                }
                Cell::from(Text::from(texto)).style(estilo)
            })
            .collect();
        Row::new(celdas_estilizadas)
    };

    // `visibles[0]` es siempre el encabezado (0), filtre lo que filtre.
    // `i` es la posición visible — lo que se compara con `estado.fila()`.
    let encabezado = fila_a_row(0, &tabla.filas[0].celdas, true);
    let filas_cuerpo: Vec<Row> = visibles
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, &real)| fila_a_row(i, &tabla.filas[real].celdas, false))
        .collect();

    let mut estado_tabla = TableState::default();
    // Fila 0 = encabezado, fuera de `filas_cuerpo`: la selección relativa
    // al cuerpo (lo único que `TableState` necesita) es un índice menos.
    if estado.fila() > 0 {
        estado_tabla.select(Some(estado.fila() - 1));
    }

    let tabla_widget = Table::new(filas_cuerpo, constraints).header(encabezado).style(estilo_base);
    frame.render_stateful_widget(tabla_widget, area, &mut estado_tabla);

    if mostrar_cursor && estado.editando() {
        if let Some(rect) = celda_en_pantalla(area, &anchos, primera_col, estado, &estado_tabla) {
            let ancho_texto = estado.edicion().unwrap_or_default().chars().count() as u16;
            let columna = rect.x + ancho_texto.min(rect.width.saturating_sub(1));
            frame.set_cursor_position((columna, rect.y));
        }
    }
}

/// Barra de una línea al pie de la tabla (BACKLOG.md P2 #9): el prompt
/// de filtro mientras se escribe (`Ctrl+K /`, con el cursor real de la
/// terminal ahí, igual que los demás prompts de una línea), o el
/// indicador de filtro activo con cuántas filas quedan y cómo quitarlo —
/// sin esto, una tabla filtrada sería indistinguible de un archivo con
/// menos filas. Devuelve el área que le queda a la tabla (toda, si no
/// hay nada que mostrar).
fn dibujar_barra_filtro(
    frame: &mut Frame,
    area: Rect,
    tabla: &TablaCsv,
    estado: &EstadoCsv,
    num_visibles: usize,
    paleta: &Paleta,
    mostrar_cursor: bool,
) -> Rect {
    let (texto, cursor) = if let Some(prompt) = estado.prompt_filtro() {
        let prefijo = format!(
            " Filtrar «{}» por (Enter aplica · vacío quita · Esc cancela): ",
            nombre_columna(tabla, estado.columna())
        );
        let columna_cursor = (prefijo.chars().count() + prompt.chars().count()) as u16;
        (format!("{prefijo}{prompt}"), Some(columna_cursor))
    } else if let Some(filtro) = estado.filtro() {
        let texto = format!(
            " Filtro: «{}» contiene «{}» — {} de {} filas · Esc lo quita",
            nombre_columna(tabla, filtro.columna),
            filtro.texto,
            num_visibles.saturating_sub(1),
            tabla.num_filas().saturating_sub(1)
        );
        (texto, None)
    } else {
        return area;
    };
    if area.height < 2 {
        return area;
    }

    let barra = Rect { x: area.x, y: area.y + area.height - 1, width: area.width, height: 1 };
    let estilo = Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto);
    frame.render_widget(Paragraph::new(texto).style(estilo), barra);
    if let (Some(columna), true) = (cursor, mostrar_cursor) {
        frame.set_cursor_position((barra.x + columna.min(barra.width.saturating_sub(1)), barra.y));
    }
    Rect { height: area.height - 1, ..area }
}

/// Nombre de una columna para mostrar en la barra de filtro: su celda de
/// encabezado, o "columna N" (desde 1) si está vacía o no existe.
fn nombre_columna(tabla: &TablaCsv, columna: usize) -> String {
    match tabla.filas.first().and_then(|f| f.celdas.get(columna)) {
        Some(nombre) if !nombre.is_empty() => nombre.clone(),
        _ => format!("columna {}", columna + 1),
    }
}

/// Ajusta `scroll` (índice de la primera columna visible) para que la
/// columna seleccionada quede dentro de la ventana visible, con el mismo
/// criterio de `vista_codigo::ajustar_scroll`: si quedó a la izquierda de
/// lo que se ve, saltar directo a ella; si quedó a la derecha de lo que
/// entra en `ancho_disponible`, avanzar de a una columna hasta que
/// vuelva a entrar (a diferencia de filas de altura uniforme, acá hay que
/// ir probando porque cada columna tiene un ancho distinto).
fn ajustar_scroll_horizontal(scroll: &mut usize, anchos: &[usize], columna_seleccionada: usize, ancho_disponible: usize) {
    if anchos.is_empty() {
        *scroll = 0;
        return;
    }
    *scroll = (*scroll).min(columna_seleccionada);
    while *scroll < columna_seleccionada {
        let ancho_ventana: usize = anchos[*scroll..=columna_seleccionada].iter().map(|a| a + ESPACIADO_COLUMNAS).sum();
        if ancho_ventana <= ancho_disponible {
            break;
        }
        *scroll += 1;
    }
}

/// Índice (exclusivo) de la última columna que entra en `ancho_disponible`
/// arrancando desde `primera_col` — siempre incluye al menos una columna,
/// aunque sea más ancha que `ancho_disponible`, para no dejar la ventana
/// vacía si una sola celda ya lo supera.
fn columna_final_visible(anchos: &[usize], primera_col: usize, ancho_disponible: usize) -> usize {
    let mut acumulado = 0;
    let mut fin = primera_col;
    for ancho in &anchos[primera_col..] {
        let siguiente = acumulado + ancho + ESPACIADO_COLUMNAS;
        if fin > primera_col && siguiente > ancho_disponible {
            break;
        }
        acumulado = siguiente;
        fin += 1;
    }
    fin
}

/// Ancho efectivo de `columna` en la vista: el fijado a mano
/// (`Ctrl+K Shift+→`/`Ctrl+K Shift+←`, BACKLOG.md P2 #9) si lo hay, o si
/// no el automático según contenido. Público porque `app` lo necesita
/// para ensanchar/angostar a partir del ancho que se está viendo (la
/// primera vez, el automático) en vez de desde un valor arbitrario.
pub fn ancho_columna(tabla: &TablaCsv, estado: &EstadoCsv, columna: usize) -> usize {
    estado.ancho_manual(columna).unwrap_or_else(|| ancho_automatico(tabla, columna))
}

/// Ancho automático de una columna en columnas de terminal: el contenido
/// más largo entre todas las filas (encabezado incluido, y también las
/// ocultas por un filtro — así filtrar no hace "bailar" los anchos),
/// recortado a `[ANCHO_MIN_COLUMNA, ANCHO_MAX_COLUMNA]`.
fn ancho_automatico(tabla: &TablaCsv, columna: usize) -> usize {
    let max_contenido =
        tabla.filas.iter().map(|f| f.celdas.get(columna).map(|c| c.chars().count()).unwrap_or(0)).max().unwrap_or(0);
    max_contenido.clamp(ANCHO_MIN_COLUMNA, ANCHO_MAX_COLUMNA)
}

/// Rectángulo de pantalla que ocupa la celda seleccionada, o `None` si
/// quedó fuera del área visible tras el scroll (p. ej. se estaba editando
/// y la ventana de la terminal se hizo más chica). `+1` de separación
/// entre columnas porque ese es el `column_spacing` por defecto de
/// `ratatui::widgets::Table`, que aquí nunca se cambia. `primera_col` es
/// la columna en la esquina izquierda de la ventana visible tras el
/// scroll horizontal (`ajustar_scroll_horizontal`) — la seleccionada
/// siempre queda dentro de esa ventana, así que solo hace falta descontar
/// el ancho de las columnas antes de ella que quedaron fuera de pantalla.
fn celda_en_pantalla(area: Rect, anchos: &[usize], primera_col: usize, estado: &EstadoCsv, estado_tabla: &TableState) -> Option<Rect> {
    let fila_pantalla = if estado.fila() == 0 {
        area.y
    } else {
        let fila_relativa = (estado.fila() - 1).checked_sub(estado_tabla.offset())?;
        area.y + 1 + fila_relativa as u16
    };
    if fila_pantalla >= area.y + area.height {
        return None;
    }

    let mut x = area.x;
    for ancho in anchos[primera_col..].iter().take(estado.columna() - primera_col) {
        x += *ancho as u16 + 1;
    }
    let ancho_celda = anchos.get(estado.columna()).copied().unwrap_or(0) as u16;
    Some(Rect { x, y: fila_pantalla, width: ancho_celda, height: 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcode_core::FilaCsv;

    fn tabla_de_prueba() -> TablaCsv {
        TablaCsv {
            delimitador: b',',
            filas: vec![
                FilaCsv { celdas: vec!["nombre".into(), "ciudad".into()], inicio_byte: 0, fin_byte: 0 },
                FilaCsv { celdas: vec!["Ana".into(), "Ciudad de México".into()], inicio_byte: 0, fin_byte: 0 },
                FilaCsv { celdas: vec!["Bo".into(), "Lima".into()], inicio_byte: 0, fin_byte: 0 },
            ],
        }
    }

    #[test]
    fn ancho_de_columna_es_el_contenido_mas_largo_recortado_al_maximo() {
        let tabla = tabla_de_prueba();
        let anchos: Vec<usize> = (0..2).map(|col| ancho_automatico(&tabla, col)).collect();
        // "nombre"/"Ana"/"Bo" -> 6 (el más largo, "nombre").
        assert_eq!(anchos[0], 6);
        // "Ciudad de México" (16 caracteres) queda por debajo del máximo.
        assert_eq!(anchos[1], 16);
    }

    #[test]
    fn ancho_de_columna_nunca_baja_del_minimo() {
        let tabla = TablaCsv {
            delimitador: b',',
            filas: vec![FilaCsv { celdas: vec!["a".into()], inicio_byte: 0, fin_byte: 0 }],
        };
        assert_eq!(ancho_automatico(&tabla, 0), ANCHO_MIN_COLUMNA);
    }

    #[test]
    fn ancho_de_columna_se_recorta_al_maximo() {
        let celda_enorme = "x".repeat(200);
        let tabla =
            TablaCsv { delimitador: b',', filas: vec![FilaCsv { celdas: vec![celda_enorme], inicio_byte: 0, fin_byte: 0 }] };
        assert_eq!(ancho_automatico(&tabla, 0), ANCHO_MAX_COLUMNA);
    }

    #[test]
    fn celda_en_pantalla_del_encabezado_siempre_esta_en_la_primera_fila() {
        let area = Rect { x: 10, y: 5, width: 50, height: 20 };
        let anchos = vec![6, 16];
        let estado = EstadoCsv::nuevo(); // fila=0, columna=0 por defecto
        let estado_tabla = TableState::default();
        let rect = celda_en_pantalla(area, &anchos, 0, &estado, &estado_tabla).unwrap();
        assert_eq!((rect.x, rect.y, rect.width), (10, 5, 6));
    }

    #[test]
    fn celda_en_pantalla_de_una_fila_de_datos_se_desplaza_por_el_encabezado_y_la_columna() {
        let area = Rect { x: 0, y: 0, width: 50, height: 20 };
        let anchos = vec![6, 16];
        let mut estado = EstadoCsv::nuevo();
        estado.mover_abajo(3); // fila 1
        estado.mover_derecha(2); // columna 1
        let estado_tabla = TableState::default(); // offset 0
        let rect = celda_en_pantalla(area, &anchos, 0, &estado, &estado_tabla).unwrap();
        // y = 1 (fila del encabezado) + 0 (fila de datos relativa) = 1;
        // x = 6 (ancho de la primera columna) + 1 (separador) = 7.
        assert_eq!((rect.x, rect.y, rect.width), (7, 1, 16));
    }

    #[test]
    fn celda_en_pantalla_devuelve_none_si_quedo_fuera_del_area_visible() {
        let area = Rect { x: 0, y: 0, width: 50, height: 1 }; // solo cabe el encabezado
        let anchos = vec![6, 16];
        let mut estado = EstadoCsv::nuevo();
        estado.mover_abajo(3); // fila 1: ya no entra en un área de 1 fila de alto
        let estado_tabla = TableState::default();
        assert!(celda_en_pantalla(area, &anchos, 0, &estado, &estado_tabla).is_none());
    }

    #[test]
    fn celda_en_pantalla_descuenta_las_columnas_scrolleadas_fuera_de_pantalla() {
        let area = Rect { x: 0, y: 0, width: 50, height: 20 };
        let anchos = vec![6, 16, 10];
        let mut estado = EstadoCsv::nuevo();
        estado.mover_derecha(3);
        estado.mover_derecha(3); // columna 2 (la tercera) — mover_derecha avanza de a una
        let estado_tabla = TableState::default();
        // Ventana visible arranca en la columna 1 (la 0 quedó scrolleada
        // fuera): x arranca en 0, no en 6+1.
        let rect = celda_en_pantalla(area, &anchos, 1, &estado, &estado_tabla).unwrap();
        // x = 16 (ancho de la columna 1, la primera visible) + 1 (separador).
        assert_eq!((rect.x, rect.width), (17, 10));
    }

    #[test]
    fn ajustar_scroll_horizontal_no_se_mueve_si_la_seleccion_ya_entra() {
        let anchos = vec![6, 16];
        let mut scroll = 0;
        ajustar_scroll_horizontal(&mut scroll, &anchos, 1, 50);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn ajustar_scroll_horizontal_avanza_hasta_que_la_seleccion_entra() {
        // 5 columnas de ancho 10 (+1 de separador = 11 cada una); con un
        // ancho disponible de 25 entran como mucho 2 columnas completas.
        let anchos = vec![10, 10, 10, 10, 10];
        let mut scroll = 0;
        ajustar_scroll_horizontal(&mut scroll, &anchos, 4, 25);
        // La columna 4 sola más la de al lado (3) ya llenan la ventana;
        // no puede entrar ninguna más a la izquierda.
        assert_eq!(scroll, 3);
    }

    #[test]
    fn ajustar_scroll_horizontal_retrocede_si_la_seleccion_quedo_a_la_izquierda() {
        let anchos = vec![10, 10, 10, 10, 10];
        let mut scroll = 3;
        ajustar_scroll_horizontal(&mut scroll, &anchos, 0, 25);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn ajustar_scroll_horizontal_no_se_traba_si_una_sola_columna_ya_no_entra() {
        let anchos = vec![50];
        let mut scroll = 0;
        ajustar_scroll_horizontal(&mut scroll, &anchos, 0, 10);
        assert_eq!(scroll, 0); // no hay a dónde más avanzar
    }

    #[test]
    fn columna_final_visible_incluye_las_que_entran_completas() {
        let anchos = vec![10, 10, 10, 10, 10];
        // 10+1 + 10+1 = 22 <= 25; con la tercera serían 33 > 25.
        assert_eq!(columna_final_visible(&anchos, 0, 25), 2);
    }

    #[test]
    fn columna_final_visible_incluye_al_menos_una_aunque_no_entre() {
        let anchos = vec![50, 10];
        assert_eq!(columna_final_visible(&anchos, 0, 10), 1);
    }
}
