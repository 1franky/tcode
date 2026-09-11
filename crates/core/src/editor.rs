use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::buffer::Buffer;
use crate::cursor::Cursor;
use crate::history::Historia;

/// Modo de edición actual. `tcode` es no-modal por defecto (PLAN.md §4): en
/// M0 solo existe `Insertar`. `Seleccion` y `Comando` se activan cuando se
/// implementen multi-cursor/selección (M3) y la paleta de comandos (M2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    Insertar,
}

/// Estado de edición de un archivo: buffer + cursor + historial de
/// deshacer/rehacer, combinados en las operaciones que un editor expone
/// (insertar, borrar, mover el cursor, guardar...).
///
/// No sabe nada de terminal ni de ratatui — eso vive en el crate `ui`, que
/// observa este estado para dibujarlo (principio arquitectónico de
/// PLAN.md §3: "el core no conoce nada de UI").
pub struct Editor {
    buffer: Buffer,
    cursor: Cursor,
    historia: Historia,
    modo: Modo,
}

impl Editor {
    pub fn nuevo() -> Self {
        Self {
            buffer: Buffer::nuevo(),
            cursor: Cursor::nuevo(),
            historia: Historia::nueva(),
            modo: Modo::Insertar,
        }
    }

    pub fn abrir(ruta: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            buffer: Buffer::desde_archivo(ruta)?,
            cursor: Cursor::nuevo(),
            historia: Historia::nueva(),
            modo: Modo::Insertar,
        })
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn modo(&self) -> Modo {
        self.modo
    }

    pub fn guardar(&mut self) -> Result<()> {
        self.buffer.guardar()
    }

    pub fn guardar_como(&mut self, ruta: impl Into<PathBuf>) -> Result<()> {
        self.buffer.guardar_como(ruta)
    }

    fn registrar_snapshot(&mut self) {
        self.historia.registrar(self.buffer.rope(), self.cursor);
    }

    /// Inserta un carácter en la posición del cursor y avanza el cursor.
    /// `'\n'` inserta un salto de línea real (usado también por `Enter`).
    pub fn insertar_char(&mut self, c: char) {
        self.registrar_snapshot();
        if c == '\n' {
            self.buffer
                .insertar_str(self.cursor.linea, self.cursor.columna, "\n");
            self.cursor.linea += 1;
            self.cursor.columna = 0;
        } else {
            self.buffer
                .insertar_char(self.cursor.linea, self.cursor.columna, c);
            self.cursor.columna += 1;
        }
    }

    /// Backspace: borra hacia atrás y fusiona con la línea anterior si el
    /// cursor está al inicio de línea.
    pub fn borrar_atras(&mut self) {
        if self.cursor.linea == 0 && self.cursor.columna == 0 {
            return;
        }
        self.registrar_snapshot();

        let fusiona_lineas = self.cursor.columna == 0;
        let columna_tras_fusion = fusiona_lineas
            .then(|| self.buffer.longitud_visible_linea(self.cursor.linea - 1));

        self.buffer.borrar_atras(self.cursor.linea, self.cursor.columna);

        if let Some(columna) = columna_tras_fusion {
            self.cursor.linea -= 1;
            self.cursor.columna = columna;
        } else {
            self.cursor.columna -= 1;
        }
    }

    /// Delete: borra el carácter bajo/después del cursor sin moverlo.
    pub fn borrar_adelante(&mut self) {
        self.registrar_snapshot();
        self.buffer
            .borrar_adelante(self.cursor.linea, self.cursor.columna);
    }

    pub fn mover_izquierda(&mut self) {
        self.cursor.mover_izquierda(&self.buffer);
    }

    pub fn mover_derecha(&mut self) {
        self.cursor.mover_derecha(&self.buffer);
    }

    pub fn mover_arriba(&mut self) {
        self.cursor.mover_arriba(&self.buffer);
    }

    pub fn mover_abajo(&mut self) {
        self.cursor.mover_abajo(&self.buffer);
    }

    pub fn inicio_linea(&mut self) {
        self.cursor.inicio_linea();
    }

    pub fn fin_linea(&mut self) {
        self.cursor.fin_linea(&self.buffer);
    }

    pub fn inicio_archivo(&mut self) {
        self.cursor.inicio_archivo();
    }

    pub fn fin_archivo(&mut self) {
        self.cursor.fin_archivo(&self.buffer);
    }

    /// Mueve el cursor a la posición del offset de bytes `offset_byte`
    /// (usado para saltar a una coincidencia de búsqueda, PLAN.md §4).
    pub fn mover_cursor_a_byte(&mut self, offset_byte: usize) {
        let (linea, columna) = self.buffer.linea_columna_desde_byte(offset_byte);
        self.cursor.linea = linea;
        self.cursor.columna = columna;
    }

    /// Reemplaza el texto en el rango de bytes `[inicio, fin)` por
    /// `reemplazo` (PLAN.md §4, "buscar.reemplazar") y deja el cursor
    /// justo después del texto insertado.
    pub fn reemplazar_rango_bytes(&mut self, inicio_byte: usize, fin_byte: usize, reemplazo: &str) {
        self.registrar_snapshot();
        self.buffer.reemplazar_rango_bytes(inicio_byte, fin_byte, reemplazo);
        self.mover_cursor_a_byte(inicio_byte + reemplazo.len());
    }

    pub fn deshacer(&mut self) {
        if let Some((rope, cursor)) = self.historia.deshacer(self.buffer.rope(), self.cursor) {
            self.buffer.reemplazar_rope(rope);
            self.cursor = cursor;
            self.cursor.recortar(&self.buffer);
        }
    }

    pub fn rehacer(&mut self) {
        if let Some((rope, cursor)) = self.historia.rehacer(self.buffer.rope(), self.cursor) {
            self.buffer.reemplazar_rope(rope);
            self.cursor = cursor;
            self.cursor.recortar(&self.buffer);
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::nuevo()
    }
}
