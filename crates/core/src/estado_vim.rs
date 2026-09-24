use crate::cursor::Cursor;
use crate::vim::gramatica::{BusquedaCaracter, Comando};
use crate::vim::linea_comando::EstadoLineaComando;

/// Estado del modo VIM opcional (M5, `config.editor.modo_vim`): el
/// registro sin nombre (lo que dejó el último `d`/`c`/`y`/`x`, lo que
/// pega `p`), las teclas de un comando a medio escribir (`d`, `3`,
/// `di`...), la última búsqueda `f`/`t` (para `;`/`,`), el ancla del modo
/// Visual, el último cambio (para `.`) y la línea de comandos `:`. Uno
/// solo para toda la app, no por panel — igual que el registro por
/// defecto de VIM real, que es del proceso, no de cada buffer (yanquear
/// en un archivo y pegar en otro funciona). Ver `tcode_core::vim` para la
/// gramática y la ejecución de los comandos que lo usan.
#[derive(Debug, Clone, Default)]
pub struct EstadoVim {
    teclas: Vec<char>,
    registro: String,
    registro_lineal: bool,
    /// Última búsqueda `f`/`t`/`F`/`T`, que repiten `;` y `,`.
    pub ultima_busqueda: Option<BusquedaCaracter>,
    /// Extremo fijo de la selección del modo Visual (el otro es el
    /// cursor del editor).
    pub ancla_visual: Option<Cursor>,
    /// Último cambio repetible con `.`.
    pub ultimo_cambio: Option<CambioRepetible>,
    /// Inserción en curso iniciada por un comando VIM (`i`, `cw`, `o`...):
    /// dónde empezó, para saber al volver a Normal qué se tipeó y poder
    /// repetirlo con `.`.
    pub insercion: Option<InicioInsercion>,
    /// Mientras `.` está reproduciendo un cambio (no se vuelve a grabar).
    pub repitiendo: bool,
    pub linea_comando: EstadoLineaComando,
}

/// Lo que `.` vuelve a hacer: el comando y, si entró a Insertar, el
/// texto que se tipeó hasta `Esc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CambioRepetible {
    pub comando: Comando,
    pub texto_insertado: Option<String>,
}

/// Dónde empezó una inserción: offset de bytes del cursor y largo total
/// del buffer en ese momento. Al salir, si el cursor quedó justo después
/// de lo agregado, lo tipeado es `texto[offset..offset + crecimiento]`
/// (cubre escribir, `Enter` y `Backspace` dentro de lo tipeado; si se
/// movió a otro lado, no se graba texto para `.`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InicioInsercion {
    pub offset: usize,
    pub largo: usize,
}

impl EstadoVim {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Texto del registro sin nombre — vacío si todavía no se yanqueó ni
    /// borró nada en esta sesión.
    pub fn registro(&self) -> &str {
        &self.registro
    }

    /// Si el registro son líneas enteras (`dd`, `yy`, `dj`...: `p` pega
    /// debajo de la línea) o caracteres sueltos (`dw`, `x`, `y$`...: `p`
    /// pega después del cursor). Un registro por líneas siempre termina
    /// en `\n`.
    pub fn registro_lineal(&self) -> bool {
        self.registro_lineal
    }

    pub fn fijar_registro(&mut self, mut texto: String, lineal: bool) {
        if lineal && !texto.ends_with('\n') {
            texto.push('\n');
        }
        self.registro = texto;
        self.registro_lineal = lineal;
    }

    /// Teclas de un comando a medio escribir (`['d']` tras un `d` suelto,
    /// `['2', 'd', 'i']`...) — vacío si no hay ninguno pendiente.
    pub fn teclas_pendientes(&self) -> &[char] {
        &self.teclas
    }

    pub fn agregar_tecla(&mut self, c: char) {
        self.teclas.push(c);
    }

    /// Descarta el comando a medio escribir (completo, inválido, o `Esc`)
    /// sin tocar el registro.
    pub fn limpiar_pendiente(&mut self) {
        self.teclas.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nuevo_arranca_sin_pendiente_y_con_el_registro_vacio() {
        let vim = EstadoVim::nuevo();
        assert!(vim.teclas_pendientes().is_empty());
        assert_eq!(vim.registro(), "");
        assert!(!vim.linea_comando.activa());
    }

    #[test]
    fn agregar_y_limpiar_teclas_pendientes() {
        let mut vim = EstadoVim::nuevo();
        vim.agregar_tecla('d');
        vim.agregar_tecla('i');
        assert_eq!(vim.teclas_pendientes(), &['d', 'i']);
        vim.limpiar_pendiente();
        assert!(vim.teclas_pendientes().is_empty());
    }

    #[test]
    fn fijar_registro_reemplaza_el_anterior_y_normaliza_las_lineas() {
        let mut vim = EstadoVim::nuevo();
        vim.fijar_registro("primera\n".to_string(), true);
        assert_eq!(vim.registro(), "primera\n");
        assert!(vim.registro_lineal());
        vim.fijar_registro("sin salto".to_string(), true);
        assert_eq!(vim.registro(), "sin salto\n");
        vim.fijar_registro("abc".to_string(), false);
        assert_eq!(vim.registro(), "abc");
        assert!(!vim.registro_lineal());
    }
}
