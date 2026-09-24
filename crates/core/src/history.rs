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
    /// Grupo de ediciones abierto (modo VIM: `cw` + lo tipeado hasta
    /// `Esc`, `o` + texto...): `Some(ya_registro)` mientras dura. Solo el
    /// PRIMER `registrar` del grupo guarda una foto; los siguientes solo
    /// invalidan rehacer, así todo el grupo se deshace en un paso. `None`
    /// (lo normal) = cada edición es su propio paso, como siempre.
    grupo: Option<bool>,
}

impl Historia {
    pub fn nueva() -> Self {
        Self::default()
    }

    /// Debe llamarse ANTES de aplicar una edición, guardando el estado al
    /// que se podría volver. Cualquier registro invalida la pila de rehacer.
    pub fn registrar(&mut self, rope: &Rope, cursores: &[CursorMultiple]) {
        if let Some(ya_registro) = &mut self.grupo {
            if *ya_registro {
                self.rehacer.clear();
                return;
            }
            *ya_registro = true;
        }
        self.deshacer.push(Snapshot { rope: rope.clone(), cursores: cursores.to_vec() });
        self.rehacer.clear();
    }

    /// Devuelve el estado anterior, o `None` si no hay nada que deshacer.
    /// El estado actual se guarda en la pila de rehacer.
    /// Abre un grupo de ediciones que se deshacen juntas (ver
    /// `Historia::grupo`). Abrir uno con otro ya abierto lo reinicia.
    pub fn abrir_grupo(&mut self) {
        self.grupo = Some(false);
    }

    /// Cierra el grupo abierto, si hay uno — la próxima edición vuelve a
    /// ser un paso propio.
    pub fn cerrar_grupo(&mut self) {
        self.grupo = None;
    }

    pub fn deshacer(&mut self, rope_actual: &Rope, cursores_actuales: &[CursorMultiple]) -> Option<(Rope, Vec<CursorMultiple>)> {
        self.grupo = None;
        let snapshot = self.deshacer.pop()?;
        self.rehacer.push(Snapshot { rope: rope_actual.clone(), cursores: cursores_actuales.to_vec() });
        Some((snapshot.rope, snapshot.cursores))
    }

    /// Devuelve el estado deshecho más reciente, o `None` si no hay nada que
    /// rehacer. El estado actual se guarda de vuelta en la pila de deshacer.
    pub fn rehacer(&mut self, rope_actual: &Rope, cursores_actuales: &[CursorMultiple]) -> Option<(Rope, Vec<CursorMultiple>)> {
        self.grupo = None;
        let snapshot = self.rehacer.pop()?;
        self.deshacer.push(Snapshot { rope: rope_actual.clone(), cursores: cursores_actuales.to_vec() });
        Some((snapshot.rope, snapshot.cursores))
    }
}
