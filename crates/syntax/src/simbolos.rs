//! Jerarquía de símbolos que contienen una posición del documento, para
//! los breadcrumbs de arriba del código (BACKLOG.md P3 #10, PLAN.md §5
//! "Interfaz"): `impl Editor > fn insertar_texto`, `class A > def b`...
//! Y el esquema del archivo entero ([`esquema`]) para el selector "Ir a
//! símbolo" (`Ctrl+K .`), con los mismos criterios.
//!
//! Sale del mismo árbol de tree-sitter que el resaltador ya mantiene de
//! forma incremental (ver `Resaltador::simbolos_en`): se baja al nodo más
//! chico que contiene la posición y se suben sus ancestros quedándose con
//! los "contenedores con nombre" — funciones, métodos, clases, structs,
//! `impl`, traits, interfaces, módulos, namespaces y, en Markdown, las
//! secciones de encabezados. En vez de un `tags.scm`/`locals.scm` por
//! lenguaje (la mayoría de las gramáticas embebidas no lo trae, y correr
//! una query cuesta más que mirar unos pocos ancestros), una tabla corta
//! de tipos de nodo por lenguaje ([`regla_para`]). Lenguajes sin reglas
//! (HTML, CSS, SQL) no devuelven símbolos: el breadcrumb muestra solo la
//! ruta.
//!
//! El costo es O(profundidad del árbol) por consulta, sin importar el
//! largo del archivo, y el resaltador además la cachea por revisión del
//! texto + posición, así que en un frame sin cambios no se recorre nada.

use tree_sitter::{Node, Point, Tree};

use crate::lenguaje::Lenguaje;

/// Un contenedor con nombre de la jerarquía del cursor. `tipo` es la
/// palabra clave del lenguaje que lo declara (`fn`, `impl`, `class`,
/// `def`, `func`, `#`...) o vacío si el lenguaje no usa ninguna (métodos
/// de JS/Java/C#/C++, cuyo `nombre` ya lleva `()` para distinguirlos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Simbolo {
    pub tipo: &'static str,
    pub nombre: String,
}

impl Simbolo {
    /// Texto tal como se muestra en el breadcrumb (`fn insertar_texto`).
    pub fn etiqueta(&self) -> String {
        if self.tipo.is_empty() {
            self.nombre.clone()
        } else {
            format!("{} {}", self.tipo, self.nombre)
        }
    }
}

/// Largo máximo (en caracteres) del nombre de un símbolo: un `impl` con
/// genéricos largos o un encabezado de Markdown de un párrafo entero no
/// deberían comerse todo el breadcrumb — igual la UI recorta al ancho.
const MAX_NOMBRE: usize = 48;

/// De dónde sale el nombre de un tipo de nodo contenedor.
#[derive(Clone, Copy)]
enum Nombre {
    /// El campo `name` de la gramática (la gran mayoría).
    Campo,
    /// Igual que `Campo`, pero es un método/función sin palabra clave:
    /// se le agrega `()` al nombre.
    Metodo,
    /// El primer hijo con nombre de alguno de estos tipos (Kotlin: su
    /// gramática no define campos).
    PrimerHijo(&'static [&'static str]),
    /// `impl Trait for Tipo` / `impl Tipo` de Rust.
    ImplRust,
    /// Método de Go: `Receptor.Nombre`.
    MetodoGo,
    /// Función de C/C++: el nombre está al fondo de la cadena de
    /// declaradores (`*f(...)`, `Clase::metodo(...)`).
    DeclaradorC,
    /// `struct`/`class`/`enum`/`union` de C/C++: solo si tiene cuerpo
    /// (si no, es un uso del tipo, p. ej. `struct Foo x;`).
    EspecificadorC,
    /// `const f = () => {...}` de JS/TS: solo si el valor es una función.
    VariableFuncion,
    /// Sección de Markdown: el texto de su encabezado.
    SeccionMarkdown,
}

