use ratatui::layout::Rect;
use ratatui::Frame;

use tcode_commands::EstadoSelectorSimbolos;

use crate::{overlay, Paleta};

/// Espacios de indentación por nivel de anidamiento en la lista.
const INDENTACION: usize = 2;

/// Dibuja el selector de símbolos del archivo actual (`Ctrl+K .`, "Ir a
/// símbolo"). Reutiliza el overlay genérico de la paleta/buscador: cada
/// fila es el símbolo indentado según cuántos contenedores lo encierran
/// (`impl Editor` > `  fn insertar`), seguido de su número de línea; las
/// letras que coinciden con el filtro van en negrita (corridas por la
/// indentación, porque el filtro compara solo contra la etiqueta).
pub fn dibujar(frame: &mut Frame, area_total: Rect, selector: &EstadoSelectorSimbolos, paleta: &Paleta) {
    let filas: Vec<(String, Vec<usize>)> = if selector.sin_simbolos() {
        vec![("(este archivo no tiene funciones, clases ni otros símbolos)".to_string(), Vec::new())]
    } else {
        selector
            .resultados()
            .iter()
            .map(|resultado| {
                let simbolo = selector.simbolo(resultado.indice);
                let sangria = simbolo.profundidad * INDENTACION;
                let texto = format!("{}{}  :{}", " ".repeat(sangria), simbolo.etiqueta, simbolo.linea);
                (texto, resultado.posiciones.iter().map(|p| p + sangria).collect())
            })
            .collect()
    };
    // Sin símbolos, la fila del aviso no se marca como seleccionada.
    let seleccion = if selector.sin_simbolos() { usize::MAX } else { selector.seleccion() };
    overlay::dibujar(frame, area_total, "Ir a símbolo", selector.consulta(), &filas, seleccion, paleta);
}
