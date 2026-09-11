//! Coincidencia difusa (fuzzy matching) de `tcode`. Un patrón coincide con
//! un texto si todos sus caracteres aparecen en el texto en el mismo
//! orden, no necesariamente consecutivos — el algoritmo que usan la
//! paleta de comandos (`Ctrl+Shift+P`) y el buscador de archivos
//! (`Ctrl+P`, PLAN.md §4) para filtrar mientras se escribe.
//!
//! No sabe nada de comandos ni de archivos: solo compara texto contra
//! texto. Insensible a mayúsculas/minúsculas.

/// Resultado de una coincidencia: mayor `puntaje` es mejor. `posiciones`
/// son los índices (en caracteres) de `texto` que matchearon el patrón,
/// útiles para resaltarlos en la UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coincidencia {
    pub puntaje: i32,
    pub posiciones: Vec<usize>,
}

/// Compara `patron` contra `texto`. `None` si `patron` no es una
/// subsecuencia de `texto`. Un patrón vacío coincide con cualquier texto,
/// con puntaje 0 (para que una consulta vacía muestre todo sin reordenar).
///
/// El puntaje favorece: coincidencias consecutivas, coincidencias al
/// inicio de palabra (tras espacio/`_`/`-`/`.`/`/`, o un cambio
/// minúscula→mayúscula tipo `camelCase`), y textos más cortos en general.
pub fn coincidir(patron: &str, texto: &str) -> Option<Coincidencia> {
    if patron.is_empty() {
        return Some(Coincidencia { puntaje: 0, posiciones: Vec::new() });
    }

    let patron_min: Vec<char> = patron.to_lowercase().chars().collect();
    let texto_chars: Vec<char> = texto.chars().collect();
    let texto_min: Vec<char> = texto.to_lowercase().chars().collect();

    let mut posiciones = Vec::with_capacity(patron_min.len());
    let mut desde = 0usize;
    let mut anterior: Option<usize> = None;
    let mut puntaje = 0i32;

    for &c in &patron_min {
        let encontrado = (desde..texto_min.len()).find(|&i| texto_min[i] == c)?;
        posiciones.push(encontrado);

        puntaje += 1;
        if anterior == Some(encontrado.wrapping_sub(1)) {
            puntaje += 5;
        }
        if es_inicio_de_palabra(&texto_chars, encontrado) {
            puntaje += 3;
        }

        anterior = Some(encontrado);
        desde = encontrado + 1;
    }

    puntaje -= (texto_chars.len() as i32) / 20;

    Some(Coincidencia { puntaje, posiciones })
}

fn es_inicio_de_palabra(texto: &[char], idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    let anterior = texto[idx - 1];
    if matches!(anterior, ' ' | '_' | '-' | '.' | '/' | '\\') {
        return true;
    }
    anterior.is_lowercase() && texto[idx].is_uppercase()
}

/// Filtra y ordena `candidatos` por coincidencia contra `patron`, de mejor
/// a peor puntaje. `extraer` obtiene el texto contra el que comparar cada
/// candidato (para no forzar que el candidato mismo sea un `&str`).
pub fn filtrar_y_ordenar<'a, T>(
    patron: &str,
    candidatos: &'a [T],
    extraer: impl Fn(&T) -> &str,
) -> Vec<(&'a T, Coincidencia)> {
    let mut resultados: Vec<(&T, Coincidencia)> = candidatos
        .iter()
        .filter_map(|c| coincidir(patron, extraer(c)).map(|m| (c, m)))
        .collect();
    resultados.sort_by_key(|(_, coincidencia)| std::cmp::Reverse(coincidencia.puntaje));
    resultados
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patron_vacio_coincide_con_todo() {
        let m = coincidir("", "cualquier cosa").unwrap();
        assert_eq!(m.puntaje, 0);
        assert!(m.posiciones.is_empty());
    }

    #[test]
    fn coincide_como_subsecuencia_no_necesariamente_consecutiva() {
        assert!(coincidir("gsv", "guardar_como").is_none());
        assert!(coincidir("gco", "guardar_como").is_some());
    }

    #[test]
    fn no_coincide_si_falta_algun_caracter() {
        assert!(coincidir("xyz", "archivo.guardar").is_none());
    }

    #[test]
    fn es_insensible_a_mayusculas() {
        assert!(coincidir("GUARDAR", "archivo.guardar").is_some());
        assert!(coincidir("guardar", "Archivo.Guardar").is_some());
    }

    #[test]
    fn coincidencias_consecutivas_puntuan_mas_que_dispersas() {
        let consecutiva = coincidir("gua", "guardar").unwrap();
        let dispersa = coincidir("gdr", "guardar").unwrap();
        assert!(consecutiva.puntaje > dispersa.puntaje);
    }

    #[test]
    fn coincidencia_al_inicio_de_palabra_puntua_mas() {
        // La única 'g' de "archivo.guardar" está justo tras el '.'
        // (inicio de palabra); la única 'g' de "algo" no lo está.
        let inicio_palabra = coincidir("g", "archivo.guardar").unwrap();
        let interior = coincidir("g", "algo").unwrap();
        assert!(inicio_palabra.puntaje > interior.puntaje);
    }

    #[test]
    fn filtrar_y_ordenar_descarta_no_coincidencias_y_ordena_por_puntaje() {
        let candidatos = vec!["archivo.guardar", "archivo.guardar_como", "editor.copiar"];
        let resultados = filtrar_y_ordenar("guardar", &candidatos, |s| s);
        assert_eq!(resultados.len(), 2);
        // "archivo.guardar" coincide de forma más compacta/consecutiva
        // que "archivo.guardar_como", así que debería ir primero.
        assert_eq!(*resultados[0].0, "archivo.guardar");
    }
}
