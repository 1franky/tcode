//! Cálculo de movimientos y objetos de texto del modo VIM sobre un
//! `Buffer`: dónde termina `3w`, qué rango cubre `di(`... Puro (no
//! modifica nada); el ejecutor (`vim::ejecutor`) decide qué hacer con el
//! resultado. Las posiciones son `Cursor` (línea/columna en caracteres);
//! la columna `longitud de la línea` representa el salto de línea (o el
//! fin del archivo en la última).

use std::collections::HashMap;

use crate::buffer::Buffer;
use crate::cursor::Cursor;

use super::gramatica::{BusquedaCaracter, Movimiento, ObjetoTexto, TipoObjeto};

/// Cómo se extiende un movimiento cuando lo usa un operador (`:help
/// exclusive`): exclusivo no incluye el carácter de destino, inclusivo
/// sí, por líneas toma las líneas enteras.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alcance {
    Exclusivo,
    Inclusivo,
    Lineal,
}

pub fn alcance(m: Movimiento) -> Alcance {
    match m {
        Movimiento::Arriba | Movimiento::Abajo | Movimiento::InicioArchivo | Movimiento::FinArchivo => Alcance::Lineal,
        Movimiento::FinLinea | Movimiento::FinPalabra { .. } | Movimiento::ParejaCorchete => Alcance::Inclusivo,
        Movimiento::BuscarCaracter(b) if !b.atras => Alcance::Inclusivo,
        _ => Alcance::Exclusivo,
    }
}

/// Rango de texto sobre el que actúa un operador: `[inicio, fin)` por
/// caracteres, o las líneas `inicio.linea..=fin.linea` enteras si
/// `lineal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rango {
    pub inicio: Cursor,
    pub fin: Cursor,
    pub lineal: bool,
}

/// Lectura de líneas del buffer como `Vec<char>`, pidiendo solo las que
/// hacen falta (un `w` mira una o dos líneas, no el archivo entero).
pub struct Lector<'a> {
    buffer: &'a Buffer,
    cache: HashMap<usize, Vec<char>>,
}

impl<'a> Lector<'a> {
    pub fn nuevo(buffer: &'a Buffer) -> Self {
        Self { buffer, cache: HashMap::new() }
    }

    pub fn num_lineas(&self) -> usize {
        self.buffer.num_lineas().max(1)
    }

    pub fn linea(&mut self, l: usize) -> &[char] {
        let buffer = self.buffer;
        self.cache.entry(l).or_insert_with(|| buffer.linea_texto(l).chars().collect())
    }

    pub fn largo(&mut self, l: usize) -> usize {
        self.linea(l).len()
    }

    /// Carácter en `p` — `'\n'` en la columna del salto de línea.
    fn car(&mut self, p: Cursor) -> char {
        self.linea(p.linea).get(p.columna).copied().unwrap_or('\n')
    }

    fn siguiente(&mut self, p: Cursor) -> Option<Cursor> {
        if p.columna < self.largo(p.linea) {
            Some(Cursor { linea: p.linea, columna: p.columna + 1 })
        } else if p.linea + 1 < self.num_lineas() {
            Some(Cursor { linea: p.linea + 1, columna: 0 })
        } else {
            None
        }
    }

    fn anterior(&mut self, p: Cursor) -> Option<Cursor> {
        if p.columna > 0 {
            Some(Cursor { linea: p.linea, columna: p.columna - 1 })
        } else if p.linea > 0 {
            let l = p.linea - 1;
            Some(Cursor { linea: l, columna: self.largo(l) })
        } else {
            None
        }
    }

    fn es_linea_vacia(&mut self, l: usize) -> bool {
        self.largo(l) == 0
    }

    /// Columna del primer carácter no blanco de `l` (o la última si la
    /// línea es toda blanca, 0 si está vacía).
    pub fn primer_no_blanco(&mut self, l: usize) -> usize {
        let linea = self.linea(l);
        linea.iter().position(|c| !c.is_whitespace()).unwrap_or(linea.len().saturating_sub(1))
    }
}

/// Clase de un carácter para `w`/`b`/`e`: 0 blanco, 1 puntuación, 2
/// palabra. Con `grande` (WORD) todo lo no blanco es una sola clase.
fn clase(c: char, grande: bool) -> u8 {
    if c.is_whitespace() {
        0
    } else if grande || c.is_alphanumeric() || c == '_' {
        2
    } else {
        1
    }
}