/// Regla para un tipo de nodo contenedor: la palabra clave que se muestra
/// antes del nombre y de dónde sale el nombre.
type Regla = (&'static str, Nombre);

/// Si `tipo_nodo` es un contenedor con nombre en `lenguaje`, cómo
/// mostrarlo.
fn regla_para(lenguaje: Lenguaje, tipo_nodo: &str) -> Option<Regla> {
    use Nombre::*;
    let regla = match (lenguaje, tipo_nodo) {
        (Lenguaje::Rust, "function_item" | "function_signature_item") => ("fn", Campo),
        (Lenguaje::Rust, "impl_item") => ("impl", ImplRust),
        (Lenguaje::Rust, "trait_item") => ("trait", Campo),
        (Lenguaje::Rust, "struct_item") => ("struct", Campo),
        (Lenguaje::Rust, "enum_item") => ("enum", Campo),
        (Lenguaje::Rust, "union_item") => ("union", Campo),
        (Lenguaje::Rust, "mod_item") => ("mod", Campo),
        (Lenguaje::Rust, "macro_definition") => ("macro_rules!", Campo),

        (Lenguaje::Python, "function_definition") => ("def", Campo),
        (Lenguaje::Python, "class_definition") => ("class", Campo),

        (Lenguaje::JavaScript | Lenguaje::TypeScript, tipo) => match tipo {
            "function_declaration" | "generator_function_declaration" | "function_signature" => ("function", Campo),
            "class_declaration" | "abstract_class_declaration" => ("class", Campo),
            "method_definition" | "method_signature" | "abstract_method_signature" => ("", Metodo),
            "variable_declarator" => ("", VariableFuncion),
            "interface_declaration" => ("interface", Campo),
            "enum_declaration" => ("enum", Campo),
            "type_alias_declaration" => ("type", Campo),
            "internal_module" => ("namespace", Campo),
            "module" => ("module", Campo),
            _ => return None,
        },

        (Lenguaje::Go, "function_declaration") => ("func", Campo),
        (Lenguaje::Go, "method_declaration") => ("func", MetodoGo),
        (Lenguaje::Go, "type_spec") => ("type", Campo),

        (Lenguaje::Java, tipo) => match tipo {
            "class_declaration" => ("class", Campo),
            "interface_declaration" => ("interface", Campo),
            "enum_declaration" => ("enum", Campo),
            "record_declaration" => ("record", Campo),
            "annotation_type_declaration" => ("@interface", Campo),
            "method_declaration" | "constructor_declaration" => ("", Metodo),
            _ => return None,
        },

        (Lenguaje::CSharp, tipo) => match tipo {
            "class_declaration" => ("class", Campo),
            "struct_declaration" => ("struct", Campo),
            "interface_declaration" => ("interface", Campo),
            "enum_declaration" => ("enum", Campo),
            "record_declaration" => ("record", Campo),
            "namespace_declaration" | "file_scoped_namespace_declaration" => ("namespace", Campo),
            "method_declaration" | "constructor_declaration" => ("", Metodo),
            _ => return None,
        },

        (Lenguaje::Kotlin, "class_declaration") => ("class", PrimerHijo(&["type_identifier"])),
        (Lenguaje::Kotlin, "object_declaration") => ("object", PrimerHijo(&["type_identifier"])),
        (Lenguaje::Kotlin, "function_declaration") => ("fun", PrimerHijo(&["simple_identifier"])),

        (Lenguaje::Ruby, "method" | "singleton_method") => ("def", Campo),
        (Lenguaje::Ruby, "class") => ("class", Campo),
        (Lenguaje::Ruby, "module") => ("module", Campo),

        (Lenguaje::Php, tipo) => match tipo {
            "class_declaration" => ("class", Campo),
            "interface_declaration" => ("interface", Campo),
            "trait_declaration" => ("trait", Campo),
            "enum_declaration" => ("enum", Campo),
            "namespace_definition" => ("namespace", Campo),
            "function_definition" | "method_declaration" => ("function", Campo),
            _ => return None,
        },

        (Lenguaje::C | Lenguaje::Cpp, tipo) => match tipo {
            "function_definition" => ("", DeclaradorC),
            "struct_specifier" => ("struct", EspecificadorC),
            "union_specifier" => ("union", EspecificadorC),
            "enum_specifier" => ("enum", EspecificadorC),
            "class_specifier" if lenguaje == Lenguaje::Cpp => ("class", EspecificadorC),
            "namespace_definition" if lenguaje == Lenguaje::Cpp => ("namespace", Campo),
            _ => return None,
        },

        (Lenguaje::Markdown, "section") => ("", SeccionMarkdown),

        _ => return None,
    };
    Some(regla)
}

/// Los contenedores con nombre que encierran `punto` (fila y columna en
/// bytes, como las usa tree-sitter), del más externo al más interno.
pub(crate) fn simbolos_en(arbol: &Tree, lenguaje: Lenguaje, fuente: &str, punto: Point) -> Vec<Simbolo> {
    let Some(mut nodo) = arbol.root_node().descendant_for_point_range(punto, punto) else {
        return Vec::new();
    };
    let mut simbolos = Vec::new();
    loop {
        if let Some(simbolo) = simbolo_de(nodo, lenguaje, fuente) {
            simbolos.push(simbolo);
        }
        match nodo.parent() {
            Some(padre) => nodo = padre,
            None => break,
        }
    }
    simbolos.reverse();
    simbolos
}

/// Un símbolo del esquema (outline) de un documento entero, para el
/// selector de símbolos (`Ctrl+K .`): el [`Simbolo`], cuántos
/// contenedores con nombre lo encierran (`profundidad`, 0 = de primer
/// nivel) y el rango de bytes de su nodo (`inicio` es donde se salta;
/// `fin` sirve para saber si contiene al cursor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimboloEsquema {
    pub simbolo: Simbolo,
    pub profundidad: usize,
    pub inicio: usize,
    pub fin: usize,
}

