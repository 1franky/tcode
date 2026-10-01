//! Snippets con saltos entre campos (BACKLOG.md P2 #24): la sintaxis de
//! snippets de LSP (la misma de VSCode/TextMate) y los campos que se
//! recorren con `Tab` después de insertarlo.
//!
//! - `$1`, `${1}`: un campo vacío; `${1:valor}`: con un valor que queda
//!   seleccionado (puede tener campos adentro: `${1:foo(${2:x})}`).
//! - El mismo número en varios lugares son espejos: se editan juntos (el
//!   editor los selecciona como varios cursores). Un `$1` sin valor toma
//!   el del primer `${1:valor}`.
//! - `${1|uno,dos|}`: opciones — se inserta la primera.
//! - `$0`: dónde termina el cursor (si no está, al final del snippet).
//! - `$NOMBRE`, `${NOMBRE}`, `${NOMBRE:defecto}`: variables (ver
//!   [`parsear`]); las transformaciones (`${NOMBRE/regex/formato/}`) se
//!   ignoran y queda el valor sin transformar.
//! - `\$`, `\}`, `\\` (y `\,`, `\|` en opciones) insertan el carácter.

use std::collections::BTreeMap;
use std::ops::Range;

/// Un snippet ya expandido: el texto a insertar y, por cada número de
/// campo, sus rangos de bytes dentro de `texto` (en el orden en que
/// aparecen).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub texto: String,
    pub campos: BTreeMap<u32, Vec<Range<usize>>>,
}

impl Snippet {
    /// Los campos en el orden en que se recorren con `Tab`: 1, 2, 3... y
    /// el `$0` al final (si no hay, uno vacío al final del texto).
    pub fn campos_en_orden(&self) -> Vec<Vec<Range<usize>>> {
        let mut orden: Vec<Vec<Range<usize>>> =
            self.campos.iter().filter(|(n, _)| **n != 0).map(|(_, r)| r.clone()).collect();
        let fin = self.texto.len();
        orden.push(self.campos.get(&0).cloned().unwrap_or_else(|| std::iter::once(fin..fin).collect()));
        orden
    }
}

struct Parser<'a, F: Fn(&str) -> Option<String>> {
    caracteres: std::iter::Peekable<std::str::Chars<'a>>,
    salida: String,
    campos: BTreeMap<u32, Vec<Range<usize>>>,
    /// El valor de cada campo con valor, para los espejos sin valor.
    valores: BTreeMap<u32, String>,
    variable: F,
}

