//! Pliegues entre sesiones: al cerrar un documento (pestaña, panel o
//! `tcode` entero) se recuerdan sus bloques plegados, y al volver a
//! abrirlo se restauran — si el archivo no cambió por fuera desde
//! entonces (ver `tcode_config::PlieguesGuardados`, que decide eso con
//! una huella del texto). Todo pasa solo al abrir o cerrar, nunca por
//! frame, y cualquier error de disco se ignora: en el peor caso el
//! archivo abre todo desplegado, como antes.

use std::path::Path;

use tcode_config::{huella, ruta_pliegues, PlieguesGuardados};
use tcode_core::{Editor, Pliegue};

/// Restaura en `editor` (recién abierto) los pliegues guardados de su
/// archivo, si los hay y siguen valiendo.
pub fn restaurar(editor: &mut Editor) {
    restaurar_en(&ruta_pliegues(), editor);
}

/// Recuerda los pliegues de `editores` (los documentos que se están por
/// cerrar). Escribe el archivo de estado una sola vez, y solo si algo
/// cambió.
pub fn recordar<'a>(editores: impl IntoIterator<Item = &'a Editor>) {
    recordar_en(&ruta_pliegues(), editores);
}

fn restaurar_en(archivo_estado: &Path, editor: &mut Editor) {
    let Some(ruta) = editor.buffer().ruta().and_then(|r| std::fs::canonicalize(r).ok()) else { return };
    let guardados = PlieguesGuardados::cargar(archivo_estado);
    // Sin nada guardado para este archivo no se calcula la huella (que
    // recorre el texto entero).
    if !guardados.tiene(&ruta) {
        return;
    }
    let huella_actual = huella(editor.buffer().rope().chunks());
    if let Some(guardados) = guardados.restaurar(&ruta, huella_actual) {
        let pliegues: Vec<Pliegue> = guardados.iter().map(|&(inicio, fin)| Pliegue { inicio, fin }).collect();
        // `plegar_todo` descarta lo que no oculte ninguna línea o quede
        // fuera del archivo, y saca el cursor de las líneas ocultas.
        editor.plegar_todo(&pliegues);
    }
}

