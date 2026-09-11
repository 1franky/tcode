//! Binario principal de `tcode`: carga de configuración y de atajos,
//! arranque/apagado de la terminal, y el bucle de eventos (PLAN.md §3,
//! crate `app`).
//!
//! Los eventos de teclado pasan por el [`Resolvedor`] de `tcode-keymap`
//! para convertirse en nombres de comando en español (`"archivo.guardar"`);
//! `ejecutar_comando` es el dispatcher que los traduce a llamadas sobre
//! [`Editor`]. Ese dispatcher se moverá a un crate `commands` dedicado
//! cuando llegue la paleta de comandos en M2 — por ahora vive aquí porque
//! es el único lugar que lo necesita.

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use tcode_config::Config;
use tcode_core::Editor;
use tcode_keymap::{Keymap, Resolucion, Resolvedor};
use tcode_ui::{EstadoUi, Paleta};

type Backend = CrosstermBackend<Stdout>;

fn main() -> Result<()> {
    let ruta_arg = std::env::args().nth(1);

    let mut editor = match &ruta_arg {
        Some(ruta) => Editor::abrir(ruta)?,
        None => Editor::nuevo(),
    };

    // La config y el keymap nunca hacen fallar el arranque: si el archivo
    // del usuario está corrupto, se sigue con los valores por defecto en
    // vez de negarse a abrir el editor.
    let config = tcode_config::cargar().unwrap_or_default();
    let keymap = tcode_keymap::cargar().unwrap_or_else(|_| tcode_keymap::keymap_por_defecto());

    let mut terminal = iniciar_terminal()?;
    let resultado = ejecutar(&mut terminal, &mut editor, config, &keymap, ruta_arg.as_deref());
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

/// Resultado de ejecutar un comando: si el bucle principal debe seguir o
/// terminar.
enum Accion {
    Continuar,
    Salir,
}

fn ejecutar(
    terminal: &mut Terminal<Backend>,
    editor: &mut Editor,
    mut config: Config,
    keymap: &Keymap,
    ruta_arg: Option<&str>,
) -> Result<()> {
    let mut estado_ui = EstadoUi::default();
    let ruta_mostrada = ruta_arg.unwrap_or("[Sin nombre]").to_string();
    let mut paleta = cargar_paleta(&config.interfaz.tema);
    let mut resolvedor = Resolvedor::nuevo(keymap);
    // Ctrl+Q con cambios sin guardar pide una segunda confirmación en vez de
    // perder trabajo en silencio (nano-style). Cualquier otra resolución la
    // cancela.
    let mut confirmar_salida = false;

    loop {
        terminal.draw(|frame| tcode_ui::dibujar(frame, editor, &mut estado_ui, &ruta_mostrada, &paleta))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        let resolucion = resolvedor.procesar(tcode_keymap::desde_evento(key));

        let reintentando_salida = matches!(&resolucion, Resolucion::Comando(n) if n == "app.salir");
        if !reintentando_salida {
            confirmar_salida = false;
        }

        match resolucion {
            Resolucion::Comando(nombre) if nombre == "config.recargar" => {
                recargar_config_y_tema(&mut config, &mut paleta);
            }
            Resolucion::Comando(nombre) => match ejecutar_comando(&nombre, editor, &config, &mut confirmar_salida) {
                Accion::Salir => break,
                Accion::Continuar => {}
            },
            Resolucion::Pendiente | Resolucion::Cancelado => {}
            Resolucion::SinCoincidencia => {
                // Ninguna tecla/chord configurado coincide: si es un
                // carácter imprimible sin Ctrl/Alt, se inserta como texto
                // normal (escribir no pasa por el sistema de atajos).
                if let KeyCode::Char(c) = key.code {
                    if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) {
                        editor.insertar_char(c);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Dispatcher comando -> acción sobre el `Editor`. Los nombres coinciden
/// con los de `runtime/keymaps/default.toml` y con PLAN.md §4.
fn ejecutar_comando(comando: &str, editor: &mut Editor, config: &Config, confirmar_salida: &mut bool) -> Accion {
    match comando {
        "app.salir" => {
            if editor.buffer().modificado() && !*confirmar_salida {
                *confirmar_salida = true;
            } else {
                return Accion::Salir;
            }
        }
        // M1 no tiene "guardar como" todavía (llega con la paleta de
        // comandos en M2): si el buffer no tiene ruta, Ctrl+S no hace nada
        // en vez de hacer fallar el editor entero.
        "archivo.guardar" => {
            let _ = editor.guardar();
        }
        "editor.deshacer" => editor.deshacer(),
        "editor.rehacer" => editor.rehacer(),
        "cursor.arriba" => editor.mover_arriba(),
        "cursor.abajo" => editor.mover_abajo(),
        "cursor.izquierda" => editor.mover_izquierda(),
        "cursor.derecha" => editor.mover_derecha(),
        "cursor.inicio_linea" => editor.inicio_linea(),
        "cursor.fin_linea" => editor.fin_linea(),
        "cursor.inicio_archivo" => editor.inicio_archivo(),
        "cursor.fin_archivo" => editor.fin_archivo(),
        "editor.borrar_atras" => editor.borrar_atras(),
        "editor.borrar_adelante" => editor.borrar_adelante(),
        "editor.nueva_linea" => editor.insertar_char('\n'),
        "editor.indentar_o_autocompletar" => insertar_tabulacion(editor, config),
        _ => {}
    }
    Accion::Continuar
}

/// `config.recargar` (`Ctrl+K Ctrl+L`): recarga `config.toml` desde disco
/// sin reiniciar el editor (PLAN.md §4) y reconstruye la paleta de colores
/// si el tema activo cambió.
fn recargar_config_y_tema(config: &mut Config, paleta: &mut Paleta) {
    if let Ok(nueva) = tcode_config::cargar() {
        let tema_cambio = nueva.interfaz.tema != config.interfaz.tema;
        *config = nueva;
        if tema_cambio {
            *paleta = cargar_paleta(&config.interfaz.tema);
        }
    }
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
