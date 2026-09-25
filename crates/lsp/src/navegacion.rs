//! Funciones de LSP más allá de diagnósticos y formateo (BACKLOG.md P1
//! #17): qué anuncia el servidor, y la traducción de las respuestas a
//! `textDocument/definition`/`references`/`hover`/`rename` a tipos
//! simples, sin UI. Las posiciones siguen en coordenadas LSP (línea +
//! columna UTF-16): quien las usa (`app`) las convierte a bytes contra el
//! texto que corresponda con [`byte_de_posicion`]/[`byte_en_linea`] — el
//! archivo de destino de una definición puede no estar abierto todavía.

use std::path::PathBuf;

use anyhow::{bail, Result};
use lsp_types::{Position, Range};
use serde_json::Value;

use crate::diagnostico::utf16_a_indice_char;

/// Lo que el servidor dijo que sabe hacer al responder `initialize`, de
/// lo que usan estas funciones. Cada `*Provider` puede venir como `true`
/// o como objeto de opciones (ambos "sí"); `false` o ausente, "no".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapacidadesLsp {
    pub definicion: bool,
    pub referencias: bool,
    pub hover: bool,
    pub renombrar: bool,
    pub completado: bool,
    /// `completionProvider.triggerCharacters`: tipear uno pide completado
    /// en el acto, sin esperar la pausa (el `.` de un método, `::`...).
    pub disparadores_completado: Vec<char>,
}

impl CapacidadesLsp {
    /// Lee las capacidades del `result` completo de `initialize`.
    pub fn desde_initialize(resultado: &Value) -> Self {
        let capacidades = &resultado["capabilities"];
        let anuncia = |campo: &str| match &capacidades[campo] {
            Value::Bool(si) => *si,
            Value::Object(_) => true,
            _ => false,
        };
        let disparadores_completado = capacidades["completionProvider"]["triggerCharacters"]
            .as_array()
            .map(|lista| lista.iter().filter_map(Value::as_str).filter_map(|s| s.chars().next()).collect())
            .unwrap_or_default();
        Self {
            definicion: anuncia("definitionProvider"),
            referencias: anuncia("referencesProvider"),
            hover: anuncia("hoverProvider"),
            renombrar: anuncia("renameProvider"),
            // `completionProvider` es siempre un objeto de opciones (no
            // hay forma booleana en la spec): su sola presencia es un sí.
            completado: capacidades["completionProvider"].is_object(),
            disparadores_completado,
        }
    }
}

/// Posición LSP del cursor que está en la línea `linea` con `prefijo`
/// (el texto de esa línea ANTES del cursor) a su izquierda: la columna
/// se mide en unidades UTF-16, como pide la spec.
pub fn posicion_en_linea(linea: usize, prefijo: &str) -> Position {
    Position { line: linea as u32, character: prefijo.chars().map(|c| c.len_utf16() as u32).sum() }
}

/// Byte dentro de `linea` (sin su `\n`) de la columna UTF-16 `utf16`;
/// más allá del final, el final de la línea (lo mismo que hace la spec).
pub fn byte_en_linea(linea: &str, utf16: u32) -> usize {
    let idx_char = utf16_a_indice_char(linea, utf16) as usize;
    linea.char_indices().nth(idx_char).map(|(byte, _)| byte).unwrap_or(linea.len())
}

/// Offset de bytes absoluto en `texto` de una `Position` LSP. Una
/// columna más allá del final de la línea se recorta a su final (sin el
/// `\n`), y una línea más allá del final del documento es el final del
/// documento.
pub fn byte_de_posicion(texto: &str, posicion: Position) -> usize {
    let Some(inicio) = inicio_de_linea(texto, posicion.line as usize) else { return texto.len() };
    let fin = texto[inicio..].find('\n').map(|i| inicio + i).unwrap_or(texto.len());
    inicio + byte_en_linea(&texto[inicio..fin], posicion.character)
}

