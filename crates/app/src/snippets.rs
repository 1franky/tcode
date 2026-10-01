//! Snippets (BACKLOG.md P2 #24) del lado de `app`: expandir un snippet
//! propio al apretar `Tab` justo después de su prefijo, las variables
//! (`$TM_FILENAME`...) y el texto de un `\t` según la config. El motor
//! (campos, `Tab` entre campos) está en `tcode_core::snippet` y en
//! `Editor::insertar_snippet`; los archivos, en `tcode_config::snippets`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use tcode_config::{cargar_snippets, snippet_para_prefijo, Config, SnippetsCargados};
use tcode_core::{parsear_snippet, Editor};
use tcode_syntax::Lenguaje;

/// Snippets ya leídos de disco, por lenguaje (`""` = sin lenguaje): se
/// leen la primera vez que hacen falta y se vuelven a leer con
/// `config.recargar` ([`recargar`]).
static CACHE: Mutex<Option<HashMap<String, SnippetsCargados>>> = Mutex::new(None);

pub fn recargar() {
    if let Ok(mut cache) = CACHE.lock() {
        *cache = None;
    }
}

/// Lo que inserta un `\t` del snippet: la indentación del archivo.
pub fn tab(config: &Config) -> String {
    if config.editor.usar_espacios {
        " ".repeat(config.editor.tamano_tabulacion)
    } else {
        "\t".to_string()
    }
}

/// Las variables de snippet que `tcode` conoce, para el documento de
/// `editor` mostrado como `ruta`.
pub fn variable(editor: &Editor, ruta: &str, nombre: &str) -> Option<String> {
    let ruta_buffer = editor.buffer().ruta();
    let cursor = editor.cursor();
    match nombre {
        "TM_FILENAME" => Path::new(ruta).file_name().map(|n| n.to_string_lossy().into_owned()),
        "TM_FILENAME_BASE" => Path::new(ruta).file_stem().map(|n| n.to_string_lossy().into_owned()),
        "TM_FILEPATH" => ruta_buffer.map(|r| std::fs::canonicalize(r).unwrap_or(r.to_path_buf()).display().to_string()),
        "TM_DIRECTORY" => ruta_buffer
            .and_then(|r| std::fs::canonicalize(r).ok())
            .and_then(|r| r.parent().map(|p| p.display().to_string())),
        "TM_LINE_INDEX" => Some(cursor.linea.to_string()),
        "TM_LINE_NUMBER" => Some((cursor.linea + 1).to_string()),
        "TM_CURRENT_LINE" => Some(editor.buffer().linea_texto(cursor.linea)),
        // Lo seleccionado se reemplaza al insertar: con `Tab` nunca hay.
        "TM_SELECTED_TEXT" => Some(String::new()),
        _ => None,
    }
}

/// `Tab` con un solo cursor sin selección: si lo que hay antes del
/// cursor termina en el prefijo de un snippet propio del lenguaje, lo
/// reemplaza por el snippet. Devuelve si expandió.
pub fn expandir_prefijo(editor: &mut Editor, ruta: &str, config: &Config) -> bool {
    if editor.tiene_multiples_cursores() || editor.cursores()[0].tiene_seleccion() {
        return false;
    }
    let id = Lenguaje::detectar_por_extension(ruta).map(|l| l.id()).unwrap_or("");
    let Ok(mut cache) = CACHE.lock() else { return false };
    let snippets = cache
        .get_or_insert_with(HashMap::new)
        .entry(id.to_string())
        .or_insert_with(|| cargar_snippets((!id.is_empty()).then_some(id)));
    if snippets.snippets.is_empty() {
        return false;
    }
    let cursor = editor.cursor();
    let linea = editor.buffer().linea_texto(cursor.linea);
    let antes: String = linea.chars().take(cursor.columna).collect();
    let Some(snippet) = snippet_para_prefijo(&snippets.snippets, &antes) else { return false };
    let fin = editor.buffer().offset_byte(cursor.linea, cursor.columna);
    let inicio = fin - snippet.prefijo.len();
    let cuerpo = snippet.cuerpo.replace("\r\n", "\n");
    drop(cache);
    let expandido = parsear_snippet(&cuerpo, |nombre| variable(editor, ruta, nombre));
    editor.insertar_snippet(inicio..fin, &expandido, &tab(config));
    true
}
