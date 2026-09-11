//! Binario principal de `tcode`: arranque/apagado de la terminal y el bucle
//! de eventos (PLAN.md §3, crate `app`).
//!
//! M0: solo atajos hardcodeados (`Ctrl+S`, `Ctrl+Q`, `Ctrl+Z`, `Ctrl+Y`,
//! flechas, Home/End, Backspace/Delete). El sistema de atajos configurable
//! en TOML (crate `keymap`) llega en M1.

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use tcode_core::Editor;
use tcode_ui::EstadoUi;

type Backend = CrosstermBackend<Stdout>;

fn main() -> Result<()> {
    let ruta_arg = std::env::args().nth(1);

    let mut editor = match &ruta_arg {
        Some(ruta) => Editor::abrir(ruta)?,
        None => Editor::nuevo(),
    };

    let mut terminal = iniciar_terminal()?;
    let resultado = ejecutar(&mut terminal, &mut editor, ruta_arg.as_deref());
    finalizar_terminal(&mut terminal)?;

    resultado
}

fn iniciar_terminal() -> Result<Terminal<Backend>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn finalizar_terminal(terminal: &mut Terminal<Backend>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Resultado de procesar una tecla: si el bucle principal debe seguir o
/// terminar.
enum Accion {
    Continuar,
    Salir,
}

fn ejecutar(terminal: &mut Terminal<Backend>, editor: &mut Editor, ruta_arg: Option<&str>) -> Result<()> {
    let mut estado_ui = EstadoUi::default();
    let ruta_mostrada = ruta_arg.unwrap_or("[Sin nombre]").to_string();
    // Ctrl+Q con cambios sin guardar pide una segunda confirmación en vez de
    // perder trabajo en silencio (nano-style). Cualquier otra tecla la
    // cancela.
    let mut confirmar_salida = false;

    loop {
        terminal.draw(|frame| tcode_ui::dibujar(frame, editor, &mut estado_ui, &ruta_mostrada))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if !es_ctrl_q(key) {
            confirmar_salida = false;
        }

        match manejar_tecla(editor, key, &mut confirmar_salida) {
            Accion::Salir => break,
            Accion::Continuar => {}
        }
    }

    Ok(())
}

fn es_ctrl_q(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn manejar_tecla(editor: &mut Editor, key: KeyEvent, confirmar_salida: &mut bool) -> Accion {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('q') if ctrl => {
            if editor.buffer().modificado() && !*confirmar_salida {
                *confirmar_salida = true;
            } else {
                return Accion::Salir;
            }
        }
        // M0 no tiene "guardar como" todavía (llega con la paleta de
        // comandos en M2): si el buffer no tiene ruta, Ctrl+S no hace nada
        // en vez de hacer fallar el editor entero.
        KeyCode::Char('s') if ctrl => {
            let _ = editor.guardar();
        }
        KeyCode::Char('z') if ctrl => editor.deshacer(),
        KeyCode::Char('y') if ctrl => editor.rehacer(),
        KeyCode::Char(c) if !ctrl => editor.insertar_char(c),
        KeyCode::Tab => editor.insertar_char('\t'),
        KeyCode::Enter => editor.insertar_char('\n'),
        KeyCode::Backspace => editor.borrar_atras(),
        KeyCode::Delete => editor.borrar_adelante(),
        KeyCode::Left => editor.mover_izquierda(),
        KeyCode::Right => editor.mover_derecha(),
        KeyCode::Up => editor.mover_arriba(),
        KeyCode::Down => editor.mover_abajo(),
        KeyCode::Home if ctrl => editor.inicio_archivo(),
        KeyCode::End if ctrl => editor.fin_archivo(),
        KeyCode::Home => editor.inicio_linea(),
        KeyCode::End => editor.fin_linea(),
        _ => {}
    }

    Accion::Continuar
}
