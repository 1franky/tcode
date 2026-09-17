use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ropey::Rope;

/// Fin de línea de un archivo. El `rope` interno de `Buffer` está *siempre*
/// normalizado a `\n` (nunca contiene `\r`) sin importar cuál sea — todo el
/// resto del editor (cursor, resaltado de sintaxis, búsqueda, vista CSV)
/// asume esa invariante y no tiene que saber nada de CRLF. Este campo solo
/// existe para reconstruir el fin de línea original al guardar
/// ([`Buffer::guardar_como`]), y para mostrarlo en la barra de estado.
///
/// Detectado y arreglado tras confirmar en Windows que abrir un archivo con
/// CRLF (lo normal ahí: scripts `.py`/`.sql`, `.txt` exportados, etc.) dejaba
/// un `\r` colgando al final de cada línea. `ratatui`/`crossterm` no tratan
/// ese `\r` como "no ocupa columna": al imprimirlo, la terminal real mueve
/// el cursor al inicio de la fila, pero el `Buffer` interno de `ratatui`
/// (que decide cuándo puede omitir un `MoveTo` porque asume que el cursor
/// ya avanzó de forma natural tras el `Print` anterior) no se entera de ese
/// salto — el resto de esa fila, y el diffing de frames siguientes, quedan
/// permanentemente desalineados. Con archivos generados en Windows esto es
/// mucho más común y más grave que el ancho ambiguo de símbolos decorativos
/// (ver `tcode_ui::BORDE_ASCII`): esto pasa con contenido común y corriente,
/// no solo con íconos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Lf,
    Crlf,
}

impl Eol {
    /// Detecta el fin de línea de `texto`: `Crlf` si contiene al menos un
    /// `\r\n` (heurística simple — en la práctica un archivo real usa un
    /// único estilo de forma consistente), `Lf` en cualquier otro caso
    /// (incluido un archivo sin saltos de línea).
    fn detectar(texto: &str) -> Self {
        if texto.contains("\r\n") {
            Eol::Crlf
        } else {
            Eol::Lf
        }
    }

    pub fn como_str(self) -> &'static str {
        match self {
            Eol::Lf => "LF",
            Eol::Crlf => "CRLF",
        }
    }
}

/// Contenido de un archivo abierto en el editor.
///
/// Usa un "rope" (`ropey`) en lugar de un `String` plano: permite insertar y
/// borrar en O(log n) incluso en archivos grandes, la misma estructura que
/// usa Helix (ver PLAN.md §2).
pub struct Buffer {
    rope: Rope,
    ruta: Option<PathBuf>,
    modificado: bool,
    eol: Eol,
}

impl Buffer {
    /// Crea un buffer vacío sin archivo asociado ("[Sin nombre]").
    pub fn nuevo() -> Self {
        Self {
            rope: Rope::new(),
            ruta: None,
            modificado: false,
            eol: Eol::Lf,
        }
    }

    /// Carga el contenido de `ruta` en un buffer nuevo. Si el archivo usa
    /// CRLF, se normaliza a `\n` para el `rope` interno (ver [`Eol`]) — el
    /// fin de línea original se recuerda para reescribirlo tal cual al
    /// guardar.
    pub fn desde_archivo(ruta: impl AsRef<Path>) -> Result<Self> {
        let ruta = ruta.as_ref();
        let contenido = std::fs::read_to_string(ruta)
            .with_context(|| format!("no se pudo leer '{}'", ruta.display()))?;
        let eol = Eol::detectar(&contenido);
        let contenido_normalizado = match eol {
            Eol::Crlf => contenido.replace("\r\n", "\n"),
            Eol::Lf => contenido,
        };
        Ok(Self {
            rope: Rope::from_str(&contenido_normalizado),
            ruta: Some(ruta.to_path_buf()),
            modificado: false,
            eol,
        })
    }

    pub fn eol(&self) -> Eol {
        self.eol
    }

    /// Guarda en la ruta ya asociada al buffer. Falla si el buffer nunca se
    /// guardó antes — quien llama (`app`) usa ese error para abrir el
    /// prompt "Guardar como" en vez de fallar en silencio (`Ctrl+Shift+S`/
    /// `Ctrl+K S`, o `Ctrl+S` sobre un buffer sin ruta).
    pub fn guardar(&mut self) -> Result<()> {
        let ruta = self
            .ruta
            .clone()
            .context("el buffer no tiene una ruta asociada; usa guardar_como")?;
        self.guardar_como(ruta)
    }

