use crate::combinacion::{Combinacion, Tecla};
use crate::keymap::Keymap;

/// Resultado de alimentar una combinación de teclas al [`Resolvedor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolucion {
    /// La secuencia acumulada coincide exactamente con un atajo.
    Comando(String),
    /// La secuencia acumulada es el prefijo de un atajo más largo: hay que
    /// esperar la siguiente tecla antes de decidir nada (chord en curso,
    /// p. ej. tras `Ctrl+K`).
    Pendiente,
    /// Ni la tecla ni ninguna secuencia que empiece con ella están
    /// configuradas. Si es una tecla suelta, quien llama puede tratarla
    /// como texto normal.
    SinCoincidencia,
    /// Rompió un chord en curso (o llegó `Esc` durante uno). A diferencia
    /// de `SinCoincidencia`, esto NO debe insertarse como texto: el chord
    /// entero era una intención de comando, no de escritura.
    Cancelado,
}

/// Máquina de estados que acumula combinaciones de teclas hasta resolver un
/// atajo completo, soportando secuencias encadenadas (chords) como
/// `Ctrl+K Ctrl+O` (PLAN.md §4).
///
/// Es dueño de su propio `Keymap` (en vez de tomarlo prestado) a propósito:
/// el editor de atajos del panel de administración (`Ctrl+,`, PLAN.md §5,
/// M4) necesita poder reemplazar el keymap activo en caliente tras
/// personalizar un atajo, y un `Keymap` prestado con lifetime propio
/// (`&'k Keymap`) hace que `Resolvedor` no pueda sobrevivir a que su
/// keymap original se reemplace en el mismo scope — clonar el `Keymap`
/// (un `HashMap` de unas pocas decenas de entradas) es un costo
/// insignificante comparado con lo que simplifica.
pub struct Resolvedor {
    keymap: Keymap,
    pendiente: Vec<Combinacion>,
}

impl Resolvedor {
    pub fn nuevo(keymap: Keymap) -> Self {
        Self { keymap, pendiente: Vec::new() }
    }

    /// Reemplaza el keymap activo (tras personalizar un atajo desde el
    /// panel de administración, o al recargar `keymap.toml` en caliente
    /// con `Ctrl+K Ctrl+L`) y descarta cualquier chord en curso — seguir
    /// esperando la continuación de un chord del keymap VIEJO con el
    /// NUEVO ya cargado podría resolver a un comando que ya no
    /// corresponde a esas teclas.
    pub fn reemplazar_keymap(&mut self, keymap: Keymap) {
        self.keymap = keymap;
        self.pendiente.clear();
    }

    /// `true` si hay un chord en curso esperando la siguiente tecla (útil
    /// para que la UI muestre algo como "Ctrl+K fue presionado...").
    pub fn chord_en_curso(&self) -> bool {
        !self.pendiente.is_empty()
    }

    pub fn procesar(&mut self, combinacion: Combinacion) -> Resolucion {
        let continuando_chord = !self.pendiente.is_empty();

        if combinacion.tecla == Tecla::Esc && continuando_chord {
            self.pendiente.clear();
            return Resolucion::Cancelado;
        }

        self.pendiente.push(combinacion);

        if let Some(comando) = self.keymap.buscar(&self.pendiente) {
            self.pendiente.clear();
            return Resolucion::Comando(comando.to_string());
        }

        if self.keymap.es_prefijo(&self.pendiente) {
            return Resolucion::Pendiente;
        }

        self.pendiente.clear();
        if continuando_chord {
            Resolucion::Cancelado
        } else {
            Resolucion::SinCoincidencia
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinacion::parsear_combinacion;
    use crate::keymap::keymap_por_defecto;

    #[test]
    fn resuelve_un_atajo_simple() {
        let keymap = keymap_por_defecto();
        let mut resolvedor = Resolvedor::nuevo(keymap);
        let r = resolvedor.procesar(parsear_combinacion("Ctrl+S").unwrap());
        assert_eq!(r, Resolucion::Comando("archivo.guardar".to_string()));
        assert!(!resolvedor.chord_en_curso());
    }

    #[test]
    fn resuelve_un_chord_de_dos_pasos() {
        let keymap = keymap_por_defecto();
        let mut resolvedor = Resolvedor::nuevo(keymap);
        let r1 = resolvedor.procesar(parsear_combinacion("Ctrl+K").unwrap());
        assert_eq!(r1, Resolucion::Pendiente);
        assert!(resolvedor.chord_en_curso());

        let r2 = resolvedor.procesar(parsear_combinacion("Ctrl+L").unwrap());
        assert_eq!(r2, Resolucion::Comando("config.recargar".to_string()));
        assert!(!resolvedor.chord_en_curso());
    }

    #[test]
    fn tecla_suelta_sin_atajo_es_sin_coincidencia() {
        let keymap = keymap_por_defecto();
        let mut resolvedor = Resolvedor::nuevo(keymap);
        let r = resolvedor.procesar(parsear_combinacion("a").unwrap());
        assert_eq!(r, Resolucion::SinCoincidencia);
    }

    #[test]
    fn chord_roto_se_cancela_sin_insertar_texto() {
        let keymap = keymap_por_defecto();
        let mut resolvedor = Resolvedor::nuevo(keymap);
        resolvedor.procesar(parsear_combinacion("Ctrl+K").unwrap());
        let r = resolvedor.procesar(parsear_combinacion("x").unwrap());
        assert_eq!(r, Resolucion::Cancelado);
        assert!(!resolvedor.chord_en_curso());
    }

    #[test]
    fn esc_cancela_un_chord_en_curso() {
        let keymap = keymap_por_defecto();
        let mut resolvedor = Resolvedor::nuevo(keymap);
        resolvedor.procesar(parsear_combinacion("Ctrl+K").unwrap());
        let r = resolvedor.procesar(parsear_combinacion("Esc").unwrap());
        assert_eq!(r, Resolucion::Cancelado);
        assert!(!resolvedor.chord_en_curso());
    }

    #[test]
    fn reemplazar_keymap_toma_efecto_de_inmediato_y_descarta_un_chord_en_curso() {
        let mut resolvedor = Resolvedor::nuevo(keymap_por_defecto());
        resolvedor.procesar(parsear_combinacion("Ctrl+K").unwrap());
        assert!(resolvedor.chord_en_curso());

        let nuevo = keymap_por_defecto().rebindear("archivo.guardar", parsear_combinacion("Ctrl+G").unwrap()).unwrap();
        resolvedor.reemplazar_keymap(nuevo);
        assert!(!resolvedor.chord_en_curso());

        let r = resolvedor.procesar(parsear_combinacion("Ctrl+G").unwrap());
        assert_eq!(r, Resolucion::Comando("archivo.guardar".to_string()));
    }
}
