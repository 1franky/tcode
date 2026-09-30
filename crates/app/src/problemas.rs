//! Panel de problemas (BACKLOG.md P2 #22): los diagnósticos del LSP de
//! todos los documentos abiertos, en cualquier pestaña o panel, juntos en
//! una lista con salto a cada uno (`Ctrl+Shift+M`/`Ctrl+K Q`), más
//! "siguiente/anterior problema" (`F8`/`Shift+F8`) como en VSCode.
//!
//! No guarda nada propio: los diagnósticos ya viven en cada documento
//! (`PanelEditor::diagnosticos`, los pone `EstadoLsp` al llegar cada
//! `publishDiagnostics`, también en pestañas de fondo). La lista es la
//! misma de "Buscar referencias" (`EstadoFuncionesLsp::lista`), así que
//! filtrar, `Enter`, el mouse y "Volver" (`Alt+←`) funcionan igual.

use std::path::{Path, PathBuf};

use tcode_commands::EntradaUbicacion;
use tcode_lsp::Severidad;
use tcode_ui::Layout as PanelLayout;

use crate::funciones_lsp::{avisar, mismo_archivo, saltar_a};
use crate::EstadoApp;

/// Un diagnóstico de un documento abierto, con lo necesario para
/// mostrarlo y saltar a él.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Problema {
    ruta: PathBuf,
    /// La ruta como se muestra: relativa al directorio actual si está
    /// adentro. También es la clave de orden entre archivos.
    mostrada: String,
    linea: usize,
    /// Columna en caracteres (como `Cursor`), para compararla con el
    /// cursor en `F8`.
    columna: usize,
    /// La misma columna en unidades UTF-16, que es lo que espera la
    /// lista de ubicaciones (la comparte con las respuestas del LSP).
    caracter: u32,
    severidad: Severidad,
    mensaje: String,
}

/// Todos los problemas de los documentos abiertos, por archivo y
/// posición. Un archivo abierto en dos paneles (con buffers distintos)
/// cuenta una vez: el primero que aparece, que es el del panel activo si
/// está ahí.
fn recolectar(layout: &PanelLayout) -> Vec<Problema> {
    let actual = std::env::current_dir().ok().and_then(|d| std::fs::canonicalize(d).ok());
    let mut documentos = layout.documentos();
    let activo = layout.panel_activo();
    if let Some(posicion) = documentos.iter().position(|d| std::ptr::eq(*d, activo)) {
        let documento = documentos.remove(posicion);
        documentos.insert(0, documento);
    }

    let mut vistos: Vec<&Path> = Vec::new();
    let mut problemas = Vec::new();
    for documento in documentos {
        let Some(ruta) = documento.editor.buffer().ruta() else { continue };
        if documento.diagnosticos.is_empty() || vistos.iter().any(|v| mismo_archivo(v, ruta)) {
            continue;
        }
        vistos.push(ruta);
        let mostrada = ruta_para_mostrar(actual.as_deref(), ruta);
        let buffer = documento.editor.buffer();
        for d in &documento.diagnosticos {
            let linea = (d.linea_inicio as usize).min(buffer.num_lineas().saturating_sub(1));
            let columna = d.columna_inicio as usize;
            let caracter = buffer.linea_texto(linea).chars().take(columna).map(|c| c.len_utf16() as u32).sum();
            problemas.push(Problema {
                ruta: ruta.to_path_buf(),
                mostrada: mostrada.clone(),
                linea,
                columna,
                caracter,
                severidad: d.severidad,
                // Un mensaje de varias líneas (rustc, pyright) en una
                // sola fila de la lista.
                mensaje: d.mensaje.split_whitespace().collect::<Vec<_>>().join(" "),
            });
        }
    }
    ordenar(&mut problemas);
    problemas
}

fn ordenar(problemas: &mut [Problema]) {
    problemas.sort_by(|a, b| (&a.mostrada, a.linea, a.columna).cmp(&(&b.mostrada, b.linea, b.columna)));
}

fn ruta_para_mostrar(actual: Option<&Path>, ruta: &Path) -> String {
    let canonica = std::fs::canonicalize(ruta).unwrap_or_else(|_| ruta.to_path_buf());
    actual
        .and_then(|a| canonica.strip_prefix(a).ok())
        .map(|r| r.display().to_string())
        .unwrap_or_else(|| ruta.display().to_string())
}

fn nombre_severidad(severidad: Severidad) -> &'static str {
    match severidad {
        Severidad::Error => "error",
        Severidad::Advertencia => "aviso",
        Severidad::Informacion => "info",
        Severidad::Sugerencia => "nota",
    }
}

/// "Problemas (2 errores, 1 aviso)" — cuenta solo lo que hay.
fn titulo(problemas: &[Problema]) -> String {
    let contar = |s: Severidad| problemas.iter().filter(|p| p.severidad == s).count();
    let partes: Vec<String> = [
        (contar(Severidad::Error), "error", "errores"),
        (contar(Severidad::Advertencia), "aviso", "avisos"),
        (contar(Severidad::Informacion), "info", "info"),
        (contar(Severidad::Sugerencia), "nota", "notas"),
    ]
    .into_iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, uno, varios)| format!("{n} {}", if n == 1 { uno } else { varios }))
    .collect();
    format!("Problemas ({})", partes.join(", "))
}

