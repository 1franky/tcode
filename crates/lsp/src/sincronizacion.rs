//! Sincronización del contenido del documento con el servidor
//! (`textDocument/didChange`): completa o incremental según lo que
//! anuncie el servidor en su respuesta a `initialize` (BACKLOG.md P1 #14).

use lsp_types::{Position, Range, TextDocumentContentChangeEvent};
use serde_json::Value;

/// Cómo quiere el servidor recibir los cambios del documento
/// (`ServerCapabilities::textDocumentSync`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoSincronizacion {
    /// Cada `didChange` lleva el texto entero.
    Completa,
    /// Cada `didChange` lleva solo el rango reemplazado y su texto nuevo.
    Incremental,
}

impl ModoSincronizacion {
    /// Lee el modo del `result` de la respuesta a `initialize`.
    /// `textDocumentSync` puede venir como número (`TextDocumentSyncKind`:
    /// 0 = ninguno, 1 = completo, 2 = incremental) o como objeto
    /// (`TextDocumentSyncOptions`, con el mismo número en `change`). Solo
    /// un 2 explícito activa el modo incremental: cualquier otra cosa
    /// (incluido "ninguno" o un campo ausente) sigue con el envío
    /// completo, que es lo que `tcode` hizo siempre y cualquier servidor
    /// acepta.
    pub fn desde_initialize(resultado: &Value) -> Self {
        let sync = &resultado["capabilities"]["textDocumentSync"];
        let tipo = sync.as_u64().or_else(|| sync["change"].as_u64());
        if tipo == Some(2) {
            ModoSincronizacion::Incremental
        } else {
            ModoSincronizacion::Completa
        }
    }
}

/// El cambio a mandar en `didChange` para que el servidor pase de `viejo`
/// (el último texto que se le mandó) a `nuevo`, o `None` si son iguales.
///
/// En modo incremental es UN solo reemplazo: la región entre el prefijo y
/// el sufijo comunes más largos — el mismo criterio que usa el
/// resaltador (`tcode_syntax`, `edicion_entre`) para deducir la edición
/// sin que el `Buffer` la avise. Entre dos frames casi siempre hubo una
/// sola edición contigua (una tecla, un pegado); si hubo varias
/// separadas (multi-cursor, "reemplazar todo"), el rango cubre desde la
/// primera hasta la última, que sigue siendo correcto — solo manda algo
/// más de texto. Los extremos se ajustan a límites de carácter (un
/// prefijo común por bytes puede cortar un carácter UTF-8 a la mitad, p.
/// ej. `á` → `é` comparten el primer byte) y se expresan como LSP pide:
/// línea + columna en unidades UTF-16 (ver [`posicion_lsp`]), medidas
/// sobre `viejo`.
pub fn cambio_entre(viejo: &str, nuevo: &str, modo: ModoSincronizacion) -> Option<TextDocumentContentChangeEvent> {
    if viejo == nuevo {
        return None;
    }
    if modo == ModoSincronizacion::Completa {
        return Some(TextDocumentContentChangeEvent { range: None, range_length: None, text: nuevo.to_string() });
    }

    let (v, n) = (viejo.as_bytes(), nuevo.as_bytes());
    let mut prefijo = v.iter().zip(n).take_while(|(a, b)| a == b).count();
    while !viejo.is_char_boundary(prefijo) || !nuevo.is_char_boundary(prefijo) {
        prefijo -= 1;
    }
    let maximo_sufijo = v.len().min(n.len()) - prefijo;
    let mut sufijo = v.iter().rev().zip(n.iter().rev()).take(maximo_sufijo).take_while(|(a, b)| a == b).count();
    while !viejo.is_char_boundary(v.len() - sufijo) || !nuevo.is_char_boundary(n.len() - sufijo) {
        sufijo -= 1;
    }

    let inicio = posicion_lsp(viejo, prefijo);
    // El fin se mide desde el inicio (mismo texto) para no recorrer el
    // prefijo dos veces.
    let fin = avanzar_posicion(inicio, &viejo[prefijo..v.len() - sufijo]);
    Some(TextDocumentContentChangeEvent {
        range: Some(Range { start: inicio, end: fin }),
        range_length: None,
        text: nuevo[prefijo..n.len() - sufijo].to_string(),
    })
}

