//! Binario principal de `tcode`: carga de configuración, arranque/apagado de
//! la terminal y el bucle de eventos (PLAN.md §3, crate `app`).
//!
//! M0/M1: atajos todavía hardcodeados (`Ctrl+S`, `Ctrl+Q`, `Ctrl+Z`,
//! `Ctrl+Y`, flechas, Home/End, Backspace/Delete, Tab). El sistema de
//! atajos configurable en TOML (crate `keymap`) es otra pieza de M1.

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use tcode_config::Config;
use tcode_core::Editor;
use tcode_ui::{EstadoUi, Paleta};

type Backend = CrosstermBackend<Stdout>;

fn main() -> Result<()> {
    let ruta_arg = std::env::args().nth(1);

    let mut editor = match &ruta_arg {
        Some(ruta) => Editor::abrir(ruta)?,
        None => Editor::nuevo(),
    };

    // La config y el tema nunca hacen fallar el arranque: si el archivo del
    // usuario está corrupto o el tema configurado no existe, se sigue con
    // los valores por defecto en vez de negarse a abrir el editor.
    let config = tcode_config::cargar().unwrap_or_default();
    let paleta = cargar_paleta(&config.interfaz.tema);

    let mut terminal = iniciar_terminal()?;
    let resultado = ejecutar(&mut terminal, &mut editor, &config, &paleta, ruta_arg.as_deref());
    finalizar_terminal(&mut terminal)?;

    resultado
}

fn cargar_paleta(nombre_tema: &str) -> Paleta {
    let tema = tcode_config::cargar_tema(nombre_tema).unwrap_or_else(|_| tcode_config::tema_por_defecto());
    Paleta::desde_tema(&tema).unwrap_or_else(|_| Paleta::basica())
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

fn ejecutar(
    terminal: &mut Terminal<Backend>,
    editor: &mut Editor,
    config: &Config,
    paleta: &Paleta,
    ruta_arg: Option<&str>,
) -> Result<()> {
    let mut estado_ui = EstadoUi::default();
    let ruta_mostrada = ruta_arg.unwrap_or("[Sin nombre]").to_string();
    // Ctrl+Q con cambios sin guardar pide una segunda confirmación en vez de
    // perder trabajo en silencio (nano-style). Cualquier otra tecla la
    // cancela.
    let mut confirmar_salida = false;

    loop {
        terminal.draw(|frame| tcode_ui::dibujar(frame, editor, &mut estado_ui, &ruta_mostrada, paleta))?;

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

        match manejar_tecla(editor, config, key, &mut confirmar_salida) {
            Accion::Salir => break,
            Accion::Continuar => {}
        }
    }

    Ok(())
}

fn es_ctrl_q(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn manejar_tecla(editor: &mut Editor, config: &Config, key: KeyEvent, confirmar_salida: &mut bool) -> Accion {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('q') if ctrl => {
            if editor.buffer().modificado() && !*confirmar_salida {
                *confirmar_salida = true;
            } else {
                return Accion::Salir;
            }
        }
        // M0/M1 no tienen "guardar como" todavía (llega con la paleta de
        // comandos en M2): si el buffer no tiene ruta, Ctrl+S no hace nada
        // en vez de hacer fallar el editor entero.
        KeyCode::Char('s') if ctrl => {
            let _ = editor.guardar();
        }
        KeyCode::Char('z') if ctrl => editor.deshacer(),
        KeyCode::Char('y') if ctrl => editor.rehacer(),
        KeyCode::Char(c) if !ctrl => editor.insertar_char(c),
        KeyCode::Tab => insertar_tabulacion(editor, config),
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

/// `editor.tamano_tabulacion` / `editor.usar_espacios` de `config.toml`
/// (panel "Editor" de PLAN.md §5) ya afectan el comportamiento real del
/// editor, no solo el tema visual.
fn insertar_tabulacion(editor: &mut Editor, config: &Config) {
    if config.editor.usar_espacios {
        for _ in 0..config.editor.tamano_tabulacion {
            editor.insertar_char(' ');
        }
    } else {
        editor.insertar_char('\t');
    }
}
