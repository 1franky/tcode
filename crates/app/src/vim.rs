//! Modo Normal de VIM (`config.editor.modo_vim`, M5, alcance "lo
//! esencial" — ver PRUEBAS.md: sin operadores combinables como `dw`/
//! `d$`, sin conteos numéricos como `3dd`, sin `:`). El estado (registro
//! sin nombre + comando de dos teclas pendiente) vive en
//! `tcode_core::EstadoVim`; acá solo se interpreta cada tecla y se la
//! traduce a llamadas sobre el `Editor` activo — mismo patrón que
//! `ejecutar_comando_csv`/`ejecutar_comando_explorador` en `main.rs`.

use tcode_core::{Editor, EstadoVim};
use tcode_ui::Layout as PanelLayout;

/// Interpreta una tecla del modo Normal sobre el panel activo de
/// `layout`. Delega en `ejecutar_tecla_normal_en` (que solo necesita un
/// `Editor`, no todo el `Layout`) para poder testear la lógica sin
/// construir un árbol de paneles completo.
pub fn ejecutar_tecla_normal(c: char, layout: &mut PanelLayout, vim: &mut EstadoVim) {
    ejecutar_tecla_normal_en(layout.editor_activo_mut(), c, vim);
}

fn ejecutar_tecla_normal_en(editor: &mut Editor, c: char, vim: &mut EstadoVim) {
    // Un comando de dos teclas iguales en espera de la segunda (`dd`,
    // `yy`, `gg`): si `c` coincide se ejecuta, si no simplemente se
    // cancela — cualquiera de los dos casos consume `c` (no cae al match
    // de abajo como si fuera una tecla nueva).
    if let Some(pendiente) = vim.pendiente() {
        vim.limpiar_pendiente();
        if c == pendiente {
            match c {
                'd' => borrar_linea_actual(editor, vim),
                'y' => yanquear_linea_actual(editor, vim),
                'g' => editor.inicio_archivo(),
                _ => {}
            }
        }
        recortar_si_sigue_en_normal(editor);
        return;
    }

    match c {
        'h' => editor.mover_izquierda(),
        'l' => editor.mover_derecha(),
        'k' => editor.mover_arriba(),
        'j' => editor.mover_abajo(),
        '0' => editor.inicio_linea(),
        '$' => editor.fin_linea(),
        'G' => editor.fin_archivo(),
        // Primer golpe de un comando de dos teclas: queda pendiente, no
        // hace nada todavía.
        'd' | 'y' | 'g' => vim.fijar_pendiente(c),
        'x' => editor.borrar_adelante(),
        'p' => pegar_despues(editor, vim),
        'u' => editor.deshacer(),
        'i' => editor.entrar_modo_insertar(),
        // `a` (append): a diferencia de `i`, entra a insertar DESPUÉS del
        // carácter bajo el cursor — con el modelo de cursor de tcode
        // (que ya permite pararse "después del último carácter", igual
        // que cualquier editor no-VIM) alcanza con moverlo una posición
        // antes de cambiar de modo.
        'a' => {
            editor.mover_derecha();
            editor.entrar_modo_insertar();
        }
        // `o`: abre una línea nueva debajo del cursor y entra a
        // insertar ahí — mover al fin de línea + insertar un salto
        // reutiliza exactamente el mismo camino que `Enter` en modo
        // Insertar (incluido el multi-cursor, si lo hubiera).
        'o' => {
            editor.fin_linea();
            editor.insertar_char('\n');
            editor.entrar_modo_insertar();
        }
        _ => {}
    }
    recortar_si_sigue_en_normal(editor);
}

/// Tras cualquier tecla que no haya cambiado a Insertar, reaplica el
/// recorte de cursor de modo Normal (`Editor::entrar_modo_normal`, que es
/// un no-op sobre `modo` si ya se estaba en `Normal` — solo hace falta
/// por el recorte que hace de paso). Sin esto, un movimiento como `l`/`$`
/// podría volver a dejar el cursor "después" del último carácter, algo
/// que VIM real nunca permite en Normal (a diferencia de Insertar).
fn recortar_si_sigue_en_normal(editor: &mut Editor) {
    if editor.modo() == tcode_core::Modo::Normal {
        editor.entrar_modo_normal();
    }
}

