use ropey::Rope;

use crate::cursor::CursorMultiple;

/// Foto del estado editable (contenido + cursores) tomada antes de una
/// edición, para poder volver a ella con "deshacer". `Rope` es una
/// estructura de datos persistente: clonarla es barato (comparte los
/// nodos internos), así que este enfoque de snapshots es válido para M0.
/// Si en el futuro esto pesa demasiado en archivos con miles de
/// ediciones, se puede migrar a un log de operaciones (diffs) sin cambiar
/// la API pública de `Historia`.
///
/// Guarda TODOS los cursores (no solo el principal): deshacer una edición
/// hecha con varios cursores a la vez (`Ctrl+D`, PLAN.md §11 M3) debe
/// devolverlos a todos a donde estaban, no colapsarlos a uno solo.
#[derive(Clone)]
struct Snapshot {
    rope: Rope,
    cursores: Vec<CursorMultiple>,
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
    pub fn registrar(&mut self, rope: &Rope, cursores: &[CursorMultiple]) {
        self.deshacer.push(Snapshot { rope: rope.clone(), cursores: cursores.to_vec() });
        self.rehacer.clear();
    }

    /// Devuelve el estado anterior, o `None` si no hay nada que deshacer.
    /// El estado actual se guarda en la pila de rehacer.
    pub fn deshacer(&mut self, rope_actual: &Rope, cursores_actuales: &[CursorMultiple]) -> Option<(Rope, Vec<CursorMultiple>)> {
        let snapshot = self.deshacer.pop()?;
        self.rehacer.push(Snapshot { rope: rope_actual.clone(), cursores: cursores_actuales.to_vec() });
        Some((snapshot.rope, snapshot.cursores))
    }

    /// Devuelve el estado deshecho más reciente, o `None` si no hay nada que
    /// rehacer. El estado actual se guarda de vuelta en la pila de deshacer.
    pub fn rehacer(&mut self, rope_actual: &Rope, cursores_actuales: &[CursorMultiple]) -> Option<(Rope, Vec<CursorMultiple>)> {
        let snapshot = self.rehacer.pop()?;
        self.deshacer.push(Snapshot { rope: rope_actual.clone(), cursores: cursores_actuales.to_vec() });
        Some((snapshot.rope, snapshot.cursores))
    }
}
