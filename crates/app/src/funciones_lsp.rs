//! Funciones de LSP más allá de diagnósticos y formateo (BACKLOG.md P1
//! #17): ir a la definición (y volver), autocompletado, hover, buscar
//! referencias y renombrar símbolo. La traducción del protocolo vive en
//! `tcode_lsp` (sin UI, con sus tests) y el envío en `EstadoLsp::pedir`;
//! acá, cuándo pedir, qué hacer con cada respuesta y las teclas de sus
//! popups/listas.
//!
//! Nada de esto espera al servidor: un comando (o una pausa al tipear)
//! deja un [`Pedido`] que se manda al principio de la próxima vuelta del
//! bucle de `ejecutar` (`enviar_pedido`, después de sincronizar el texto),
//! y la respuesta llega cuando llega por el mismo `select!` que los
//! diagnósticos — `procesar_respuestas` la interpreta contra lo que haya
//! en pantalla EN ESE MOMENTO (si el cursor ya se fue de la palabra, un
//! completado viejo se descarta).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use serde_json::{json, Value};
use tcode_commands::{EntradaUbicacion, EstadoListaUbicaciones};
use tcode_core::{Editor, Modo};
use tcode_lsp::{EstadoCompletado, ItemCompletado, Ubicacion};
use tcode_ui::{Layout as PanelLayout, ModoCsv, Paleta};

use crate::lsp::{RespuestaLsp, TipoPedido};
use crate::{abrir_ruta_desde_explorador, sin_modificadores, sincronizar_lsp, EstadoApp, Foco};

/// Pausa al tipear un identificador antes de pedir completado: lo
/// bastante corta para que la lista aparezca sola al dudar, y lo bastante
/// larga para que tipear de corrido no mande una petición por tecla
/// (tipear nunca espera al servidor de todos modos: pedir es escribir un
/// mensaje en su stdin).
const PAUSA_COMPLETADO: Duration = Duration::from_millis(150);

/// Lo mismo tras un carácter de disparo (`.`): casi inmediato, pero sin
/// pedir nada mientras se tipea de corrido (`c.saldo` escrito de un
/// tirón no necesita la lista de `c.`) — cada respuesta que llega es un
/// redibujo, y con varias por palabra se notaba al tipear rápido.
const PAUSA_DISPARADOR: Duration = Duration::from_millis(40);

/// Cuántos saltos se recuerdan para "Volver" (`Alt+←`).
const MAX_PILA_VOLVER: usize = 50;

/// Una petición que se manda al principio de la próxima vuelta del bucle
/// (ver la nota del módulo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pedido {
    Definicion,
    Referencias,
    Hover,
    /// `disparador`: el carácter de disparo recién tipeado (`.`), si fue
    /// eso; `manual`: `Ctrl+Espacio` (avisa si no hay sugerencias).
    /// `reintento`: la lista anterior vino incompleta.
    Completado { disparador: Option<char>, manual: bool, reintento: bool },
    Renombrar(String),
}

/// Estado de las funciones de este módulo, un campo de `EstadoApp`.
#[derive(Default)]
pub struct EstadoFuncionesLsp {
    pub pedido: Option<Pedido>,
    pub completado: EstadoCompletado,
    /// Cuándo pedir completado por la pausa al tipear (ver
    /// [`PAUSA_COMPLETADO`]); lo vigila una rama del `select!`.
    pub completado_programado: Option<Instant>,
    /// El carácter de disparo que programó la pausa, si fue uno.
    disparador_programado: Option<char>,
    /// Se pidió completado y todavía se quiere la respuesta: cualquier
    /// tecla que no sigue la palabra lo apaga, y una respuesta que llega
    /// después se ignora.
    esperando_completado: bool,
    completado_manual: bool,
    /// Texto del popup de hover abierto (se cierra con cualquier tecla).
    pub hover: Option<String>,
    /// Varias definiciones, o las referencias.
    pub lista: EstadoListaUbicaciones,
    /// Prompt de "Renombrar símbolo" abierto, con el nombre escrito.
    pub renombrar: Option<String>,
    /// `Buffer::revision` del documento activo al pedir el renombrado: si
    /// cambió cuando llega la respuesta, sus posiciones ya no valen.
    revision_renombrar: Option<u64>,
    /// De dónde se saltó (archivo, byte), para "Volver".
    pila_volver: Vec<(PathBuf, usize)>,
}

impl EstadoFuncionesLsp {
    /// Si hay un campo de texto de este módulo capturando el teclado (lo
    /// pegado se reparte en teclas, ver `pegar_texto`).
    pub fn captura_texto(&self) -> bool {
        self.lista.activo() || self.renombrar.is_some()
    }

