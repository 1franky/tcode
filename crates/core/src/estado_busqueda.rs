use crate::busqueda::{buscar_coincidencias, Coincidencia, OpcionesBusqueda};

/// Qué campo recibe el texto que se escribe: la consulta, o (solo en modo
/// reemplazar) el texto de reemplazo. `Tab` alterna entre ambos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoBusqueda {
    Consulta,
    Reemplazo,
}

/// Estado de la barra de búsqueda/reemplazo (`Ctrl+F`/`Ctrl+H`, PLAN.md
/// §4): si está abierta, qué se escribió, las opciones activas
/// (`Alt+R`/`Alt+C`/`Alt+W`) y las coincidencias actuales. No sabe nada
/// de terminal/UI ni del `Editor` — el crate `ui` la dibuja; `app` decide
/// qué tecla llega aquí y usa `Editor::reemplazar_rango_bytes` con la
/// coincidencia que corresponda.
pub struct EstadoBusqueda {
    activa: bool,
    modo_reemplazar: bool,
    consulta: String,
    reemplazo: String,
    campo_activo: CampoBusqueda,
    opciones: OpcionesBusqueda,
    coincidencias: Vec<Coincidencia>,
    indice_actual: Option<usize>,
    /// Error de compilación del patrón (solo posible con `regex`
    /// activado) — se conserva para que la UI lo muestre en vez de
    /// simplemente vaciar los resultados sin explicación.
    error: Option<String>,
}

impl EstadoBusqueda {
    pub fn nueva() -> Self {
        Self {
            activa: false,
            modo_reemplazar: false,
            consulta: String::new(),
            reemplazo: String::new(),
            campo_activo: CampoBusqueda::Consulta,
            opciones: OpcionesBusqueda::default(),
            coincidencias: Vec::new(),
            indice_actual: None,
            error: None,
        }
    }

    pub fn activa(&self) -> bool {
        self.activa
    }

    pub fn modo_reemplazar(&self) -> bool {
        self.modo_reemplazar
    }

    pub fn consulta(&self) -> &str {
        &self.consulta
    }

    pub fn reemplazo(&self) -> &str {
        &self.reemplazo
    }

    pub fn campo_activo(&self) -> CampoBusqueda {
        self.campo_activo
    }

    pub fn opciones(&self) -> OpcionesBusqueda {
        self.opciones
    }

    pub fn coincidencias(&self) -> &[Coincidencia] {
        &self.coincidencias
    }

