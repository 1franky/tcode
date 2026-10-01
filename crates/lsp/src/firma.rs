//! Ayuda de firma (BACKLOG.md P2 #23, `textDocument/signatureHelp`): la
//! firma de la función que se está llamando, con el parámetro en el que
//! está el cursor, mientras se escriben los argumentos. Sin UI: solo la
//! traducción de la respuesta a [`AyudaFirma`].

use serde_json::Value;

use crate::diagnostico::utf16_a_indice_char;

/// Lo que se muestra: una firma (la activa) con el rango del parámetro
/// activo dentro de `etiqueta`, en CARACTERES (no bytes ni UTF-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AyudaFirma {
    pub etiqueta: String,
    pub parametro_activo: Option<(usize, usize)>,
    /// Primera línea de la documentación del parámetro activo o, si no
    /// tiene, de la firma.
    pub documentacion: Option<String>,
    /// Cuál de las firmas (sobrecargas) es, y cuántas hay.
    pub indice: usize,
    pub total: usize,
}

fn texto_documentacion(documentacion: &Value) -> Option<String> {
    let texto = documentacion.as_str().or_else(|| documentacion["value"].as_str())?;
    let primera = texto.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(primera.to_string())
}

/// Parsea la respuesta a `textDocument/signatureHelp`. `None` si no hay
/// firma (`null` o sin `signatures`): el cursor ya no está en una llamada.
pub fn parsear_ayuda_firma(resultado: &Value) -> Option<AyudaFirma> {
    let firmas = resultado["signatures"].as_array().filter(|f| !f.is_empty())?;
    let indice = resultado["activeSignature"].as_u64().map(|i| i as usize).filter(|&i| i < firmas.len()).unwrap_or(0);
    let firma = &firmas[indice];
    let etiqueta = firma["label"].as_str()?.to_string();
    let parametros = firma["parameters"].as_array().cloned().unwrap_or_default();
    // El de la firma manda sobre el general (LSP 3.16).
    let activo = firma["activeParameter"]
        .as_u64()
        .or_else(|| resultado["activeParameter"].as_u64())
        .unwrap_or(0) as usize;
    let parametro = parametros.get(activo);
    let parametro_activo = parametro.and_then(|p| match &p["label"] {
        // Desplazamientos en UTF-16 dentro de la etiqueta.
        Value::Array(rango) => {
            let inicio = rango.first()?.as_u64()? as u32;
            let fin = rango.get(1)?.as_u64()? as u32;
            Some((utf16_a_indice_char(&etiqueta, inicio) as usize, utf16_a_indice_char(&etiqueta, fin) as usize))
        }
        // Un texto: se busca en la etiqueta, después del primer `(` (así
        // `a` no se encuentra en el nombre de la función).
        Value::String(texto) if !texto.is_empty() => {
            let desde = etiqueta.find('(').map(|i| i + 1).unwrap_or(0);
            let byte = etiqueta[desde..].find(texto.as_str())? + desde;
            let inicio = etiqueta[..byte].chars().count();
            Some((inicio, inicio + texto.chars().count()))
        }
        _ => None,
    });
    let documentacion = parametro
        .and_then(|p| texto_documentacion(&p["documentation"]))
        .or_else(|| texto_documentacion(&firma["documentation"]));
    Some(AyudaFirma { etiqueta, parametro_activo, documentacion, indice, total: firmas.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parametro_por_texto_se_busca_despues_del_parentesis() {
        let r = json!({
            "signatures": [{
                "label": "a(a: int, b: str) -> None",
                "parameters": [{ "label": "a: int" }, { "label": "b: str", "documentation": "El segundo.\nMás." }],
                "documentation": { "kind": "markdown", "value": "\nHace algo." },
            }],
            "activeSignature": 0,
            "activeParameter": 1,
        });
        let firma = parsear_ayuda_firma(&r).unwrap();
        assert_eq!(firma.parametro_activo, Some((10, 16)));
        assert_eq!(&firma.etiqueta[10..16], "b: str");
        assert_eq!(firma.documentacion.as_deref(), Some("El segundo."));
        assert_eq!((firma.indice, firma.total), (0, 1));
        // Sin documentación del parámetro, la de la firma.
        let r = json!({ "signatures": [{ "label": "a(a: int)", "parameters": [{ "label": "a: int" }],
            "documentation": { "kind": "markdown", "value": "\nHace algo." } }] });
        assert_eq!(parsear_ayuda_firma(&r).unwrap().documentacion.as_deref(), Some("Hace algo."));
    }

    #[test]
    fn parametro_por_rango_utf16_y_activo_de_la_firma() {
        // "ñ" = 1 unidad UTF-16, "😀" = 2: el rango [7, 12) en UTF-16 es
        // "y: u8" en caracteres [6, 11).
        let r = json!({
            "signatures": [
                { "label": "otra()" },
                { "label": "f😀(x, y: u8)", "parameters": [{ "label": [4, 5] }, { "label": [7, 12] }], "activeParameter": 1 },
            ],
            "activeSignature": 1,
            "activeParameter": 0,
        });
        let firma = parsear_ayuda_firma(&r).unwrap();
        let (inicio, fin) = firma.parametro_activo.unwrap();
        let texto: String = firma.etiqueta.chars().skip(inicio).take(fin - inicio).collect();
        assert_eq!(texto, "y: u8");
        assert_eq!((firma.indice, firma.total), (1, 2));
    }

    #[test]
    fn sin_firmas_es_none() {
        assert_eq!(parsear_ayuda_firma(&Value::Null), None);
        assert_eq!(parsear_ayuda_firma(&json!({ "signatures": [] })), None);
        // Fuera de rango: la primera; sin parámetros: sin resaltado.
        let firma = parsear_ayuda_firma(&json!({ "signatures": [{ "label": "f()" }], "activeSignature": 7 })).unwrap();
        assert_eq!((firma.etiqueta.as_str(), firma.parametro_activo), ("f()", None));
    }
}