fn palabra_siguiente(lector: &mut Lector, desde: Cursor, grande: bool) -> Cursor {
    let mut p = desde;
    let inicial = clase(lector.car(p), grande);
    if inicial != 0 {
        loop {
            let Some(n) = lector.siguiente(p) else { return fin_de(lector) };
            p = n;
            if clase(lector.car(p), grande) != inicial {
                break;
            }
        }
    }
    // Saltea blancos (incluidos saltos de línea); una línea vacía cuenta
    // como palabra y detiene el salto.
    while clase(lector.car(p), grande) == 0 {
        if p != desde && p.columna == 0 && lector.es_linea_vacia(p.linea) {
            break;
        }
        let Some(n) = lector.siguiente(p) else { return fin_de(lector) };
        p = n;
    }
    p
}

/// La posición de fin de archivo (después del último carácter).
fn fin_de(lector: &mut Lector) -> Cursor {
    let l = lector.num_lineas() - 1;
    Cursor { linea: l, columna: lector.largo(l) }
}

fn fin_palabra(lector: &mut Lector, desde: Cursor, grande: bool) -> Cursor {
    let Some(mut p) = lector.siguiente(desde) else { return desde };
    while clase(lector.car(p), grande) == 0 {
        let Some(n) = lector.siguiente(p) else { return p };
        p = n;
    }
    let cls = clase(lector.car(p), grande);
    while let Some(n) = lector.siguiente(p) {
        if clase(lector.car(n), grande) != cls {
            break;
        }
        p = n;
    }
    p
}

fn palabra_anterior(lector: &mut Lector, desde: Cursor, grande: bool) -> Cursor {
    let Some(mut p) = lector.anterior(desde) else { return desde };
    while clase(lector.car(p), grande) == 0 {
        if p.columna == 0 && lector.es_linea_vacia(p.linea) {
            return p;
        }
        let Some(n) = lector.anterior(p) else { return p };
        p = n;
    }
    let cls = clase(lector.car(p), grande);
    while let Some(n) = lector.anterior(p) {
        if clase(lector.car(n), grande) != cls {
            break;
        }
        p = n;
    }
    p
}

/// `f`/`t`/`F`/`T` repetido `veces`. `repitiendo`: con `;`/`,` sobre un
/// `t`/`T`, se saltea el carácter pegado (si no, `;` no avanzaría nunca).
pub fn buscar_caracter(
    lector: &mut Lector,
    desde: Cursor,
    b: BusquedaCaracter,
    veces: usize,
    repitiendo: bool,
) -> Option<Cursor> {
    let linea = lector.linea(desde.linea).to_vec();
    let mut col = desde.columna;
    if repitiendo && b.hasta {
        col = if b.atras { col.checked_sub(1)? } else { col + 1 };
    }
    for _ in 0..veces {
        col = if b.atras {
            (0..col).rev().find(|&i| linea[i] == b.caracter)?
        } else {
            (col + 1..linea.len()).find(|&i| linea[i] == b.caracter)?
        };
    }
    let col = match (b.hasta, b.atras) {
        (true, false) => col - 1,
        (true, true) => col + 1,
        _ => col,
    };
    Some(Cursor { linea: desde.linea, columna: col })
}

/// `%`: el corchete que corresponde al primero que haya en la línea a
/// partir del cursor. No distingue strings ni comentarios.
fn pareja_corchete(lector: &mut Lector, desde: Cursor) -> Option<Cursor> {
    let linea = lector.linea(desde.linea).to_vec();
    let col = (desde.columna..linea.len()).find(|&i| "()[]{}".contains(linea[i]))?;
    let c = linea[col];
    let (abre, cierra, adelante) = match c {
        '(' => ('(', ')', true),
        '[' => ('[', ']', true),
        '{' => ('{', '}', true),
        ')' => ('(', ')', false),
        ']' => ('[', ']', false),
        _ => ('{', '}', false),
    };
    let inicio = Cursor { linea: desde.linea, columna: col };
    if adelante {
        buscar_cierre(lector, inicio, abre, cierra)
    } else {
        buscar_apertura(lector, inicio, abre, cierra)
    }
}

