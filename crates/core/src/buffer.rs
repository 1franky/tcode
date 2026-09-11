use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ropey::Rope;

/// Contenido de un archivo abierto en el editor.
///
/// Usa un "rope" (`ropey`) en lugar de un `String` plano: permite insertar y
/// borrar en O(log n) incluso en archivos grandes, la misma estructura que
/// usa Helix (ver PLAN.md §2).
pub struct Buffer {
    rope: Rope,
    ruta: Option<PathBuf>,
    modificado: bool,
}

impl Buffer {
    /// Crea un buffer vacío sin archivo asociado ("[Sin nombre]").
    pub fn nuevo() -> Self {
        Self {
            rope: Rope::new(),
            ruta: None,
            modificado: false,
        }
    }

    /// Carga el contenido de `ruta` en un buffer nuevo.
    pub fn desde_archivo(ruta: impl AsRef<Path>) -> Result<Self> {
        let ruta = ruta.as_ref();
        let contenido = std::fs::read_to_string(ruta)
            .with_context(|| format!("no se pudo leer '{}'", ruta.display()))?;
        Ok(Self {
            rope: Rope::from_str(&contenido),
            ruta: Some(ruta.to_path_buf()),
            modificado: false,
        })
    }

    /// Guarda en la ruta ya asociada al buffer. Falla si el buffer nunca se
    /// guardó antes (todavía no hay flujo de "guardar como" en M0).
    pub fn guardar(&mut self) -> Result<()> {
        let ruta = self
            .ruta
            .clone()
            .context("el buffer no tiene una ruta asociada; usa guardar_como")?;
        self.guardar_como(ruta)
    }

    /// Escribe el contenido actual en `ruta` y la adopta como ruta del buffer.
    pub fn guardar_como(&mut self, ruta: impl Into<PathBuf>) -> Result<()> {
        let ruta = ruta.into();
        let mut archivo = std::fs::File::create(&ruta)
            .with_context(|| format!("no se pudo crear '{}'", ruta.display()))?;
        for fragmento in self.rope.chunks() {
            archivo.write_all(fragmento.as_bytes())?;
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
