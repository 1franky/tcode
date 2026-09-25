//! Integración del modo VIM (`config.editor.modo_vim`) con la app. La
//! gramática (operador + conteo + movimiento/objeto), los movimientos, la
//! ejecución sobre el `Editor` y el parseo de la línea `:` viven en
//! `tcode_core::vim`, sin UI y con sus tests; acá solo se rutea cada
//! tecla al panel activo y se ejecutan los comandos `:` que necesitan
//! paneles, pestañas o disco (`:w`, `:q`, `:e`...) — por los mismos
//! caminos que sus atajos (`guardar_archivo_activo`, `cierre_confirmado`,
//! `abrir_ruta_desde_explorador`).

use crossterm::event::{KeyCode, KeyEvent};
use tcode_config::Config;
use tcode_core::vim::{self as nucleo, ComandoLinea, OpcionesVim};
use tcode_core::{EstadoVim, Modo, TextoCopiado};
use tcode_ui::Layout as PanelLayout;

use crate::portapapeles::{self, Portapapeles};
use crate::{abrir_ruta_desde_explorador, cierre_confirmado, guardar_archivo_activo, sin_modificadores, Accion, EstadoApp};

/// Id con el que `:q` arma la confirmación de cierre con cambios (ver
/// `cierre_confirmado`): distinto del de `Ctrl+W` para que uno no confirme
/// al otro.
const ID_CIERRE_VIM: &str = "vim.cerrar";

fn opciones(config: &Config) -> OpcionesVim {
    let ancho = config.editor.tamano_tabulacion.max(1);
    let indentacion = if config.editor.usar_espacios { " ".repeat(ancho) } else { "\t".to_string() };
    OpcionesVim { indentacion, ancho_tabulacion: ancho }
}

/// Una tecla (carácter sin modificadores) en modo Normal o Visual sobre
/// el panel activo. El aviso del ejecutor (si hay) o, si no, las teclas
/// de un comando a medio escribir (`d2`, `ci`...) quedan en la barra de
/// estado, como el `showcmd` de VIM.
///
/// Portapapeles del sistema (BACKLOG.md P0 #15): el registro sin nombre
/// sigue siendo interno salvo con `vim_sincronizar_portapapeles` (como
/// `clipboard=unnamedplus` de Neovim: lo yanqueado/borrado va al
/// portapapeles y `p` pega desde ahí) o con el prefijo `"+`/`"*` delante
/// de un comando (`"+yy`, `"+p`, `"+dw`...) — el único registro con
/// nombre que se soporta. `"+p` no pisa el registro sin nombre, como en
/// VIM.
pub fn ejecutar_tecla_normal(
    c: char,
    layout: &mut PanelLayout,
    vim: &mut EstadoVim,
    config: &Config,
    portapapeles: &mut Portapapeles,
) {
    if std::mem::take(&mut vim.esperando_registro) {
        let mensaje = if c == '+' || c == '*' {
            vim.registro_portapapeles = true;
            format!("\"{c}")
        } else {
            format!("Registro no soportado: \"{c} (solo \"+ y \"*)")
        };
        layout.panel_activo_mut().mensaje_estado = Some(mensaje);
        return;
    }
    if c == '"' && vim.teclas_pendientes().is_empty() && !vim.registro_portapapeles {
        vim.esperando_registro = true;
        layout.panel_activo_mut().mensaje_estado = Some("\"".to_string());
        return;
    }

    let modo = config.editor.portapapeles;
    let usar_portapapeles = vim.registro_portapapeles || config.editor.vim_sincronizar_portapapeles;
    // `p`/`P` (con o sin conteo): el registro se carga desde el
    // portapapeles justo antes de pegar. Con `"+p` se guarda el registro
    // sin nombre para devolverlo después.
    let pega = matches!(c, 'p' | 'P') && vim.teclas_pendientes().iter().all(|t| t.is_ascii_digit());
    let mut registro_previo = None;
    if usar_portapapeles && pega {
        if let Some(copiado) = portapapeles.leer(modo) {
            if copiado.texto != vim.registro() {
                if vim.registro_portapapeles {
                    registro_previo = Some((vim.registro().to_string(), vim.registro_lineal()));
                }
                // Texto de otra app: por líneas si termina en salto de
                // línea (mismo criterio que Neovim con `"+`).
                let lineal = copiado.lineal || copiado.texto.ends_with('\n');
                vim.fijar_registro(copiado.texto, lineal);
            }
        }
    }
    let version = vim.version_registro();

    let aviso = nucleo::ejecutar_tecla(layout.editor_activo_mut(), vim, c, &opciones(config));
    let pendientes: String = vim.teclas_pendientes().iter().collect();
    let mut mensaje = aviso.or_else(|| (!pendientes.is_empty()).then_some(pendientes));

    if usar_portapapeles && vim.version_registro() != version {
        let copiado = TextoCopiado { texto: vim.registro().to_string(), lineal: vim.registro_lineal() };
        let lineas = portapapeles::describir_lineas(&copiado);
        let resultado = portapapeles.copiar(copiado, modo);
        // Con la sincronización prendida cada `y`/`d` copia: el aviso
        // solo se muestra cuando se pidió a propósito (`"+`).
        if vim.registro_portapapeles {
            let destino = if resultado.salio_de_tcode() { "" } else { " (solo dentro de tcode)" };
            mensaje = Some(format!("Copiado: {lineas}{destino}"));
        }
    }
    // El prefijo `"+` vale para un solo comando: se termina en cuanto no
    // quedan teclas pendientes (completo o inválido).
    if vim.teclas_pendientes().is_empty() {
        vim.registro_portapapeles = false;
        if let Some((texto, lineal)) = registro_previo {
            if vim.version_registro() == version {
                vim.fijar_registro(texto, lineal);
            }
        }
    }
    if mensaje.is_some() {
        layout.panel_activo_mut().mensaje_estado = mensaje;
    }
}

