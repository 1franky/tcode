//! Inlay hints (BACKLOG.md P2 #23, `textDocument/inlayHint`): texto que
//! el servidor sugiere mostrar ADENTRO del código sin que esté en el
//! archivo — el tipo inferido de una variable (`: i32`), el nombre de un
//! parámetro en una llamada (`ancho: `). Sin UI: solo la traducción de la
//! respuesta. Las posiciones quedan en coordenadas LSP (línea + columna
//! UTF-16): `app` las convierte contra el texto que tenía el servidor.

use serde_json::Value;

/// Un hint: dónde va (antes del carácter en esa posición) y qué se
/// muestra, ya con los espacios de relleno que pidió el servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PistaInlay {
    pub linea: u32,
    pub caracter: u32,
    pub texto: String,
}

/// Parsea la respuesta a `textDocument/inlayHint` (`InlayHint[] | null`).
/// La etiqueta puede ser un texto o una lista de partes (se concatenan
/// sus `value`); `paddingLeft`/`paddingRight` agregan un espacio de cada
/// lado. Los saltos de línea de una etiqueta se cambian por espacios (un
/// hint ocupa una sola fila).
pub fn parsear_pistas(resultado: &Value) -> Vec<PistaInlay> {
    let Some(lista) = resultado.as_array() else { return Vec::new() };
    lista
        .iter()
        .filter_map(|h| {
            let linea = h["position"]["line"].as_u64()? as u32;
            let caracter = h["position"]["character"].as_u64()? as u32;
            let etiqueta = match &h["label"] {
                Value::String(texto) => texto.clone(),
                Value::Array(partes) => partes.iter().filter_map(|p| p["value"].as_str()).collect(),
                _ => return None,
            };
            let etiqueta: String = etiqueta.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
            if etiqueta.trim().is_empty() {
                return None;
            }
            let izquierda = if h["paddingLeft"].as_bool() == Some(true) { " " } else { "" };
            let derecha = if h["paddingRight"].as_bool() == Some(true) { " " } else { "" };
            Some(PistaInlay { linea, caracter, texto: format!("{izquierda}{etiqueta}{derecha}") })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn etiquetas_de_texto_y_de_partes_con_relleno() {
        let respuesta = json!([
            { "position": { "line": 1, "character": 9 }, "label": ": i32", "kind": 1 },
            { "position": { "line": 3, "character": 12 }, "kind": 2, "paddingRight": true,
              "label": [{ "value": "ancho" }, { "value": ":" }] },
            { "position": { "line": 4, "character": 0 }, "label": "a\nb", "paddingLeft": true },
            { "position": { "line": 5, "character": 0 }, "label": "   " },
            { "label": "sin posición" },
        ]);
        let pistas = parsear_pistas(&respuesta);
        let resumen: Vec<_> = pistas.iter().map(|p| (p.linea, p.caracter, p.texto.as_str())).collect();
        assert_eq!(resumen, [(1, 9, ": i32"), (3, 12, "ancho: "), (4, 0, " a b")]);
        assert!(parsear_pistas(&Value::Null).is_empty());
    }
}
