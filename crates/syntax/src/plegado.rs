//! Rangos plegables de un documento (BACKLOG.md P2 #7, PLAN.md §4
//! "Plegado"): qué bloques de líneas se pueden plegar. Qué rangos ESTÁN
//! plegados lo guarda `tcode_core::Plegado`; esto solo calcula los
//! candidatos, a pedido (al plegar), nunca en cada frame.
//!
//! Dos fuentes:
//!
//! - El árbol de tree-sitter del documento (el mismo que el resaltador
//!   ya mantiene de forma incremental, ver
//!   `Resaltador::rangos_plegables`). En vez de un `folds.scm` por
//!   lenguaje (la mayoría de las gramáticas embebidas no lo trae), una
//!   regla genérica — cualquier nodo de varias líneas que abre y cierra
//!   con `{}`/`[]`/`()` (bloques, cuerpos de clase/impl, objetos,
//!   arrays, listas de argumentos...) — más una lista corta de tipos de
//!   nodo propios de cada lenguaje para lo que no usa llaves (cuerpos de
//!   Python, `def ... end` de Ruby, elementos HTML, comentarios de bloque
//!   y strings de varias líneas).
//! - Indentación ([`rangos_por_indentacion`]), para archivos sin
//!   gramática (texto plano, YAML, TOML...): una línea seguida de otras
//!   más indentadas es la cabecera de un bloque.
//!
//! Markdown queda sin plegado a propósito: su vista dividida
//! (`Ctrl+K V`) comparte el scroll con el preview asumiendo una fila por
//! línea lógica, y ocultar líneas lo desincronizaría.

use tree_sitter::{Node, Tree};

use crate::lenguaje::Lenguaje;

/// Un bloque que se puede plegar: la línea `inicio` queda visible (la
/// cabecera, p. ej. `fn f() {`) y se ocultan `inicio + 1 ..= fin`.
/// Siempre `fin > inicio`. La línea de cierre (`}`, `end`, `</div>`...)
/// NO entra en el rango: queda visible debajo de la cabecera plegada,
/// igual que en VSCode — así un `} else {` nunca queda escondido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RangoPlegable {
    pub inicio: usize,
    pub fin: usize,
}

/// Tipos de nodo plegables además de la regla genérica de llaves, por
/// lenguaje.
fn tipos_propios(lenguaje: Lenguaje) -> &'static [&'static str] {
    match lenguaje {
        Lenguaje::Rust => &["block_comment", "string_literal", "raw_string_literal"],
        Lenguaje::Python => &["block", "string", "comment"],
        Lenguaje::JavaScript | Lenguaje::TypeScript => &["comment", "template_string", "jsx_element"],
        Lenguaje::Ruby => &[
            "method",
            "singleton_method",
            "class",
            "module",
            "do_block",
            "if",
            "unless",
            "while",
            "until",
            "for",
            "case",
            "begin",
            "comment",
        ],
        Lenguaje::Html => &["element", "script_element", "style_element", "comment"],
        Lenguaje::Go
        | Lenguaje::Java
        | Lenguaje::C
        | Lenguaje::Cpp
        | Lenguaje::Kotlin
        | Lenguaje::CSharp
        | Lenguaje::Php
        | Lenguaje::Css
        | Lenguaje::Sql => &["comment", "block_comment", "multiline_comment"],
        Lenguaje::Markdown => &[],
    }
}

fn es_delimitado(nodo: Node) -> bool {
    let (Some(primero), Some(ultimo)) = (nodo.child(0), nodo.child(nodo.child_count().saturating_sub(1))) else {
        return false;
    };
    if nodo.child_count() < 2 || primero.is_named() || ultimo.is_named() {
        return false;
    }
    matches!((primero.kind(), ultimo.kind()), ("{", "}") | ("[", "]") | ("(", ")"))
}