    pub fn indice_actual(&self) -> Option<usize> {
        self.indice_actual
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn coincidencia_actual(&self) -> Option<Coincidencia> {
        self.indice_actual.and_then(|i| self.coincidencias.get(i).copied())
    }

    /// Abre la barra (`Ctrl+F` con `modo_reemplazar = false`, `Ctrl+H`
    /// con `true`). La consulta y las opciones se conservan entre
    /// aperturas, como en VSCode: reabrir recuerda la última búsqueda.
    pub fn abrir(&mut self, modo_reemplazar: bool, texto: &str) {
        self.activa = true;
        self.modo_reemplazar = modo_reemplazar;
        self.campo_activo = CampoBusqueda::Consulta;
        self.recalcular(texto);
    }

    pub fn cerrar(&mut self) {
        self.activa = false;
    }

    pub fn escribir(&mut self, c: char, texto: &str) {
        match self.campo_activo {
            CampoBusqueda::Consulta => {
                self.consulta.push(c);
                self.recalcular(texto);
            }
            CampoBusqueda::Reemplazo => self.reemplazo.push(c),
        }
    }

    pub fn borrar(&mut self, texto: &str) {
        match self.campo_activo {
            CampoBusqueda::Consulta => {
                self.consulta.pop();
                self.recalcular(texto);
            }
            CampoBusqueda::Reemplazo => {
                self.reemplazo.pop();
            }
        }
    }

    /// `Tab`: alterna entre el campo de consulta y el de reemplazo. No
    /// hace nada si la barra está en modo "solo buscar" (no hay campo de
    /// reemplazo que mostrar).
    pub fn alternar_campo(&mut self) {
        if !self.modo_reemplazar {
            return;
        }
        self.campo_activo = match self.campo_activo {
            CampoBusqueda::Consulta => CampoBusqueda::Reemplazo,
            CampoBusqueda::Reemplazo => CampoBusqueda::Consulta,
        };
    }

    pub fn alternar_regex(&mut self, texto: &str) {
        self.opciones.regex = !self.opciones.regex;
        self.recalcular(texto);
    }

    pub fn alternar_mayusculas(&mut self, texto: &str) {
        self.opciones.sensible_mayusculas = !self.opciones.sensible_mayusculas;
        self.recalcular(texto);
    }

    pub fn alternar_palabra(&mut self, texto: &str) {
        self.opciones.palabra_completa = !self.opciones.palabra_completa;
        self.recalcular(texto);
    }

    /// Vuelve a calcular las coincidencias contra `texto` (el contenido
    /// actual del buffer). Hay que llamarlo también después de cualquier
    /// edición del documento mientras la búsqueda está abierta (p. ej.
    /// tras un reemplazo) — el texto cambió y los offsets viejos ya no
    /// sirven.
    pub fn recalcular(&mut self, texto: &str) {
        match buscar_coincidencias(texto, &self.consulta, self.opciones) {
            Ok(coincidencias) => {
                self.error = None;
                self.indice_actual = if coincidencias.is_empty() { None } else { Some(0) };
                self.coincidencias = coincidencias;
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.coincidencias.clear();
                self.indice_actual = None;
            }
        }
    }

    /// Vuelve a calcular las coincidencias contra `texto` tras una edición
    /// (p. ej. un reemplazo, PLAN.md §4) y deja seleccionada la primera
    /// coincidencia en o después de `offset_byte` (el punto donde quedó
    /// el cursor) en vez de saltar siempre a la primera del archivo como
    /// haría `recalcular` — así "reemplazar" se siente como que avanza,
    /// no que reinicia la búsqueda. Si no hay ninguna a partir de ahí, da
    /// la vuelta a la primera del archivo (mismo criterio que `siguiente`).
    pub fn recalcular_y_posicionar(&mut self, texto: &str, offset_byte: usize) {
        self.recalcular(texto);
        if self.coincidencias.is_empty() {
            return;
        }
        self.indice_actual = Some(self.coincidencias.iter().position(|c| c.inicio >= offset_byte).unwrap_or(0));
    }

    pub fn siguiente(&mut self) {
        if self.coincidencias.is_empty() {
            return;
        }
        self.indice_actual = Some(match self.indice_actual {
            Some(i) => (i + 1) % self.coincidencias.len(),
            None => 0,
        });
    }

    pub fn anterior(&mut self) {
        if self.coincidencias.is_empty() {
            return;
        }
        self.indice_actual = Some(match self.indice_actual {
            Some(0) | None => self.coincidencias.len() - 1,
            Some(i) => i - 1,
        });
    }
}

impl Default for EstadoBusqueda {
    fn default() -> Self {
        Self::nueva()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abrir_calcula_coincidencias_de_la_consulta_recordada() {
        let mut estado = EstadoBusqueda::nueva();
        for c in "ola".chars() {
            estado.escribir(c, "hola holaola");
        }
        estado.cerrar();
        estado.abrir(false, "hola holaola");
        // "ola" aparece en "hola" (índice 1) y dos veces dentro de
        // "holaola" (índices 1 y 4): 3 en total.
        assert_eq!(estado.coincidencias().len(), 3);
    }

    #[test]
    fn escribir_actualiza_coincidencias_en_vivo() {
        let mut estado = EstadoBusqueda::nueva();
        estado.abrir(false, "gato gato perro");
        for c in "gato".chars() {
            estado.escribir(c, "gato gato perro");
        }
        assert_eq!(estado.coincidencias().len(), 2);
        assert_eq!(estado.indice_actual(), Some(0));
    }

    #[test]
    fn siguiente_y_anterior_dan_la_vuelta() {
        let mut estado = EstadoBusqueda::nueva();
        estado.abrir(false, "a a a");
        for c in "a".chars() {
            estado.escribir(c, "a a a");
        }
        assert_eq!(estado.coincidencias().len(), 3);

        estado.siguiente();
        assert_eq!(estado.indice_actual(), Some(1));
        estado.siguiente();
        estado.siguiente();
        assert_eq!(estado.indice_actual(), Some(0)); // dio la vuelta

        estado.anterior();
        assert_eq!(estado.indice_actual(), Some(2)); // dio la vuelta al revés
    }

    #[test]
    fn alternar_campo_solo_funciona_en_modo_reemplazar() {
        let mut estado = EstadoBusqueda::nueva();
        estado.abrir(false, "");
        estado.alternar_campo();
        assert_eq!(estado.campo_activo(), CampoBusqueda::Consulta);

        estado.abrir(true, "");
        estado.alternar_campo();
        assert_eq!(estado.campo_activo(), CampoBusqueda::Reemplazo);
        estado.alternar_campo();
        assert_eq!(estado.campo_activo(), CampoBusqueda::Consulta);
    }

    #[test]
    fn regex_invalido_deja_un_error_y_vacia_las_coincidencias() {
        let mut estado = EstadoBusqueda::nueva();
        estado.abrir(false, "texto");
        estado.alternar_regex("texto");
        for c in "(".chars() {
            estado.escribir(c, "texto");
        }
        assert!(estado.error().is_some());
        assert!(estado.coincidencias().is_empty());
    }

    #[test]
    fn recalcular_y_posicionar_avanza_a_partir_del_punto_de_edicion() {
        let mut estado = EstadoBusqueda::nueva();
        estado.abrir(false, "gato gato gato");
        for c in "gato".chars() {
            estado.escribir(c, "gato gato gato");
        }
        assert_eq!(estado.coincidencias().len(), 3);

        // Tras "reemplazar" la primera coincidencia (offsets 0..4) por
        // algo que ya no matchea, el texto queda "X gato gato" y el
        // punto de edición es el byte 1 (justo después de "X").
        estado.recalcular_y_posicionar("X gato gato", 1);
        assert_eq!(estado.coincidencias().len(), 2);
        // La primera coincidencia restante empieza en el byte 2 ("X "),
        // que es >= 1: debe quedar seleccionada esa, no la de después.
        assert_eq!(estado.indice_actual(), Some(0));
    }

    #[test]
    fn recalcular_y_posicionar_da_la_vuelta_si_no_hay_coincidencias_despues_del_punto() {
        let mut estado = EstadoBusqueda::nueva();
        estado.abrir(false, "gato");
        for c in "gato".chars() {
            estado.escribir(c, "gato");
        }
        // El punto de edición (100) queda después de cualquier
        // coincidencia posible: debe dar la vuelta a la primera (índice 0).
        estado.recalcular_y_posicionar("gato", 100);
        assert_eq!(estado.indice_actual(), Some(0));
    }

    #[test]
    fn escribir_en_el_campo_reemplazo_no_toca_las_coincidencias() {
        let mut estado = EstadoBusqueda::nueva();
        estado.abrir(true, "hola");
        for c in "hola".chars() {
            estado.escribir(c, "hola");
        }
        let coincidencias_antes = estado.coincidencias().len();

        estado.alternar_campo();
        for c in "chau".chars() {
            estado.escribir(c, "hola");
        }
        assert_eq!(estado.reemplazo(), "chau");
        assert_eq!(estado.coincidencias().len(), coincidencias_antes);
    }
}
