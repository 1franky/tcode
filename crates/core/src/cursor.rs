use crate::buffer::Buffer;

/// Posición del cursor dentro de un buffer, en coordenadas base-cero
/// (`linea`/`columna`). La UI las convierte a base-uno para mostrarlas en la
/// statusbar (`Ln 42, Col 7`, ver PLAN.md §1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub linea: usize,
    pub columna: usize,
}

impl Cursor {
    pub fn nuevo() -> Self {
        Self::default()
    }

    pub fn mover_izquierda(&mut self, buffer: &Buffer) {
        if self.columna > 0 {
            self.columna -= 1;
        } else if self.linea > 0 {
            self.linea -= 1;
            self.columna = buffer.longitud_visible_linea(self.linea);
        }
    }

    pub fn mover_derecha(&mut self, buffer: &Buffer) {
        let len = buffer.longitud_visible_linea(self.linea);
        if self.columna < len {
            self.columna += 1;
        } else if self.linea + 1 < buffer.num_lineas() {
            self.linea += 1;
            self.columna = 0;
        }
    }

    pub fn mover_arriba(&mut self, buffer: &Buffer) {
        if self.linea > 0 {
            self.linea -= 1;
            self.ajustar_columna(buffer);
        }
    }

    pub fn mover_abajo(&mut self, buffer: &Buffer) {
        if self.linea + 1 < buffer.num_lineas() {
            self.linea += 1;
            self.ajustar_columna(buffer);
        }
    }

    pub fn inicio_linea(&mut self) {
        self.columna = 0;
    }

    pub fn fin_linea(&mut self, buffer: &Buffer) {
        self.columna = buffer.longitud_visible_linea(self.linea);
    }

    pub fn inicio_archivo(&mut self) {
        self.linea = 0;
        self.columna = 0;
    }

    pub fn fin_archivo(&mut self, buffer: &Buffer) {
        self.linea = buffer.num_lineas().saturating_sub(1);
        self.columna = buffer.longitud_visible_linea(self.linea);
    }

    /// Recorta línea/columna a límites válidos del buffer dado. Se usa tras
    /// deshacer/rehacer, cuando el contenido puede haber cambiado de tamaño.
    pub fn recortar(&mut self, buffer: &Buffer) {
        let max_linea = buffer.num_lineas().saturating_sub(1);
        if self.linea > max_linea {
            self.linea = max_linea;
        }
        self.ajustar_columna(buffer);
    }

    fn ajustar_columna(&mut self, buffer: &Buffer) {
        let len = buffer.longitud_visible_linea(self.linea);
        if self.columna > len {
            self.columna = len;
        }
    }
}

/// Un cursor con selección opcional — la unidad real que maneja
/// `Editor` desde multi-cursor (`Ctrl+D`/`Ctrl+Shift+L`/`Ctrl+Alt+↑↓`,
/// PLAN.md §11 M3): `ancla` es el extremo fijo de la selección (donde
/// empezó) y `cursor` el extremo activo (el que se sigue moviendo). Sin
/// selección, `ancla == cursor` — un cursor "normal" es simplemente el
/// caso particular de esto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorMultiple {
    pub ancla: Cursor,
    pub cursor: Cursor,
}

impl CursorMultiple {
    pub fn sin_seleccion(cursor: Cursor) -> Self {
        Self { ancla: cursor, cursor }
    }

    pub fn tiene_seleccion(&self) -> bool {
        self.ancla != self.cursor
    }

    /// Recorta ambos extremos a límites válidos del buffer dado — igual
    /// que `Cursor::recortar`, tras deshacer/rehacer.
    pub fn recortar(&mut self, buffer: &Buffer) {
        self.ancla.recortar(buffer);
        self.cursor.recortar(buffer);
    }
}

impl From<Cursor> for CursorMultiple {
    fn from(cursor: Cursor) -> Self {
        Self::sin_seleccion(cursor)
    }
}

#[cfg(test)]
mod tests_multiple {
    use super::*;

    #[test]
    fn sin_seleccion_tiene_ancla_igual_al_cursor() {
        let c = Cursor { linea: 2, columna: 5 };
        let cm = CursorMultiple::sin_seleccion(c);
        assert_eq!(cm.ancla, c);
        assert_eq!(cm.cursor, c);
        assert!(!cm.tiene_seleccion());
    }

    #[test]
    fn con_ancla_distinta_tiene_seleccion() {
        let cm = CursorMultiple { ancla: Cursor { linea: 0, columna: 0 }, cursor: Cursor { linea: 0, columna: 3 } };
        assert!(cm.tiene_seleccion());
    }
}