impl<F: Fn(&str) -> Option<String>> Parser<'_, F> {
    /// Procesa hasta el final o, si `dentro`, hasta el `}` que cierra.
    fn procesar(&mut self, dentro: bool) {
        while let Some(c) = self.caracteres.next() {
            match c {
                '\\' => match self.caracteres.peek() {
                    Some('$' | '}' | '\\') => {
                        let c = self.caracteres.next().unwrap_or('\\');
                        self.salida.push(c);
                    }
                    _ => self.salida.push('\\'),
                },
                '}' if dentro => return,
                '$' => self.dolar(),
                _ => self.salida.push(c),
            }
        }
    }

    fn numero(&mut self) -> Option<u32> {
        let mut texto = String::new();
        while let Some(d) = self.caracteres.peek().filter(|d| d.is_ascii_digit()) {
            texto.push(*d);
            self.caracteres.next();
        }
        texto.parse().ok()
    }

    fn nombre(&mut self) -> String {
        let mut texto = String::new();
        while let Some(d) = self.caracteres.peek().filter(|d| d.is_ascii_alphanumeric() || **d == '_') {
            texto.push(*d);
            self.caracteres.next();
        }
        texto
    }

    fn agregar_campo(&mut self, numero: u32, inicio: usize) {
        let fin = self.salida.len();
        self.campos.entry(numero).or_default().push(inicio..fin);
        if fin > inicio {
            self.valores.entry(numero).or_insert_with(|| self.salida[inicio..fin].to_string());
        }
    }

    fn dolar(&mut self) {
        match self.caracteres.peek().copied() {
            Some(d) if d.is_ascii_digit() => {
                let numero = self.numero().unwrap_or(0);
                let inicio = self.salida.len();
                if let Some(valor) = self.valores.get(&numero).cloned() {
                    self.salida.push_str(&valor);
                }
                self.agregar_campo(numero, inicio);
            }
            Some(d) if d.is_ascii_alphabetic() || d == '_' => {
                let nombre = self.nombre();
                if let Some(valor) = (self.variable)(&nombre) {
                    self.salida.push_str(&valor);
                }
            }
            Some('{') => {
                self.caracteres.next();
                if self.caracteres.peek().is_some_and(char::is_ascii_digit) {
                    let numero = self.numero().unwrap_or(0);
                    let inicio = self.salida.len();
                    match self.caracteres.next() {
                        Some(':') => self.procesar(true),
                        Some('|') => self.opciones(),
                        // `${1}` o algo mal formado: campo vacío (o espejo).
                        _ => {
                            if let Some(valor) = self.valores.get(&numero).cloned() {
                                self.salida.push_str(&valor);
                            }
                        }
                    }
                    self.agregar_campo(numero, inicio);
                } else {
                    let nombre = self.nombre();
                    let valor = (self.variable)(&nombre);
                    match self.caracteres.next() {
                        Some(':') => {
                            // El defecto se procesa igual (puede tener
                            // campos); si la variable tiene valor, se
                            // descarta lo que generó.
                            let inicio = self.salida.len();
                            let campos_antes = self.campos.clone();
                            self.procesar(true);
                            if let Some(valor) = valor {
                                self.salida.truncate(inicio);
                                self.campos = campos_antes;
                                self.salida.push_str(&valor);
                            }
                        }
                        Some('/') => {
                            // Transformación: se saltea hasta el `}` que la
                            // cierra (el formato puede tener `${1:/upcase}`).
                            let (mut escapado, mut profundidad) = (false, 0usize);
                            for d in self.caracteres.by_ref() {
                                match d {
                                    '\\' if !escapado => escapado = true,
                                    '{' if !escapado => profundidad += 1,
                                    '}' if !escapado && profundidad == 0 => break,
                                    '}' if !escapado => profundidad -= 1,
                                    _ => escapado = false,
                                }
                                if d != '\\' {
                                    escapado = false;
                                }
                            }
                            self.salida.push_str(&valor.unwrap_or_default());
                        }
                        _ => self.salida.push_str(&valor.unwrap_or_default()),
                    }
                }
            }
            _ => self.salida.push('$'),
        }
    }

    /// `${1|uno,dos|}` (ya consumido hasta el primer `|`): inserta la
    /// primera opción.
    fn opciones(&mut self) {
        let mut primera = true;
        while let Some(c) = self.caracteres.next() {
            match c {
                '\\' => {
                    if let Some(d) = self.caracteres.next() {
                        if primera {
                            self.salida.push(d);
                        }
                    }
                }
                ',' => primera = false,
                '|' => {
                    if self.caracteres.peek() == Some(&'}') {
                        self.caracteres.next();
                    }
                    return;
                }
                _ if primera => self.salida.push(c),
                _ => {}
            }
        }
    }
}

/// Expande `fuente` (sintaxis de snippets de LSP). `variable` da el valor
/// de una variable (`TM_FILENAME`, `TM_SELECTED_TEXT`...): `None` si no
/// se conoce, y entonces se usa su valor por defecto o queda vacía.
pub fn parsear(fuente: &str, variable: impl Fn(&str) -> Option<String>) -> Snippet {
    let mut parser = Parser {
        caracteres: fuente.chars().peekable(),
        salida: String::with_capacity(fuente.len()),
        campos: BTreeMap::new(),
        valores: BTreeMap::new(),
        variable,
    };
    parser.procesar(false);
    Snippet { texto: parser.salida, campos: parser.campos }
}