/// Desde un `abre` en `p`, su `cierra` correspondiente (con anidamiento).
fn buscar_cierre(lector: &mut Lector, p: Cursor, abre: char, cierra: char) -> Option<Cursor> {
    let mut nivel = 0usize;
    let mut q = p;
    loop {
        q = lector.siguiente(q)?;
        let c = lector.car(q);
        if c == abre {
            nivel += 1;
        } else if c == cierra {
            if nivel == 0 {
                return Some(q);
            }
            nivel -= 1;
        }
    }
}

/// Desde un `cierra` en `p` (o cualquier posición, con `p` excluida),
/// el `abre` sin cerrar más cercano hacia atrás.
fn buscar_apertura(lector: &mut Lector, p: Cursor, abre: char, cierra: char) -> Option<Cursor> {
    let mut nivel = 0usize;
    let mut q = p;
    loop {
        q = lector.anterior(q)?;
        let c = lector.car(q);
        if c == cierra {
            nivel += 1;
        } else if c == abre {
            if nivel == 0 {
                return Some(q);
            }
            nivel -= 1;
        }
    }
}

fn parrafo(lector: &mut Lector, desde: Cursor, adelante: bool, veces: usize) -> Cursor {
    let ultima = lector.num_lineas() - 1;
    let mut l = desde.linea;
    for _ in 0..veces {
        if adelante {
            // Saltea las vacías de donde se está, después las no vacías.
            while l < ultima && lector.es_linea_vacia(l) {
                l += 1;
            }
            while l < ultima && !lector.es_linea_vacia(l) {
                l += 1;
            }
            if l == ultima && !lector.es_linea_vacia(l) {
                return Cursor { linea: l, columna: lector.largo(l) };
            }
        } else {
            while l > 0 && lector.es_linea_vacia(l) {
                l -= 1;
            }
            while l > 0 && !lector.es_linea_vacia(l) {
                l -= 1;
            }
        }
    }
    Cursor { linea: l, columna: 0 }
}

/// Destino de `veces` repeticiones de `m` desde `desde`, o `None` si el
/// movimiento falla (un `f` que no encuentra nada, `h` en la columna 0...:
/// VIM entonces cancela el operador entero). `RepetirBusqueda` ya tiene
/// que venir resuelto por el ejecutor (ver `resolver_repeticion`); si
/// llega acá, falla.
pub fn calcular(lector: &mut Lector, desde: Cursor, m: Movimiento, veces: usize, conteo_explicito: bool) -> Option<Cursor> {
    let veces = veces.max(1);
    let ultima = lector.num_lineas() - 1;
    let columna_en = |lector: &mut Lector, linea: usize| desde.columna.min(lector.largo(linea));
    Some(match m {
        Movimiento::Izquierda => {
            if desde.columna == 0 {
                return None;
            }
            Cursor { linea: desde.linea, columna: desde.columna.saturating_sub(veces) }
        }
        Movimiento::Derecha => {
            let largo = lector.largo(desde.linea);
            if desde.columna >= largo {
                return None;
            }
            Cursor { linea: desde.linea, columna: (desde.columna + veces).min(largo) }
        }
        Movimiento::Arriba => {
            if desde.linea == 0 {
                return None;
            }
            let linea = desde.linea.saturating_sub(veces);
            Cursor { linea, columna: columna_en(lector, linea) }
        }
        Movimiento::Abajo => {
            if desde.linea >= ultima {
                return None;
            }
            let linea = (desde.linea + veces).min(ultima);
            Cursor { linea, columna: columna_en(lector, linea) }
        }
        Movimiento::InicioLinea => Cursor { linea: desde.linea, columna: 0 },
        Movimiento::PrimerNoBlanco => Cursor { linea: desde.linea, columna: lector.primer_no_blanco(desde.linea) },
        Movimiento::FinLinea => {
            let linea = (desde.linea + veces - 1).min(ultima);
            Cursor { linea, columna: lector.largo(linea).saturating_sub(1) }
        }
        Movimiento::PalabraSiguiente { grande } => {
            let mut p = desde;
            for _ in 0..veces {
                p = palabra_siguiente(lector, p, grande);
            }
            p
        }
        Movimiento::PalabraAnterior { grande } => {
            let mut p = desde;
            for _ in 0..veces {
                p = palabra_anterior(lector, p, grande);
            }
            p
        }
        Movimiento::FinPalabra { grande } => {
            let mut p = desde;
            for _ in 0..veces {
                p = fin_palabra(lector, p, grande);
            }
            p
        }
        Movimiento::InicioArchivo | Movimiento::FinArchivo => {
            let linea = if conteo_explicito {
                (veces - 1).min(ultima)
            } else if m == Movimiento::InicioArchivo {
                0
            } else {
                ultima
            };
            Cursor { linea, columna: lector.primer_no_blanco(linea) }
        }
        Movimiento::BuscarCaracter(b) => buscar_caracter(lector, desde, b, veces, false)?,
        Movimiento::RepetirBusqueda { .. } => return None,
        Movimiento::ParejaCorchete => pareja_corchete(lector, desde)?,
        Movimiento::ParrafoSiguiente => parrafo(lector, desde, true, veces),
        Movimiento::ParrafoAnterior => parrafo(lector, desde, false, veces),
    })
}