/// Los rangos plegables de `arbol`, ordenados y sin repetidos.
pub(crate) fn rangos_de_arbol(arbol: &Tree, lenguaje: Lenguaje, fuente: &str) -> Vec<RangoPlegable> {
    if lenguaje == Lenguaje::Markdown {
        return Vec::new();
    }
    let tipos = tipos_propios(lenguaje);
    let lineas: Vec<&str> = fuente.split('\n').collect();
    let mut rangos = Vec::new();
    let mut cursor = arbol.walk();
    // Recorrido en profundidad sin recursión; los nodos de una sola
    // línea no pueden contener nada plegable, así que no se baja a sus
    // hijos (con un archivo de miles de líneas, la mayoría del árbol).
    'recorrido: loop {
        let nodo = cursor.node();
        let (fila_inicio, fila_fin) = (nodo.start_position().row, nodo.end_position().row);
        let varias_lineas = fila_fin > fila_inicio;
        if varias_lineas && (es_delimitado(nodo) || tipos.contains(&nodo.kind())) {
            // Un cuerpo de Python (`block`) arranca en la primera
            // sentencia, no en el `def`/`if`/`class` — la cabecera es la
            // línea donde arranca el nodo padre.
            let inicio = if lenguaje == Lenguaje::Python && nodo.kind() == "block" {
                nodo.parent().map_or(fila_inicio, |p| p.start_position().row)
            } else {
                fila_inicio
            };
            let fin = if cierra_sola_en_su_linea(&lineas, nodo) { fila_fin - 1 } else { fila_fin };
            if fin > inicio {
                rangos.push(RangoPlegable { inicio, fin });
            }
        }
        if varias_lineas && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'recorrido;
            }
            if !cursor.goto_parent() {
                break 'recorrido;
            }
        }
    }
    rangos.sort_unstable();
    rangos.dedup();
    rangos
}

/// Si en la última línea de `nodo` no hay nada más que su cierre (`}`,
/// `)`, `*/`, `"""`, `end`, `</div>`...) — o si el nodo termina en la
/// columna 0 (incluye el salto de línea final). En ese caso esa línea no
/// se pliega: queda visible como en VSCode.
fn cierra_sola_en_su_linea(lineas: &[&str], nodo: Node) -> bool {
    let fin = nodo.end_position();
    if fin.column == 0 {
        return true;
    }
    let Some(linea) = lineas.get(fin.row) else { return false };
    let Some(antes) = linea.get(..fin.column) else { return false };
    let cierre = antes.trim();
    cierre.chars().all(|c| !c.is_alphanumeric()) || cierre == "end" || cierre.starts_with("</")
}