/// Todos los contenedores con nombre del árbol, en orden de aparición
/// (recorrido en preorden), con su anidamiento. O(nodos del árbol): se
/// llama solo al abrir el selector, nunca por frame.
pub(crate) fn esquema(arbol: &Tree, lenguaje: Lenguaje, fuente: &str) -> Vec<SimboloEsquema> {
    let mut resultado = Vec::new();
    let mut cursor = arbol.walk();
    // Por cada nivel del recorrido, si el nodo de ese nivel era un
    // símbolo: la profundidad de un símbolo es cuántos `true` hay arriba.
    let mut pila: Vec<bool> = Vec::new();
    loop {
        let nodo = cursor.node();
        let simbolo = simbolo_de(nodo, lenguaje, fuente);
        let es_simbolo = simbolo.is_some();
        if let Some(simbolo) = simbolo {
            let profundidad = pila.iter().filter(|&&b| b).count();
            resultado.push(SimboloEsquema { simbolo, profundidad, inicio: nodo.start_byte(), fin: nodo.end_byte() });
        }
        if cursor.goto_first_child() {
            pila.push(es_simbolo);
            continue;
        }
        // Sin hijos: al hermano siguiente, o subiendo hasta encontrar uno.
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return resultado;
            }
            pila.pop();
        }
    }
}