fn inicio_de_linea(texto: &str, linea: usize) -> Option<usize> {
    if linea == 0 {
        return Some(0);
    }
    texto.match_indices('\n').nth(linea - 1).map(|(i, _)| i + 1)
}

/// Ruta local de un URI `file://`, deshaciendo el percent-encoding (lo
/// inverso de `uri_de_archivo` en `app/lsp.rs`). En Windows el URI trae
/// la unidad detrás de una barra (`file:///C:/...`): se le saca esa
/// barra. `None` si no es `file://` o el resultado no es UTF-8 válido.
pub fn ruta_desde_uri(uri: &str) -> Option<PathBuf> {
    let resto = uri.strip_prefix("file://")?;
    // Autoridad vacía (`file:///...`) o `localhost`; otra, no es local.
    let camino = if resto.starts_with('/') { resto } else { resto.strip_prefix("localhost")? };
    let bytes = camino.as_bytes();
    let mut salida = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            salida.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            salida.push(bytes[i]);
            i += 1;
        }
    }
    let texto = String::from_utf8(salida).ok()?;
    let es_unidad_windows =
        texto.len() >= 3 && texto.as_bytes()[0] == b'/' && texto.as_bytes()[2] == b':' && texto.as_bytes()[1].is_ascii_alphabetic();
    Some(PathBuf::from(if es_unidad_windows { &texto[1..] } else { &texto[..] }))
}

/// Un lugar del código devuelto por el servidor (definición o
/// referencia): archivo y posición LSP del inicio.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ubicacion {
    pub ruta: PathBuf,
    pub linea: u32,
    pub caracter: u32,
}

/// Parsea la respuesta a `textDocument/definition` o `references`:
/// `Location | Location[] | LocationLink[] | null`. De un `LocationLink`
/// se usa `targetSelectionRange` (el nombre del símbolo, no el cuerpo
/// entero). Se descartan los URIs que no son archivos locales, y el
/// resultado queda ordenado por archivo y posición, sin repetidos.
pub fn parsear_ubicaciones(resultado: &Value) -> Vec<Ubicacion> {
    let elementos: Vec<&Value> = match resultado {
        Value::Array(lista) => lista.iter().collect(),
        Value::Object(_) => vec![resultado],
        _ => Vec::new(),
    };
    let mut ubicaciones: Vec<Ubicacion> = elementos
        .into_iter()
        .filter_map(|e| {
            let (uri, rango) = match e.get("targetUri") {
                Some(uri) => (uri, e.get("targetSelectionRange").or_else(|| e.get("targetRange"))?),
                None => (e.get("uri")?, e.get("range")?),
            };
            let inicio = &rango["start"];
            Some(Ubicacion {
                ruta: ruta_desde_uri(uri.as_str()?)?,
                linea: inicio["line"].as_u64()? as u32,
                caracter: inicio["character"].as_u64()? as u32,
            })
        })
        .collect();
    ubicaciones.sort();
    ubicaciones.dedup();
    ubicaciones
}

/// Cuántas líneas del texto de hover se muestran como mucho: la
/// documentación de algunas funciones de la biblioteca estándar ocupa
/// pantallas enteras.
pub const MAX_LINEAS_HOVER: usize = 20;