/// El rango sobre el que actúa un operador con el movimiento `m`,
/// aplicando las reglas especiales de VIM:
/// - `cw` sobre una palabra se comporta como `ce` (no se come el espacio
///   de después);
/// - `dw` sobre la última palabra de una línea no se come el salto de
///   línea (`:help word`, caso especial de `w` con operador);
/// - un movimiento exclusivo que termina en la columna 0 de otra línea
///   termina en realidad al final de la anterior, y si además arrancó en
///   (o antes de) el primer no blanco, pasa a ser por líneas (`:help
///   exclusive-linewise`).
pub fn rango_de_movimiento(
    lector: &mut Lector,
    desde: Cursor,
    m: Movimiento,
    veces: usize,
    conteo_explicito: bool,
    es_cambio: bool,
) -> Option<Rango> {
    let veces = veces.max(1);
    // `cw`/`cW` sobre algo que no es blanco: como `ce`.
    if let (true, Movimiento::PalabraSiguiente { grande }) = (es_cambio, m) {
        if !lector.car(desde).is_whitespace() {
            // Si el cursor ya está en el final de la palabra, `ce` iría a
            // la siguiente: con conteo 1, `cw` cambia solo hasta acá.
            let mut fin = desde;
            let cls = clase(lector.car(desde), grande);
            for i in 0..veces {
                if i == 0 {
                    while let Some(n) = lector.siguiente(fin) {
                        if n.linea != fin.linea || clase(lector.car(n), grande) != cls {
                            break;
                        }
                        fin = n;
                    }
                } else {
                    fin = fin_palabra(lector, fin, grande);
                }
            }
            return Some(Rango { inicio: desde, fin: incluir(lector, fin), lineal: false });
        }
    }

    if let Movimiento::PalabraSiguiente { grande } = m {
        let mut anterior = desde;
        let mut p = desde;
        for _ in 0..veces {
            anterior = p;
            p = palabra_siguiente(lector, p, grande);
        }
        if p.linea > anterior.linea {
            // La última palabra recorrida terminaba su línea: no seguir
            // hasta la siguiente.
            let l = anterior.linea;
            let largo = lector.largo(l);
            // `dw` sobre una línea vacía: VIM la borra entera.
            if largo == 0 {
                return Some(Rango { inicio: desde, fin: desde, lineal: true });
            }
            return Some(Rango { inicio: desde, fin: Cursor { linea: l, columna: largo }, lineal: false });
        }
        return Some(Rango { inicio: desde, fin: p, lineal: false });
    }

    let destino = calcular(lector, desde, m, veces, conteo_explicito)?;
    Some(rango_entre(lector, desde, destino, alcance(m)))
}

/// Rango entre `desde` y `destino` según el `alcance` del movimiento.
pub fn rango_entre(lector: &mut Lector, desde: Cursor, destino: Cursor, alcance: Alcance) -> Rango {
    let (a, b) = if (destino.linea, destino.columna) < (desde.linea, desde.columna) { (destino, desde) } else { (desde, destino) };
    match alcance {
        Alcance::Lineal => Rango { inicio: a, fin: b, lineal: true },
        Alcance::Inclusivo => Rango { inicio: a, fin: incluir(lector, b), lineal: false },
        Alcance::Exclusivo => {
            if b.columna == 0 && b.linea > a.linea {
                let l = b.linea - 1;
                if a.columna <= lector.primer_no_blanco(a.linea) {
                    return Rango { inicio: a, fin: Cursor { linea: l, columna: 0 }, lineal: true };
                }
                let fin = Cursor { linea: l, columna: lector.largo(l) };
                return Rango { inicio: a, fin, lineal: false };
            }
            Rango { inicio: a, fin: b, lineal: false }
        }
    }
}

