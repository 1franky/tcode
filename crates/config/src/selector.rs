use crate::tema::{InfoTema, TEMAS_EMBEBIDOS};

/// Filtro de tipo de tema del selector (`Ctrl+K Ctrl+T`, PLAN.md §7). El
/// plan también menciona "alto contraste", pero ningún tema embebido está
/// etiquetado así todavía — se suma cuando exista uno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FiltroTipoTema {
    #[default]
    Todos,
    Oscuro,
    Claro,
}

impl FiltroTipoTema {
    /// Etiqueta en español para mostrar en la barra del selector.
    pub fn etiqueta(&self) -> &'static str {
        match self {
            FiltroTipoTema::Todos => "Todos",
            FiltroTipoTema::Oscuro => "Oscuro",
            FiltroTipoTema::Claro => "Claro",
        }
    }

    fn siguiente(&self) -> Self {
        match self {
            FiltroTipoTema::Todos => FiltroTipoTema::Oscuro,
            FiltroTipoTema::Oscuro => FiltroTipoTema::Claro,
            FiltroTipoTema::Claro => FiltroTipoTema::Todos,
        }
    }

    fn coincide(&self, tipo: &str) -> bool {
        match self {
            FiltroTipoTema::Todos => true,
            FiltroTipoTema::Oscuro => tipo == "dark",
            FiltroTipoTema::Claro => tipo == "light",
        }
    }
}

/// Estado del selector de temas (`Ctrl+K Ctrl+T`, PLAN.md §5/§7): si está
/// abierto, con qué filtro, qué fila está seleccionada, y cuál era el tema
/// activo antes de abrirlo (para poder revertir con `Esc` tras el preview
/// en vivo). No sabe nada de terminal/`ratatui` ni de la `Paleta` de
/// colores resuelta — el crate `app` es quien, en cada movimiento de
/// selección, vuelve a cargar y aplicar la paleta del tema bajo el cursor;
/// esto solo decide QUÉ tema está bajo el cursor.
#[derive(Debug, Clone)]
pub struct EstadoSelectorTema {
    activa: bool,
    filtro: FiltroTipoTema,
    seleccion: usize,
    tema_original: String,
}

impl EstadoSelectorTema {
    pub fn nueva() -> Self {
        Self { activa: false, filtro: FiltroTipoTema::default(), seleccion: 0, tema_original: String::new() }
    }

    pub fn activa(&self) -> bool {
        self.activa
    }

    pub fn filtro(&self) -> FiltroTipoTema {
        self.filtro
    }

    /// El tema que estaba activo antes de abrir el selector — a lo que
    /// hay que volver si se cancela con `Esc`.
    pub fn tema_original(&self) -> &str {
        &self.tema_original
    }

    pub fn seleccion(&self) -> usize {
        self.seleccion
    }

    /// Abre el selector con el filtro en "Todos" y la selección puesta
    /// sobre `tema_actual` si aparece en la lista (si no, sobre la
    /// primera fila).
    pub fn abrir(&mut self, tema_actual: &str) {
        self.activa = true;
        self.filtro = FiltroTipoTema::Todos;
        self.tema_original = tema_actual.to_string();
        self.seleccion = self.temas_filtrados().iter().position(|t| t.id == tema_actual).unwrap_or(0);
    }

    pub fn cerrar(&mut self) {
        self.activa = false;
    }

    /// Temas embebidos que pasan el filtro actual, en el orden de
    /// [`TEMAS_EMBEBIDOS`].
    pub fn temas_filtrados(&self) -> Vec<InfoTema> {
        TEMAS_EMBEBIDOS.iter().filter(|t| self.filtro.coincide(t.tipo)).copied().collect()
    }

    pub fn mover_abajo(&mut self) {
        let total = self.temas_filtrados().len();
        if total == 0 {
            return;
        }
        self.seleccion = (self.seleccion + 1).min(total - 1);
    }

    pub fn mover_arriba(&mut self) {
        self.seleccion = self.seleccion.saturating_sub(1);
    }

    /// Cambia al siguiente filtro (Todos -> Oscuro -> Claro -> Todos),
    /// reseteando la selección a la primera fila del nuevo subconjunto.
    pub fn alternar_filtro(&mut self) {
        self.filtro = self.filtro.siguiente();
        self.seleccion = 0;
    }

    /// El id del tema bajo la fila seleccionada ahora mismo, si hay
    /// alguno visible con el filtro actual — es lo que `app` usa para el
    /// preview en vivo en cada movimiento.
    pub fn tema_seleccionado(&self) -> Option<&'static str> {
        self.temas_filtrados().get(self.seleccion).map(|t| t.id)
    }

    /// Confirma la fila seleccionada: cierra el selector y devuelve el id
    /// del tema a persistir (`None` si el filtro actual no deja ninguna
    /// fila visible).
    pub fn confirmar(&mut self) -> Option<&'static str> {
        let id = self.tema_seleccionado();
        self.cerrar();
        id
    }
}

impl Default for EstadoSelectorTema {
    fn default() -> Self {
        Self::nueva()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abrir_ubica_la_seleccion_sobre_el_tema_actual() {
        let mut selector = EstadoSelectorTema::nueva();
        selector.abrir("nord");
        assert!(selector.activa());
        assert_eq!(selector.tema_original(), "nord");
        assert_eq!(selector.tema_seleccionado(), Some("nord"));
    }

    #[test]
    fn abrir_con_tema_desconocido_cae_en_la_primera_fila() {
        let mut selector = EstadoSelectorTema::nueva();
        selector.abrir("no-existe-este-tema");
        assert_eq!(selector.seleccion(), 0);
    }

    #[test]
    fn mover_arriba_y_abajo_se_recorta_a_los_limites() {
        let mut selector = EstadoSelectorTema::nueva();
        selector.abrir("dracula");
        selector.mover_arriba();
        assert_eq!(selector.seleccion(), 0);

        let total = selector.temas_filtrados().len();
        for _ in 0..(total + 5) {
            selector.mover_abajo();
        }
        assert_eq!(selector.seleccion(), total - 1);
    }

    #[test]
    fn alternar_filtro_recorre_todos_oscuro_claro_y_vuelve() {
        let mut selector = EstadoSelectorTema::nueva();
        selector.abrir("dracula");
        assert_eq!(selector.filtro(), FiltroTipoTema::Todos);

        selector.alternar_filtro();
        assert_eq!(selector.filtro(), FiltroTipoTema::Oscuro);
        assert!(selector.temas_filtrados().iter().all(|t| t.tipo == "dark"));

        selector.alternar_filtro();
        assert_eq!(selector.filtro(), FiltroTipoTema::Claro);
        assert!(selector.temas_filtrados().iter().all(|t| t.tipo == "light"));

        selector.alternar_filtro();
        assert_eq!(selector.filtro(), FiltroTipoTema::Todos);
    }

    #[test]
    fn confirmar_devuelve_el_id_seleccionado_y_cierra() {
        let mut selector = EstadoSelectorTema::nueva();
        selector.abrir("dracula");
        selector.mover_abajo();
        let esperado = selector.tema_seleccionado();
        let confirmado = selector.confirmar();
        assert_eq!(confirmado, esperado);
        assert!(!selector.activa());
    }

    #[test]
    fn cancelar_no_cambia_tema_original() {
        let mut selector = EstadoSelectorTema::nueva();
        selector.abrir("dracula");
        selector.mover_abajo();
        selector.mover_abajo();
        selector.cerrar();
        assert_eq!(selector.tema_original(), "dracula");
    }
}