    fn cerrar_completado(&mut self) {
        self.completado.cerrar();
        self.completado_programado = None;
        self.disparador_programado = None;
        self.esperando_completado = false;
    }
}

fn es_identificador(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// La línea del cursor principal, su texto, el byte (dentro de la línea)
/// del cursor y el byte donde empieza la palabra que termina en él.
struct ContextoCursor {
    linea: usize,
    texto: String,
    byte_cursor: usize,
    byte_palabra: usize,
    inicio_linea: usize,
}

fn contexto_cursor(editor: &Editor) -> ContextoCursor {
    let cursor = editor.cursor();
    let buffer = editor.buffer();
    let texto = buffer.linea_texto(cursor.linea);
    let byte_cursor = texto.char_indices().nth(cursor.columna).map(|(b, _)| b).unwrap_or(texto.len());
    let byte_palabra = texto[..byte_cursor]
        .char_indices()
        .rev()
        .take_while(|(_, c)| es_identificador(*c))
        .last()
        .map(|(b, _)| b)
        .unwrap_or(byte_cursor);
    ContextoCursor { linea: cursor.linea, byte_cursor, byte_palabra, inicio_linea: buffer.inicio_byte_linea(cursor.linea), texto }
}

/// Si el documento activo es uno sobre el que tienen sentido estas
/// funciones: foco en el editor, vista de texto (no la tabla CSV).
fn editor_de_texto(layout: &PanelLayout, estado: &EstadoApp) -> bool {
    estado.foco == Foco::Editor && layout.panel_activo().modo_csv != ModoCsv::Tabla
}

fn avisar(layout: &mut PanelLayout, texto: impl Into<String>) {
    layout.panel_activo_mut().mensaje_estado = Some(texto.into());
}

/// Los comandos `lsp.*` de este módulo (atajos y paleta). Devuelve
/// `false` si no aplica acá y el comando tiene que seguir su camino
/// normal (`F2` en la vista de tabla CSV edita la celda).
pub fn comando(id: &str, layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    if !editor_de_texto(layout, estado) {
        return false;
    }
    let funciones = &mut estado.funciones_lsp;
    match id {
        "lsp.ir_a_definicion" => funciones.pedido = Some(Pedido::Definicion),
        "lsp.referencias" => funciones.pedido = Some(Pedido::Referencias),
        "lsp.hover" => funciones.pedido = Some(Pedido::Hover),
        "lsp.completar" => {
            funciones.pedido = Some(Pedido::Completado { disparador: None, manual: true, reintento: false })
        }
        "lsp.renombrar" => {
            // Precargado con la palabra bajo el cursor (a los dos lados),
            // como "Guardar como" con la ruta actual.
            let contexto = contexto_cursor(layout.editor_activo());
            let fin = contexto.texto[contexto.byte_cursor..]
                .char_indices()
                .find(|(_, c)| !es_identificador(*c))
                .map(|(b, _)| contexto.byte_cursor + b)
                .unwrap_or(contexto.texto.len());
            let palabra = &contexto.texto[contexto.byte_palabra..fin];
            let soporta = estado.lsp.capacidades(&layout.panel_activo().ruta_mostrada).map(|c| c.renombrar);
            if soporta != Some(true) {
                // Sin preguntar el nombre en vano: el mismo aviso que
                // daría `EstadoLsp::pedir`.
                let motivo = if soporta.is_some() { "el LSP no soporta renombrar" } else { "sin LSP activo" };
                avisar(layout, format!("LSP: {motivo}"));
            } else if palabra.is_empty() {
                avisar(layout, "Renombrar: el cursor no está sobre un nombre");
            } else {
                estado.funciones_lsp.renombrar = Some(palabra.to_string());
            }
        }
        "lsp.volver" => volver(layout, estado),
        _ => return false,
    }
    true
}

/// Manda el [`Pedido`] pendiente (ver la nota del módulo): sincroniza
/// antes el texto con el servidor, para que la posición del cursor sea
/// la misma para los dos. Un error (sin LSP, no soportado...) queda como
/// aviso en la barra de estado, salvo en el completado automático, que
/// tiene que ser invisible si no hay nada que ofrecer.
pub async fn enviar_pedido(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some(pedido) = estado.funciones_lsp.pedido.take() else { return };
    if !editor_de_texto(layout, estado) {
        return;
    }
    sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;

    let ruta = layout.panel_activo().ruta_mostrada.clone();
    let contexto = contexto_cursor(layout.editor_activo());
    let posicion = tcode_lsp::posicion_en_linea(contexto.linea, &contexto.texto[..contexto.byte_cursor]);
    let (tipo, extra) = match &pedido {
        Pedido::Definicion => (TipoPedido::Definicion, Value::Null),
        Pedido::Referencias => (TipoPedido::Referencias, json!({ "context": { "includeDeclaration": true } })),
        Pedido::Hover => (TipoPedido::Hover, Value::Null),
        Pedido::Completado { disparador, reintento, .. } => {
            // `triggerKind`: 1 invocado (a mano o por la pausa), 2 por un
            // carácter de disparo, 3 porque la lista vino incompleta.
            let contexto = match (disparador, reintento) {
                (Some(c), _) => json!({ "triggerKind": 2, "triggerCharacter": c.to_string() }),
                (None, true) => json!({ "triggerKind": 3 }),
                (None, false) => json!({ "triggerKind": 1 }),
            };
            (TipoPedido::Completado, json!({ "context": contexto }))
        }
        Pedido::Renombrar(nombre) => (TipoPedido::Renombrar, json!({ "newName": nombre })),
    };
    match estado.lsp.pedir(tipo, &ruta, posicion, extra).await {
        Ok(()) => match pedido {
            Pedido::Completado { manual, .. } => {
                let funciones = &mut estado.funciones_lsp;
                funciones.esperando_completado = true;
                funciones.completado_manual = manual;
                funciones.completado.ruta = ruta;
                funciones.completado.linea = contexto.linea;
                funciones.completado.inicio_palabra = contexto.inicio_linea + contexto.byte_palabra;
                funciones.completado.caracter_pedido = posicion.character;
            }
            Pedido::Renombrar(_) => {
                estado.funciones_lsp.revision_renombrar = Some(layout.editor_activo().buffer().revision());
            }
            _ => {}
        },
        Err(motivo) => {
            let automatico = matches!(pedido, Pedido::Completado { manual: false, .. });
            if !automatico {
                avisar(layout, format!("LSP: {motivo}"));
            }
        }
    }
}

/// Interpreta las respuestas llegadas (`EstadoLsp::tomar_respuestas`).
/// Casi siempre no hay ninguna: se llama en cada vuelta del bucle.
/// Devuelve si cambió algo visible — un completado que llegó tarde (ya
/// se siguió escribiendo otra cosa) se descarta sin redibujar.
pub fn procesar_respuestas(layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    let mut cambio = false;
    for RespuestaLsp { tipo, resultado } in estado.lsp.tomar_respuestas() {
        cambio |= tipo != TipoPedido::Completado;
        let valor = match resultado {
            Ok(valor) => valor,
            Err(motivo) => {
                if tipo == TipoPedido::Completado {
                    let manual = estado.funciones_lsp.completado_manual;
                    estado.funciones_lsp.esperando_completado = false;
                    if !manual {
                        continue;
                    }
                    cambio = true;
                }
                avisar(layout, format!("LSP: {motivo}"));
                continue;
            }
        };
        match tipo {
            TipoPedido::Definicion => {
                let ubicaciones = tcode_lsp::parsear_ubicaciones(&valor);
                match ubicaciones.as_slice() {
                    [] => avisar(layout, "No se encontró la definición"),
                    [unica] => saltar_a(layout, estado, unica.ruta.clone(), unica.linea, unica.caracter),
                    _ => abrir_lista(layout, estado, "Definiciones", ubicaciones),
                }
            }
            TipoPedido::Referencias => {
                let ubicaciones = tcode_lsp::parsear_ubicaciones(&valor);
                if ubicaciones.is_empty() {
                    avisar(layout, "No se encontraron referencias");
                } else {
                    let titulo = format!("Referencias ({})", ubicaciones.len());
                    abrir_lista(layout, estado, &titulo, ubicaciones);
                }
            }
            TipoPedido::Hover => match tcode_lsp::texto_hover(&valor) {
                Some(texto) => estado.funciones_lsp.hover = Some(texto),
                None => avisar(layout, "Sin información para mostrar"),
            },
            TipoPedido::Completado => cambio |= abrir_completado(layout, estado, &valor),
            TipoPedido::Renombrar => aplicar_renombrado(layout, estado, &valor),
        }
    }
    cambio
}

/// Si `a` y `b` son el mismo archivo: iguales tal cual o una vez
/// resueltos los enlaces (en macOS `/tmp` es `/private/tmp`, y un
/// servidor puede devolver cualquiera de las dos) y las rutas relativas.
fn mismo_archivo(a: &Path, b: &Path) -> bool {
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// Deja activo el archivo `ruta` en el panel activo: el que ya se ve, una
/// pestaña que ya lo tenía, o una nueva (`abrir_ruta_desde_explorador`).
/// `false` si no se pudo abrir.
fn activar_archivo(layout: &mut PanelLayout, estado: &mut EstadoApp, ruta: &Path) -> bool {
    let ya_abierto = layout
        .documentos_panel_activo()
        .filter_map(|d| d.editor.buffer().ruta())
        .find(|r| mismo_archivo(r, ruta))
        .map(Path::to_path_buf);
    match ya_abierto {
        Some(propia) => {
            layout.activar_pestana_de(&propia);
        }
        None => {
            // Relativa al directorio actual si está adentro (como al
            // abrirlo desde el explorador): el servidor devuelve rutas
            // absolutas, que en la pestaña y la barra de estado ocupan
            // demasiado.
            let relativa = std::env::current_dir()
                .and_then(std::fs::canonicalize)
                .ok()
                .and_then(|actual| Some(std::fs::canonicalize(ruta).ok()?.strip_prefix(actual).ok()?.to_path_buf()));
            let a_abrir = relativa.unwrap_or_else(|| ruta.to_path_buf());
            abrir_ruta_desde_explorador(layout, &mut estado.foco, a_abrir, &estado.config.editor)
        }
    }
    layout.editor_activo().buffer().ruta().is_some_and(|r| mismo_archivo(r, ruta))
}

/// Byte del buffer de la posición LSP `linea`/`caracter` (UTF-16).
fn byte_de(editor: &Editor, linea: u32, caracter: u32) -> usize {
    let buffer = editor.buffer();
    let linea = (linea as usize).min(buffer.num_lineas().saturating_sub(1));
    buffer.inicio_byte_linea(linea) + tcode_lsp::byte_en_linea(&buffer.linea_texto(linea), caracter)
}

/// Salta a una ubicación (abriendo el archivo en una pestaña si hace
/// falta), recordando de dónde se venía para "Volver".
fn saltar_a(layout: &mut PanelLayout, estado: &mut EstadoApp, ruta: PathBuf, linea: u32, caracter: u32) {
    let editor = layout.editor_activo();
    let origen = editor.buffer().ruta().map(|r| {
        let cursor = editor.cursor();
        (r.to_path_buf(), editor.buffer().offset_byte(cursor.linea, cursor.columna))
    });
    if !activar_archivo(layout, estado, &ruta) {
        avisar(layout, format!("No se pudo abrir {}", ruta.display()));
        return;
    }
    if let Some(origen) = origen {
        let pila = &mut estado.funciones_lsp.pila_volver;
        pila.push(origen);
        if pila.len() > MAX_PILA_VOLVER {
            pila.remove(0);
        }
    }
    let editor = layout.editor_activo_mut();
    let byte = byte_de(editor, linea, caracter);
    editor.mover_cursor_a_byte(byte);
}

/// "Volver" (`Alt+←`): al lugar de antes del último salto.
fn volver(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some((ruta, byte)) = estado.funciones_lsp.pila_volver.pop() else {
        avisar(layout, "No hay a dónde volver");
        return;
    };
    if activar_archivo(layout, estado, &ruta) {
        let editor = layout.editor_activo_mut();
        let byte = byte.min(editor.buffer().len_bytes());
        editor.mover_cursor_a_byte(byte);
    }
}

/// Abre la lista de ubicaciones: cada fila es `ruta:línea  código`, con
/// la ruta relativa al directorio actual si está adentro, y el texto de
/// esa línea sacado del buffer si el archivo está abierto (puede tener
/// cambios sin guardar) o del disco si no (leído una vez por archivo).
fn abrir_lista(layout: &mut PanelLayout, estado: &mut EstadoApp, titulo: &str, ubicaciones: Vec<Ubicacion>) {
    let actual = std::env::current_dir().ok().and_then(|d| std::fs::canonicalize(d).ok());
    let mut textos: Vec<(PathBuf, Option<Vec<String>>)> = Vec::new();
    let entradas = ubicaciones
        .into_iter()
        .map(|u| {
            if !textos.iter().any(|(r, _)| *r == u.ruta) {
                let abierto = layout
                    .documentos()
                    .into_iter()
                    .find(|d| d.editor.buffer().ruta().is_some_and(|r| mismo_archivo(r, &u.ruta)))
                    .map(|d| d.editor.buffer().lineas_texto());
                let lineas = abierto
                    .or_else(|| std::fs::read_to_string(&u.ruta).ok().map(|t| t.lines().map(str::to_string).collect()));
                textos.push((u.ruta.clone(), lineas));
            }
            let codigo = textos
                .iter()
                .find(|(r, _)| *r == u.ruta)
                .and_then(|(_, l)| l.as_ref()?.get(u.linea as usize))
                .map(|l| l.trim().to_string())
                .unwrap_or_default();
            let canonica = std::fs::canonicalize(&u.ruta).unwrap_or_else(|_| u.ruta.clone());
            let mostrada = actual
                .as_ref()
                .and_then(|a| canonica.strip_prefix(a).ok())
                .map(|r| r.display().to_string())
                .unwrap_or_else(|| u.ruta.display().to_string());
            EntradaUbicacion {
                etiqueta: format!("{mostrada}:{}  {codigo}", u.linea + 1),
                ruta: u.ruta,
                linea: u.linea,
                caracter: u.caracter,
            }
        })
        .collect();
    estado.funciones_lsp.lista.abrir(titulo, entradas);
}

/// Si el popup de completado puede seguir abierto (o abrirse) con el
/// cursor donde está: mismo archivo y línea que al pedir, en modo de
/// inserción, un solo cursor, y todavía en la palabra que empezaba en
/// `inicio_palabra`. Devuelve lo escrito de esa palabra (el filtro).
fn prefijo_vigente(layout: &PanelLayout, estado: &EstadoApp) -> Option<String> {
    let completado = &estado.funciones_lsp.completado;
    let panel = layout.panel_activo();
    let editor = &panel.editor;
    if !editor_de_texto(layout, estado)
        || panel.ruta_mostrada != completado.ruta
        || editor.modo() != Modo::Insertar
        || editor.tiene_multiples_cursores()
        || editor.cursor().linea != completado.linea
    {
        return None;
    }
    let contexto = contexto_cursor(editor);
    (contexto.inicio_linea + contexto.byte_palabra == completado.inicio_palabra)
        .then(|| contexto.texto[contexto.byte_palabra..contexto.byte_cursor].to_string())
}

/// Devuelve si cambió algo visible (se abrió el popup o hay aviso).
fn abrir_completado(layout: &mut PanelLayout, estado: &mut EstadoApp, valor: &Value) -> bool {
    let funciones = &mut estado.funciones_lsp;
    if !std::mem::take(&mut funciones.esperando_completado) {
        return false;
    }
    let manual = funciones.completado_manual;
    // Primero si todavía sirve (barato), después parsear (la lista de
    // pyright puede traer miles de items con los de auto-import).
    let Some(prefijo) = prefijo_vigente(layout, estado) else { return false };
    let (items, incompleto) = tcode_lsp::parsear_completado(valor);
    let completado = &mut estado.funciones_lsp.completado;
    completado.abrir(items, incompleto, &prefijo);
    if !completado.activo() && manual {
        avisar(layout, "Sin sugerencias");
    }
    completado.activo() || manual
}

/// Aplica el item elegido del completado como UN paso de deshacer
/// (`Editor::aplicar_ediciones`): su texto reemplaza la palabra escrita
/// hasta el cursor (o el rango de su `textEdit`, estirado hasta el cursor
/// por lo que se siguió escribiendo después de pedir), más sus
/// `additionalTextEdits` (un `use`/`import`). El cursor queda al final de
/// lo insertado.
fn aceptar_completado(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some(item) = estado.funciones_lsp.completado.aceptar() else { return };
    let caracter_pedido = estado.funciones_lsp.completado.caracter_pedido;
    let inicio_palabra = estado.funciones_lsp.completado.inicio_palabra;
    let editor = layout.editor_activo_mut();
    let ediciones = ediciones_de_item(editor, &item, inicio_palabra, caracter_pedido);
    let Some((principal, _)) = ediciones.first().cloned() else { return };
    // Lo que agregan/quitan las ediciones adicionales ANTES del reemplazo
    // principal corre el final de lo insertado.
    let corrimiento: isize = ediciones[1..]
        .iter()
        .filter(|(rango, _)| rango.end <= principal.start)
        .map(|(rango, texto)| texto.len() as isize - rango.len() as isize)
        .sum();
    editor.aplicar_ediciones(&ediciones);
    let fin = (principal.start as isize + corrimiento) as usize + item.insertar.len();
    editor.mover_cursor_a_byte(fin.min(editor.buffer().len_bytes()));
}

/// Las ediciones (en bytes del buffer actual) de aceptar `item`: la
/// principal primero. Ver [`aceptar_completado`].
fn ediciones_de_item(
    editor: &Editor,
    item: &ItemCompletado,
    inicio_palabra: usize,
    caracter_pedido: u32,
) -> Vec<(std::ops::Range<usize>, String)> {
    let contexto = contexto_cursor(editor);
    let cursor = contexto.inicio_linea + contexto.byte_cursor;
    // Columna UTF-16 del cursor ahora (lo escrito después de pedir la
    // corrió respecto de `caracter_pedido`).
    let caracter_cursor = tcode_lsp::posicion_en_linea(contexto.linea, &contexto.texto[..contexto.byte_cursor]).character;
    let (inicio, fin) = match item.rango {
        Some(rango) if rango.start.line as usize == contexto.linea && rango.start.character <= caracter_pedido => {
            let inicio = contexto.inicio_linea + tcode_lsp::byte_en_linea(&contexto.texto, rango.start.character);
            let sobrante = rango.end.character.saturating_sub(caracter_pedido);
            let fin = contexto.inicio_linea + tcode_lsp::byte_en_linea(&contexto.texto, caracter_cursor + sobrante);
            (inicio.min(cursor), fin.max(cursor))
        }
        _ => (inicio_palabra.min(cursor), cursor),
    };
    let mut ediciones = vec![(inicio..fin, item.insertar.clone())];
    if !item.adicionales.is_empty() {
        let texto = editor.buffer().a_texto();
        for (rango, nuevo) in &item.adicionales {
            let a = tcode_lsp::byte_de_posicion(&texto, rango.start);
            let b = tcode_lsp::byte_de_posicion(&texto, rango.end);
            // Una adicional que pise el reemplazo principal (no debería
            // pasar) se descarta en vez de invalidar todo.
            if a <= b && (b <= inicio || a >= fin) {
                ediciones.push((a..b, nuevo.clone()));
            }
        }
    }
    ediciones
}

/// Aplica la respuesta a `textDocument/rename` (un `WorkspaceEdit`). En
/// los documentos abiertos (en cualquier pestaña de cualquier panel), con
/// `aplicar_ediciones`: un paso de deshacer por archivo. Los que no están
/// abiertos se abren en pestañas nuevas del panel activo y se editan ahí,
/// SIN guardar: quedan como cambios pendientes para revisar (y guardar o
/// deshacer) — lo más seguro, `tcode` nunca escribe al disco sin que se
/// vea. Si el documento activo cambió desde que se pidió, no se aplica
/// nada: las posiciones ya no valen.
fn aplicar_renombrado(layout: &mut PanelLayout, estado: &mut EstadoApp, valor: &Value) {
    let revision = estado.funciones_lsp.revision_renombrar.take();
    let archivos = match tcode_lsp::parsear_workspace_edit(valor) {
        Ok(archivos) => archivos,
        Err(error) => return avisar(layout, format!("Renombrar: {error}")),
    };
    if archivos.is_empty() {
        return avisar(layout, "Renombrar: el LSP no devolvió cambios");
    }
    if revision != Some(layout.editor_activo().buffer().revision()) {
        return avisar(layout, "Renombrar: el archivo cambió mientras tanto, probá de nuevo");
    }

    let original = layout.editor_activo().buffer().ruta().map(Path::to_path_buf);
    let (mut cambios, mut abiertos_nuevos, mut fallidos) = (0, 0, 0);
    for archivo in &archivos {
        let aplicar = |editor: &mut Editor| {
            let texto = editor.buffer().a_texto();
            let ediciones: Vec<(std::ops::Range<usize>, String)> = archivo
                .ediciones
                .iter()
                .map(|(r, t)| (tcode_lsp::byte_de_posicion(&texto, r.start)..tcode_lsp::byte_de_posicion(&texto, r.end), t.clone()))
                .collect();
            editor.aplicar_ediciones(&ediciones)
        };
        let mut encontrado = false;
        for panel in layout.paneles_mut() {
            if panel.editor.buffer().ruta().is_some_and(|r| mismo_archivo(r, &archivo.ruta)) {
                encontrado = true;
                if !aplicar(&mut panel.editor) {
                    fallidos += 1;
                }
            }
        }
        if !encontrado {
            if activar_archivo(layout, estado, &archivo.ruta) {
                abiertos_nuevos += 1;
                if !aplicar(layout.editor_activo_mut()) {
                    fallidos += 1;
                }
            } else {
                fallidos += 1;
            }
        }
        cambios += archivo.ediciones.len();
    }
    if let Some(original) = original {
        activar_archivo(layout, estado, &original);
    }

    let mut aviso = format!("Renombrado: {cambios} cambios en {} archivo(s)", archivos.len());
    if abiertos_nuevos > 0 {
        aviso.push_str(&format!(", {abiertos_nuevos} abierto(s) en pestañas sin guardar"));
    }
    if fallidos > 0 {
        aviso.push_str(&format!(" ({fallidos} no se pudieron aplicar)"));
    }
    avisar(layout, aviso);
}

/// Teclas que capturan la lista de ubicaciones, el prompt de renombrar,
/// el popup de hover y el de completado, ANTES del sistema de atajos.
/// Devuelve si la tecla se consumió. La lista y el prompt capturan todo;
/// el hover se cierra con cualquier tecla (que después sigue su camino,
/// salvo `Esc`); el completado solo se queda con `↑`/`↓`/`Tab`/`Enter`/
/// `Esc` — lo demás se sigue escribiendo y [`despues_de_tecla`] refiltra.
pub fn manejar_tecla(key: KeyEvent, layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    let funciones = &mut estado.funciones_lsp;
    if funciones.lista.activo() {
        match key.code {
            KeyCode::Esc => funciones.lista.cerrar(),
            KeyCode::Up => funciones.lista.mover_arriba(),
            KeyCode::Down => funciones.lista.mover_abajo(),
            KeyCode::Backspace => funciones.lista.borrar(),
            KeyCode::Enter => {
                if let Some(entrada) = funciones.lista.confirmar() {
                    saltar_a(layout, estado, entrada.ruta, entrada.linea, entrada.caracter);
                }
            }
            KeyCode::Char(c) if sin_modificadores(key) => funciones.lista.escribir(c),
            _ => {}
        }
        return true;
    }
    if let Some(nombre) = &mut funciones.renombrar {
        match key.code {
            KeyCode::Esc => funciones.renombrar = None,
            KeyCode::Backspace => {
                nombre.pop();
            }
            KeyCode::Enter => {
                let nombre = funciones.renombrar.take().unwrap_or_default();
                if !nombre.is_empty() {
                    funciones.pedido = Some(Pedido::Renombrar(nombre));
                }
            }
            KeyCode::Char(c) if sin_modificadores(key) => nombre.push(c),
            _ => {}
        }
        return true;
    }
    if funciones.hover.take().is_some() && key.code == KeyCode::Esc {
        return true;
    }
    if funciones.completado.activo() {
        let simple = key.modifiers.is_empty();
        match key.code {
            KeyCode::Up if simple => funciones.completado.mover_arriba(),
            KeyCode::Down if simple => funciones.completado.mover_abajo(),
            KeyCode::Esc => funciones.cerrar_completado(),
            KeyCode::Tab | KeyCode::Enter if simple => {
                funciones.completado_programado = None;
                aceptar_completado(layout, estado);
            }
            _ => return false,
        }
        return true;
    }
    false
}

/// Qué hizo con el texto la tecla que acaba de pasar por el camino normal
/// (atajos o texto): lo que le importa al completado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tecleo {
    /// Se insertó este carácter (no era un atajo).
    Caracter(char),
    /// `editor.borrar_atras`.
    Borrar,
    /// Cualquier otra cosa: un atajo, una tecla sin efecto, un chord a
    /// medias.
    Otro,
}

/// Después de que una tecla llegó al editor por el camino normal: decide
/// qué pasa con el completado. Un carácter de identificador filtra el
/// popup abierto (y si la lista vino incompleta, programa otra petición),
/// o programa una tras la pausa si no había popup; un carácter de disparo
/// del servidor (`.`) pide ya mismo; `Backspace` refiltra; cualquier otra
/// cosa lo cierra. Barato: no toca más que la línea del cursor.
pub fn despues_de_tecla(tecleo: Tecleo, layout: &PanelLayout, estado: &mut EstadoApp) {
    let editor = layout.editor_activo();
    let apto = editor_de_texto(layout, estado) && editor.modo() == Modo::Insertar && !editor.tiene_multiples_cursores();
    let caracter = match tecleo {
        Tecleo::Caracter(c) if apto => Some(c),
        Tecleo::Borrar if apto => None,
        _ => {
            estado.funciones_lsp.cerrar_completado();
            return;
        }
    };

    if estado.funciones_lsp.completado.activo() && caracter.is_none_or(es_identificador) {
        match prefijo_vigente(layout, estado) {
            Some(prefijo) => {
                let completado = &mut estado.funciones_lsp.completado;
                completado.filtrar(&prefijo);
                if completado.incompleto && caracter.is_some() {
                    estado.funciones_lsp.completado_programado = Some(Instant::now() + PAUSA_COMPLETADO);
                }
            }
            None => estado.funciones_lsp.cerrar_completado(),
        }
        return;
    }
    estado.funciones_lsp.cerrar_completado();
    let Some(c) = caracter else { return };
    let ruta = &layout.panel_activo().ruta_mostrada;
    let Some(capacidades) = estado.lsp.capacidades(ruta).filter(|c| c.completado) else { return };
    if capacidades.disparadores_completado.contains(&c) {
        estado.funciones_lsp.completado_programado = Some(Instant::now() + PAUSA_DISPARADOR);
        estado.funciones_lsp.disparador_programado = Some(c);
    } else if es_identificador(c) {
        estado.funciones_lsp.completado_programado = Some(Instant::now() + PAUSA_COMPLETADO);
    }
}

/// Venció la pausa al tipear (rama del `select!` de `ejecutar`): pide el
/// completado. `reintento` si ya había una lista abierta (incompleta).
pub fn pausa_vencida(estado: &mut EstadoApp) {
    let funciones = &mut estado.funciones_lsp;
    funciones.completado_programado = None;
    let reintento = funciones.completado.activo();
    let disparador = funciones.disparador_programado.take();
    funciones.pedido = Some(Pedido::Completado { disparador, manual: false, reintento });
}

/// Dibuja lo de este módulo encima de todo lo demás.
pub fn dibujar(frame: &mut Frame, layout: &PanelLayout, funciones: &EstadoFuncionesLsp, paleta: &Paleta) {
    let area = frame.area();
    if funciones.lista.activo() {
        tcode_ui::panel_lsp::dibujar_lista_ubicaciones(frame, area, &funciones.lista, paleta);
        return;
    }
    if let Some(nombre) = &funciones.renombrar {
        tcode_ui::panel_lsp::dibujar_prompt_renombrar(frame, area, nombre, paleta);
        return;
    }
    let Some(cursor) = layout.panel_activo().estado_ui.posicion_cursor() else { return };
    if funciones.completado.activo() {
        // Anclado al inicio de la palabra (no al cursor), para que no se
        // corra a cada letra escrita.
        let contexto = contexto_cursor(layout.editor_activo());
        let escritas = contexto.texto[contexto.byte_palabra..contexto.byte_cursor].chars().count() as u16;
        let ancla = (cursor.0.saturating_sub(escritas), cursor.1);
        tcode_ui::panel_lsp::dibujar_completado(frame, area, ancla, &funciones.completado, paleta);
    } else if let Some(texto) = &funciones.hover {
        tcode_ui::panel_lsp::dibujar_hover(frame, area, cursor, texto, paleta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};

    fn editor_con(texto: &str) -> Editor {
        let mut editor = Editor::nuevo();
        editor.insertar_texto(texto);
        editor
    }

    fn item(insertar: &str, rango: Option<Range>, adicionales: Vec<(Range, String)>) -> ItemCompletado {
        ItemCompletado {
            etiqueta: insertar.to_string(),
            detalle: None,
            tipo: "",
            texto_filtro: insertar.to_string(),
            orden: String::new(),
            insertar: insertar.to_string(),
            rango,
            adicionales,
        }
    }

    fn rango(linea: u32, c1: u32, c2: u32) -> Range {
        Range { start: Position { line: linea, character: c1 }, end: Position { line: linea, character: c2 } }
    }

    #[test]
    fn la_palabra_del_cursor_con_acentos() {
        let editor = editor_con("x = señal.año");
        let contexto = contexto_cursor(&editor);
        assert_eq!(&contexto.texto[contexto.byte_palabra..contexto.byte_cursor], "año");
    }

    #[test]
    fn sin_text_edit_reemplaza_la_palabra_hasta_el_cursor() {
        // Pedido tras `ñ.` (cursor en la columna UTF-16 2); después se
        // escribió `pu` y se elige `push`.
        let mut editor = editor_con("ñ.pu");
        let ediciones = ediciones_de_item(&editor, &item("push", None, Vec::new()), 3, 2);
        assert_eq!(ediciones, vec![(3..5, "push".to_string())]);
        editor.aplicar_ediciones(&ediciones);
        assert_eq!(editor.buffer().a_texto(), "ñ.push");
    }

    #[test]
    fn el_text_edit_se_estira_hasta_lo_escrito_despues_de_pedir() {
        // `😀.p|` pedido con el cursor en UTF-16 4 (el emoji ocupa 2
        // unidades); el servidor reemplaza [3, 4) (la `p`), y después se
        // escribió `u`.
        let editor = editor_con("😀.pu");
        let ediciones = ediciones_de_item(&editor, &item("push", Some(rango(0, 3, 4)), Vec::new()), 5, 4);
        assert_eq!(ediciones, vec![(5..7, "push".to_string())]);
    }

    #[test]
    fn el_text_edit_que_reemplaza_mas_alla_del_cursor_se_corre_con_lo_escrito() {
        // Pedido en `a.b|c` (UTF-16 3) con un rango que llega a 4 (la `c`
        // de la derecha); después se escribió una `x`: el fin se corre 1.
        let mut editor = editor_con("a.bc");
        editor.mover_izquierda();
        editor.insertar_char('x');
        let ediciones = ediciones_de_item(&editor, &item("bcd", Some(rango(0, 2, 4)), Vec::new()), 2, 3);
        assert_eq!(ediciones, vec![(2..5, "bcd".to_string())]);
    }

    #[test]
    fn las_ediciones_adicionales_van_aparte_y_la_que_pisa_se_descarta() {
        let editor = editor_con("fn a() {}\nlet x = Ha");
        let adicionales =
            vec![(rango(0, 0, 0), "use std::collections::HashMap;\n".to_string()), (rango(1, 8, 9), "!".to_string())];
        let ediciones = ediciones_de_item(&editor, &item("HashMap", None, adicionales), 18, 10);
        assert_eq!(ediciones.len(), 2);
        assert_eq!(ediciones[0], (18..20, "HashMap".to_string()));
        assert_eq!(ediciones[1].0, 0..0);
    }
}