/// Ajusta el snippet para insertarlo en una línea con sangría `sangria`:
/// cada `\t` se reemplaza por `tab` (la indentación del archivo) y cada
/// línea después de la primera empieza con `sangria`. Los rangos de los
/// campos se corren en consecuencia.
pub fn adaptar_sangria(snippet: &Snippet, sangria: &str, tab: &str) -> Snippet {
    let mut texto = String::with_capacity(snippet.texto.len());
    // nuevo[i] = posición en el texto nuevo del byte i del viejo.
    let mut nuevo = Vec::with_capacity(snippet.texto.len() + 1);
    for c in snippet.texto.chars() {
        for _ in 0..c.len_utf8() {
            nuevo.push(texto.len());
        }
        match c {
            '\t' => texto.push_str(tab),
            '\n' => {
                texto.push('\n');
                texto.push_str(sangria);
            }
            _ => texto.push(c),
        }
    }
    nuevo.push(texto.len());
    // Un campo que empieza justo después de un salto de línea empieza
    // después de la sangría agregada, no antes.
    let inicio = |i: usize| {
        if i > 0 && snippet.texto.as_bytes()[i - 1] == b'\n' {
            nuevo[i - 1] + 1 + sangria.len()
        } else {
            nuevo[i]
        }
    };
    let campos = snippet
        .campos
        .iter()
        .map(|(n, rangos)| (*n, rangos.iter().map(|r| inicio(r.start)..inicio(r.end)).collect()))
        .collect();
    Snippet { texto, campos }
}

/// Un snippet insertado cuyos campos se están recorriendo (ver
/// `Editor::insertar_snippet`): los rangos de bytes en el buffer de cada
/// campo, en el orden de `Tab` (el último es el `$0`), y cuál es el
/// actual. Los rangos se corren con cada edición del buffer
/// ([`SesionSnippet::ajustar`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SesionSnippet {
    pub campos: Vec<Vec<Range<usize>>>,
    pub actual: usize,
}

