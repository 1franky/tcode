//! Acciones rápidas (BACKLOG.md P2 #23, `textDocument/codeAction`): los
//! arreglos y refactors que el servidor ofrece para una posición o
//! selección (agregar un import, corregir un nombre, extraer una
//! variable...). Sin UI: solo la traducción de la respuesta.
//!
//! Cada acción puede traer un `WorkspaceEdit` (los cambios, que `app`
//! aplica como los de renombrar), un comando del servidor
//! (`workspace/executeCommand`: el servidor hace el trabajo y, si hay que
//! cambiar texto, lo pide con `workspace/applyEdit`) o las dos cosas, en
//! ese orden. La respuesta es una lista mezclada de `CodeAction` y de
//! `Command` sueltos (la forma vieja), que se tratan igual.

use serde_json::Value;

/// Una acción ofrecida por el servidor.
#[derive(Debug, Clone, PartialEq)]
pub struct AccionRapida {
    pub titulo: String,
    /// `kind` (`quickfix`, `refactor.extract`, `source.organizeImports`...).
    pub tipo: Option<String>,
    /// `isPreferred`: el arreglo que el servidor recomienda.
    pub preferida: bool,
    /// El `WorkspaceEdit` crudo (se parsea al aplicarlo, con
    /// `parsear_workspace_edit`).
    pub edicion: Option<Value>,
    /// El `Command` crudo (`{ title, command, arguments }`).
    pub comando: Option<Value>,
}

/// Parsea la respuesta a `textDocument/codeAction`. Deja afuera las
/// deshabilitadas (`disabled`: el servidor explica por qué no aplica,
/// pero no se pueden usar) y las que no traen ni cambios ni comando. Las
/// preferidas van primero; el resto conserva el orden del servidor.
pub fn parsear_acciones(resultado: &Value) -> Vec<AccionRapida> {
    let Some(lista) = resultado.as_array() else { return Vec::new() };
    let mut acciones: Vec<AccionRapida> = lista
        .iter()
        .filter_map(|a| {
            let titulo = a["title"].as_str()?.to_string();
            // Un `Command` suelto tiene `command` como texto; en un
            // `CodeAction`, `command` es un objeto `Command`.
            if a["command"].is_string() {
                return Some(AccionRapida {
                    titulo,
                    tipo: None,
                    preferida: false,
                    edicion: None,
                    comando: Some(a.clone()),
                });
            }
            if a.get("disabled").is_some_and(|d| !d.is_null()) {
                return None;
            }
            let edicion = a.get("edit").filter(|e| !e.is_null()).cloned();
            let comando = a.get("command").filter(|c| c.is_object()).cloned();
            if edicion.is_none() && comando.is_none() {
                return None;
            }
            Some(AccionRapida {
                titulo,
                tipo: a["kind"].as_str().map(str::to_string),
                preferida: a["isPreferred"].as_bool().unwrap_or(false),
                edicion,
                comando,
            })
        })
        .collect();
    // `sort_by_key` es estable: entre las preferidas y entre las demás
    // queda el orden del servidor.
    acciones.sort_by_key(|a| !a.preferida);
    acciones
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mezcla_de_code_actions_y_commands() {
        let respuesta = json!([
            { "title": "Organizar imports", "command": "pyright.organizeimports", "arguments": ["file:///a.py"] },
            { "title": "Extraer variable", "kind": "refactor.extract", "edit": { "changes": {} } },
            { "title": "Importar HashMap", "kind": "quickfix", "isPreferred": true,
              "edit": { "changes": {} }, "command": { "title": "x", "command": "rust-analyzer.algo" } },
            { "title": "Deshabilitada", "kind": "refactor", "edit": {}, "disabled": { "reason": "no aplica" } },
            { "title": "Vacía", "kind": "quickfix" },
            { "sin": "título" },
        ]);
        let acciones = parsear_acciones(&respuesta);
        let titulos: Vec<_> = acciones.iter().map(|a| a.titulo.as_str()).collect();
        assert_eq!(titulos, ["Importar HashMap", "Organizar imports", "Extraer variable"]);
        assert!(acciones[0].preferida && acciones[0].edicion.is_some() && acciones[0].comando.is_some());
        assert_eq!(acciones[0].tipo.as_deref(), Some("quickfix"));
        // El `Command` suelto se ejecuta tal cual.
        assert_eq!(acciones[1].comando.as_ref().unwrap()["command"], "pyright.organizeimports");
        assert!(acciones[1].edicion.is_none());
    }

    #[test]
    fn null_o_forma_rara_es_sin_acciones() {
        assert!(parsear_acciones(&Value::Null).is_empty());
        assert!(parsear_acciones(&json!({})).is_empty());
    }
}
