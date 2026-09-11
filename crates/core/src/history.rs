use ropey::Rope;

use crate::cursor::Cursor;

/// Foto del estado editable (contenido + cursor) tomada antes de una edición,
/// para poder volver a ella con "deshacer". `Rope` es una estructura de datos
/// persistente: clonarla es barato (comparte los nodos internos), así que
/// este enfoque de snapshots es válido para M0. Si en el futuro esto pesa
/// demasiado en archivos con miles de ediciones, se puede migrar a un log de
/// operaciones (diffs) sin cambiar la API pública de `Historia`.
#[derive(Clone)]
struct Snapshot {
    rope: Rope,
    cursor: Cursor,
}

/// Pilas de deshacer/rehacer de un buffer.
#[derive(Default)]
pub struct Historia {
    deshacer: Vec<Snapshot>,
    rehacer: Vec<Snapshot>,
}

impl Historia {
    pub fn nueva() -> Self {
        Self::default()
    }

    /// Debe llamarse ANTES de aplicar una edición, guardando el estado al
    /// que se podría volver. Cualquier registro invalida la pila de rehacer.
    pub fn registrar(&mut self, rope: &Rope, cursor: Cursor) {
        self.deshacer.push(Snapshot {
            rope: rope.clone(),
            cursor,
        });
        self.rehacer.clear();
    }

    /// Devuelve el estado anterior, o `None` si no hay nada que deshacer.
    /// El estado actual se guarda en la pila de rehacer.
    pub fn deshacer(&mut self, rope_actual: &Rope, cursor_actual: Cursor) -> Option<(Rope, Cursor)> {
        let snapshot = self.deshacer.pop()?;
        self.rehacer.push(Snapshot {
            rope: rope_actual.clone(),
            cursor: cursor_actual,
        });
        Some((snapshot.rope, snapshot.cursor))
    }

    /// Devuelve el estado deshecho más reciente, o `None` si no hay nada que
    /// rehacer. El estado actual se guarda de vuelta en la pila de deshacer.
    pub fn rehacer(&mut self, rope_actual: &Rope, cursor_actual: Cursor) -> Option<(Rope, Cursor)> {
        let snapshot = self.rehacer.pop()?;
        self.deshacer.push(Snapshot {
            rope: rope_actual.clone(),
            cursor: cursor_actual,
        });
        Some((snapshot.rope, snapshot.cursor))
    }
}