impl SesionSnippet {
    /// Corre los rangos por una edición que reemplazó `[a, b)` por `n`
    /// bytes. Escribir en el campo actual (también justo en sus bordes)
    /// lo agranda; en otro campo, solo si cae estrictamente adentro (así
    /// dos campos pegados no crecen los dos). Un campo que la edición tapó
    /// entero (escribir sobre un valor con campos anidados) desaparece.
    pub fn ajustar(&mut self, a: usize, b: usize, n: usize) {
        let delta = n as isize - (b - a) as isize;
        let correr = |p: usize| (p as isize + delta).max(0) as usize;
        for (i, rangos) in self.campos.iter_mut().enumerate() {
            let actual = i == self.actual;
            rangos.retain_mut(|r| {
                let (s, e) = (r.start, r.end);
                if actual && a >= s && b <= e {
                    r.end = correr(e);
                } else if b <= s && !(actual && a == s) {
                    *r = correr(s)..correr(e);
                } else if a >= e {
                } else if a >= s && b <= e && !(a == s && b == e) {
                    r.end = correr(e);
                } else if a <= s && b >= e {
                    return false;
                } else {
                    // Se superponen en parte: queda lo que sobrevive.
                    *r = s.min(a)..correr(e.max(b)).max(a + n);
                }
                true
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sin_variables(fuente: &str) -> Snippet {
        parsear(fuente, |_| None)
    }

    fn textos(s: &Snippet) -> Vec<(u32, Vec<&str>)> {
        s.campos.iter().map(|(n, r)| (*n, r.iter().map(|r| &s.texto[r.clone()]).collect())).collect()
    }

    #[test]
    fn campos_valores_espejos_y_cero() {
        let s = sin_variables("fn ${1:nombre}(${2:a: i32}) -> $3 {\n\t$0\n}\n// $1");
        assert_eq!(s.texto, "fn nombre(a: i32) ->  {\n\t\n}\n// nombre");
        assert_eq!(textos(&s), [(0, vec![""]), (1, vec!["nombre", "nombre"]), (2, vec!["a: i32"]), (3, vec![""])]);
        let orden = s.campos_en_orden();
        assert_eq!(orden.len(), 4);
        assert_eq!(&s.texto[orden[0][0].clone()], "nombre");
        // Sin `$0`, uno vacío al final.
        let s = sin_variables("a${1:b}c");
        assert_eq!(s.campos_en_orden().last().unwrap().as_slice(), std::slice::from_ref(&(3..3)));
    }

    #[test]
    fn anidados_opciones_escapes_y_variables() {
        let s = sin_variables("${1:foo(${2:x})} ${3|uno,dos\\,tres|} \\$HOME \\} \\\\ $ 5");
        assert_eq!(s.texto, "foo(x) uno $HOME } \\ $ 5");
        assert_eq!(textos(&s), [(1, vec!["foo(x)"]), (2, vec!["x"]), (3, vec!["uno"])]);

        let variables = |v: &str| (v == "TM_FILENAME").then(|| "main.rs".to_string());
        let s = parsear("// $TM_FILENAME ${TM_FILENAME} ${NADA:por ${1:defecto}} ${TM_FILENAME:x} ${TM_FILENAME/(.*)/${1:/upcase}/}", variables);
        assert_eq!(s.texto, "// main.rs main.rs por defecto main.rs main.rs");
        assert_eq!(textos(&s), [(1, vec!["defecto"])]);
    }

    #[test]
    fn sangria_y_tabs() {
        let s = sin_variables("if $1 {\n\t${2:cuerpo}\n}$0");
        let a = adaptar_sangria(&s, "    ", "  ");
        assert_eq!(a.texto, "if  {\n      cuerpo\n    }");
        assert_eq!(textos(&a), [(0, vec![""]), (1, vec![""]), (2, vec!["cuerpo"])]);
        assert_eq!(a.campos[&0], vec![a.texto.len()..a.texto.len()]);
        // Un campo al principio de una línea queda después de la sangría.
        let s = sin_variables("a\n$1b");
        let a = adaptar_sangria(&s, "  ", "\t");
        assert_eq!(a.texto, "a\n  b");
        assert_eq!(a.campos[&1], vec![4..4]);
    }

    #[test]
    fn ajustar_escribiendo_en_campos() {
        // "f(a, b)": campo 1 = "a" [2,3), campo 2 = "b" [5,6), $0 al final.
        let mut sesion = SesionSnippet { campos: vec![vec![2..3], vec![5..6], vec![7..7]], actual: 0 };
        // Reemplazar "a" por "xy": crece el campo actual, se corren los demás.
        sesion.ajustar(2, 3, 2);
        assert_eq!(sesion.campos, vec![vec![2..4], vec![6..7], vec![8..8]]);
        // Seguir escribiendo al final del campo actual lo agranda.
        sesion.ajustar(4, 4, 1);
        assert_eq!(sesion.campos, vec![vec![2..5], vec![7..8], vec![9..9]]);
        // Una edición antes de todo corre todo.
        sesion.ajustar(0, 0, 3);
        assert_eq!(sesion.campos, vec![vec![5..8], vec![10..11], vec![12..12]]);
    }

    #[test]
    fn campos_pegados_y_anidados() {
        // "ab": campo 1 = "a" [0,1), campo 2 = "b" [1,2). Escribir al final
        // del 1 (actual) no agranda el 2: lo corre.
        let mut sesion = SesionSnippet { campos: vec![vec![0..1], vec![1..2], vec![2..2]], actual: 0 };
        sesion.ajustar(1, 1, 1);
        assert_eq!(sesion.campos, vec![vec![0..2], vec![2..3], vec![3..3]]);
        // "foo(x)": 1 = [0,6), 2 = "x" [4,5). Reemplazar el 1 entero borra
        // el 2, que estaba adentro.
        let mut sesion = SesionSnippet { campos: vec![vec![0..6], vec![4..5], vec![6..6]], actual: 0 };
        sesion.ajustar(0, 6, 1);
        assert_eq!(sesion.campos, vec![vec![0..1], vec![], vec![1..1]]);
        // Con el 2 actual, escribir en él agranda también el 1, que lo
        // contiene.
        let mut sesion = SesionSnippet { campos: vec![vec![0..6], vec![4..5], vec![6..6]], actual: 1 };
        sesion.ajustar(4, 5, 3);
        assert_eq!(sesion.campos, vec![vec![0..8], vec![4..7], vec![8..8]]);
    }
}