/// Limpia cualquier comando de dos teclas en espera — `Esc` en modo
/// Normal (no cambia de modo, ya se está en Normal; solo resetea el
/// estado a uno "limpio", igual que VIM real).
pub fn cancelar_pendiente(vim: &mut EstadoVim) {
    vim.limpiar_pendiente();
}

/// `dd`: borra la línea bajo el cursor entera (con su propio salto de
/// línea) y la deja en el registro sin nombre — el registro se fija
/// DESPUÉS de borrar porque `reemplazar_rango_bytes` mueve el cursor;
/// leer `linea_con_salto` antes no depende de esa posición.
fn borrar_linea_actual(editor: &mut Editor, vim: &mut EstadoVim) {
    let linea = editor.cursor().linea;
    let inicio = editor.buffer().inicio_byte_linea(linea);
    let texto = editor.buffer().linea_con_salto(linea);
    editor.reemplazar_rango_bytes(inicio, inicio + texto.len(), "");
    vim.fijar_registro(texto);
}

/// `yy`: copia la línea bajo el cursor al registro sin nombre, sin
/// modificar el buffer.
fn yanquear_linea_actual(editor: &Editor, vim: &mut EstadoVim) {
    let linea = editor.cursor().linea;
    vim.fijar_registro(editor.buffer().linea_con_salto(linea));
}

