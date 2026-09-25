use anyhow::{Context, Result};
use regex::Regex;

/// Opciones de búsqueda (`Alt+R`/`Alt+C`/`Alt+W`, PLAN.md §4): tratar la
/// consulta como regex, distinguir mayúsculas/minúsculas, exigir palabra
/// completa. Las tres se implementan compilando siempre un regex interno
/// — para una consulta literal se escapa con [`regex::escape`], para no
/// tener dos caminos de matching distintos que mantener.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpcionesBusqueda {
    pub regex: bool,
    pub sensible_mayusculas: bool,
    pub palabra_completa: bool,
}

/// Una coincidencia: rango de bytes `[inicio, fin)` en el texto completo
/// del buffer (mismo tipo de offset que usa `tcode-syntax` para sus
/// tokens, así que ambos se pueden combinar al dibujar una línea).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coincidencia {
    pub inicio: usize,
    pub fin: usize,
}

/// El regex interno que usan todas las búsquedas: la del archivo
/// (`Ctrl+F`) y la de todo el proyecto (`tcode_fs::busqueda_proyecto`,
/// BACKLOG.md P1 #16), que lo compila una vez y lo reusa en cada archivo
/// — así las dos interpretan igual las mismas opciones.
pub fn compilar_patron(patron: &str, opciones: OpcionesBusqueda) -> Result<Regex> {
    let base = if opciones.regex { patron.to_string() } else { regex::escape(patron) };
    let con_palabra = if opciones.palabra_completa { format!(r"\b{base}\b") } else { base };
    let con_flags = if opciones.sensible_mayusculas { con_palabra } else { format!("(?i){con_palabra}") };
    Regex::new(&con_flags).context("patrón de búsqueda inválido")
}

/// Busca todas las coincidencias de `patron` en `texto`, en orden de
/// aparición. `Err` si `patron` es un regex inválido (solo posible con
/// `opciones.regex == true`) — quien llama decide qué hacer (p. ej. no
/// actualizar los resultados hasta que el patrón vuelva a ser válido).
pub fn buscar_coincidencias(texto: &str, patron: &str, opciones: OpcionesBusqueda) -> Result<Vec<Coincidencia>> {
    if patron.is_empty() {
        return Ok(Vec::new());
    }
    let re = compilar_patron(patron, opciones)?;
    Ok(re.find_iter(texto).map(|m| Coincidencia { inicio: m.start(), fin: m.end() }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buscar(texto: &str, patron: &str) -> Vec<Coincidencia> {
        buscar_coincidencias(texto, patron, OpcionesBusqueda::default()).unwrap()
    }

    #[test]
    fn encuentra_coincidencias_literales_insensibles_a_mayusculas_por_defecto() {
        let coincidencias = buscar("Hola hola HOLA", "hola");
        assert_eq!(coincidencias.len(), 3);
    }

    #[test]
    fn patron_vacio_no_da_coincidencias() {
        assert!(buscar("cualquier cosa", "").is_empty());
    }

    #[test]
    fn sensible_a_mayusculas_cuando_se_activa() {
        let opciones = OpcionesBusqueda { sensible_mayusculas: true, ..Default::default() };
        let coincidencias = buscar_coincidencias("Hola hola HOLA", "hola", opciones).unwrap();
        assert_eq!(coincidencias.len(), 1);
    }

    #[test]
    fn palabra_completa_no_matchea_dentro_de_otra_palabra() {
        let opciones = OpcionesBusqueda { palabra_completa: true, ..Default::default() };
        let coincidencias = buscar_coincidencias("cat catalog concat cat", "cat", opciones).unwrap();
        // Coincide con "cat" y "cat" (inicio y fin), no con la parte
        // "cat" dentro de "catalog" ni "concat".
        assert_eq!(coincidencias.len(), 2);
    }

    #[test]
    fn consulta_literal_sin_regex_trata_los_caracteres_especiales_tal_cual() {
        // Sin opciones.regex, "a.b" busca el texto exacto "a.b", el punto
        // no es "cualquier carácter".
        let coincidencias = buscar("a.b axb", "a.b");
        assert_eq!(coincidencias.len(), 1);
        assert_eq!(coincidencias[0], Coincidencia { inicio: 0, fin: 3 });
    }

    #[test]
    fn con_regex_activado_los_caracteres_especiales_funcionan_como_regex() {
        let opciones = OpcionesBusqueda { regex: true, ..Default::default() };
        let coincidencias = buscar_coincidencias("a.b axb", "a.b", opciones).unwrap();
        assert_eq!(coincidencias.len(), 2); // "a.b" y "axb"
    }

    #[test]
    fn regex_invalido_da_error() {
        let opciones = OpcionesBusqueda { regex: true, ..Default::default() };
        assert!(buscar_coincidencias("texto", "(", opciones).is_err());
    }
}
