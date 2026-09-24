//! Diff mínimo entre dos versiones de un texto, expresado como ediciones
//! `(rango de bytes del texto VIEJO, texto nuevo)` — la misma forma que
//! espera `Editor::aplicar_ediciones`. Lo usa el formateador externo
//! (`formateador` en `app`): un formateador por stdin/stdout devuelve el
//! archivo ENTERO, y reemplazar todo el buffer de una movería el cursor
//! al final del texto nuevo (o lo dejaría en cualquier lado) y borraría
//! los pliegues; con ediciones mínimas, `aplicar_ediciones` los corre
//! solo por lo que cambió antes de ellos, igual que con el `TextEdit[]`
//! de un LSP.
//!
//! Dos pasadas: primero un diff por LÍNEAS (Myers, O((N+M)·D), sobre lo
//! que queda después de recortar las líneas iguales del principio y del
//! final), y después cada bloque de líneas cambiadas se achica a nivel de
//! carácter (prefijo/sufijo comunes y otro Myers por caracteres) — así
//! `let x=1;` → `let x = 1;` queda como una inserción de dos espacios y
//! no como el reemplazo de la línea entera. Si los textos difieren demasiado
//! (`MAX_DISTANCIA` líneas), en vez de seguir buscando el diff óptimo se
//! reemplaza el tramo del medio de una sola vez: sigue siendo correcto,
//! solo menos fino.

use std::ops::Range;

/// Tope de líneas distintas para el diff de Myers (acota tiempo y
/// memoria — la traza guarda O(D²) enteros). Un formateador rara vez
/// toca tantas líneas; si lo hace, se cae al reemplazo del tramo entero.
const MAX_DISTANCIA: usize = 1000;