fn simbolo_de(nodo: Node, lenguaje: Lenguaje, fuente: &str) -> Option<Simbolo> {
    let (tipo, regla) = regla_para(lenguaje, nodo.kind())?;
    let texto = |n: Node| fuente.get(n.start_byte()..n.end_byte()).unwrap_or("");
    let campo = |n: Node, nombre: &str| n.child_by_field_name(nombre).map(texto);
    let (tipo, nombre) = match regla {
        Nombre::Campo => (tipo, campo(nodo, "name")?.to_string()),
        Nombre::Metodo => (tipo, format!("{}()", campo(nodo, "name")?)),
        Nombre::PrimerHijo(tipos) => {
            let mut cursor = nodo.walk();
            let hijo = nodo.named_children(&mut cursor).find(|h| tipos.contains(&h.kind()))?;
            (tipo, texto(hijo).to_string())
        }
        Nombre::ImplRust => {
            let tipo_impl = campo(nodo, "type")?;
            let nombre = match campo(nodo, "trait") {
                Some(rasgo) => format!("{rasgo} for {tipo_impl}"),
                None => tipo_impl.to_string(),
            };
            (tipo, nombre)
        }
        Nombre::MetodoGo => {
            let nombre = campo(nodo, "name")?;
            // `(e *Editor)` -> `Editor`: el tipo del (único) parámetro
            // del receptor, sin el `*` de puntero.
            let receptor = nodo.child_by_field_name("receiver").and_then(|lista| {
                let mut cursor = lista.walk();
                let parametro = lista.named_children(&mut cursor).find(|p| p.kind() == "parameter_declaration");
                parametro.and_then(|p| campo(p, "type"))
            });
            match receptor {
                Some(r) => (tipo, format!("{}.{nombre}", r.trim_start_matches('*'))),
                None => (tipo, nombre.to_string()),
            }
        }
        Nombre::DeclaradorC => {
            let mut declarador = nodo.child_by_field_name("declarator")?;
            // Se baja por `declarator` (puntero, función, referencia...)
            // hasta el identificador; `reference_declarator` de C++ no
            // usa el campo, su declarador es el último hijo con nombre.
            loop {
                let siguiente = declarador.child_by_field_name("declarator").or_else(|| {
                    (declarador.kind() == "reference_declarator")
                        .then(|| declarador.named_child(declarador.named_child_count().saturating_sub(1) as u32))
                        .flatten()
                });
                match siguiente {
                    Some(s) => declarador = s,
                    None => break,
                }
            }
            (tipo, format!("{}()", texto(declarador)))
        }
        Nombre::EspecificadorC => {
            nodo.child_by_field_name("body")?;
            (tipo, campo(nodo, "name").unwrap_or("(anónimo)").to_string())
        }
        Nombre::VariableFuncion => {
            let valor = nodo.child_by_field_name("value")?;
            if !matches!(valor.kind(), "arrow_function" | "function_expression" | "function" | "generator_function") {
                return None;
            }
            (tipo, format!("{}()", campo(nodo, "name")?))
        }
        Nombre::SeccionMarkdown => {
            let encabezado = nodo.named_child(0).filter(|h| matches!(h.kind(), "atx_heading" | "setext_heading"))?;
            let nivel = (0..encabezado.child_count())
                .filter_map(|i| encabezado.child(i))
                .find_map(|h| match h.kind() {
                    "atx_h1_marker" | "setext_h1_underline" => Some("#"),
                    "atx_h2_marker" | "setext_h2_underline" => Some("##"),
                    "atx_h3_marker" => Some("###"),
                    "atx_h4_marker" => Some("####"),
                    "atx_h5_marker" => Some("#####"),
                    "atx_h6_marker" => Some("######"),
                    _ => None,
                })
                .unwrap_or("#");
            (nivel, campo(encabezado, "heading_content")?.to_string())
        }
    };
    let nombre = normalizar(&nombre);
    (!nombre.is_empty()).then_some(Simbolo { tipo, nombre })
}