/// Texto plano de la respuesta a `textDocument/hover` (`Hover | null`),
/// o `None` si no hay nada que mostrar. `contents` puede ser
/// `MarkupContent` (`{kind, value}`), un `MarkedString` (texto, o
/// `{language, value}`) o una lista de ellos. El markdown no se
/// interpreta: se sacan las líneas de cerco de código (```` ``` ````) y
/// los separadores `---`, las barras de escape (`\_` → `_`), las líneas
/// en blanco repetidas, y se recorta a [`MAX_LINEAS_HOVER`] líneas.
pub fn texto_hover(resultado: &Value) -> Option<String> {
    fn partes(valor: &Value, salida: &mut Vec<String>) {
        match valor {
            Value::String(texto) => salida.push(texto.clone()),
            Value::Array(lista) => lista.iter().for_each(|v| partes(v, salida)),
            Value::Object(objeto) => {
                if let Some(Value::String(texto)) = objeto.get("value") {
                    salida.push(texto.clone());
                }
            }
            _ => {}
        }
    }
    let mut crudas = Vec::new();
    partes(resultado.get("contents")?, &mut crudas);

    let mut lineas: Vec<String> = Vec::new();
    for linea in crudas.join("\n\n").lines() {
        let recortada = linea.trim_end();
        if recortada.trim_start().starts_with("```") || recortada.trim() == "---" {
            continue;
        }
        let limpia = sin_escapes_markdown(recortada);
        if limpia.trim().is_empty() && lineas.last().is_none_or(|l| l.trim().is_empty()) {
            continue;
        }
        lineas.push(limpia);
    }
    while lineas.last().is_some_and(|l| l.trim().is_empty()) {
        lineas.pop();
    }
    if lineas.is_empty() {
        return None;
    }
    if lineas.len() > MAX_LINEAS_HOVER {
        lineas.truncate(MAX_LINEAS_HOVER);
        lineas.push("...".to_string());
    }
    Some(lineas.join("\n"))
}

/// `\_`, `\*`, `\(`... → el carácter solo: los servidores escapan la
/// puntuación de markdown aunque dentro del texto no signifique nada.
fn sin_escapes_markdown(linea: &str) -> String {
    let mut salida = String::with_capacity(linea.len());
    let mut caracteres = linea.chars().peekable();
    while let Some(c) = caracteres.next() {
        if c == '\\' && caracteres.peek().is_some_and(|s| s.is_ascii_punctuation()) {
            continue;
        }
        salida.push(c);
    }
    salida
}

/// Las ediciones de un `WorkspaceEdit` para UN archivo: rangos LSP sobre
/// el texto que tenía el servidor (el del buffer si está abierto, el del
/// disco si no) y el texto nuevo de cada uno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdicionArchivo {
    pub ruta: PathBuf,
    pub ediciones: Vec<(Range, String)>,
}