/// Posición justo después de `p` en su misma línea (para rangos
/// inclusivos: nunca se come el salto de línea).
fn incluir(lector: &mut Lector, p: Cursor) -> Cursor {
    let largo = lector.largo(p.linea);
    Cursor { linea: p.linea, columna: (p.columna + 1).min(largo) }
}

/// Rango de un objeto de texto alrededor de `cursor`, o `None` si no hay
/// ninguno (p. ej. `i(` fuera de todo paréntesis).
pub fn rango_de_objeto(lector: &mut Lector, cursor: Cursor, objeto: ObjetoTexto) -> Option<Rango> {
    match objeto.tipo {
        TipoObjeto::Palabra => objeto_palabra(lector, cursor, objeto.interior, false),
        TipoObjeto::PalabraGrande => objeto_palabra(lector, cursor, objeto.interior, true),
        TipoObjeto::Comillas(q) => objeto_comillas(lector, cursor, objeto.interior, q),
        TipoObjeto::Par(abre, cierra) => objeto_par(lector, cursor, objeto.interior, abre, cierra),
    }
}

fn objeto_palabra(lector: &mut Lector, cursor: Cursor, interior: bool, grande: bool) -> Option<Rango> {
    let linea = lector.linea(cursor.linea).to_vec();
    if linea.is_empty() {
        return None;
    }
    let col = cursor.columna.min(linea.len() - 1);
    let tramo = |desde: usize| -> (usize, usize) {
        let cls = clase(linea[desde], grande);
        let mut i = desde;
        while i > 0 && clase(linea[i - 1], grande) == cls {
            i -= 1;
        }
        let mut f = desde + 1;
        while f < linea.len() && clase(linea[f], grande) == cls {
            f += 1;
        }
        (i, f)
    };
    let (mut ini, mut fin) = tramo(col);
    if !interior {
        if clase(linea[col], grande) == 0 {
            // Sobre blancos: los blancos más la palabra que sigue.
            if fin < linea.len() {
                fin = tramo(fin).1;
            }
        } else if fin < linea.len() && clase(linea[fin], grande) == 0 {
            fin = tramo(fin).1;
        } else if ini > 0 && clase(linea[ini - 1], grande) == 0 {
            ini = tramo(ini - 1).0;
        }
    }
    let l = cursor.linea;
    Some(Rango { inicio: Cursor { linea: l, columna: ini }, fin: Cursor { linea: l, columna: fin }, lineal: false })
}

fn objeto_comillas(lector: &mut Lector, cursor: Cursor, interior: bool, q: char) -> Option<Rango> {
    let linea = lector.linea(cursor.linea).to_vec();
    let posiciones: Vec<usize> =
        (0..linea.len()).filter(|&i| linea[i] == q && (i == 0 || linea[i - 1] != '\\')).collect();
    let col = cursor.columna;
    // Pares consecutivos; el que contiene al cursor, o si no el primero
    // que empieza después (como VIM).
    let par = posiciones
        .chunks(2)
        .filter(|p| p.len() == 2)
        .find(|p| p[0] <= col && col <= p[1])
        .or_else(|| posiciones.chunks(2).filter(|p| p.len() == 2).find(|p| p[0] > col))?;
    let (a, b) = (par[0], par[1]);
    let (mut ini, mut fin) = if interior { (a + 1, b) } else { (a, b + 1) };
    if !interior {
        let fin_blancos = (fin..linea.len()).find(|&i| !linea[i].is_whitespace()).unwrap_or(linea.len());
        if fin_blancos > fin {
            fin = fin_blancos;
        } else {
            while ini > 0 && linea[ini - 1].is_whitespace() {
                ini -= 1;
            }
        }
    }
    let l = cursor.linea;
    Some(Rango { inicio: Cursor { linea: l, columna: ini }, fin: Cursor { linea: l, columna: fin }, lineal: false })
}