/// Ediciones que convierten `viejo` en `nuevo`: rangos de bytes de
/// `viejo` (siempre en límites de carácter, ordenados y sin solaparse) y
/// el texto que va en cada uno. Vacía si los dos textos son iguales.
pub fn ediciones_minimas(viejo: &str, nuevo: &str) -> Vec<(Range<usize>, String)> {
    if viejo == nuevo {
        return Vec::new();
    }
    let lineas_a: Vec<&str> = viejo.split_inclusive('\n').collect();
    let lineas_b: Vec<&str> = nuevo.split_inclusive('\n').collect();
    let offsets_a = offsets(&lineas_a);
    let offsets_b = offsets(&lineas_b);

    // Recortar líneas iguales al principio y al final antes de Myers:
    // es el caso común (el formateador toca unas pocas zonas).
    let prefijo = lineas_a.iter().zip(&lineas_b).take_while(|(a, b)| a == b).count();
    let sufijo = lineas_a[prefijo..]
        .iter()
        .rev()
        .zip(lineas_b[prefijo..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let medio_a = &lineas_a[prefijo..lineas_a.len() - sufijo];
    let medio_b = &lineas_b[prefijo..lineas_b.len() - sufijo];

    let bloques = match bloques_myers(medio_a, medio_b) {
        Some(bloques) => bloques,
        None => vec![(0..medio_a.len(), 0..medio_b.len())],
    };

    let mut ediciones = Vec::new();
    for (a, b) in bloques {
        // Mismo número de líneas de cada lado (lo típico de reindentar
        // o espaciar operadores): se refina línea contra línea, así cada
        // línea queda con sus propias ediciones chicas y el diff por
        // caracteres trabaja sobre tramos cortos.
        let pares: Vec<(Range<usize>, Range<usize>)> = if a.len() == b.len() {
            a.clone().zip(b.clone()).map(|(i, j)| (i..i + 1, j..j + 1)).collect()
        } else {
            vec![(a, b)]
        };
        for (a, b) in pares {
            let rango_viejo = offsets_a[prefijo + a.start]..offsets_a[prefijo + a.end];
            let tramo_nuevo = &nuevo[offsets_b[prefijo + b.start]..offsets_b[prefijo + b.end]];
            ediciones.extend(refinar(&viejo[rango_viejo.clone()], tramo_nuevo, rango_viejo.start));
        }
    }
    ediciones
}

/// Offset de bytes donde empieza cada línea, más uno final con el largo
/// total (así `offsets[i]..offsets[j]` son las líneas `i..j`).
fn offsets(lineas: &[&str]) -> Vec<usize> {
    let mut resultado = Vec::with_capacity(lineas.len() + 1);
    let mut acumulado = 0;
    resultado.push(0);
    for linea in lineas {
        acumulado += linea.len();
        resultado.push(acumulado);
    }
    resultado
}

/// Achica un bloque cambiado a su parte distinta: recorta el prefijo y el
/// sufijo comunes y, sobre lo que queda, hace un segundo Myers por
/// CARACTERES — así `let x=1;` → `    let x = 1;` da dos inserciones
/// (la indentación y los espacios del `=`) en vez de un reemplazo que va
/// de una a la otra. Trabaja por `char`, así que nunca corta un carácter
/// multibyte a la mitad. Si el tramo difiere demasiado, se queda con el
/// reemplazo del medio entero (correcto, menos fino).
fn refinar(viejo: &str, nuevo: &str, base: usize) -> Vec<(Range<usize>, String)> {
    let prefijo: usize = viejo.chars().zip(nuevo.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
    let (resto_viejo, resto_nuevo) = (&viejo[prefijo..], &nuevo[prefijo..]);
    let sufijo: usize = resto_viejo
        .chars()
        .rev()
        .zip(resto_nuevo.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let viejo_distinto = &resto_viejo[..resto_viejo.len() - sufijo];
    let nuevo_distinto = &resto_nuevo[..resto_nuevo.len() - sufijo];
    if viejo_distinto.is_empty() && nuevo_distinto.is_empty() {
        return Vec::new();
    }
    let inicio = base + prefijo;
    if viejo_distinto.is_empty() || nuevo_distinto.is_empty() {
        return vec![(inicio..inicio + viejo_distinto.len(), nuevo_distinto.to_string())];
    }

    let chars_a: Vec<(usize, char)> = viejo_distinto.char_indices().collect();
    let chars_b: Vec<(usize, char)> = nuevo_distinto.char_indices().collect();
    let solo_a: Vec<char> = chars_a.iter().map(|(_, c)| *c).collect();
    let solo_b: Vec<char> = chars_b.iter().map(|(_, c)| *c).collect();
    let Some(bloques) = bloques_myers(&solo_a, &solo_b) else {
        return vec![(inicio..inicio + viejo_distinto.len(), nuevo_distinto.to_string())];
    };
    // Índice de carácter → offset de bytes (con uno de más para el final).
    let byte_de = |chars: &[(usize, char)], total: usize, i: usize| chars.get(i).map_or(total, |(b, _)| *b);
    bloques
        .into_iter()
        .map(|(a, b)| {
            let desde = byte_de(&chars_a, viejo_distinto.len(), a.start);
            let hasta = byte_de(&chars_a, viejo_distinto.len(), a.end);
            let texto =
                &nuevo_distinto[byte_de(&chars_b, nuevo_distinto.len(), b.start)..byte_de(&chars_b, nuevo_distinto.len(), b.end)];
            (inicio + desde..inicio + hasta, texto.to_string())
        })
        .collect()
}

/// Paso del camino de edición de Myers.
#[derive(Clone, Copy, PartialEq)]
enum Paso {
    Igual,
    Borrar,
    Insertar,
}

/// Diff de Myers entre `a` y `b` agrupado en bloques de líneas cambiadas
/// `(rango en a, rango en b)`, en orden. `None` si la distancia supera
/// `MAX_DISTANCIA`.
fn bloques_myers<T: PartialEq>(a: &[T], b: &[T]) -> Option<Vec<(Range<usize>, Range<usize>)>> {
    let pasos = pasos_myers(a, b)?;
    let mut bloques = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut actual: Option<(usize, usize)> = None;
    for paso in pasos {
        if paso == Paso::Igual {
            if let Some((ia, jb)) = actual.take() {
                bloques.push((ia..i, jb..j));
            }
            i += 1;
            j += 1;
            continue;
        }
        actual.get_or_insert((i, j));
        match paso {
            Paso::Borrar => i += 1,
            Paso::Insertar => j += 1,
            Paso::Igual => unreachable!(),
        }
    }
    if let Some((ia, jb)) = actual {
        bloques.push((ia..i, jb..j));
    }
    Some(bloques)
}

/// Camino de edición más corto de Myers ("An O(ND) Difference Algorithm
/// and Its Variations", 1986), con la traza de cada paso para
/// reconstruirlo hacia atrás. `traza[d]` guarda la diagonal `v` ANTES
/// del paso `d`, recortada a `k ∈ [-d-1, d+1]` (lo único que ese paso y
/// el retroceso leen) — memoria O(D²) en vez de O(D·(N+M)).
fn pasos_myers<T: PartialEq>(a: &[T], b: &[T]) -> Option<Vec<Paso>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let maximo = (n + m) as usize;
    let desplazamiento = maximo as isize + 1;
    let mut v = vec![0isize; 2 * maximo + 3];
    let idx = |k: isize| (k + desplazamiento) as usize;
    let mut traza: Vec<Vec<isize>> = Vec::new();

    let mut d_final = None;
    'externo: for d in 0..=(maximo.min(MAX_DISTANCIA) as isize) {
        traza.push(v[idx(-d - 1)..=idx(d + 1)].to_vec());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && v[idx(k - 1)] < v[idx(k + 1)]) { v[idx(k + 1)] } else { v[idx(k - 1)] + 1 };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx(k)] = x;
            if x >= n && y >= m {
                d_final = Some(d);
                break 'externo;
            }
            k += 2;
        }
    }
    let d_final = d_final?;

    // Retroceso desde (n, m): en cada `d`, la diagonal previa según la
    // misma regla que la ida, la serpiente de iguales, y un paso de
    // edición (salvo en d = 0, que es solo serpiente).
    let mut pasos = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..=d_final).rev() {
        let previa = &traza[d as usize];
        let leer = |k: isize| previa[(k + d + 1) as usize];
        let k = x - y;
        let k_previa = if k == -d || (k != d && leer(k - 1) < leer(k + 1)) { k + 1 } else { k - 1 };
        let x_previa = if d == 0 { 0 } else { leer(k_previa) };
        let y_previa = if d == 0 { 0 } else { x_previa - k_previa };
        while x > x_previa && y > y_previa {
            pasos.push(Paso::Igual);
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            pasos.push(if x == x_previa { Paso::Insertar } else { Paso::Borrar });
        }
        x = x_previa;
        y = y_previa;
    }
    pasos.reverse();
    Some(pasos)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Aplica las ediciones de atrás hacia adelante (misma convención que
    /// `Editor::aplicar_ediciones`) y verifica las invariantes.
    fn aplicar(viejo: &str, ediciones: &[(Range<usize>, String)]) -> String {
        for par in ediciones.windows(2) {
            assert!(par[0].0.end <= par[1].0.start, "ediciones solapadas o desordenadas: {ediciones:?}");
        }
        let mut texto = viejo.to_string();
        for (rango, nuevo) in ediciones.iter().rev() {
            assert!(viejo.is_char_boundary(rango.start) && viejo.is_char_boundary(rango.end));
            texto.replace_range(rango.clone(), nuevo);
        }
        texto
    }

    fn comprobar(viejo: &str, nuevo: &str) -> Vec<(Range<usize>, String)> {
        let ediciones = ediciones_minimas(viejo, nuevo);
        assert_eq!(aplicar(viejo, &ediciones), nuevo, "viejo={viejo:?} ediciones={ediciones:?}");
        ediciones
    }

    #[test]
    fn textos_iguales_no_dan_ninguna_edicion() {
        assert!(comprobar("a\nb\n", "a\nb\n").is_empty());
        assert!(comprobar("", "").is_empty());
    }

    #[test]
    fn cambio_dentro_de_una_linea_queda_acotado_a_lo_distinto() {
        let ediciones = comprobar("fn main(){\nlet x=1;\n}\n", "fn main() {\n    let x = 1;\n}\n");
        // Nada de "reemplazar la línea entera": solo lo que cambia de
        // verdad: un espacio antes de `{`, la indentación y los espacios
        // alrededor del `=` (el `=` mismo no se toca).
        assert_eq!(
            ediciones,
            vec![
                (9..9, " ".to_string()),
                (11..11, "    ".to_string()),
                (16..16, " ".to_string()),
                (17..17, " ".to_string()),
            ]
        );
    }

    #[test]
    fn lineas_iguales_lejos_del_cambio_no_se_tocan() {
        let viejo = "uno\ndos\ntres\ncuatro\ncinco\n";
        let nuevo = "uno\ndos\nTRES\ncuatro\ncinco\n";
        let ediciones = comprobar(viejo, nuevo);
        assert_eq!(ediciones, vec![(8..12, "TRES".to_string())]);
    }

    #[test]
    fn varias_zonas_cambiadas_dan_varias_ediciones() {
        let viejo = "a=1\nigual\nigual2\nb=2\nfin\n";
        let nuevo = "a = 1\nigual\nigual2\nb = 2\nfin\n";
        let ediciones = comprobar(viejo, nuevo);
        // Dos espacios por cada `=`, y nada en las líneas iguales del medio.
        assert_eq!(ediciones.len(), 4);
        assert!(ediciones.iter().all(|(r, nuevo)| r.is_empty() && nuevo == " "));
    }

    #[test]
    fn lineas_insertadas_y_borradas() {
        comprobar("a\nb\nc\n", "a\nx\ny\nb\nc\n");
        comprobar("a\nx\ny\nb\nc\n", "a\nb\nc\n");
        comprobar("a\nb\nc\n", "c\nb\na\n");
        comprobar("", "todo nuevo\n");
        comprobar("todo viejo\n", "");
    }

    #[test]
    fn acentos_y_emoji_nunca_se_cortan_a_la_mitad() {
        // Comparten el primer byte UTF-8 ("á" = C3 A1, "é" = C3 A9): el
        // recorte por carácter no puede quedarse con medio "á".
        comprobar("let s=\"á\";\n", "let s = \"é\";\n");
        comprobar("x = \"ñandú 😀\"\n", "x = \"ñandú 😃\"\n");
        let ediciones = comprobar("😀😀😀\n", "😀😁😀\n");
        assert_eq!(ediciones, vec![(4..8, "😁".to_string())]);
        comprobar("año\nüber\n", "año\n  über\n");
    }

    #[test]
    fn archivo_sin_salto_de_linea_final() {
        // El formateador agrega el `\n` final que faltaba.
        let ediciones = comprobar("a\nb", "a\nb\n");
        assert_eq!(ediciones, vec![(3..3, "\n".to_string())]);
        // O lo quita, o cambia la última línea sin `\n`.
        comprobar("a\nb\n", "a\nb");
        comprobar("a\nx=1", "a\nx = 1");
        comprobar("sola", "sola\n");
    }

    #[test]
    fn diferencias_enormes_caen_al_reemplazo_del_tramo_del_medio() {
        // Más de `MAX_DISTANCIA` líneas distintas: sigue siendo correcto.
        let viejo: String = (0..1500).map(|i| format!("v{i}\n")).collect();
        let nuevo: String = (0..1400).map(|i| format!("n{i}\n")).collect();
        let viejo = format!("cabecera\n{viejo}pie\n");
        let nuevo = format!("cabecera\n{nuevo}pie\n");
        let ediciones = comprobar(&viejo, &nuevo);
        assert!(ediciones.len() <= 2, "{}", ediciones.len());
        assert!(ediciones[0].0.start >= "cabecera\n".len());
    }

    #[test]
    fn casos_mezclados_siempre_reconstruyen_el_texto_nuevo() {
        let textos = [
            "",
            "\n",
            "a",
            "a\n",
            "a\nb\nc\nd\ne\n",
            "a\nc\nd\nb\ne",
            "x\na\nb\ny\nc\nd\ne\n",
            "é\n\n\nñ\n",
            "  a\n\tb\n",
        ];
        for viejo in textos {
            for nuevo in textos {
                comprobar(viejo, nuevo);
            }
        }
    }
}