    /// Escribe el contenido actual en `ruta` y la adopta como ruta del
    /// buffer. Si el archivo se abrió con CRLF, reintroduce el `\r` antes
    /// de cada `\n` al escribir — el `rope` interno nunca lo tiene (ver
    /// [`Eol`]), así que esto no puede duplicarlo. Reemplazar dentro de
    /// cada fragmento de `chunks()` es seguro porque un `\n` (1 byte)
    /// nunca queda partido entre dos fragmentos.
    pub fn guardar_como(&mut self, ruta: impl Into<PathBuf>) -> Result<()> {
        let ruta = ruta.into();
        let mut archivo = std::fs::File::create(&ruta)
            .with_context(|| format!("no se pudo crear '{}'", ruta.display()))?;
        for fragmento in self.rope.chunks() {
            match self.eol {
                Eol::Lf => archivo.write_all(fragmento.as_bytes())?,
                Eol::Crlf => archivo.write_all(fragmento.replace('\n', "\r\n").as_bytes())?,
            }
        }
        self.ruta = Some(ruta);
        self.modificado = false;
        Ok(())
    }

    pub fn ruta(&self) -> Option<&Path> {
        self.ruta.as_deref()
    }

    pub fn modificado(&self) -> bool {
        self.modificado
    }

    pub fn num_lineas(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    /// Reemplaza el rope completo (usado por deshacer/rehacer). Marca el
    /// buffer como modificado: solo `guardar`/`guardar_como` lo limpian.
    pub fn reemplazar_rope(&mut self, rope: Rope) {
        self.rope = rope;
        self.modificado = true;
    }

    /// Líneas del archivo como texto plano, sin el salto de línea final,
    /// listas para pintar en la vista de código.
    pub fn lineas_texto(&self) -> Vec<String> {
        self.rope
            .lines()
            .map(|linea| {
                let texto = linea.to_string();
                texto.strip_suffix('\n').unwrap_or(&texto).to_string()
            })
            .collect()
    }

    /// Offset en bytes (UTF-8) del inicio de una línea dentro del texto
    /// completo. Es lo que permite correlacionar los rangos de bytes que
    /// produce `tcode-syntax` (que trabaja sobre el archivo completo) con
    /// el texto de una línea concreta al dibujarla.
    pub fn inicio_byte_linea(&self, linea: usize) -> usize {
        let linea = linea.min(self.rope.len_lines().saturating_sub(1));
        self.rope.line_to_byte(linea)
    }

    /// Longitud (en caracteres) de una línea sin contar el salto de línea.
    /// Es el límite de columna válido para el cursor en esa línea.
    pub fn longitud_visible_linea(&self, linea: usize) -> usize {
        if linea >= self.rope.len_lines() {
            return 0;
        }
        let l = self.rope.line(linea);
        let len = l.len_chars();
        if len > 0 && l.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    /// Convierte una posición línea/columna en un índice de carácter absoluto
    /// dentro del rope, recortando a límites válidos.
    fn indice_char(&self, linea: usize, columna: usize) -> usize {
        let linea = linea.min(self.rope.len_lines().saturating_sub(1));
        let inicio_linea = self.rope.line_to_char(linea);
        let columna = columna.min(self.longitud_visible_linea(linea));
        inicio_linea + columna
    }

    pub fn insertar_char(&mut self, linea: usize, columna: usize, c: char) {
        let idx = self.indice_char(linea, columna);
        self.rope.insert_char(idx, c);
        self.modificado = true;
    }

    pub fn insertar_str(&mut self, linea: usize, columna: usize, texto: &str) {
        let idx = self.indice_char(linea, columna);
        self.rope.insert(idx, texto);
        self.modificado = true;
    }

    /// Borra el carácter inmediatamente anterior al cursor (Backspace). Si el
    /// cursor está al inicio de una línea, fusiona con la anterior.
    pub fn borrar_atras(&mut self, linea: usize, columna: usize) {
        let idx = self.indice_char(linea, columna);
        if idx == 0 {
            return;
        }
        self.rope.remove(idx - 1..idx);
        self.modificado = true;
    }

    /// Borra el carácter bajo/después del cursor (Delete).
    pub fn borrar_adelante(&mut self, linea: usize, columna: usize) {
        let idx = self.indice_char(linea, columna);
        if idx >= self.rope.len_chars() {
            return;
        }
        self.rope.remove(idx..idx + 1);
        self.modificado = true;
    }

    pub fn a_texto(&self) -> String {
        self.rope.to_string()
    }

    /// Convierte un offset de bytes absoluto (en el texto completo, el
    /// mismo tipo de offset que usan `tcode-syntax` y la búsqueda de
    /// `tcode_core::busqueda`) a una posición línea/columna — columna en
    /// caracteres, igual que el resto de `Cursor`. Usado para mover el
    /// cursor a una coincidencia de búsqueda.
    pub fn linea_columna_desde_byte(&self, offset_byte: usize) -> (usize, usize) {
        let offset_byte = offset_byte.min(self.rope.len_bytes());
        let linea = self.rope.byte_to_line(offset_byte);
        let inicio_linea_byte = self.rope.line_to_byte(linea);
        let offset_local_byte = offset_byte - inicio_linea_byte;
        let columna = self.rope.line(linea).byte_to_char(offset_local_byte);
        (linea, columna)
    }

    /// Reemplaza el texto en el rango de bytes `[inicio, fin)` por
    /// `reemplazo` (usado por "reemplazar" en la búsqueda, PLAN.md §4).
    pub fn reemplazar_rango_bytes(&mut self, inicio_byte: usize, fin_byte: usize, reemplazo: &str) {
        let inicio_char = self.rope.byte_to_char(inicio_byte);
        let fin_char = self.rope.byte_to_char(fin_byte);
        self.rope.remove(inicio_char..fin_char);
        self.rope.insert(inicio_char, reemplazo);
        self.modificado = true;
    }

    /// Offset de bytes absoluto de una posición línea/columna, recortada
    /// a límites válidos — inversa de `linea_columna_desde_byte`. Permite
    /// tratar cualquier cursor como un offset de bytes uniforme, el mismo
    /// sistema de coordenadas que ya usan la búsqueda y el CSV (multi-
    /// cursor, PLAN.md §11 M3).
    pub fn offset_byte(&self, linea: usize, columna: usize) -> usize {
        self.rope.char_to_byte(self.indice_char(linea, columna))
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::nuevo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Escribe `contenido` en un archivo temporal y devuelve su ruta —
    /// `tempfile` no es una dependencia del crate, así que se usa
    /// `std::env::temp_dir()` con un nombre único por test para no pisarse
    /// entre ejecuciones en paralelo.
    fn archivo_temporal(nombre: &str, contenido: &[u8]) -> PathBuf {
        let ruta = std::env::temp_dir().join(format!("tcode_buffer_test_{nombre}_{}", std::process::id()));
        std::fs::write(&ruta, contenido).unwrap();
        ruta
    }

    #[test]
    fn buffer_nuevo_es_lf_por_defecto() {
        assert_eq!(Buffer::nuevo().eol(), Eol::Lf);
    }

    #[test]
    fn desde_archivo_detecta_lf() {
        let ruta = archivo_temporal("lf", b"fn main() {\n    1\n}\n");
        let buffer = Buffer::desde_archivo(&ruta).unwrap();
        assert_eq!(buffer.eol(), Eol::Lf);
        std::fs::remove_file(ruta).ok();
    }

    /// El caso que rompía en Windows: un `\r` colgando al final de cada
    /// línea si no se normaliza al cargar (ver doc de [`Eol`]).
    #[test]
    fn desde_archivo_detecta_crlf_y_normaliza_el_rope_a_solo_lf() {
        let ruta = archivo_temporal("crlf", b"fn main() {\r\n    1\r\n}\r\n");
        let buffer = Buffer::desde_archivo(&ruta).unwrap();
        assert_eq!(buffer.eol(), Eol::Crlf);
        assert!(!buffer.a_texto().contains('\r'));
        assert_eq!(buffer.lineas_texto(), vec!["fn main() {", "    1", "}", ""]);
        std::fs::remove_file(ruta).ok();
    }

    #[test]
    fn guardar_como_reescribe_crlf_tal_cual_lo_encontro() {
        let origen = archivo_temporal("crlf_origen", b"a\r\nb\r\n");
        let mut buffer = Buffer::desde_archivo(&origen).unwrap();
        buffer.insertar_char(0, 1, 'X'); // "aX\r\nb\r\n" tras reescribir

        let destino = archivo_temporal("crlf_destino", b"");
        buffer.guardar_como(&destino).unwrap();
        let bytes_guardados = std::fs::read(&destino).unwrap();
        assert_eq!(bytes_guardados, b"aX\r\nb\r\n");

        std::fs::remove_file(origen).ok();
        std::fs::remove_file(destino).ok();
    }

    #[test]
    fn guardar_como_no_introduce_cr_en_un_archivo_lf() {
        let origen = archivo_temporal("lf_origen", b"a\nb\n");
        let mut buffer = Buffer::desde_archivo(&origen).unwrap();

        let destino = archivo_temporal("lf_destino", b"");
        buffer.guardar_como(&destino).unwrap();
        let bytes_guardados = std::fs::read(&destino).unwrap();
        assert_eq!(bytes_guardados, b"a\nb\n");

        std::fs::remove_file(origen).ok();
        std::fs::remove_file(destino).ok();
    }
}