/// Posición LSP (línea base 0 + columna en unidades UTF-16) del offset
/// de bytes `byte` de `texto` — la conversión inversa de la que hacen los
/// diagnósticos (`utf16_a_indice_char`). Solo `\n` separa líneas: el
/// `Buffer` de `tcode` nunca contiene `\r` (se normaliza al abrir).
fn posicion_lsp(texto: &str, byte: usize) -> Position {
    avanzar_posicion(Position { line: 0, character: 0 }, &texto[..byte])
}

/// La posición a la que se llega partiendo de `desde` y recorriendo
/// `tramo`.
fn avanzar_posicion(desde: Position, tramo: &str) -> Position {
    match tramo.rfind('\n') {
        Some(ultimo_salto) => Position {
            line: desde.line + tramo.bytes().filter(|&b| b == b'\n').count() as u32,
            character: utf16_de(&tramo[ultimo_salto + 1..]),
        },
        None => Position { line: desde.line, character: desde.character + utf16_de(tramo) },
    }
}

fn utf16_de(texto: &str) -> u32 {
    texto.chars().map(|c| c.len_utf16() as u32).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Aplica un cambio como lo haría el servidor (posiciones UTF-16 →
    /// bytes), para verificar que el resultado es exactamente `nuevo`.
    fn aplicar(texto: &str, cambio: &TextDocumentContentChangeEvent) -> String {
        let Some(rango) = cambio.range else { return cambio.text.clone() };
        let a_byte = |p: Position| {
            let inicio_linea: usize =
                texto.split_inclusive('\n').take(p.line as usize).map(str::len).sum();
            let mut unidades = 0;
            let mut byte = inicio_linea;
            for c in texto[inicio_linea..].chars() {
                if unidades >= p.character || c == '\n' {
                    break;
                }
                unidades += c.len_utf16() as u32;
                byte += c.len_utf8();
            }
            byte
        };
        let (inicio, fin) = (a_byte(rango.start), a_byte(rango.end));
        format!("{}{}{}", &texto[..inicio], cambio.text, &texto[fin..])
    }

    #[test]
    fn modo_desde_initialize_acepta_numero_u_objeto() {
        let con = |sync: Value| ModoSincronizacion::desde_initialize(&json!({ "capabilities": { "textDocumentSync": sync } }));
        assert_eq!(con(json!(2)), ModoSincronizacion::Incremental);
        assert_eq!(con(json!(1)), ModoSincronizacion::Completa);
        assert_eq!(con(json!({ "openClose": true, "change": 2 })), ModoSincronizacion::Incremental);
        assert_eq!(con(json!({ "openClose": true, "change": 1 })), ModoSincronizacion::Completa);
        assert_eq!(con(json!(0)), ModoSincronizacion::Completa);
        assert_eq!(ModoSincronizacion::desde_initialize(&json!({ "capabilities": {} })), ModoSincronizacion::Completa);
        assert_eq!(ModoSincronizacion::desde_initialize(&Value::Null), ModoSincronizacion::Completa);
    }

    #[test]
    fn sin_cambios_no_hay_nada_que_mandar() {
        assert!(cambio_entre("igual", "igual", ModoSincronizacion::Incremental).is_none());
        assert!(cambio_entre("igual", "igual", ModoSincronizacion::Completa).is_none());
    }

    #[test]
    fn modo_completo_manda_el_texto_entero() {
        let cambio = cambio_entre("a\nb", "a\nbc", ModoSincronizacion::Completa).unwrap();
        assert!(cambio.range.is_none());
        assert_eq!(cambio.text, "a\nbc");
    }

    #[test]
    fn una_tecla_en_medio_manda_solo_ese_caracter() {
        let cambio = cambio_entre("uno\ndos\ntres\n", "uno\ndoXs\ntres\n", ModoSincronizacion::Incremental).unwrap();
        let rango = cambio.range.unwrap();
        assert_eq!((rango.start.line, rango.start.character), (1, 2));
        assert_eq!((rango.end.line, rango.end.character), (1, 2));
        assert_eq!(cambio.text, "X");
    }

    #[test]
    fn columnas_en_utf16_despues_de_un_emoji_y_acentos() {
        // "😀" son 2 unidades UTF-16 (4 bytes), "á" 1 (2 bytes): la "x"
        // borrada está en la columna UTF-16 4.
        let cambio = cambio_entre("a😀áx = 1\n", "a😀á = 1\n", ModoSincronizacion::Incremental).unwrap();
        let rango = cambio.range.unwrap();
        assert_eq!((rango.start.line, rango.start.character), (0, 4));
        assert_eq!((rango.end.line, rango.end.character), (0, 5));
        assert_eq!(cambio.text, "");
    }

    #[test]
    fn no_parte_un_caracter_multibyte_que_comparte_el_primer_byte() {
        // "á" = C3 A1 y "é" = C3 A9: el prefijo común por bytes cortaría
        // el carácter a la mitad.
        let cambio = cambio_entre("cáfe", "céfe", ModoSincronizacion::Incremental).unwrap();
        let rango = cambio.range.unwrap();
        assert_eq!((rango.start.character, rango.end.character), (1, 2));
        assert_eq!(cambio.text, "é");
    }

    #[test]
    fn borrar_un_salto_de_linea_une_las_lineas() {
        let viejo = "fn a() {\n    1\n}\n";
        let nuevo = "fn a() {    1\n}\n";
        let cambio = cambio_entre(viejo, nuevo, ModoSincronizacion::Incremental).unwrap();
        let rango = cambio.range.unwrap();
        assert_eq!((rango.start.line, rango.start.character), (0, 8));
        assert_eq!((rango.end.line, rango.end.character), (1, 0));
        assert_eq!(aplicar(viejo, &cambio), nuevo);
    }

    /// Cientos de ediciones pseudoaleatorias (insertar/borrar/reemplazar,
    /// con saltos de línea, acentos y emoji): aplicar el cambio al texto
    /// viejo con la semántica de LSP siempre da exactamente el nuevo.
    #[test]
    fn ediciones_aleatorias_reconstruyen_el_texto_nuevo() {
        let mut semilla: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut azar = |n: usize| {
            semilla ^= semilla << 13;
            semilla ^= semilla >> 7;
            semilla ^= semilla << 17;
            (semilla % n as u64) as usize
        };
        let piezas = ["a", "b", "\n", "á", "é", "😀", "ñ", " ", "𝔸", "x\ny", ""];
        let mut texto = String::from("def f():\n    return 1  # ñandú 😀\n");
        for _ in 0..2000 {
            let caracteres: Vec<(usize, char)> = texto.char_indices().collect();
            let limite = |i: usize| caracteres.get(i).map_or(texto.len(), |(b, _)| *b);
            let desde = azar(caracteres.len() + 1);
            let hasta = (desde + azar(4)).min(caracteres.len());
            let insertado: String = (0..azar(3)).map(|_| piezas[azar(piezas.len())]).collect();
            let nuevo = format!("{}{}{}", &texto[..limite(desde)], insertado, &texto[limite(hasta)..]);
            if let Some(cambio) = cambio_entre(&texto, &nuevo, ModoSincronizacion::Incremental) {
                assert_eq!(aplicar(&texto, &cambio), nuevo, "de {texto:?} a {nuevo:?}: {cambio:?}");
            } else {
                assert_eq!(texto, nuevo);
            }
            texto = nuevo;
        }
    }
}
