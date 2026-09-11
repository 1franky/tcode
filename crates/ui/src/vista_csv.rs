use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Text;
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use tcode_core::{EstadoCsv, TablaCsv};

use crate::Paleta;

/// Ancho de columna (en columnas de terminal) recortado al contenido más
/// largo de esa columna, entre estos dos límites — sin esto una celda
/// enorme haría inútil el resto de la tabla, y una vacía se vería como
/// una franja invisible.
const ANCHO_MIN_COLUMNA: usize = 4;
const ANCHO_MAX_COLUMNA: usize = 30;

/// Dibuja la tabla CSV/TSV (PLAN.md §9, `Ctrl+K T`): fila de encabezado
/// congelada (`tabla.filas[0]`, siempre visible arriba, nunca se
/// desplaza), la celda seleccionada resaltada y, si se está editando una,
/// su texto en construcción en vez del valor guardado. `mostrar_cursor`
/// sigue la misma convención que `vista_codigo`/`panel_busqueda`: solo
/// debe ser `true` para el panel activo, para no pelear por el único
/// cursor real de la terminal cuando hay varios paneles (`Ctrl+\`).
///
/// El desplazamiento vertical que mantiene la fila seleccionada visible
/// lo calcula `ratatui` (vía `TableState`); aquí solo se le dice cuál
/// está seleccionada — no hay estado de scroll propio que llevar.
pub fn dibujar(frame: &mut Frame, area: Rect, tabla: &TablaCsv, estado: &EstadoCsv, paleta: &Paleta, mostrar_cursor: bool) {
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);

    if tabla.filas.is_empty() {
        frame.render_widget(Paragraph::new("(archivo CSV/TSV vacío)").style(estilo_base), area);
        return;
    }

    let num_columnas = tabla.num_columnas();
    let anchos = anchos_por_columna(tabla, num_columnas);
    let constraints: Vec<Constraint> = anchos.iter().map(|a| Constraint::Length(*a as u16)).collect();

    let fila_a_row = |indice_fila: usize, celdas: &[String], es_encabezado: bool| -> Row<'static> {
        let celdas_estilizadas: Vec<Cell> = (0..num_columnas)
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

    let encabezado = fila_a_row(0, &tabla.filas[0].celdas, true);
    let filas_cuerpo: Vec<Row> =
        tabla.filas[1..].iter().enumerate().map(|(i, f)| fila_a_row(i + 1, &f.celdas, false)).collect();

    let mut estado_tabla = TableState::default();
    // Fila 0 = encabezado, fuera de `filas_cuerpo`: la selección relativa
    // al cuerpo (lo único que `TableState` necesita) es un índice menos.
    if estado.fila() > 0 {
        estado_tabla.select(Some(estado.fila() - 1));
    }

    let tabla_widget = Table::new(filas_cuerpo, constraints).header(encabezado).style(estilo_base);
    frame.render_stateful_widget(tabla_widget, area, &mut estado_tabla);

    if mostrar_cursor && estado.editando() {
        if let Some(rect) = celda_en_pantalla(area, &anchos, estado, &estado_tabla) {
            let ancho_texto = estado.edicion().unwrap_or_default().chars().count() as u16;
            let columna = rect.x + ancho_texto.min(rect.width.saturating_sub(1));
            frame.set_cursor_position((columna, rect.y));
        }
    }
}

/// Ancho de cada columna en columnas de terminal: el contenido más largo
/// entre todas las filas (encabezado incluido), recortado a
/// `[ANCHO_MIN_COLUMNA, ANCHO_MAX_COLUMNA]`.
fn anchos_por_columna(tabla: &TablaCsv, num_columnas: usize) -> Vec<usize> {
    (0..num_columnas)
        .map(|col| {
            let max_contenido =
                tabla.filas.iter().map(|f| f.celdas.get(col).map(|c| c.chars().count()).unwrap_or(0)).max().unwrap_or(0);
            max_contenido.clamp(ANCHO_MIN_COLUMNA, ANCHO_MAX_COLUMNA)
        })
        .collect()
}

/// Rectángulo de pantalla que ocupa la celda seleccionada, o `None` si
/// quedó fuera del área visible tras el scroll (p. ej. se estaba editando
/// y la ventana de la terminal se hizo más chica). `+1` de separación
/// entre columnas porque ese es el `column_spacing` por defecto de
/// `ratatui::widgets::Table`, que aquí nunca se cambia.
fn celda_en_pantalla(area: Rect, anchos: &[usize], estado: &EstadoCsv, estado_tabla: &TableState) -> Option<Rect> {
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
    for ancho in anchos.iter().take(estado.columna()) {
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
        let anchos = anchos_por_columna(&tabla, 2);
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
        assert_eq!(anchos_por_columna(&tabla, 1)[0], ANCHO_MIN_COLUMNA);
    }

    #[test]
    fn ancho_de_columna_se_recorta_al_maximo() {
        let celda_enorme = "x".repeat(200);
        let tabla =
            TablaCsv { delimitador: b',', filas: vec![FilaCsv { celdas: vec![celda_enorme], inicio_byte: 0, fin_byte: 0 }] };
        assert_eq!(anchos_por_columna(&tabla, 1)[0], ANCHO_MAX_COLUMNA);
    }

    #[test]
    fn celda_en_pantalla_del_encabezado_siempre_esta_en_la_primera_fila() {
        let area = Rect { x: 10, y: 5, width: 50, height: 20 };
        let anchos = vec![6, 16];
        let estado = EstadoCsv::nuevo(); // fila=0, columna=0 por defecto
        let estado_tabla = TableState::default();
        let rect = celda_en_pantalla(area, &anchos, &estado, &estado_tabla).unwrap();
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
        let rect = celda_en_pantalla(area, &anchos, &estado, &estado_tabla).unwrap();
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
        assert!(celda_en_pantalla(area, &anchos, &estado, &estado_tabla).is_none());
    }
}