/// Una sola línea (genéricos o parámetros partidos en varias líneas),
/// sin espacios repetidos, y recortado a [`MAX_NOMBRE`] caracteres.
fn normalizar(nombre: &str) -> String {
    let mut limpio = nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if limpio.chars().count() > MAX_NOMBRE {
        limpio = limpio.chars().take(MAX_NOMBRE - 2).collect::<String>() + "..";
    }
    limpio
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Resaltador;

    /// Etiquetas de los símbolos en la posición marcada con `|` dentro de
    /// `fuente` (el `|` se saca antes de parsear).
    fn en_marca(lenguaje: Lenguaje, fuente_con_marca: &str) -> Vec<String> {
        let byte = fuente_con_marca.find('|').expect("falta la marca |");
        let fuente = fuente_con_marca.replacen('|', "", 1);
        let antes = &fuente[..byte];
        let fila = antes.matches('\n').count();
        let columna = byte - antes.rfind('\n').map_or(0, |i| i + 1);
        let mut resaltador = Resaltador::nuevo();
        resaltador
            .simbolos_en("prueba", lenguaje, None, || fuente.clone(), fila, columna)
            .iter()
            .map(Simbolo::etiqueta)
            .collect()
    }

    #[test]
    fn rust_impl_y_fn_anidados() {
        let fuente = "mod editor {\n    struct Editor;\n    impl Editor {\n        fn insertar_texto(&mut self) {\n            let x = |1;\n        }\n    }\n}\n";
        assert_eq!(en_marca(Lenguaje::Rust, fuente), ["mod editor", "impl Editor", "fn insertar_texto"]);
    }

    #[test]
    fn rust_impl_de_trait_y_struct() {
        let fuente = "impl<T> Display for Caja<T> {\n    fn fmt(&self) {| }\n}\n";
        assert_eq!(en_marca(Lenguaje::Rust, fuente), ["impl Display for Caja<T>", "fn fmt"]);
        let fuente = "struct Punto {\n    x: |i32,\n}\n";
        assert_eq!(en_marca(Lenguaje::Rust, fuente), ["struct Punto"]);
    }

    #[test]
    fn fuera_de_todo_contenedor_no_hay_simbolos() {
        let fuente = "fn a() {}\n|\nfn b() {}\n";
        assert!(en_marca(Lenguaje::Rust, fuente).is_empty());
    }

    #[test]
    fn python_class_y_def() {
        let fuente = "class Editor:\n    def insertar(self):\n        x = |1\n";
        assert_eq!(en_marca(Lenguaje::Python, fuente), ["class Editor", "def insertar"]);
        let fuente = "def externa():\n    def interna():\n        return |1\n";
        assert_eq!(en_marca(Lenguaje::Python, fuente), ["def externa", "def interna"]);
    }

    #[test]
    fn javascript_clase_metodo_y_funcion_flecha() {
        let fuente = "class Editor {\n  insertar(texto) {\n    return |texto;\n  }\n}\n";
        assert_eq!(en_marca(Lenguaje::JavaScript, fuente), ["class Editor", "insertar()"]);
        let fuente = "function principal() {\n  const ayudante = () => {\n    return |1;\n  };\n}\n";
        assert_eq!(en_marca(Lenguaje::JavaScript, fuente), ["function principal", "ayudante()"]);
        // Una variable común no es un contenedor.
        let fuente = "const x = {\n  a: |1,\n};\n";
        assert!(en_marca(Lenguaje::JavaScript, fuente).is_empty());
    }

    #[test]
    fn typescript_interfaz_namespace_y_clase() {
        let fuente = "namespace App {\n  export class Editor {\n    insertar(t: string): void {\n      |t;\n    }\n  }\n}\n";
        assert_eq!(en_marca(Lenguaje::TypeScript, fuente), ["namespace App", "class Editor", "insertar()"]);
        let fuente = "interface Punto {\n  x: |number;\n}\n";
        assert_eq!(en_marca(Lenguaje::TypeScript, fuente), ["interface Punto"]);
    }

    #[test]
    fn go_funcion_metodo_y_tipo() {
        let fuente = "package main\n\nfunc (e *Editor) Insertar() {\n\tx := |1\n}\n";
        assert_eq!(en_marca(Lenguaje::Go, fuente), ["func Editor.Insertar"]);
        let fuente = "package main\n\ntype Editor struct {\n\tx |int\n}\n";
        assert_eq!(en_marca(Lenguaje::Go, fuente), ["type Editor"]);
        let fuente = "package main\n\nfunc main() {\n\t|x()\n}\n";
        assert_eq!(en_marca(Lenguaje::Go, fuente), ["func main"]);
    }

    #[test]
    fn java_clase_anidada_y_metodo() {
        let fuente = "class Editor {\n  static class Cursor {\n    void mover() {\n      int x = |1;\n    }\n  }\n}\n";
        assert_eq!(en_marca(Lenguaje::Java, fuente), ["class Editor", "class Cursor", "mover()"]);
    }

    #[test]
    fn c_y_cpp_funciones_y_clases() {
        let fuente = "static int *crear(void) {\n    return |0;\n}\n";
        assert_eq!(en_marca(Lenguaje::C, fuente), ["crear()"]);
        let fuente = "namespace app {\nclass Editor {\n  void f() { |g(); }\n};\n}\n";
        assert_eq!(en_marca(Lenguaje::Cpp, fuente), ["namespace app", "class Editor", "f()"]);
        let fuente = "void Editor::insertar() {\n  |g();\n}\n";
        assert_eq!(en_marca(Lenguaje::Cpp, fuente), ["Editor::insertar()"]);
    }

    #[test]
    fn otros_lenguajes_con_reglas() {
        let fuente = "module App\n  class Editor\n    def insertar\n      |1\n    end\n  end\nend\n";
        assert_eq!(en_marca(Lenguaje::Ruby, fuente), ["module App", "class Editor", "def insertar"]);
        let fuente = "namespace App {\n  class Editor {\n    void Insertar() {\n      |x();\n    }\n  }\n}\n";
        assert_eq!(en_marca(Lenguaje::CSharp, fuente), ["namespace App", "class Editor", "Insertar()"]);
        let fuente = "<?php\nclass Editor {\n  function insertar() {\n    |f();\n  }\n}\n";
        assert_eq!(en_marca(Lenguaje::Php, fuente), ["class Editor", "function insertar"]);
        let fuente = "class Editor {\n  fun insertar() {\n    |f()\n  }\n}\n";
        assert_eq!(en_marca(Lenguaje::Kotlin, fuente), ["class Editor", "fun insertar"]);
    }

    #[test]
    fn markdown_encabezados_anidados() {
        let fuente = "# Manual\n\nIntro.\n\n## Atajos\n\nTexto |aca.\n";
        assert_eq!(en_marca(Lenguaje::Markdown, fuente), ["# Manual", "## Atajos"]);
    }

    #[test]
    fn lenguajes_sin_reglas_no_devuelven_simbolos() {
        let fuente = "body {\n  color: |red;\n}\n";
        assert!(en_marca(Lenguaje::Css, fuente).is_empty());
    }

    #[test]
    fn nombres_largos_o_de_varias_lineas_se_normalizan() {
        assert_eq!(normalizar("Caja<\n    T,\n    U>"), "Caja< T, U>");
        let largo = "x".repeat(100);
        assert_eq!(normalizar(&largo).chars().count(), MAX_NOMBRE);
        assert!(normalizar(&largo).ends_with(".."));
    }

    fn esquema_de(lenguaje: Lenguaje, fuente: &str) -> Vec<(usize, String)> {
        let mut resaltador = Resaltador::nuevo();
        resaltador
            .esquema("prueba", lenguaje, None, || fuente.to_string())
            .into_iter()
            .map(|s| (s.profundidad, s.simbolo.etiqueta()))
            .collect()
    }

    #[test]
    fn esquema_rust_en_orden_y_con_anidamiento() {
        let fuente = "struct A;
impl A {
    fn uno(&self) {
        fn interna() {}
    }
    fn dos(&self) {}
}
fn suelta() {}
";
        assert_eq!(
            esquema_de(Lenguaje::Rust, fuente),
            [
                (0, "struct A".to_string()),
                (0, "impl A".to_string()),
                (1, "fn uno".to_string()),
                (2, "fn interna".to_string()),
                (1, "fn dos".to_string()),
                (0, "fn suelta".to_string()),
            ]
        );
    }

    #[test]
    fn esquema_python_y_rangos_de_bytes() {
        let fuente = "class A:
    def b(self):
        pass

def c():
    pass
";
        assert_eq!(
            esquema_de(Lenguaje::Python, fuente),
            [(0, "class A".to_string()), (1, "def b".to_string()), (0, "def c".to_string())]
        );
        let mut resaltador = Resaltador::nuevo();
        let esquema = resaltador.esquema("p", Lenguaje::Python, None, || fuente.to_string());
        assert_eq!(esquema[1].inicio, fuente.find("def b").unwrap());
        assert!(esquema[0].fin >= fuente.find("pass").unwrap());
        assert_eq!(esquema[2].inicio, fuente.find("def c").unwrap());
    }

    #[test]
    fn esquema_de_un_lenguaje_sin_reglas_esta_vacio() {
        assert!(esquema_de(Lenguaje::Css, "body { color: red; }
").is_empty());
    }

    #[test]
    fn la_consulta_se_cachea_por_revision_y_posicion() {
        let mut resaltador = Resaltador::nuevo();
        let fuente = "fn a() {\n    1\n}\n".to_string();
        let primero = resaltador.simbolos_en("doc", Lenguaje::Rust, Some(1), || fuente.clone(), 1, 4);
        assert_eq!(primero, [Simbolo { tipo: "fn", nombre: "a".into() }]);
        // Misma revisión: ni siquiera pide el texto.
        let segundo = resaltador.simbolos_en("doc", Lenguaje::Rust, Some(1), || panic!("no debía pedir el texto"), 1, 4);
        assert_eq!(primero, segundo);
        // Revisión nueva: vuelve a mirar el árbol del texto nuevo.
        let nuevo = "fn b() {\n    1\n}\n".to_string();
        let tercero = resaltador.simbolos_en("doc", Lenguaje::Rust, Some(2), || nuevo.clone(), 1, 4);
        assert_eq!(tercero, [Simbolo { tipo: "fn", nombre: "b".into() }]);
    }
}