/// Parsea un `WorkspaceEdit` (respuesta a `textDocument/rename`), en
/// cualquiera de sus dos formas: `changes` (URI → `TextEdit[]`) o
/// `documentChanges` (`TextDocumentEdit[]`). Las operaciones sobre
/// archivos (crear/renombrar/borrar, que `tcode` no anuncia soportar)
/// hacen fallar el conjunto entero: aplicar solo una parte de un
/// renombrado dejaría el código roto. `null` es "nada que cambiar".
pub fn parsear_workspace_edit(resultado: &Value) -> Result<Vec<EdicionArchivo>> {
    fn ediciones_de(lista: &Value) -> Result<Vec<(Range, String)>> {
        let Some(lista) = lista.as_array() else { bail!("lista de ediciones con forma inesperada") };
        lista
            .iter()
            .map(|e| {
                let rango: Range = serde_json::from_value(e["range"].clone())?;
                let Some(texto) = e["newText"].as_str() else { bail!("edición sin newText") };
                Ok((rango, texto.replace("\r\n", "\n").replace('\r', "\n")))
            })
            .collect()
    }
    let mut archivos: Vec<EdicionArchivo> = Vec::new();
    let mut agregar = |uri: &str, ediciones: Vec<(Range, String)>| -> Result<()> {
        let Some(ruta) = ruta_desde_uri(uri) else { bail!("el cambio toca algo que no es un archivo local: {uri}") };
        match archivos.iter_mut().find(|a| a.ruta == ruta) {
            Some(archivo) => archivo.ediciones.extend(ediciones),
            None => archivos.push(EdicionArchivo { ruta, ediciones }),
        }
        Ok(())
    };
    if resultado.is_null() {
        return Ok(Vec::new());
    }
    if let Some(cambios) = resultado.get("documentChanges").and_then(Value::as_array) {
        for cambio in cambios {
            if cambio.get("kind").is_some() {
                bail!("el cambio incluye crear, renombrar o borrar archivos (no soportado)");
            }
            let Some(uri) = cambio["textDocument"]["uri"].as_str() else { bail!("TextDocumentEdit sin uri") };
            agregar(uri, ediciones_de(&cambio["edits"])?)?;
        }
    } else if let Some(cambios) = resultado.get("changes").and_then(Value::as_object) {
        for (uri, lista) in cambios {
            agregar(uri, ediciones_de(lista)?)?;
        }
    }
    Ok(archivos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capacidades_con_true_objeto_o_ausentes() {
        let resultado = json!({ "capabilities": {
            "definitionProvider": true,
            "referencesProvider": { "workDoneProgress": false },
            "hoverProvider": false,
            "completionProvider": { "triggerCharacters": [".", ":", ""] },
        }});
        let capacidades = CapacidadesLsp::desde_initialize(&resultado);
        assert!(capacidades.definicion && capacidades.referencias && capacidades.completado);
        assert!(!capacidades.hover && !capacidades.renombrar);
        assert_eq!(capacidades.disparadores_completado, vec!['.', ':']);
        assert_eq!(CapacidadesLsp::desde_initialize(&Value::Null), CapacidadesLsp::default());
    }

    #[test]
    fn posicion_en_linea_cuenta_unidades_utf16() {
        // "é" = 1 unidad, "😀" = 2.
        assert_eq!(posicion_en_linea(3, "é😀x"), Position { line: 3, character: 4 });
    }

    #[test]
    fn byte_en_linea_con_acentos_y_emoji() {
        let linea = "a😀éx";
        assert_eq!(byte_en_linea(linea, 4), 7);
        assert_eq!(&linea[7..], "x");
        assert_eq!(byte_en_linea(linea, 99), linea.len());
    }

    #[test]
    fn byte_de_posicion_en_otra_linea_y_fuera_de_rango() {
        let texto = "uno\nñandú😀 = 1\n";
        let byte = byte_de_posicion(texto, Position { line: 1, character: 7 });
        assert_eq!(&texto[byte..], " = 1\n");
        assert_eq!(byte_de_posicion(texto, Position { line: 0, character: 50 }), 3);
        assert_eq!(byte_de_posicion(texto, Position { line: 9, character: 0 }), texto.len());
    }

    #[test]
    fn ruta_desde_uri_decodifica_y_quita_la_barra_de_la_unidad() {
        assert_eq!(ruta_desde_uri("file:///tmp/mi%20archivo%23.rs"), Some(PathBuf::from("/tmp/mi archivo#.rs")));
        assert_eq!(ruta_desde_uri("file:///tmp/%C3%B1and%C3%BA.py"), Some(PathBuf::from("/tmp/ñandú.py")));
        assert_eq!(ruta_desde_uri("file:///C%3A/Users/a.rs"), Some(PathBuf::from("C:/Users/a.rs")));
        assert_eq!(ruta_desde_uri("https://ejemplo.com/a.rs"), None);
        assert_eq!(ruta_desde_uri("file:///a%2"), Some(PathBuf::from("/a%2")));
    }

    #[test]
    fn ubicaciones_desde_location_lista_y_location_link() {
        let rango = json!({ "start": { "line": 4, "character": 2 }, "end": { "line": 4, "character": 5 } });
        let una = json!({ "uri": "file:///p/a.rs", "range": rango });
        assert_eq!(parsear_ubicaciones(&una), vec![Ubicacion { ruta: "/p/a.rs".into(), linea: 4, caracter: 2 }]);

        let enlace = json!([{
            "targetUri": "file:///p/b.rs",
            "targetRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 9, "character": 1 } },
            "targetSelectionRange": rango,
        }]);
        assert_eq!(parsear_ubicaciones(&enlace)[0], Ubicacion { ruta: "/p/b.rs".into(), linea: 4, caracter: 2 });
        assert!(parsear_ubicaciones(&Value::Null).is_empty());
    }

    #[test]
    fn ubicaciones_ordenadas_y_sin_repetidos() {
        let en = |uri: &str, linea: u32| json!({ "uri": uri, "range": { "start": { "line": linea, "character": 0 }, "end": { "line": linea, "character": 1 } } });
        let lista = json!([en("file:///p/b.rs", 1), en("file:///p/a.rs", 7), en("file:///p/a.rs", 2), en("file:///p/b.rs", 1)]);
        let lineas: Vec<(String, u32)> =
            parsear_ubicaciones(&lista).into_iter().map(|u| (u.ruta.display().to_string(), u.linea)).collect();
        assert_eq!(lineas, [("/p/a.rs".to_string(), 2), ("/p/a.rs".to_string(), 7), ("/p/b.rs".to_string(), 1)]);
    }

    #[test]
    fn hover_markdown_como_texto_plano() {
        let resultado = json!({ "contents": { "kind": "markdown", "value": "```rust\nfn suma(a: i32) -> i32\n```\n\n---\n\n\nSuma \\_todo\\_." } });
        assert_eq!(texto_hover(&resultado).unwrap(), "fn suma(a: i32) -> i32\n\nSuma _todo_.");
    }

    #[test]
    fn hover_con_marked_strings_y_vacio() {
        let resultado = json!({ "contents": [{ "language": "python", "value": "def f() -> None" }, "Doc."] });
        assert_eq!(texto_hover(&resultado).unwrap(), "def f() -> None\n\nDoc.");
        assert_eq!(texto_hover(&json!({ "contents": "" })), None);
        assert_eq!(texto_hover(&Value::Null), None);
    }

    #[test]
    fn hover_largo_se_recorta() {
        let largo: Vec<String> = (0..50).map(|i| format!("línea {i}")).collect();
        let texto = texto_hover(&json!({ "contents": largo.join("\n") })).unwrap();
        assert_eq!(texto.lines().count(), MAX_LINEAS_HOVER + 1);
        assert!(texto.ends_with("..."));
    }

    fn edicion(linea: u32, c1: u32, c2: u32, texto: &str) -> Value {
        json!({ "range": { "start": { "line": linea, "character": c1 }, "end": { "line": linea, "character": c2 } }, "newText": texto })
    }

    #[test]
    fn workspace_edit_con_changes_y_document_changes() {
        let con_changes = json!({ "changes": { "file:///p/a.py": [edicion(0, 0, 3, "nuevo")] } });
        let archivos = parsear_workspace_edit(&con_changes).unwrap();
        assert_eq!(archivos.len(), 1);
        assert_eq!(archivos[0].ruta, PathBuf::from("/p/a.py"));
        assert_eq!(archivos[0].ediciones[0].1, "nuevo");

        let con_document_changes = json!({ "documentChanges": [
            { "textDocument": { "uri": "file:///p/a.rs", "version": 3 }, "edits": [edicion(1, 4, 7, "b")] },
            { "textDocument": { "uri": "file:///p/c.rs", "version": null }, "edits": [edicion(0, 0, 1, "b")] },
            { "textDocument": { "uri": "file:///p/a.rs", "version": 3 }, "edits": [edicion(5, 0, 1, "b")] },
        ]});
        let archivos = parsear_workspace_edit(&con_document_changes).unwrap();
        assert_eq!(archivos.len(), 2);
        assert_eq!(archivos[0].ediciones.len(), 2, "las del mismo archivo se juntan");
        assert!(parsear_workspace_edit(&Value::Null).unwrap().is_empty());
    }

    #[test]
    fn workspace_edit_con_operaciones_de_archivo_falla_entero() {
        let resultado = json!({ "documentChanges": [
            { "textDocument": { "uri": "file:///p/a.rs", "version": 1 }, "edits": [edicion(0, 0, 1, "b")] },
            { "kind": "rename", "oldUri": "file:///p/a.rs", "newUri": "file:///p/b.rs" },
        ]});
        assert!(parsear_workspace_edit(&resultado).is_err());
    }
}
