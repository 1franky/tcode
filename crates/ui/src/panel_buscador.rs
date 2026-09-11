use ratatui::layout::Rect;
use ratatui::Frame;

use tcode_fs::BuscadorArchivos;

use crate::{overlay, Paleta};

/// Dibuja el buscador difuso de archivos (`Ctrl+P`, PLAN.md §4).
pub fn dibujar(frame: &mut Frame, area_total: Rect, buscador: &BuscadorArchivos, paleta: &Paleta) {
    let filas: Vec<(String, Vec<usize>)> =
        buscador.resultados().into_iter().map(|r| (r.ruta_mostrada, r.posiciones)).collect();

    overlay::dibujar(frame, area_total, "Buscar archivo", buscador.consulta(), &filas, buscador.seleccion(), paleta);
}
