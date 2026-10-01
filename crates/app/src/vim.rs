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
        } else if c.is_ascii_alphabetic() || c == '_' {
            // Registros con nombre (BACKLOG.md P3 #28): `"a`-`"z`,
            // `"A`-`"Z` (agrega al final) y `"_` (descarta).
            vim.registro_nombrado = Some(c);
            format!("\"{c}")
        } else {
            format!("Registro no soportado: \"{c} (\"a-\"z, \"A-\"Z, \"_, \"+ y \"*)")
        };
        layout.panel_activo_mut().mensaje_estado = Some(mensaje);
        return;
    }
    if c == '"' && vim.teclas_pendientes().is_empty() && !vim.registro_portapapeles && vim.registro_nombrado.is_none() {
        vim.esperando_registro = true;
        layout.panel_activo_mut().mensaje_estado = Some("\"".to_string());
        return;
    }

    let modo = config.editor.portapapeles;
    // Con un registro con nombre, el portapapeles no se toca (como
    // `"ayy` con `clipboard=unnamedplus`: va solo a `a`).
    let nombrado = vim.registro_nombrado;
    let usar_portapapeles =
        vim.registro_portapapeles || (config.editor.vim_sincronizar_portapapeles && nombrado.is_none());
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
    // `"ap`: el registro `a` se carga en el sin nombre para pegar, y se
    // devuelve después. `"_`: lo que se borre no pisa el sin nombre.
    if let Some(r) = nombrado {
        if (pega && r != '_') || r == '_' {
            registro_previo.get_or_insert_with(|| (vim.registro().to_string(), vim.registro_lineal()));
        }
        if pega && r != '_' {
            let (texto, lineal) = vim.registros.get(&r.to_ascii_lowercase()).cloned().unwrap_or_default();
            vim.fijar_registro(texto, lineal);
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
    // Lo que un `y`/`d`/`c` con `"a` dejó en el registro sin nombre va
    // también a `a` (`"A` lo agrega al final; por líneas si alguno de los
    // dos lo era). Con `"_` se descarta: el sin nombre vuelve a lo de antes.
    if let Some(r) = nombrado.filter(|_| vim.version_registro() != version) {
        if r == '_' {
            if let Some((texto, lineal)) = registro_previo.take() {
                vim.fijar_registro(texto, lineal);
            }
        } else {
            let nuevo = (vim.registro().to_string(), vim.registro_lineal());
            let entrada = vim.registros.entry(r.to_ascii_lowercase()).or_default();
            if r.is_ascii_uppercase() && !entrada.0.is_empty() {
                let separador = if (entrada.1 || nuevo.1) && !entrada.0.ends_with('\n') { "\n" } else { "" };
                entrada.0 = format!("{}{separador}{}", entrada.0, nuevo.0);
                entrada.1 |= nuevo.1;
            } else {
                *entrada = nuevo;
            }
            registro_previo = None;
        }
    }
    // Los prefijos `"+`/`"a` valen para un solo comando: se terminan en
    // cuanto no quedan teclas pendientes (completo o inválido).
    if vim.teclas_pendientes().is_empty() {
        vim.registro_portapapeles = false;
        vim.registro_nombrado = None;
        if let Some((texto, lineal)) = registro_previo {
            if vim.version_registro() == version || nombrado == Some('_') {
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
    vim.registro_nombrado = None;
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
            let prefijo = linea.prefijo();
            let texto = linea.confirmar();
            // `/` y `?` (BACKLOG.md P3 #28): buscar, no un comando.
            if prefijo != ':' {
                let aviso = nucleo::buscar_desde_prompt(layout.editor_activo_mut(), &mut estado.vim, &texto, prefijo == '?');
                if aviso.is_some() {
                    layout.panel_activo_mut().mensaje_estado = aviso;
                }
                return Accion::Continuar;
            }
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

/// Cuántas veces puede expandirse una macro desde la última tecla real:
/// una macro que se llama a sí misma (`qa...@aq`) se corta acá en vez de
/// colgar `tcode`.
const MAX_EXPANSIONES_MACRO: usize = 1000;

/// Macros (BACKLOG.md P3 #28): `q{registro}` empieza a grabar, `q`
/// termina, `[conteo]@{registro}` reproduce, `@@` repite la última. Se
/// graban los eventos de teclado tal cual llegaron (también `Esc`,
/// `Enter`, flechas y lo tipeado en Insertar) y se reproducen poniéndolos
/// en la cola de teclas sintéticas del bucle principal, así pasan por el
/// mismo camino que si se tipearan. Viven acá y no en `tcode_core` porque
/// son eventos de `crossterm`. Son un espacio aparte de los registros de
/// texto (`"ap` no pega una macro).
#[derive(Default)]
pub struct Macros {
    grabando: Option<(char, Vec<KeyEvent>)>,
    guardadas: std::collections::HashMap<char, Vec<KeyEvent>>,
    ultima: Option<char>,
    /// `q` o `@` a la espera del registro, con el conteo del `@`.
    esperando: Option<(char, usize)>,
    expansiones: usize,
}

impl Macros {
    /// El registro que se está grabando, si hay.
    pub fn grabando(&self) -> Option<char> {
        self.grabando.as_ref().map(|(r, _)| *r)
    }

    /// Cada tecla que llegó del teclado de verdad (no de una macro ni de
    /// un pegado): se graba si hay una grabación en curso, y vuelve a
    /// habilitar las expansiones.
    pub fn tecla_real(&mut self, key: KeyEvent) {
        self.expansiones = 0;
        if let Some((_, teclas)) = &mut self.grabando {
            teclas.push(key);
        }
    }

    /// Un carácter en modo Normal/Visual: si es parte de `q`/`@`, lo
    /// maneja y devuelve `Some(aviso)`; si no, `None` y sigue como
    /// comando VIM.
    pub fn tecla(
        &mut self,
        c: char,
        vim: &mut EstadoVim,
        cola: &mut std::collections::VecDeque<KeyEvent>,
    ) -> Option<Option<String>> {
        if let Some((tipo, conteo)) = self.esperando.take() {
            return Some(match tipo {
                'q' if c.is_ascii_alphanumeric() => {
                    self.grabando = Some((c, Vec::new()));
                    Some(format!("grabando @{c}"))
                }
                'q' => Some(format!("Registro inválido para grabar: {c}")),
                _ => self.reproducir(if c == '@' { self.ultima } else { Some(c) }, conteo, cola),
            });
        }
        if vim.esperando_registro || vim.registro_nombrado.is_some() || vim.registro_portapapeles {
            return None;
        }
        let pendientes = vim.teclas_pendientes();
        if c == 'q' && pendientes.is_empty() {
            if let Some((registro, mut teclas)) = self.grabando.take() {
                teclas.pop(); // la `q` que termina la grabación
                let cantidad = teclas.len();
                let destino = registro.to_ascii_lowercase();
                if registro.is_ascii_uppercase() {
                    self.guardadas.entry(destino).or_default().extend(teclas);
                } else {
                    self.guardadas.insert(destino, teclas);
                }
                return Some(Some(format!("Macro @{destino} grabada ({cantidad} teclas)")));
            }
            self.esperando = Some(('q', 1));
            return Some(Some("q".to_string()));
        }
        if c == '@' && pendientes.iter().all(char::is_ascii_digit) {
            let conteo: String = pendientes.iter().collect();
            let conteo = conteo.parse::<usize>().unwrap_or(1).max(1);
            vim.limpiar_pendiente();
            self.esperando = Some(('@', conteo));
            return Some(Some("@".to_string()));
        }
        None
    }

    fn reproducir(
        &mut self,
        registro: Option<char>,
        conteo: usize,
        cola: &mut std::collections::VecDeque<KeyEvent>,
    ) -> Option<String> {
        let Some(registro) = registro.map(|r| r.to_ascii_lowercase()) else {
            return Some("Todavía no se reprodujo ninguna macro".to_string());
        };
        let Some(teclas) = self.guardadas.get(&registro).filter(|t| !t.is_empty()) else {
            return Some(format!("La macro @{registro} está vacía"));
        };
        self.expansiones += 1;
        if self.expansiones > MAX_EXPANSIONES_MACRO {
            cola.clear();
            return Some("Macro cortada: se llama a sí misma demasiadas veces".to_string());
        }
        // Al principio de la cola y en orden: lo que ya estaba pendiente
        // (el resto de otra macro) sigue después.
        for _ in 0..conteo {
            for key in teclas.iter().rev() {
                cola.push_front(*key);
            }
        }
        self.ultima = Some(registro);
        None
    }
}

#[cfg(test)]
mod tests_macros {
    use std::collections::VecDeque;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tcode_core::EstadoVim;

    use super::Macros;

    fn tecla(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    /// Como el bucle principal: cada tecla real se graba y, si es parte
    /// de `q`/`@`, la maneja `Macros`.
    fn tipear(macros: &mut Macros, vim: &mut EstadoVim, cola: &mut VecDeque<KeyEvent>, teclas: &str) -> Option<String> {
        let mut ultimo = None;
        for c in teclas.chars() {
            macros.tecla_real(tecla(c));
            if let Some(aviso) = macros.tecla(c, vim, cola) {
                ultimo = aviso;
            }
        }
        ultimo
    }

    fn texto(cola: &VecDeque<KeyEvent>) -> String {
        cola.iter().filter_map(|k| if let KeyCode::Char(c) = k.code { Some(c) } else { None }).collect()
    }

    #[test]
    fn grabar_reproducir_repetir_y_agregar() {
        let (mut macros, mut vim, mut cola) = (Macros::default(), EstadoVim::nuevo(), VecDeque::new());
        assert_eq!(tipear(&mut macros, &mut vim, &mut cola, "qa").as_deref(), Some("grabando @a"));
        assert_eq!(macros.grabando(), Some('a'));
        // Lo del medio se procesaría como comandos VIM; acá solo se graba.
        let aviso = tipear(&mut macros, &mut vim, &mut cola, "xjq");
        assert_eq!(aviso.as_deref(), Some("Macro @a grabada (2 teclas)"));
        assert_eq!(macros.grabando(), None);
        // `3@a` pone las teclas tres veces en la cola; `@@` repite.
        tipear(&mut macros, &mut vim, &mut cola, "3");
        vim.agregar_tecla('3');
        tipear(&mut macros, &mut vim, &mut cola, "@a");
        assert_eq!(texto(&cola), "xjxjxj");
        cola.clear();
        tipear(&mut macros, &mut vim, &mut cola, "@@");
        assert_eq!(texto(&cola), "xj");
        // `qA` agrega al final de `a`.
        cola.clear();
        tipear(&mut macros, &mut vim, &mut cola, "qAkq@a");
        assert_eq!(texto(&cola), "xjk");
    }

    #[test]
    fn vacias_invalidas_y_recursion() {
        let (mut macros, mut vim, mut cola) = (Macros::default(), EstadoVim::nuevo(), VecDeque::new());
        assert!(tipear(&mut macros, &mut vim, &mut cola, "@z").is_some_and(|a| a.contains("vacía")));
        assert!(tipear(&mut macros, &mut vim, &mut cola, "@@").is_some_and(|a| a.contains("ninguna macro")));
        assert!(tipear(&mut macros, &mut vim, &mut cola, "q!").is_some_and(|a| a.contains("inválido")));
        // Con un registro elegido (`"a`), `q` no es grabar.
        vim.registro_nombrado = Some('a');
        assert!(macros.tecla('q', &mut vim, &mut cola).is_none());
        vim.registro_nombrado = None;
        // Una macro que se llama a sí misma se corta.
        tipear(&mut macros, &mut vim, &mut cola, "qb@bq");
        let mut aviso = None;
        for _ in 0..2000 {
            if let Some(a) = macros.tecla('@', &mut vim, &mut cola).and(macros.tecla('b', &mut vim, &mut cola)).flatten() {
                aviso = Some(a);
                break;
            }
        }
        assert!(aviso.is_some_and(|a| a.contains("demasiadas veces")));
        assert!(cola.is_empty());
    }
}
