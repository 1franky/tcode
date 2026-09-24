//! Línea de comandos `:` del modo VIM: el texto que se está escribiendo
//! (con historial por `↑`/`↓`) y el parseo de lo escrito a un
//! `ComandoLinea`. Ejecutarlo (guardar, cerrar la pestaña, abrir otro
//! archivo...) lo hace `app`, que es quien conoce paneles y pestañas.

use crate::buffer::Buffer;
use crate::busqueda::{buscar_coincidencias, OpcionesBusqueda};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComandoLinea {
    /// `:w`
    Guardar,
    /// `:q` (`forzar` con `:q!`)
    Cerrar { forzar: bool },
    /// `:wq` y `:x` (`solo_si_modificado` con `:x`)
    GuardarYCerrar { solo_si_modificado: bool },
    /// `:qa` / `:qa!`
    Salir { forzar: bool },
    /// `:{n}` (1-based, como se escribe)
    IrALinea(usize),
    /// `:e <ruta>`
    Abrir(String),
    /// `:s/patrón/reemplazo/[flags]` y `:%s/...`
    Sustituir(Sustitucion),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sustitucion {
    /// `%`: todo el archivo; si no, solo la línea del cursor.
    pub todo_el_archivo: bool,
    pub patron: String,
    pub reemplazo: String,
    /// `g`: todas las coincidencias de cada línea, no solo la primera.
    pub global: bool,
    /// `i`: sin distinguir mayúsculas.
    pub ignorar_mayusculas: bool,
}

/// Parsea lo escrito después de `:` (sin el `:`). `Err` con un mensaje
/// para la barra de estado si no es un comando soportado.
pub fn parsear(texto: &str) -> Result<ComandoLinea, String> {
    let texto = texto.trim();
    if texto.is_empty() {
        return Err(String::new());
    }
    if let Ok(n) = texto.parse::<usize>() {
        return Ok(ComandoLinea::IrALinea(n));
    }
    if let Some(resto) = texto.strip_prefix("%s") {
        return parsear_sustitucion(resto, true);
    }
    if let Some(resto) = texto.strip_prefix('s') {
        if resto.starts_with(|c: char| !c.is_alphanumeric() && !c.is_whitespace()) {
            return parsear_sustitucion(resto, false);
        }
    }
    let (nombre, argumento) = match texto.split_once(char::is_whitespace) {
        Some((n, a)) => (n, a.trim()),
        None => (texto, ""),
    };
    let sin_argumento = |c: ComandoLinea| if argumento.is_empty() { Ok(c) } else { Err(format!("{nombre}: no admite argumentos")) };
    match nombre {
        "w" | "write" => sin_argumento(ComandoLinea::Guardar),
        "q" | "quit" => sin_argumento(ComandoLinea::Cerrar { forzar: false }),
        "q!" | "quit!" => sin_argumento(ComandoLinea::Cerrar { forzar: true }),
        "wq" | "wq!" => sin_argumento(ComandoLinea::GuardarYCerrar { solo_si_modificado: false }),
        "x" | "x!" | "xit" => sin_argumento(ComandoLinea::GuardarYCerrar { solo_si_modificado: true }),
        "qa" | "qall" | "qa!" | "qall!" => sin_argumento(ComandoLinea::Salir { forzar: nombre.ends_with('!') }),
        "e" | "edit" | "e!" => {
            if argumento.is_empty() {
                Err("Uso: :e <ruta>".to_string())
            } else {
                Ok(ComandoLinea::Abrir(argumento.to_string()))
            }
        }
        _ => Err(format!("Comando no soportado: :{texto}")),
    }
}

/// `/patrón/reemplazo/flags` (cualquier separador no alfanumérico, como
/// en VIM; `\` escapa el separador). El patrón es un regex con la
/// sintaxis de Rust (la de la búsqueda de tcode), no la de VIM.
fn parsear_sustitucion(resto: &str, todo_el_archivo: bool) -> Result<ComandoLinea, String> {
    let mut chars = resto.chars();
    let Some(sep) = chars.next() else { return Err("Uso: :s/patrón/reemplazo/[g]".to_string()) };
    let mut partes: Vec<String> = vec![String::new()];
    let mut escape = false;
    for c in chars {
        if escape {
            if c != sep {
                partes.last_mut().unwrap().push('\\');
            }
            partes.last_mut().unwrap().push(c);
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if c == sep && partes.len() < 3 {
            partes.push(String::new());
        } else {
            partes.last_mut().unwrap().push(c);
        }
    }
    if escape {
        partes.last_mut().unwrap().push('\\');
    }
    if partes.len() < 2 || partes[0].is_empty() {
        return Err("Uso: :s/patrón/reemplazo/[g]".to_string());
    }
    let flags = partes.get(2).cloned().unwrap_or_default();
    if let Some(f) = flags.chars().find(|c| !matches!(c, 'g' | 'i' | 'I')) {
        return Err(format!(":s: flag no soportado '{f}'"));
    }
    Ok(ComandoLinea::Sustituir(Sustitucion {
        todo_el_archivo,
        patron: partes[0].clone(),
        reemplazo: partes[1].clone(),
        global: flags.contains('g'),
        ignorar_mayusculas: flags.contains('i'),
    }))
}

/// Las ediciones `(rango de bytes, texto nuevo)` de una sustitución sobre
/// `buffer` (línea `linea_cursor` o todo el archivo), listas para
/// `Editor::aplicar_ediciones` (un solo paso de deshacer). El reemplazo es
/// literal. `Err` si el patrón no es un regex válido.
pub fn ediciones_de_sustitucion(
    buffer: &Buffer,
    linea_cursor: usize,
    s: &Sustitucion,
) -> Result<Vec<(std::ops::Range<usize>, String)>, String> {
    let texto = buffer.a_texto();
    let opciones = OpcionesBusqueda { regex: true, sensible_mayusculas: !s.ignorar_mayusculas, palabra_completa: false };
    let coincidencias = buscar_coincidencias(&texto, &s.patron, opciones).map_err(|e| format!("{e:#}"))?;
    let (desde, hasta) = if s.todo_el_archivo {
        (0, texto.len())
    } else {
        let inicio = buffer.inicio_byte_linea(linea_cursor);
        (inicio, inicio + buffer.linea_texto(linea_cursor).len())
    };
    let mut ediciones = Vec::new();
    let mut ultima_linea: Option<usize> = None;
    for c in coincidencias {
        // Coincidencias vacías (`^`, `x*`) o que cruzan el salto de línea
        // se ignoran: `:s` trabaja línea por línea.
        if c.inicio < desde || c.fin > hasta || c.inicio == c.fin || texto[c.inicio..c.fin].contains('\n') {
            continue;
        }
        let linea = buffer.linea_columna_desde_byte(c.inicio).0;
        if !s.global && ultima_linea == Some(linea) {
            continue;
        }
        ultima_linea = Some(linea);
        ediciones.push((c.inicio..c.fin, s.reemplazo.clone()));
    }
    Ok(ediciones)
}

/// Estado del prompt `:` mientras se escribe, más el historial de los
/// comandos ya ejecutados (de la sesión).
#[derive(Debug, Clone, Default)]
pub struct EstadoLineaComando {
    activa: bool,
    texto: String,
    historial: Vec<String>,
    /// Posición en el historial mientras se navega con `↑`/`↓` (`None` =
    /// editando una línea nueva).
    posicion: Option<usize>,
    /// Lo que se estaba escribiendo antes de empezar a navegar, para
    /// volver a eso al bajar más allá del último.
    borrador: String,
}

impl EstadoLineaComando {
    pub fn activa(&self) -> bool {
        self.activa
    }

    pub fn texto(&self) -> &str {
        &self.texto
    }

    pub fn abrir(&mut self) {
        self.activa = true;
        self.texto.clear();
        self.posicion = None;
        self.borrador.clear();
    }

    pub fn cerrar(&mut self) {
        self.activa = false;
        self.texto.clear();
        self.posicion = None;
    }

    pub fn escribir(&mut self, c: char) {
        self.texto.push(c);
        self.posicion = None;
    }

    /// `Backspace`: borra un carácter; con la línea ya vacía cierra el
    /// prompt, igual que VIM.
    pub fn borrar(&mut self) {
        if self.texto.pop().is_none() {
            self.cerrar();
        }
    }

    /// `Enter`: devuelve lo escrito, lo agrega al historial (sin repetir
    /// el último) y cierra el prompt.
    pub fn confirmar(&mut self) -> String {
        let texto = std::mem::take(&mut self.texto);
        if !texto.trim().is_empty() && self.historial.last() != Some(&texto) {
            self.historial.push(texto.clone());
        }
        self.cerrar();
        texto
    }

    /// `↑`: el comando anterior del historial.
    pub fn historial_anterior(&mut self) {
        if self.historial.is_empty() {
            return;
        }
        let nueva = match self.posicion {
            None => {
                self.borrador = self.texto.clone();
                self.historial.len() - 1
            }
            Some(0) => 0,
            Some(p) => p - 1,
        };
        self.posicion = Some(nueva);
        self.texto = self.historial[nueva].clone();
    }

    /// `↓`: el siguiente del historial, o lo que se estaba escribiendo.
    pub fn historial_siguiente(&mut self) {
        let Some(p) = self.posicion else { return };
        if p + 1 < self.historial.len() {
            self.posicion = Some(p + 1);
            self.texto = self.historial[p + 1].clone();
        } else {
            self.posicion = None;
            self.texto = std::mem::take(&mut self.borrador);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comandos_basicos() {
        assert_eq!(parsear("w"), Ok(ComandoLinea::Guardar));
        assert_eq!(parsear(" q "), Ok(ComandoLinea::Cerrar { forzar: false }));
        assert_eq!(parsear("q!"), Ok(ComandoLinea::Cerrar { forzar: true }));
        assert_eq!(parsear("wq"), Ok(ComandoLinea::GuardarYCerrar { solo_si_modificado: false }));
        assert_eq!(parsear("x"), Ok(ComandoLinea::GuardarYCerrar { solo_si_modificado: true }));
        assert_eq!(parsear("qa!"), Ok(ComandoLinea::Salir { forzar: true }));
        assert_eq!(parsear("42"), Ok(ComandoLinea::IrALinea(42)));
        assert_eq!(parsear("e src/main.rs"), Ok(ComandoLinea::Abrir("src/main.rs".to_string())));
        assert!(parsear("e").is_err());
        assert!(parsear("w otro.txt").is_err());
        assert!(parsear("foo").is_err());
    }

    #[test]
    fn sustituciones() {
        assert_eq!(
            parsear("s/a/b/"),
            Ok(ComandoLinea::Sustituir(Sustitucion {
                todo_el_archivo: false,
                patron: "a".into(),
                reemplazo: "b".into(),
                global: false,
                ignorar_mayusculas: false
            }))
        );
        let Ok(ComandoLinea::Sustituir(s)) = parsear("%s#x\\#y#z#gi") else { panic!() };
        assert!(s.todo_el_archivo && s.global && s.ignorar_mayusculas);
        assert_eq!((s.patron.as_str(), s.reemplazo.as_str()), ("x#y", "z"));
        // Sin la barra final también vale, y un `\d` del regex se conserva.
        let Ok(ComandoLinea::Sustituir(s)) = parsear("s/\\d+/N") else { panic!() };
        assert_eq!((s.patron.as_str(), s.reemplazo.as_str()), ("\\d+", "N"));
        assert!(parsear("s//b/").is_err());
        assert!(parsear("s/a/b/q").is_err());
    }

    fn aplicar(texto: &str, linea: usize, comando: &str) -> String {
        let mut b = Buffer::nuevo();
        b.insertar_str(0, 0, texto);
        let Ok(ComandoLinea::Sustituir(s)) = parsear(comando) else { panic!() };
        let mut ediciones = ediciones_de_sustitucion(&b, linea, &s).unwrap();
        ediciones.reverse();
        let mut resultado = texto.to_string();
        for (r, t) in ediciones {
            resultado.replace_range(r, &t);
        }
        resultado
    }

    #[test]
    fn ediciones_de_sustitucion_por_linea_y_global() {
        let t = "a a\na a";
        assert_eq!(aplicar(t, 1, "s/a/b/"), "a a\nb a");
        assert_eq!(aplicar(t, 1, "s/a/b/g"), "a a\nb b");
        assert_eq!(aplicar(t, 0, "%s/a/b/"), "b a\nb a");
        assert_eq!(aplicar(t, 0, "%s/a/b/g"), "b b\nb b");
        assert_eq!(aplicar("A a", 0, "s/a/x/gi"), "x x");
    }

    #[test]
    fn historial_con_flechas() {
        let mut e = EstadoLineaComando::default();
        for cmd in ["w", "q"] {
            e.abrir();
            for c in cmd.chars() {
                e.escribir(c);
            }
            e.confirmar();
        }
        e.abrir();
        e.escribir('1');
        e.historial_anterior();
        assert_eq!(e.texto(), "q");
        e.historial_anterior();
        assert_eq!(e.texto(), "w");
        e.historial_anterior();
        assert_eq!(e.texto(), "w");
        e.historial_siguiente();
        assert_eq!(e.texto(), "q");
        e.historial_siguiente();
        assert_eq!(e.texto(), "1");
    }

    #[test]
    fn backspace_en_la_linea_vacia_cierra() {
        let mut e = EstadoLineaComando::default();
        e.abrir();
        e.escribir('w');
        e.borrar();
        assert!(e.activa());
        e.borrar();
        assert!(!e.activa());
    }
}
