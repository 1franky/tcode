use ratatui::layout::Rect;
use ratatui::Frame;

use tcode_config::EstadoSelectorTema;

use crate::{overlay, Paleta};

/// Dibuja el selector de temas (`Ctrl+K Ctrl+T`, PLAN.md §5/§7). Reutiliza
/// el overlay genérico de "lista con campo arriba" (paleta de comandos,
/// buscador de archivos): el "campo de consulta" no se escribe acá, se usa
/// para mostrar el filtro activo y el atajo para cambiarlo (`Tab`). El
/// nombre del tema actualmente activo (antes de abrir el selector) se
/// marca con `●` para no perder de referencia cuál era, mientras se
/// navega el preview en vivo con `↑`/`↓`.
pub fn dibujar(frame: &mut Frame, area_total: Rect, selector: &EstadoSelectorTema, paleta: &Paleta) {
    let filas: Vec<(String, Vec<usize>)> = selector
        .temas_filtrados()
        .iter()
        .map(|tema| {
            let marca = if tema.id == selector.tema_original() { "● " } else { "  " };
            (format!("{marca}{} ({})", tema.nombre, etiqueta_tipo(tema.tipo)), Vec::new())
        })
        .collect();

    let consulta = format!("Filtro: {} (Tab para cambiar)", selector.filtro().etiqueta());

    overlay::dibujar(frame, area_total, "Seleccionar tema", &consulta, &filas, selector.seleccion(), paleta);
}

fn etiqueta_tipo(tipo: &str) -> &'static str {
    match tipo {
        "light" => "claro",
        _ => "oscuro",
    }
}
