use crate::tema::{descubrir_temas_usuario, InfoTemaListado, TEMAS_EMBEBIDOS};

/// Filtro de tipo de tema del selector (`Ctrl+K Ctrl+T`, PLAN.md §7): los
/// tres que menciona el plan — "alto contraste" quedó pendiente hasta que
/// existiera un tema realmente etiquetado así (`Tema::alto_contraste`,
/// M5), ya lo hay (`alto-contraste.toml`). Es un eje independiente de
/// Oscuro/Claro (`InfoTema::tipo`): un tema de alto contraste también es
/// "dark" o "light" para ESE otro filtro, pero acá se filtra por la
/// bandera, no por `tipo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FiltroTipoTema {
    #[default]
    Todos,
    Oscuro,
    Claro,
    AltoContraste,
}

impl FiltroTipoTema {
    /// Etiqueta en español para mostrar en la barra del selector.
    pub fn etiqueta(&self) -> &'static str {
        match self {
            FiltroTipoTema::Todos => "Todos",
            FiltroTipoTema::Oscuro => "Oscuro",
            FiltroTipoTema::Claro => "Claro",
            FiltroTipoTema::AltoContraste => "Alto contraste",
        }
    }

    fn siguiente(&self) -> Self {
        match self {
            FiltroTipoTema::Todos => FiltroTipoTema::Oscuro,
            FiltroTipoTema::Oscuro => FiltroTipoTema::Claro,
            FiltroTipoTema::Claro => FiltroTipoTema::AltoContraste,
            FiltroTipoTema::AltoContraste => FiltroTipoTema::Todos,
        }
    }

    fn coincide(&self, tipo: &str, alto_contraste: bool) -> bool {
        match self {
            FiltroTipoTema::Todos => true,
            FiltroTipoTema::Oscuro => tipo == "dark",
            FiltroTipoTema::Claro => tipo == "light",
            FiltroTipoTema::AltoContraste => alto_contraste,
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

    /// Temas que pasan el filtro actual: primero los embebidos (en el
    /// orden de [`TEMAS_EMBEBIDOS`]), después los que el usuario dejó en
    /// su carpeta de temas (`descubrir_temas_usuario`, alfabético) — ver
    /// PLAN.md §7 "Compartir temas: importar temas desde archivo".
    pub fn temas_filtrados(&self) -> Vec<InfoTemaListado> {
        TEMAS_EMBEBIDOS
            .iter()
            .map(InfoTemaListado::from)
            .chain(descubrir_temas_usuario())
            .filter(|t| self.filtro.coincide(&t.tipo, t.alto_contraste))
            .collect()
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

    /// Cambia al siguiente filtro (Todos -> Oscuro -> Claro -> Alto
    /// contraste -> Todos), reseteando la selección a la primera fila del
    /// nuevo subconjunto.
    pub fn alternar_filtro(&mut self) {
        self.filtro = self.filtro.siguiente();
        self.seleccion = 0;
    }

    /// El id del tema bajo la fila seleccionada ahora mismo, si hay
    /// alguno visible con el filtro actual — es lo que `app` usa para el
    /// preview en vivo en cada movimiento. `String` propio (no
    /// `&'static str`) porque un tema descubierto en la carpeta de
    /// usuario no tiene esa duración de vida.
    pub fn tema_seleccionado(&self) -> Option<String> {
        self.temas_filtrados().into_iter().nth(self.seleccion).map(|t| t.id)
    }

    /// Confirma la fila seleccionada: cierra el selector y devuelve el id
    /// del tema a persistir (`None` si el filtro actual no deja ninguna
    /// fila visible).
    pub fn confirmar(&mut self) -> Option<String> {
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
        assert_eq!(selector.tema_seleccionado(), Some("nord".to_string()));
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
    fn alternar_filtro_recorre_todos_oscuro_claro_alto_contraste_y_vuelve() {
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
        assert_eq!(selector.filtro(), FiltroTipoTema::AltoContraste);
        let alto_contraste = selector.temas_filtrados();
        assert!(alto_contraste.iter().all(|t| t.alto_contraste));
        assert!(alto_contraste.iter().any(|t| t.id == "alto-contraste"));

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
