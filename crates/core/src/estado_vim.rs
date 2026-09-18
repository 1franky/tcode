/// Estado del modo VIM opcional (M5, `config.editor.modo_vim`): el
/// registro sin nombre (lo que dejó el último `dd`/`yy`, lo que pega
/// `p`) y el primer carácter de un comando de dos teclas en espera del
/// segundo (`dd`/`yy`/`gg`). Uno solo para toda la app, no por panel —
/// igual que el registro por defecto de VIM real, que es del proceso, no
/// de cada buffer (yanquear en un archivo y pegar en otro funciona). Ver
/// `crates/app/src/vim.rs` para la interpretación de teclas que lo usa.
#[derive(Debug, Clone, Default)]
pub struct EstadoVim {
    pendiente: Option<char>,
    registro: String,
}

impl EstadoVim {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Texto del registro sin nombre — vacío si todavía no se yanqueó ni
    /// borró ninguna línea en esta sesión.
    pub fn registro(&self) -> &str {
        &self.registro
    }

    pub fn fijar_registro(&mut self, texto: String) {
        self.registro = texto;
    }

    /// Primer carácter de un comando de dos teclas en espera del segundo
    /// (`Some('d')` tras un `d` suelto, a la espera de otro `d` para
    /// completar `dd`), o `None` si no hay ninguno pendiente.
    pub fn pendiente(&self) -> Option<char> {
        self.pendiente
    }

    pub fn fijar_pendiente(&mut self, c: char) {
        self.pendiente = Some(c);
    }

    /// Limpia el comando de dos teclas en espera (una segunda tecla que
    /// no coincidió, o `Esc`) sin tocar el registro.
    pub fn limpiar_pendiente(&mut self) {
        self.pendiente = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nuevo_arranca_sin_pendiente_y_con_el_registro_vacio() {
        let vim = EstadoVim::nuevo();
        assert_eq!(vim.pendiente(), None);
        assert_eq!(vim.registro(), "");
    }

    #[test]
    fn fijar_y_limpiar_pendiente() {
        let mut vim = EstadoVim::nuevo();
        vim.fijar_pendiente('d');
        assert_eq!(vim.pendiente(), Some('d'));
        vim.limpiar_pendiente();
        assert_eq!(vim.pendiente(), None);
    }

    #[test]
    fn fijar_registro_reemplaza_el_anterior() {
        let mut vim = EstadoVim::nuevo();
        vim.fijar_registro("primera\n".to_string());
        assert_eq!(vim.registro(), "primera\n");
        vim.fijar_registro("segunda\n".to_string());
        assert_eq!(vim.registro(), "segunda\n");
    }
}
