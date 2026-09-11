use ratatui::layout::Rect;
use ratatui::Frame;

use tcode_commands::EstadoPaleta;

use crate::{overlay, Paleta};

/// Dibuja la paleta de comandos (`Ctrl+Shift+P`/`F1`, PLAN.md §4).
pub fn dibujar(frame: &mut Frame, area_total: Rect, paleta_comandos: &EstadoPaleta, paleta: &Paleta) {
    let filas: Vec<(String, Vec<usize>)> = paleta_comandos
        .resultados()
        .into_iter()
        .map(|r| (r.comando.descripcion.to_string(), r.posiciones))
        .collect();

    overlay::dibujar(
        frame,
        area_total,
        "Paleta de comandos",
        paleta_comandos.consulta(),
        &filas,
        paleta_comandos.seleccion(),
        paleta,
    );
}
