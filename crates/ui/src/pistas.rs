//! Inlay hints de un documento (BACKLOG.md P2 #23) y cómo afectan a la
//! vista de código: se dibujan como texto virtual antes del carácter de su
//! posición, así que el cursor en pantalla y la columna de un clic tienen
//! que descontar su ancho ([`columna_visual`], [`columna_de_texto`]).
//!
//! Se guardan con la revisión del buffer para la que se calcularon.
//! Mientras se escribe (la revisión cambió y todavía no llegaron los
//! nuevos) se siguen mostrando los de las otras líneas si la cantidad de
//! líneas no cambió — así el código no salta a cada tecla —, pero no los
//! de las líneas con cursor (donde se está escribiendo y quedarían en un
//! lugar equivocado). Si cambió la cantidad de líneas, ninguno, hasta que
//! lleguen los nuevos.

/// Un hint ya en coordenadas del buffer: línea y columna en caracteres.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pista {
    pub linea: usize,
    pub columna: usize,
    pub texto: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PistasInlay {
    /// `Buffer::revision` para la que valen.
    pub revision: u64,
    pub num_lineas: usize,
    /// Ordenadas por línea y columna.
    pub pistas: Vec<Pista>,
}

impl PistasInlay {
    pub fn nuevas(revision: u64, num_lineas: usize, mut pistas: Vec<Pista>) -> Self {
        pistas.sort_by_key(|p| (p.linea, p.columna));
        Self { revision, num_lineas, pistas }
    }

    /// Las de `linea` que se pueden mostrar con el texto actual (ver la
    /// nota del módulo).
    pub fn de_linea(&self, linea: usize, revision: u64, num_lineas: usize, lineas_con_cursor: &[usize]) -> &[Pista] {
        let vigente = self.revision == revision
            || (self.num_lineas == num_lineas && !lineas_con_cursor.contains(&linea));
        if !vigente {
            return &[];
        }
        let desde = self.pistas.partition_point(|p| p.linea < linea);
        let hasta = self.pistas.partition_point(|p| p.linea <= linea);
        &self.pistas[desde..hasta]
    }
}

fn ancho(p: &Pista) -> usize {
    p.texto.chars().count()
}

/// Columna en pantalla (sin el área) del carácter `columna` de la línea:
/// se le suman los hints que se dibujan antes de él, incluidos los de su
/// misma posición (un hint va ANTES del carácter de su posición), así el
/// cursor queda sobre el carácter real y nunca sobre un hint.
pub fn columna_visual(pistas: &[Pista], columna: usize) -> usize {
    columna + pistas.iter().filter(|p| p.columna <= columna).map(ancho).sum::<usize>()
}

/// La columna del texto bajo la columna de pantalla `x`: descuenta los
/// hints que hay antes; un clic sobre un hint va a su posición.
pub fn columna_de_texto(pistas: &[Pista], x: usize) -> usize {
    let mut acumulado = 0;
    for p in pistas {
        let inicio = p.columna + acumulado;
        if x < inicio {
            break;
        }
        if x < inicio + ancho(p) {
            return p.columna;
        }
        acumulado += ancho(p);
    }
    x - acumulado
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pista(linea: usize, columna: usize, texto: &str) -> Pista {
        Pista { linea, columna, texto: texto.to_string() }
    }

    #[test]
    fn columnas_con_hints() {
        // "let x = f(1);" con ": i32" en 5 (después de x) y "a: " en 10.
        let pistas = [pista(0, 5, ": i32"), pista(0, 10, "a: ")];
        assert_eq!(columna_visual(&pistas, 4), 4);
        assert_eq!(columna_visual(&pistas, 5), 10, "sobre el carácter, después del hint");
        assert_eq!(columna_visual(&pistas, 6), 11);
        assert_eq!(columna_visual(&pistas, 10), 18);
        assert_eq!(columna_visual(&pistas, 11), 19);
        // Ida y vuelta; sobre un hint, su posición.
        for columna in [0, 4, 5, 6, 9, 10, 11, 13] {
            assert_eq!(columna_de_texto(&pistas, columna_visual(&pistas, columna)), columna);
        }
        assert_eq!(columna_de_texto(&pistas, 7), 5);
        assert_eq!(columna_de_texto(&pistas, 16), 10);
    }

    #[test]
    fn cuales_se_muestran_mientras_se_escribe() {
        let pistas = PistasInlay::nuevas(7, 3, vec![pista(2, 1, "b"), pista(0, 1, "a"), pista(2, 0, "c")]);
        assert_eq!(pistas.de_linea(2, 7, 3, &[2]).iter().map(|p| p.texto.as_str()).collect::<Vec<_>>(), ["c", "b"]);
        // Otra revisión, mismas líneas: todas menos las del cursor.
        assert_eq!(pistas.de_linea(0, 8, 3, &[2]).len(), 1);
        assert!(pistas.de_linea(2, 8, 3, &[2]).is_empty());
        // Cambió la cantidad de líneas: ninguna.
        assert!(pistas.de_linea(0, 8, 4, &[2]).is_empty());
        assert!(pistas.de_linea(1, 7, 3, &[]).is_empty());
    }
}
