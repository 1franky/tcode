//! Núcleo del editor `tcode`: buffer de texto, cursor e historial de
//! deshacer/rehacer. No depende de terminal ni de UI (ver PLAN.md §3), lo
//! que permite testear la lógica de edición sin necesidad de una TUI.

pub mod buffer;
pub mod busqueda;
pub mod csv;
pub mod cursor;
pub mod editor;
pub mod estado_busqueda;
pub mod estado_csv;
pub mod estado_guardar_como;
pub mod estado_vim;
pub mod history;

pub use buffer::{Buffer, Eol};
pub use busqueda::{buscar_coincidencias, Coincidencia, OpcionesBusqueda};
pub use csv::{analizar as analizar_csv, delimitador_por_extension, serializar_fila as serializar_fila_csv, FilaCsv, TablaCsv};
pub use cursor::{Cursor, CursorMultiple};
pub use editor::{Editor, Modo};
pub use estado_busqueda::{CampoBusqueda, EstadoBusqueda};
pub use estado_csv::EstadoCsv;
pub use estado_guardar_como::EstadoGuardarComo;
pub use estado_vim::EstadoVim;
pub use history::Historia;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertar_y_mover_cursor() {
        let mut editor = Editor::nuevo();
        for c in "hola".chars() {
            editor.insertar_char(c);
        }
        assert_eq!(editor.buffer().a_texto(), "hola");
        assert_eq!(editor.cursor().columna, 4);

        editor.insertar_char('\n');
        editor.insertar_char('!');
        assert_eq!(editor.buffer().a_texto(), "hola\n!");
        assert_eq!(editor.cursor().linea, 1);
        assert_eq!(editor.cursor().columna, 1);
    }

    #[test]
    fn backspace_fusiona_lineas() {
        let mut editor = Editor::nuevo();
        for c in "ab\ncd".chars() {
            editor.insertar_char(c);
        }
        // Cursor al final: línea 1, columna 2 ("cd").
        editor.inicio_linea();
        editor.borrar_atras();
        assert_eq!(editor.buffer().a_texto(), "abcd");
        assert_eq!(editor.cursor().linea, 0);
        assert_eq!(editor.cursor().columna, 2);
    }

    #[test]
    fn deshacer_rehacer_restauran_contenido_y_cursor() {
        let mut editor = Editor::nuevo();
        editor.insertar_char('a');
        editor.insertar_char('b');
        assert_eq!(editor.buffer().a_texto(), "ab");

        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "a");
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "");

        editor.rehacer();
        assert_eq!(editor.buffer().a_texto(), "a");
        editor.rehacer();
        assert_eq!(editor.buffer().a_texto(), "ab");
        assert_eq!(editor.cursor().columna, 2);
    }

    #[test]
    fn guardar_y_recargar_archivo() {
        let dir = std::env::temp_dir().join(format!("tcode-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("prueba.txt");

        let mut editor = Editor::nuevo();
        for c in "contenido de prueba".chars() {
            editor.insertar_char(c);
        }
        editor.guardar_como(&ruta).unwrap();
        assert!(!editor.buffer().modificado());

        let recargado = Editor::abrir(&ruta).unwrap();
        assert_eq!(recargado.buffer().a_texto(), "contenido de prueba");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn inicio_byte_linea_correlaciona_con_el_texto_completo() {
        let mut editor = Editor::nuevo();
        for c in "áé\nb\ncd".chars() {
            editor.insertar_char(c);
        }
        let buffer = editor.buffer();
        assert_eq!(buffer.inicio_byte_linea(0), 0);
        // "áé\n" son 5 bytes en UTF-8 (2+2+1), no 3 (que sería en chars).
        assert_eq!(buffer.inicio_byte_linea(1), "áé\n".len());
        assert_eq!(buffer.inicio_byte_linea(2), "áé\nb\n".len());
    }

    #[test]
    fn cursor_no_se_sale_de_los_limites() {
        let mut editor = Editor::nuevo();
        editor.mover_izquierda();
        editor.mover_arriba();
        assert_eq!(editor.cursor(), Cursor::nuevo());

        for c in "abc".chars() {
            editor.insertar_char(c);
        }
        editor.mover_derecha();
        assert_eq!(editor.cursor().columna, 3);
    }
}