fn recordar_en<'a>(archivo_estado: &Path, editores: impl IntoIterator<Item = &'a Editor>) {
    let mut guardados: Option<PlieguesGuardados> = None;
    let mut cambio = false;
    for editor in editores {
        let buffer = editor.buffer();
        // Con cambios sin guardar (se está cerrando descartándolos), los
        // pliegues son de un texto que no es el del disco: se deja lo que
        // hubiera guardado, que sigue valiendo para el archivo tal cual.
        if buffer.modificado() {
            continue;
        }
        let Some(ruta) = buffer.ruta().and_then(|r| std::fs::canonicalize(r).ok()) else { continue };
        let pliegues: Vec<(usize, usize)> = editor.plegado().pliegues().iter().map(|p| (p.inicio, p.fin)).collect();
        let guardados = guardados.get_or_insert_with(|| PlieguesGuardados::cargar(archivo_estado));
        if pliegues.is_empty() && !guardados.tiene(&ruta) {
            continue;
        }
        let huella_actual = if pliegues.is_empty() { 0 } else { huella(buffer.rope().chunks()) };
        cambio |= guardados.recordar(&ruta, huella_actual, &pliegues);
    }
    if let (true, Some(guardados)) = (cambio, guardados) {
        let _ = guardados.guardar(archivo_estado);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dos bloques plegables (líneas 0..=3 y 4..=7).
    const TEXTO: &str = "fn a() {\n    1\n    2\n}\nfn b() {\n    3\n    4\n}\n";

    /// Carpeta temporal propia de cada test (sin `tempfile`, igual que
    /// los demás tests de este crate), que se borra al soltarla.
    struct Carpeta(std::path::PathBuf);

    impl Carpeta {
        fn nueva(nombre: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tcode-test-pliegues-{nombre}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Carpeta {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn abrir(ruta: &Path) -> Editor {
        Editor::abrir(ruta).unwrap()
    }

    fn pliegues_de(editor: &Editor) -> Vec<(usize, usize)> {
        editor.plegado().pliegues().iter().map(|p| (p.inicio, p.fin)).collect()
    }

    #[test]
    fn cerrar_y_reabrir_restaura_los_pliegues() {
        let dir = Carpeta::nueva("restaura");
        let estado = dir.path().join("estado/pliegues.toml");
        let ruta = dir.path().join("a.rs");
        std::fs::write(&ruta, TEXTO).unwrap();

        let mut editor = abrir(&ruta);
        editor.plegar_todo(&[Pliegue { inicio: 0, fin: 3 }, Pliegue { inicio: 4, fin: 7 }]);
        recordar_en(&estado, [&editor]);
        assert!(estado.exists());

        let mut reabierto = abrir(&ruta);
        restaurar_en(&estado, &mut reabierto);
        assert_eq!(pliegues_de(&reabierto), [(0, 3), (4, 7)]);
    }

    #[test]
    fn si_el_archivo_cambio_por_fuera_no_se_restaura_nada() {
        let dir = Carpeta::nueva("cambio_por_fuera");
        let estado = dir.path().join("pliegues.toml");
        let ruta = dir.path().join("a.rs");
        std::fs::write(&ruta, TEXTO).unwrap();

        let mut editor = abrir(&ruta);
        editor.plegar_todo(&[Pliegue { inicio: 4, fin: 7 }]);
        recordar_en(&estado, [&editor]);

        std::fs::write(&ruta, format!("// nuevo\n{TEXTO}")).unwrap();
        let mut reabierto = abrir(&ruta);
        restaurar_en(&estado, &mut reabierto);
        assert!(reabierto.plegado().esta_vacio());
    }

    #[test]
    fn con_cambios_sin_guardar_no_se_pisa_lo_guardado() {
        let dir = Carpeta::nueva("sin_guardar");
        let estado = dir.path().join("pliegues.toml");
        let ruta = dir.path().join("a.rs");
        std::fs::write(&ruta, TEXTO).unwrap();

        let mut editor = abrir(&ruta);
        editor.plegar_todo(&[Pliegue { inicio: 0, fin: 3 }]);
        recordar_en(&estado, [&editor]);

        // Otra sesión: despliega, edita y cierra descartando los cambios.
        let mut otro = abrir(&ruta);
        otro.desplegar_todo();
        otro.insertar_texto("x");
        recordar_en(&estado, [&otro]);

        let mut reabierto = abrir(&ruta);
        restaurar_en(&estado, &mut reabierto);
        assert_eq!(pliegues_de(&reabierto), [(0, 3)]);
    }

    #[test]
    fn desplegar_todo_y_cerrar_olvida_el_archivo() {
        let dir = Carpeta::nueva("olvida");
        let estado = dir.path().join("pliegues.toml");
        let ruta = dir.path().join("a.rs");
        std::fs::write(&ruta, TEXTO).unwrap();

        let mut editor = abrir(&ruta);
        editor.plegar_todo(&[Pliegue { inicio: 0, fin: 3 }]);
        recordar_en(&estado, [&editor]);
        editor.desplegar_todo();
        recordar_en(&estado, [&editor]);
        assert!(PlieguesGuardados::cargar(&estado).is_empty());
    }

    #[test]
    fn sin_ruta_o_sin_nada_que_guardar_no_escribe_el_estado() {
        let dir = Carpeta::nueva("no_escribe");
        let estado = dir.path().join("pliegues.toml");
        let ruta = dir.path().join("a.rs");
        std::fs::write(&ruta, TEXTO).unwrap();
        recordar_en(&estado, [&Editor::nuevo(), &abrir(&ruta)]);
        assert!(!estado.exists());
    }

    #[test]
    fn un_estado_ilegible_no_rompe_nada() {
        let dir = Carpeta::nueva("ilegible");
        // Una carpeta donde debería ir el archivo: ni se lee ni se escribe.
        let estado = dir.path().join("pliegues.toml");
        std::fs::create_dir(&estado).unwrap();
        let ruta = dir.path().join("a.rs");
        std::fs::write(&ruta, TEXTO).unwrap();
        let mut editor = abrir(&ruta);
        editor.plegar_todo(&[Pliegue { inicio: 0, fin: 3 }]);
        recordar_en(&estado, [&editor]);
        restaurar_en(&estado, &mut editor);
    }
}