/// `Esc` en Normal/Visual: cancela el comando a medio escribir y sale de
/// Visual.
pub fn cancelar(layout: &mut PanelLayout, vim: &mut EstadoVim) {
    vim.esperando_registro = false;
    vim.registro_portapapeles = false;
    nucleo::cancelar(layout.editor_activo_mut(), vim);
}

/// `Esc` en Insertar con el modo VIM prendido.
pub fn salir_de_insertar(layout: &mut PanelLayout, vim: &mut EstadoVim) {
    nucleo::salir_de_insertar(layout.editor_activo_mut(), vim);
}

/// Una tecla con el prompt `:` abierto. Devuelve `Accion::Salir` si el
/// comando cierra la app (`:q` sobre la última pestaña, `:qa`...).
pub async fn tecla_linea_comando(key: KeyEvent, layout: &mut PanelLayout, estado: &mut EstadoApp) -> Accion {
    let linea = &mut estado.vim.linea_comando;
    match key.code {
        KeyCode::Esc => {
            linea.cerrar();
            return Accion::Continuar;
        }
        KeyCode::Up => linea.historial_anterior(),
        KeyCode::Down => linea.historial_siguiente(),
        KeyCode::Backspace => linea.borrar(),
        KeyCode::Enter => {
            let texto = linea.confirmar();
            return ejecutar_linea_comando(&texto, layout, estado).await;
        }
        KeyCode::Char(c) if sin_modificadores(key) => linea.escribir(c),
        _ => {}
    }
    // Mientras se escribe el comando, una confirmación de `:q` armada
    // sigue armada (ver `EstadoApp::cierre_pedido`): así `:q` + `:q`
    // cierra sin guardar igual que `Ctrl+W` + `Ctrl+W`.
    estado.cierre_pedido = estado.cierre_armado.clone();
    Accion::Continuar
}