/// `problemas.ver`: abre la lista. Sin problemas, solo un aviso.
pub fn ver(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let problemas = recolectar(layout);
    if problemas.is_empty() {
        avisar(layout, "Sin problemas en los archivos abiertos");
        return;
    }
    let titulo = titulo(&problemas);
    let entradas = problemas
        .into_iter()
        .map(|p| EntradaUbicacion {
            etiqueta: format!(
                "{:<5} {}:{}:{}  {}",
                nombre_severidad(p.severidad),
                p.mostrada,
                p.linea + 1,
                p.columna + 1,
                p.mensaje
            ),
            ruta: p.ruta,
            linea: p.linea as u32,
            caracter: p.caracter,
        })
        .collect();
    estado.funciones_lsp.abrir_ubicaciones(titulo, entradas);
}

/// El índice del problema al que saltar desde `(archivo, línea, columna)`
/// hacia adelante o hacia atrás, dando la vuelta al llegar a un extremo.
/// `desde` es `None` si el documento activo no tiene problemas ni está
/// entre los archivos que los tienen (se va al primero/último).
fn indice_destino(problemas: &[Problema], desde: Option<(&str, usize, usize)>, adelante: bool) -> usize {
    let n = problemas.len();
    let Some(desde) = desde else { return if adelante { 0 } else { n - 1 } };
    fn clave(p: &Problema) -> (&str, usize, usize) {
        (p.mostrada.as_str(), p.linea, p.columna)
    }
    if adelante {
        problemas.iter().position(|p| clave(p) > desde).unwrap_or(0)
    } else {
        problemas.iter().rposition(|p| clave(p) < desde).unwrap_or(n - 1)
    }
}

/// `problemas.siguiente`/`problemas.anterior` (`F8`/`Shift+F8`): salta al
/// próximo problema después (o antes) del cursor, pasando a otro archivo
/// abierto al terminar los de este, y muestra el mensaje en la barra.
pub fn saltar(adelante: bool, layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let problemas = recolectar(layout);
    if problemas.is_empty() {
        avisar(layout, "Sin problemas en los archivos abiertos");
        return;
    }
    let editor = layout.editor_activo();
    let cursor = editor.cursor();
    let actual = std::env::current_dir().ok().and_then(|d| std::fs::canonicalize(d).ok());
    let mostrada_activa = editor.buffer().ruta().map(|r| ruta_para_mostrar(actual.as_deref(), r));
    let desde = mostrada_activa.as_deref().map(|m| (m, cursor.linea, cursor.columna));
    let p = problemas[indice_destino(&problemas, desde, adelante)].clone();
    saltar_a(layout, estado, p.ruta, p.linea as u32, p.caracter);
    avisar(layout, format!("{}: {}", nombre_severidad(p.severidad), p.mensaje));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problema(mostrada: &str, linea: usize, columna: usize, severidad: Severidad) -> Problema {
        Problema {
            ruta: PathBuf::from(mostrada),
            mostrada: mostrada.to_string(),
            linea,
            columna,
            caracter: columna as u32,
            severidad,
            mensaje: String::new(),
        }
    }

    #[test]
    fn se_ordenan_por_archivo_y_posicion() {
        let mut v = vec![
            problema("b.py", 1, 0, Severidad::Error),
            problema("a.py", 9, 2, Severidad::Advertencia),
            problema("a.py", 9, 1, Severidad::Error),
            problema("a.py", 2, 5, Severidad::Sugerencia),
        ];
        ordenar(&mut v);
        let orden: Vec<_> = v.iter().map(|p| (p.mostrada.as_str(), p.linea, p.columna)).collect();
        assert_eq!(orden, [("a.py", 2, 5), ("a.py", 9, 1), ("a.py", 9, 2), ("b.py", 1, 0)]);
    }

    #[test]
    fn el_titulo_cuenta_solo_lo_que_hay() {
        let v = vec![
            problema("a", 0, 0, Severidad::Error),
            problema("a", 1, 0, Severidad::Error),
            problema("a", 2, 0, Severidad::Advertencia),
        ];
        assert_eq!(titulo(&v), "Problemas (2 errores, 1 aviso)");
        assert_eq!(titulo(&v[2..]), "Problemas (1 aviso)");
    }

    #[test]
    fn f8_avanza_retrocede_y_da_la_vuelta() {
        let v = vec![
            problema("a.py", 2, 0, Severidad::Error),
            problema("a.py", 9, 4, Severidad::Error),
            problema("b.py", 1, 0, Severidad::Error),
        ];
        // Desde antes del primero, entre dos, justo encima de uno.
        assert_eq!(indice_destino(&v, Some(("a.py", 0, 0)), true), 0);
        assert_eq!(indice_destino(&v, Some(("a.py", 5, 0)), true), 1);
        assert_eq!(indice_destino(&v, Some(("a.py", 9, 4)), true), 2);
        assert_eq!(indice_destino(&v, Some(("a.py", 9, 4)), false), 0);
        // Pasa al archivo siguiente, y del último vuelve al primero.
        assert_eq!(indice_destino(&v, Some(("a.py", 20, 0)), true), 2);
        assert_eq!(indice_destino(&v, Some(("b.py", 1, 0)), true), 0);
        assert_eq!(indice_destino(&v, Some(("a.py", 2, 0)), false), 2);
        // Desde un archivo sin problemas: el de más adelante en el orden.
        assert_eq!(indice_destino(&v, Some(("c.py", 0, 0)), true), 0);
        assert_eq!(indice_destino(&v, Some(("a0.py", 0, 0)), true), 2);
        assert_eq!(indice_destino(&v, None, false), 2);
    }
}