/// `p`: pega el registro sin nombre como una línea nueva justo después
/// de la línea del cursor — sin efecto si el registro está vacío
/// (todavía no se yanqueó ni borró nada en esta sesión). Si el cursor
/// está en la última línea y esta no termina en salto de línea, agrega
/// uno antes de pegar para no fusionar el texto pegado con ella.
fn pegar_despues(editor: &mut Editor, vim: &EstadoVim) {
    let registro = vim.registro();
    if registro.is_empty() {
        return;
    }
    let buffer = editor.buffer();
    let linea = editor.cursor().linea;
    let (punto, texto) = if linea + 1 < buffer.num_lineas() {
        (buffer.inicio_byte_linea(linea + 1), registro.to_string())
    } else {
        let fin = buffer.len_bytes();
        let texto =
            if fin == 0 || buffer.termina_en_salto_de_linea() { registro.to_string() } else { format!("\n{registro}") };
        (fin, texto)
    };
    editor.reemplazar_rango_bytes(punto, punto, &texto);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escribir(editor: &mut Editor, texto: &str) {
        for c in texto.chars() {
            editor.insertar_char(c);
        }
    }

    /// `ejecutar_tecla_normal`/`_en` en producción solo se llama con el
    /// editor ya en `Modo::Normal` (el bucle principal lo comprueba antes
    /// de despachar acá, `crates/app/src/main.rs`) — replicarlo en los
    /// tests importa de verdad porque `entrar_modo_normal` recorta el
    /// cursor si hacía falta (ver `recortar_si_sigue_en_normal`).
    fn editor_con(texto: &str) -> Editor {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, texto);
        editor.inicio_archivo();
        editor.entrar_modo_normal();
        editor
    }

    #[test]
    fn hjkl_mueven_el_cursor_como_las_flechas() {
        let mut editor = editor_con("ab\ncd");
        let mut vim = EstadoVim::nuevo();

        ejecutar_tecla_normal_en(&mut editor, 'l', &mut vim);
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (0, 1));
        ejecutar_tecla_normal_en(&mut editor, 'j', &mut vim);
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (1, 1));
        ejecutar_tecla_normal_en(&mut editor, 'h', &mut vim);
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (1, 0));
        ejecutar_tecla_normal_en(&mut editor, 'k', &mut vim);
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (0, 0));
    }

    #[test]
    fn cero_y_signo_pesos_van_al_inicio_y_fin_de_linea() {
        let mut editor = editor_con("hola");
        let mut vim = EstadoVim::nuevo();

        ejecutar_tecla_normal_en(&mut editor, '$', &mut vim);
        // 3, no 4: en modo Normal el cursor nunca queda "después" del
        // último carácter (a diferencia de Insertar) — ver
        // `recortar_si_sigue_en_normal`.
        assert_eq!(editor.cursor().columna, 3);
        ejecutar_tecla_normal_en(&mut editor, '0', &mut vim);
        assert_eq!(editor.cursor().columna, 0);
    }

    #[test]
    fn el_cursor_nunca_queda_despues_del_ultimo_caracter_en_modo_normal() {
        let mut editor = editor_con("abc");
        let mut vim = EstadoVim::nuevo();
        // 'l' repetido no debería poder empujar el cursor más allá de la
        // 'c' (columna 2) — con el movimiento compartido de siempre
        // (`mover_derecha`) sin este recorte, terminaría en columna 3.
        for _ in 0..5 {
            ejecutar_tecla_normal_en(&mut editor, 'l', &mut vim);
        }
        assert_eq!(editor.cursor().columna, 2);
    }

    #[test]
    fn entrar_modo_normal_recorta_el_cursor_que_quedo_al_final_tras_insertar() {
        // Simula `Esc` desde Insertar con el cursor al final de la línea
        // (como al escribir hasta el final) — el mismo recorte que aplica
        // VIM real al volver a Normal.
        let mut editor = editor_con("hola");
        editor.fin_linea(); // columna 4, la posición normal en Insertar
        assert_eq!(editor.cursor().columna, 4);
        editor.entrar_modo_normal();
        assert_eq!(editor.cursor().columna, 3);
    }

    #[test]
    fn entrar_modo_normal_no_hace_nada_raro_en_una_linea_vacia() {
        let mut editor = Editor::nuevo(); // buffer vacío, una sola línea sin contenido
        editor.entrar_modo_normal();
        assert_eq!(editor.cursor().columna, 0);
    }

    #[test]
    fn gg_va_al_inicio_del_archivo_y_mayuscula_g_al_final() {
        let mut editor = editor_con("uno\ndos\ntres");
        let mut vim = EstadoVim::nuevo();

        ejecutar_tecla_normal_en(&mut editor, 'G', &mut vim);
        assert_eq!(editor.cursor().linea, 2);

        ejecutar_tecla_normal_en(&mut editor, 'g', &mut vim);
        assert_eq!(vim.pendiente(), Some('g'), "la primera 'g' queda pendiente");
        assert_eq!(editor.cursor().linea, 2, "todavía no se movió");
        ejecutar_tecla_normal_en(&mut editor, 'g', &mut vim);
        assert_eq!(editor.cursor().linea, 0);
        assert_eq!(vim.pendiente(), None);
    }

    #[test]
    fn una_tecla_distinta_cancela_el_comando_de_dos_teclas_pendiente() {
        let mut editor = editor_con("uno\ndos");
        let mut vim = EstadoVim::nuevo();

        ejecutar_tecla_normal_en(&mut editor, 'd', &mut vim);
        assert_eq!(vim.pendiente(), Some('d'));
        // 'j' no coincide con 'd': cancela el pendiente y no borra nada.
        ejecutar_tecla_normal_en(&mut editor, 'j', &mut vim);
        assert_eq!(vim.pendiente(), None);
        assert_eq!(editor.buffer().a_texto(), "uno\ndos");
    }

    #[test]
    fn x_borra_el_caracter_bajo_el_cursor() {
        let mut editor = editor_con("hola");
        let mut vim = EstadoVim::nuevo();
        ejecutar_tecla_normal_en(&mut editor, 'x', &mut vim);
        assert_eq!(editor.buffer().a_texto(), "ola");
    }

    #[test]
    fn dd_borra_la_linea_completa_y_la_deja_en_el_registro() {
        let mut editor = editor_con("uno\ndos\ntres");
        let mut vim = EstadoVim::nuevo();
        editor.mover_abajo(); // cursor en "dos"

        ejecutar_tecla_normal_en(&mut editor, 'd', &mut vim);
        ejecutar_tecla_normal_en(&mut editor, 'd', &mut vim);

        assert_eq!(editor.buffer().a_texto(), "uno\ntres");
        assert_eq!(vim.registro(), "dos\n");
        // El cursor queda al inicio de la línea que ocupó el lugar de la borrada.
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (1, 0));
    }

    #[test]
    fn dd_sobre_la_ultima_linea_sin_salto_final_tambien_funciona() {
        let mut editor = editor_con("uno\ndos"); // "dos" no tiene \n final
        let mut vim = EstadoVim::nuevo();
        editor.mover_abajo();

        ejecutar_tecla_normal_en(&mut editor, 'd', &mut vim);
        ejecutar_tecla_normal_en(&mut editor, 'd', &mut vim);

        assert_eq!(editor.buffer().a_texto(), "uno\n");
        assert_eq!(vim.registro(), "dos");
    }

    #[test]
    fn yy_copia_la_linea_sin_modificar_el_buffer() {
        let mut editor = editor_con("uno\ndos");
        let mut vim = EstadoVim::nuevo();

        ejecutar_tecla_normal_en(&mut editor, 'y', &mut vim);
        ejecutar_tecla_normal_en(&mut editor, 'y', &mut vim);

        assert_eq!(editor.buffer().a_texto(), "uno\ndos");
        assert_eq!(vim.registro(), "uno\n");
    }

    #[test]
    fn p_sin_haber_yanqueado_nada_no_hace_nada() {
        let mut editor = editor_con("uno");
        let mut vim = EstadoVim::nuevo();
        ejecutar_tecla_normal_en(&mut editor, 'p', &mut vim);
        assert_eq!(editor.buffer().a_texto(), "uno");
    }

    #[test]
    fn p_pega_el_registro_como_una_linea_nueva_debajo() {
        let mut editor = editor_con("uno\ndos\ntres");
        let mut vim = EstadoVim::nuevo();

        // yy sobre "uno", bajar a "dos", pegar: "uno" reaparece entre
        // "dos" y "tres".
        ejecutar_tecla_normal_en(&mut editor, 'y', &mut vim);
        ejecutar_tecla_normal_en(&mut editor, 'y', &mut vim);
        editor.mover_abajo();
        ejecutar_tecla_normal_en(&mut editor, 'p', &mut vim);

        assert_eq!(editor.buffer().a_texto(), "uno\ndos\nuno\ntres");
    }

    #[test]
    fn p_en_la_ultima_linea_sin_salto_final_agrega_uno_antes_de_pegar() {
        let mut editor = editor_con("uno\ndos"); // "dos" es la última línea, sin \n
        let mut vim = EstadoVim::nuevo();
        editor.mover_abajo();
        ejecutar_tecla_normal_en(&mut editor, 'y', &mut vim);
        ejecutar_tecla_normal_en(&mut editor, 'y', &mut vim);

        ejecutar_tecla_normal_en(&mut editor, 'p', &mut vim);

        assert_eq!(editor.buffer().a_texto(), "uno\ndos\ndos");
    }

    #[test]
    fn u_deshace_el_ultimo_cambio() {
        let mut editor = editor_con("hola");
        let mut vim = EstadoVim::nuevo();
        ejecutar_tecla_normal_en(&mut editor, 'x', &mut vim);
        assert_eq!(editor.buffer().a_texto(), "ola");
        ejecutar_tecla_normal_en(&mut editor, 'u', &mut vim);
        assert_eq!(editor.buffer().a_texto(), "hola");
    }

    #[test]
    fn i_entra_a_insertar_sin_mover_el_cursor() {
        let mut editor = editor_con("hola");
        let mut vim = EstadoVim::nuevo();
        editor.entrar_modo_normal();
        ejecutar_tecla_normal_en(&mut editor, 'i', &mut vim);
        assert_eq!(editor.modo(), tcode_core::Modo::Insertar);
        assert_eq!(editor.cursor().columna, 0);
    }

    #[test]
    fn a_entra_a_insertar_moviendo_el_cursor_un_lugar_a_la_derecha() {
        let mut editor = editor_con("hola");
        let mut vim = EstadoVim::nuevo();
        editor.entrar_modo_normal();
        ejecutar_tecla_normal_en(&mut editor, 'a', &mut vim);
        assert_eq!(editor.modo(), tcode_core::Modo::Insertar);
        assert_eq!(editor.cursor().columna, 1);
    }

    #[test]
    fn o_abre_una_linea_nueva_debajo_y_entra_a_insertar() {
        let mut editor = editor_con("uno\ndos");
        let mut vim = EstadoVim::nuevo();
        editor.entrar_modo_normal();

        ejecutar_tecla_normal_en(&mut editor, 'o', &mut vim);

        assert_eq!(editor.buffer().a_texto(), "uno\n\ndos");
        assert_eq!(editor.modo(), tcode_core::Modo::Insertar);
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (1, 0));
    }

    #[test]
    fn cancelar_pendiente_limpia_un_comando_de_dos_teclas_a_medias() {
        let mut vim = EstadoVim::nuevo();
        vim.fijar_pendiente('d');
        cancelar_pendiente(&mut vim);
        assert_eq!(vim.pendiente(), None);
    }
}