/// Ejecuta lo escrito en la línea `:`. Los errores (comando desconocido,
/// patrón inválido, archivo que no existe) quedan en la barra de estado.
async fn ejecutar_linea_comando(texto: &str, layout: &mut PanelLayout, estado: &mut EstadoApp) -> Accion {
    let comando = match nucleo::parsear_linea_comando(texto) {
        Ok(c) => c,
        Err(error) => {
            if !error.is_empty() {
                layout.panel_activo_mut().mensaje_estado = Some(error);
            }
            return Accion::Continuar;
        }
    };
    match comando {
        ComandoLinea::Guardar => {
            guardar(layout, estado).await;
        }
        ComandoLinea::GuardarYCerrar { solo_si_modificado } => {
            let modificado = layout.editor_activo().buffer().modificado();
            if (modificado || !solo_si_modificado) && !guardar(layout, estado).await {
                return Accion::Continuar;
            }
            return cerrar(layout, estado, true);
        }
        ComandoLinea::Cerrar { forzar } => return cerrar(layout, estado, forzar),
        ComandoLinea::Salir { forzar } => {
            let modificados = layout.documentos_modificados();
            if forzar || modificados == 0 {
                return Accion::Salir;
            }
            layout.panel_activo_mut().mensaje_estado =
                Some(format!("{modificados} archivo(s) con cambios sin guardar: :qa! para salir igual"));
        }
        ComandoLinea::IrALinea(n) => {
            let editor = layout.editor_activo_mut();
            let linea = n.max(1).min(editor.buffer().num_lineas()) - 1;
            let destino = editor.buffer().inicio_byte_linea(linea);
            editor.mover_cursor_a_byte(destino);
            if editor.modo() == Modo::Normal {
                editor.entrar_modo_normal();
            }
        }
        ComandoLinea::Abrir(ruta) => match std::fs::canonicalize(&ruta) {
            Ok(ruta) if ruta.is_file() => {
                abrir_ruta_desde_explorador(layout, &mut estado.foco, ruta, &estado.config.editor);
            }
            _ => layout.panel_activo_mut().mensaje_estado = Some(format!("No existe el archivo: {ruta}")),
        },
        ComandoLinea::Sustituir(s) => {
            let editor = layout.editor_activo_mut();
            match nucleo::ediciones_de_sustitucion(editor.buffer(), editor.cursor().linea, &s) {
                Ok(ediciones) if ediciones.is_empty() => {
                    layout.panel_activo_mut().mensaje_estado = Some(format!("Patrón no encontrado: {}", s.patron));
                }
                Ok(ediciones) => {
                    let cantidad = ediciones.len();
                    editor.aplicar_ediciones(&ediciones);
                    if editor.modo() == Modo::Normal {
                        editor.entrar_modo_normal();
                    }
                    layout.panel_activo_mut().mensaje_estado = Some(format!("{cantidad} reemplazo(s)"));
                }
                Err(error) => layout.panel_activo_mut().mensaje_estado = Some(error),
            }
        }
    }
    Accion::Continuar
}

/// `:w`: el mismo camino que `Ctrl+S` (`guardar_archivo_activo`, que
/// formatea al guardar si corresponde). Un buffer sin ruta abre el prompt
/// "Guardar como", igual que `Ctrl+S`. Devuelve si quedó guardado.
async fn guardar(layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    if layout.editor_activo().buffer().ruta().is_none() {
        estado.guardar_como.abrir("");
        return false;
    }
    guardar_archivo_activo(layout, estado).await.is_ok()
}

/// `:q`/`:q!`: cierra la pestaña activa (con cambios, `:q` pide
/// confirmación como `Ctrl+W`: avisa, y un segundo `:q` seguido cierra
/// igual; `:q!` no pregunta). Si es la única pestaña del único panel,
/// sale de la app, como VIM.
fn cerrar(layout: &mut PanelLayout, estado: &mut EstadoApp, forzar: bool) -> Accion {
    let modificado = !forzar && layout.editor_activo().buffer().modificado();
    if !cierre_confirmado(layout, estado, ID_CIERRE_VIM, modificado, ":q") {
        return Accion::Continuar;
    }
    if layout.num_paneles() == 1 && layout.num_pestanas() == 1 {
        return Accion::Salir;
    }
    layout.cerrar_pestana_activa();
    Accion::Continuar
}
