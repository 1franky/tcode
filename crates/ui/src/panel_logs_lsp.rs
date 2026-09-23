use ratatui::layout::Rect;
use ratatui::Frame;

use tcode_lsp::EstadoLogsLsp;

use crate::{overlay, Paleta};

/// Dibuja el visor de logs de la sesión LSP activa (`Ctrl+K R`, PLAN.md
/// §5.3). Reutiliza el overlay genérico de "escribir para buscar" (paleta
/// de comandos, buscador de archivos, selector de temas): el campo de
/// arriba es el filtro de texto sobre las líneas, no algo que se
/// confirme con `Enter` — no hay ninguna acción que ejecutar sobre una
/// línea de log, solo mirarlas. Se actualiza en vivo mientras está
/// abierto (BACKLOG.md P1 #2); la fila resaltada es solo para hacer
/// scroll con `↑`/`↓` — "siguiendo lo más nuevo" (sin fila) se le pasa
/// como `usize::MAX`, que nunca coincide con ningún índice real, y el
/// overlay dibuja desde arriba, donde entran las líneas nuevas.
pub fn dibujar(frame: &mut Frame, area_total: Rect, estado: &EstadoLogsLsp, paleta: &Paleta) {
    let filas: Vec<(String, Vec<usize>)> = if estado.sin_logs() {
        vec![("(sin logs — no hay ninguna sesión LSP activa, o no escribió nada en stderr)".to_string(), Vec::new())]
    } else {
        estado.lineas_filtradas().into_iter().map(|(linea, posiciones)| (linea.to_string(), posiciones)).collect()
    };

    let siguiendo = if estado.seleccion().is_none() { "en vivo" } else { "↑ para volver a lo nuevo" };
    let consulta = format!("Filtrar: {} ({siguiendo}, ↑/↓ recorre, Esc cierra)", estado.filtro());

    let seleccion = estado.seleccion().unwrap_or(usize::MAX);
    overlay::dibujar(frame, area_total, "Logs del LSP activo", &consulta, &filas, seleccion, paleta);
}