/// Rangos por indentación, para archivos sin gramática: cada línea no
/// vacía seguida de líneas más indentadas que ella (las vacías del medio
/// no cortan el bloque, las del final no entran) es la cabecera de un
/// bloque. Un tab cuenta como 4 espacios.
pub fn rangos_por_indentacion(fuente: &str) -> Vec<RangoPlegable> {
    let sangrias: Vec<Option<usize>> = fuente
        .split('\n')
        .map(|l| {
            if l.trim().is_empty() {
                return None;
            }
            Some(l.chars().take_while(|c| c.is_whitespace()).map(|c| if c == '\t' { 4 } else { 1 }).sum())
        })
        .collect();
    let mut rangos = Vec::new();
    for (inicio, sangria) in sangrias.iter().enumerate() {
        let Some(sangria) = *sangria else { continue };
        let mut fin = inicio;
        for (i, otra) in sangrias.iter().enumerate().skip(inicio + 1) {
            match otra {
                None => continue,
                Some(s) if *s > sangria => fin = i,
                Some(_) => break,
            }
        }
        if fin > inicio {
            rangos.push(RangoPlegable { inicio, fin });
        }
    }
    rangos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Resaltador;

    fn rangos(lenguaje: Lenguaje, fuente: &str) -> Vec<(usize, usize)> {
        Resaltador::nuevo()
            .rangos_plegables("doc", Some(lenguaje), fuente)
            .into_iter()
            .map(|r| (r.inicio, r.fin))
            .collect()
    }

    #[test]
    fn rust_pliega_cuerpos_dejando_visible_la_llave_de_cierre() {
        let fuente = "fn a() {\n    if x {\n        y();\n    } else {\n        z();\n    }\n}\n\nstruct S {\n    c: u8,\n}\n";
        let r = rangos(Lenguaje::Rust, fuente);
        // Cuerpo de `a` (0..=5), el `if` (1..=2) y el `else` (3..=4) por
        // separado, y el struct (8..=9).
        assert!(r.contains(&(0, 5)), "{r:?}");
        assert!(r.contains(&(1, 2)), "{r:?}");
        assert!(r.contains(&(3, 4)), "{r:?}");
        assert!(r.contains(&(8, 9)), "{r:?}");
    }

    #[test]
    fn python_pliega_desde_la_cabecera_hasta_la_ultima_sentencia() {
        let fuente = "class A:\n    def f(self):\n        x = 1\n        return x\n\ndatos = [\n    1,\n    2,\n]\n";
        let r = rangos(Lenguaje::Python, fuente);
        assert!(r.contains(&(0, 3)), "{r:?}");
        assert!(r.contains(&(1, 3)), "{r:?}");
        assert!(r.contains(&(5, 7)), "{r:?}");
    }

    #[test]
    fn javascript_pliega_funciones_objetos_y_comentarios_de_bloque() {
        let fuente = "/*\n * doc\n */\nfunction f() {\n  return {\n    a: 1,\n  };\n}\n";
        let r = rangos(Lenguaje::JavaScript, fuente);
        assert!(r.contains(&(0, 1)), "{r:?}");
        assert!(r.contains(&(3, 6)), "{r:?}");
        assert!(r.contains(&(4, 5)), "{r:?}");
    }

    #[test]
    fn typescript_y_el_resto_de_lenguajes_con_llaves_usan_la_regla_generica() {
        let ts = "interface I {\n  a: string;\n}\n";
        assert!(rangos(Lenguaje::TypeScript, ts).contains(&(0, 1)));
        let go = "package main\nfunc main() {\n\tx := 1\n}\n";
        assert!(rangos(Lenguaje::Go, go).contains(&(1, 2)));
    }

    #[test]
    fn ruby_pliega_def_end_dejando_visible_el_end() {
        let fuente = "def saludar\n  puts 1\n  puts 2\nend\n";
        assert!(rangos(Lenguaje::Ruby, fuente).contains(&(0, 2)));
    }

    #[test]
    fn markdown_no_tiene_plegado() {
        assert!(rangos(Lenguaje::Markdown, "# T\n\n- a\n  - b\n").is_empty());
    }

    #[test]
    fn indentacion_ignora_lineas_vacias_del_medio_y_del_final() {
        let fuente = "a:\n  b: 1\n\n  c:\n    d: 2\n\ne: 3\n";
        let r: Vec<(usize, usize)> = rangos_por_indentacion(fuente).into_iter().map(|r| (r.inicio, r.fin)).collect();
        assert_eq!(r, vec![(0, 4), (3, 4)]);
    }

    #[test]
    fn todas_las_muestras_dan_rangos_validos() {
        let muestras = [
            (Lenguaje::Rust, include_str!("../../core/src/editor.rs")),
            (Lenguaje::Python, include_str!("../tests/muestras/muestra.py")),
            (Lenguaje::JavaScript, include_str!("../tests/muestras/muestra.js")),
            (Lenguaje::TypeScript, include_str!("../tests/muestras/muestra.ts")),
            (Lenguaje::Go, include_str!("../tests/muestras/muestra.go")),
            (Lenguaje::Java, include_str!("../tests/muestras/muestra.java")),
            (Lenguaje::C, include_str!("../tests/muestras/muestra.c")),
            (Lenguaje::Cpp, include_str!("../tests/muestras/muestra.cpp")),
            (Lenguaje::Kotlin, include_str!("../tests/muestras/muestra.kt")),
            (Lenguaje::CSharp, include_str!("../tests/muestras/muestra.cs")),
            (Lenguaje::Ruby, include_str!("../tests/muestras/muestra.rb")),
            (Lenguaje::Php, include_str!("../tests/muestras/muestra.php")),
            (Lenguaje::Html, include_str!("../tests/muestras/muestra.html")),
            (Lenguaje::Css, include_str!("../tests/muestras/muestra.css")),
        ];
        for (lenguaje, fuente) in muestras {
            let r = rangos(lenguaje, fuente);
            let num_lineas = fuente.split('\n').count();
            assert!(!r.is_empty(), "{lenguaje:?}: ningún rango plegable");
            assert!(r.iter().all(|(i, f)| f > i && *f < num_lineas), "{lenguaje:?}: {r:?}");
        }
    }
}
