//! Binario principal de `tcode`: carga de configuración y de atajos,
//! arranque/apagado de la terminal, y el bucle de eventos (PLAN.md §3,
//! crate `app`).
//!
//! Los eventos de teclado pasan por el [`Resolvedor`] de `tcode-keymap`
//! para convertirse en nombres de comando en español (`"archivo.guardar"`);
//! `ejecutar_comando` es el dispatcher que los traduce a llamadas sobre
//! [`Editor`] o el [`Explorador`]. Ese dispatcher se moverá a un crate
//! `commands` dedicado cuando llegue la paleta de comandos en M2 — por
//! ahora vive aquí porque es el único lugar que lo necesita.

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use tcode_config::Config;
use tcode_core::Editor;
use tcode_fs::Explorador;
use tcode_keymap::{Keymap, Resolucion, Resolvedor};
use tcode_syntax::Resaltador;
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
    let explorador = crear_explorador(ruta_arg.as_deref());

    let mut terminal = iniciar_terminal()?;
    let resultado = ejecutar(&mut terminal, &mut editor, config, &keymap, explorador, ruta_arg.as_deref());
    finalizar_terminal(&mut terminal)?;

    resultado
}

fn cargar_paleta(nombre_tema: &str) -> Paleta {
    let tema = tcode_config::cargar_tema(nombre_tema).unwrap_or_else(|_| tcode_config::tema_por_defecto());
    Paleta::desde_tema(&tema).unwrap_or_else(|_| Paleta::basica())
}

/// El árbol del explorador nunca hace fallar el arranque: si la carpeta no
/// se puede leer, queda vacío (se ve un panel sin filas al abrirlo con
/// `Ctrl+B`) en vez de negarse a abrir el editor.
fn crear_explorador(ruta_arg: Option<&str>) -> Explorador {
    let raiz = tcode_fs::raiz_por_defecto(ruta_arg);
    Explorador::nuevo(raiz).unwrap_or_else(|_| Explorador::vacio())
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

/// Qué panel recibe las teclas de navegación/edición genéricas
/// (`cursor.*`, `Enter`...). Los comandos globales (guardar, deshacer,
/// salir, recargar config) funcionan sin importar el foco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Foco {
    Editor,
    Explorador,
}

/// Resultado de ejecutar un comando: si el bucle principal debe seguir,
/// terminar, o si se abrió un archivo nuevo desde el explorador (y hay que
/// actualizar la ruta mostrada en la statusbar).
enum Accion {
    Continuar,
    Salir,
    ArchivoAbierto(PathBuf),
}

#[allow(clippy::too_many_arguments)]
fn ejecutar(
    terminal: &mut Terminal<Backend>,
    editor: &mut Editor,
    mut config: Config,
    keymap: &Keymap,
    mut explorador: Explorador,
    ruta_arg: Option<&str>,
) -> Result<()> {
    let mut estado_ui = EstadoUi::default();
    let mut ruta_mostrada = ruta_arg.unwrap_or("[Sin nombre]").to_string();
    let mut paleta = cargar_paleta(&config.interfaz.tema);
    let mut resaltador = Resaltador::nuevo();
    let mut resolvedor = Resolvedor::nuevo(keymap);
    let mut foco = Foco::Editor;
    // Ctrl+Q con cambios sin guardar pide una segunda confirmación en vez de
    // perder trabajo en silencio (nano-style). Cualquier otra resolución la
    // cancela.
    let mut confirmar_salida = false;

    loop {
        terminal.draw(|frame| {
            tcode_ui::dibujar(frame, editor, &mut estado_ui, &ruta_mostrada, &paleta, &mut resaltador, &explorador)
        })?;

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
            Resolucion::Comando(nombre) => {
                let accion = ejecutar_comando(&nombre, editor, &config, &mut explorador, &mut foco, &mut confirmar_salida);
                match accion {
                    Accion::Salir => break,
                    Accion::Continuar => {}
                    Accion::ArchivoAbierto(ruta) => ruta_mostrada = ruta.display().to_string(),
                }
            }
            Resolucion::Pendiente | Resolucion::Cancelado => {}
            Resolucion::SinCoincidencia => {
                // Ninguna tecla/chord configurado coincide: si es un
                // carácter imprimible sin Ctrl/Alt Y el foco está en el
                // editor, se inserta como texto normal (escribir no pasa
                // por el sistema de atajos; el explorador no recibe texto).
                if foco == Foco::Editor {
                    if let KeyCode::Char(c) = key.code {
                        if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) {
                            editor.insertar_char(c);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Dispatcher comando -> acción. Los nombres coinciden con los de
/// `runtime/keymaps/default.toml` y con PLAN.md §4.
fn ejecutar_comando(
    comando: &str,
    editor: &mut Editor,
    config: &Config,
    explorador: &mut Explorador,
    foco: &mut Foco,
    confirmar_salida: &mut bool,
) -> Accion {
    // Comandos globales: funcionan sin importar qué panel tiene el foco.
    match comando {
        "app.salir" => {
            if editor.buffer().modificado() && !*confirmar_salida {
                *confirmar_salida = true;
            } else {
                return Accion::Salir;
            }
            return Accion::Continuar;
        }
        // M1 no tiene "guardar como" todavía (llega con la paleta de
        // comandos en M2): si el buffer no tiene ruta, Ctrl+S no hace nada
        // en vez de hacer fallar el editor entero.
        "archivo.guardar" => {
            let _ = editor.guardar();
            return Accion::Continuar;
        }
        "editor.deshacer" => {
            editor.deshacer();
            return Accion::Continuar;
        }
        "editor.rehacer" => {
            editor.rehacer();
            return Accion::Continuar;
        }
        "panel.alternar_lateral" => {
            explorador.alternar_visibilidad();
            *foco = if explorador.visible() { Foco::Explorador } else { Foco::Editor };
            return Accion::Continuar;
        }
        "explorador.enfocar_editor" => {
            *foco = Foco::Editor;
            return Accion::Continuar;
        }
        _ => {}
    }

    if *foco == Foco::Explorador {
        return ejecutar_comando_explorador(comando, editor, explorador, foco);
    }

    match comando {
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

/// Comandos genéricos de navegación reinterpretados para el explorador:
/// las mismas teclas mueven la selección del árbol o abren/expanden la
/// fila seleccionada, en vez de mover el cursor del editor.
fn ejecutar_comando_explorador(comando: &str, editor: &mut Editor, explorador: &mut Explorador, foco: &mut Foco) -> Accion {
    match comando {
        "cursor.arriba" => explorador.mover_arriba(),
        "cursor.abajo" => explorador.mover_abajo(),
        "editor.nueva_linea" => {
            if let Ok(Some(ruta)) = explorador.activar_seleccion() {
                if let Ok(nuevo_editor) = Editor::abrir(&ruta) {
                    *editor = nuevo_editor;
                    // Abrir un archivo devuelve el foco al editor: el
                    // usuario ya eligió qué quería, tiene sentido poder
                    // escribir de inmediato en vez de seguir en el árbol.
                    *foco = Foco::Editor;
                    return Accion::ArchivoAbierto(ruta);
                }
            }
        }
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