fn objeto_par(lector: &mut Lector, cursor: Cursor, interior: bool, abre: char, cierra: char) -> Option<Rango> {
    let c = lector.car(cursor);
    // Sobre el `cierra` también sirve buscar hacia atrás: `buscar_apertura`
    // no cuenta la posición de partida.
    let apertura = if c == abre { cursor } else { buscar_apertura(lector, cursor, abre, cierra)? };
    let cierre = buscar_cierre(lector, apertura, abre, cierra)?;
    if !interior {
        return Some(Rango { inicio: apertura, fin: incluir(lector, cierre), lineal: false });
    }
    let mut inicio = lector.siguiente(apertura)?;
    let fin = cierre;
    // Bloque multilínea (`{` al final de una línea, `}` solo con blancos
    // antes): lo de adentro son líneas enteras, como en VIM.
    if inicio.columna == lector.largo(inicio.linea) && inicio.linea < fin.linea {
        inicio = Cursor { linea: inicio.linea + 1, columna: 0 };
        let solo_blancos = lector.linea(fin.linea)[..fin.columna].iter().all(|c| c.is_whitespace());
        if solo_blancos {
            if inicio.linea >= fin.linea {
                // `{\n}`: no hay nada adentro.
                return Some(Rango { inicio, fin: inicio, lineal: false });
            }
            return Some(Rango { inicio, fin: Cursor { linea: fin.linea - 1, columna: 0 }, lineal: true });
        }
    }
    Some(Rango { inicio, fin, lineal: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(texto: &str) -> Buffer {
        let mut b = Buffer::nuevo();
        b.insertar_str(0, 0, texto);
        b
    }

    fn c(linea: usize, columna: usize) -> Cursor {
        Cursor { linea, columna }
    }

    fn mover(texto: &str, desde: Cursor, m: Movimiento, veces: usize) -> Option<Cursor> {
        let b = buffer(texto);
        let mut l = Lector::nuevo(&b);
        calcular(&mut l, desde, m, veces, false)
    }

    const W: Movimiento = Movimiento::PalabraSiguiente { grande: false };
    const BW: Movimiento = Movimiento::PalabraAnterior { grande: false };
    const E: Movimiento = Movimiento::FinPalabra { grande: false };

    #[test]
    fn w_b_e_por_palabras() {
        let t = "foo.bar baz\n\n  qux";
        assert_eq!(mover(t, c(0, 0), W, 1), Some(c(0, 3)));
        assert_eq!(mover(t, c(0, 3), W, 1), Some(c(0, 4)));
        assert_eq!(mover(t, c(0, 0), W, 3), Some(c(0, 8)));
        // La línea vacía cuenta como palabra.
        assert_eq!(mover(t, c(0, 8), W, 1), Some(c(1, 0)));
        assert_eq!(mover(t, c(1, 0), W, 1), Some(c(2, 2)));
        assert_eq!(mover(t, c(0, 0), E, 1), Some(c(0, 2)));
        assert_eq!(mover(t, c(0, 2), E, 1), Some(c(0, 3)));
        assert_eq!(mover(t, c(2, 2), BW, 1), Some(c(1, 0)));
        assert_eq!(mover(t, c(0, 8), BW, 1), Some(c(0, 4)));
        assert_eq!(mover(t, c(0, 8), BW, 2), Some(c(0, 3)));
    }

    #[test]
    fn palabras_grandes_ignoran_la_puntuacion() {
        let t = "foo.bar baz";
        assert_eq!(mover(t, c(0, 0), Movimiento::PalabraSiguiente { grande: true }, 1), Some(c(0, 8)));
        assert_eq!(mover(t, c(0, 0), Movimiento::FinPalabra { grande: true }, 1), Some(c(0, 6)));
        assert_eq!(mover(t, c(0, 10), Movimiento::PalabraAnterior { grande: true }, 2), Some(c(0, 0)));
    }

    #[test]
    fn busqueda_de_caracter() {
        let t = "a,b,c,d";
        let f = |c2, hasta, atras| Movimiento::BuscarCaracter(BusquedaCaracter { caracter: c2, hasta, atras });
        assert_eq!(mover(t, c(0, 0), f(',', false, false), 1), Some(c(0, 1)));
        assert_eq!(mover(t, c(0, 0), f(',', false, false), 2), Some(c(0, 3)));
        assert_eq!(mover(t, c(0, 0), f(',', true, false), 2), Some(c(0, 2)));
        assert_eq!(mover(t, c(0, 6), f(',', false, true), 1), Some(c(0, 5)));
        assert_eq!(mover(t, c(0, 6), f(',', true, true), 1), Some(c(0, 6)));
        assert_eq!(mover(t, c(0, 0), f('z', false, false), 1), None);
    }

    #[test]
    fn corchetes_y_parrafos() {
        let t = "f(a, (b)) {\n  x\n}";
        assert_eq!(mover(t, c(0, 0), Movimiento::ParejaCorchete, 1), Some(c(0, 8)));
        assert_eq!(mover(t, c(0, 8), Movimiento::ParejaCorchete, 1), Some(c(0, 1)));
        assert_eq!(mover(t, c(0, 9), Movimiento::ParejaCorchete, 1), Some(c(2, 0)));
        let p = "a\nb\n\nc\nd";
        assert_eq!(mover(p, c(0, 0), Movimiento::ParrafoSiguiente, 1), Some(c(2, 0)));
        assert_eq!(mover(p, c(2, 0), Movimiento::ParrafoSiguiente, 1), Some(c(4, 1)));
        assert_eq!(mover(p, c(4, 0), Movimiento::ParrafoAnterior, 1), Some(c(2, 0)));
    }

    #[test]
    fn g_con_y_sin_conteo() {
        let t = "uno\n  dos\ntres";
        let b = buffer(t);
        let mut l = Lector::nuevo(&b);
        assert_eq!(calcular(&mut l, c(0, 0), Movimiento::FinArchivo, 1, false), Some(c(2, 0)));
        assert_eq!(calcular(&mut l, c(0, 0), Movimiento::FinArchivo, 2, true), Some(c(1, 2)));
        assert_eq!(calcular(&mut l, c(2, 0), Movimiento::InicioArchivo, 1, false), Some(c(0, 0)));
    }

    fn objeto(texto: &str, cursor: Cursor, interior: bool, tipo: TipoObjeto) -> Option<(Cursor, Cursor, bool)> {
        let b = buffer(texto);
        let mut l = Lector::nuevo(&b);
        rango_de_objeto(&mut l, cursor, ObjetoTexto { interior, tipo }).map(|r| (r.inicio, r.fin, r.lineal))
    }

    #[test]
    fn objetos_de_palabra_y_comillas() {
        let t = "uno dos  tres";
        assert_eq!(objeto(t, c(0, 5), true, TipoObjeto::Palabra), Some((c(0, 4), c(0, 7), false)));
        assert_eq!(objeto(t, c(0, 5), false, TipoObjeto::Palabra), Some((c(0, 4), c(0, 9), false)));
        // La última palabra sin espacios después toma los de antes.
        assert_eq!(objeto(t, c(0, 10), false, TipoObjeto::Palabra), Some((c(0, 7), c(0, 13), false)));
        let q = r#"x = "hola" + "chau";"#;
        assert_eq!(objeto(q, c(0, 6), true, TipoObjeto::Comillas('"')), Some((c(0, 5), c(0, 9), false)));
        assert_eq!(objeto(q, c(0, 6), false, TipoObjeto::Comillas('"')), Some((c(0, 4), c(0, 11), false)));
        // Antes de las comillas: toma el primer par de la línea.
        assert_eq!(objeto(q, c(0, 0), true, TipoObjeto::Comillas('"')), Some((c(0, 5), c(0, 9), false)));
    }

    #[test]
    fn objetos_de_pares() {
        let t = "f(a, (b), c)";
        assert_eq!(objeto(t, c(0, 3), true, TipoObjeto::Par('(', ')')), Some((c(0, 2), c(0, 11), false)));
        assert_eq!(objeto(t, c(0, 6), true, TipoObjeto::Par('(', ')')), Some((c(0, 6), c(0, 7), false)));
        assert_eq!(objeto(t, c(0, 1), false, TipoObjeto::Par('(', ')')), Some((c(0, 1), c(0, 12), false)));
        assert_eq!(objeto("sin nada", c(0, 2), true, TipoObjeto::Par('(', ')')), None);
        let bloque = "fn x() {\n    a;\n    b;\n}";
        assert_eq!(objeto(bloque, c(1, 4), true, TipoObjeto::Par('{', '}')), Some((c(1, 0), c(2, 0), true)));
    }
}
