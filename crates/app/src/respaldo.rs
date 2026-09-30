//! Recuperación ante cierres inesperados (BACKLOG.md P2 #21): copias de
//! respaldo de los buffers con cambios sin guardar mientras `tcode`
//! corre, y al arrancar, la oferta de recuperar las que dejó un cierre de
//! golpe en esta carpeta (el disco y el bloqueo entre procesos están en
//! `tcode_config::respaldos`).
//!
//! Escribir nunca frena el tipeo: el bucle solo compara las revisiones de
//! los buffers modificados en el tick y, si cambiaron, le pasa clones de
//! sus `Rope` (O(1)) a un hilo aparte que arma el texto y escribe. Se
//! escribe tras [`PAUSA`] sin cambios, o cada [`DEMORA_MAXIMA`] como
//! mucho si se sigue escribiendo sin parar.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ropey::Rope;
use tcode_config::{CarpetaRespaldos, Huerfano, Respaldo};
use tcode_core::{Cursor, Editor};
use tcode_ui::Layout as PanelLayout;

use crate::funciones_lsp::mismo_archivo;
use crate::{abrir_ruta_desde_explorador, EstadoApp};

/// Sin cambios durante esto, se escribe.
const PAUSA: Duration = Duration::from_secs(1);
/// Escribiendo sin parar, se escribe igual cada esto.
const DEMORA_MAXIMA: Duration = Duration::from_secs(5);

/// Un buffer modificado tal como se le pasa al hilo que escribe.
struct Pendiente {
    ruta: Option<String>,
    ruta_mostrada: String,
    texto: Rope,
    cursor: Cursor,
}

enum Mensaje {
    Escribir(Vec<Pendiente>),
    Terminar,
}

/// Las copias de respaldo de este proceso.
pub struct EstadoRespaldo {
    hilo: Option<(Sender<Mensaje>, JoinHandle<()>)>,
    /// Revisiones de los buffers modificados en la última escritura.
    escrita: Vec<u64>,
    /// Las últimas vistas en el tick, desde cuándo, y desde cuándo hay
    /// algo sin escribir.
    vista: Vec<u64>,
    vista_desde: Instant,
    pendiente_desde: Option<Instant>,
}

impl EstadoRespaldo {
    /// Crea la carpeta de respaldos de este proceso en `base` y el hilo
    /// que escribe. Si no se puede (disco de solo lectura, sin permisos),
    /// sigue sin respaldos: no es motivo para no abrir el editor.
    pub fn iniciar(base: &Path, proyecto: &Path) -> Self {
        let hilo = CarpetaRespaldos::crear(base, proyecto).ok().and_then(|carpeta| {
            let (enviar, recibir) = mpsc::channel::<Mensaje>();
            let hilo = std::thread::Builder::new()
                .name("tcode-respaldos".to_string())
                .spawn(move || {
                    while let Ok(mut mensaje) = recibir.recv() {
                        // Si se acumularon varias, solo importa la última.
                        while let Ok(siguiente) = recibir.try_recv() {
                            mensaje = siguiente;
                        }
                        match mensaje {
                            Mensaje::Escribir(pendientes) => {
                                let respaldos = pendientes
                                    .into_iter()
                                    .map(|p| Respaldo {
                                        ruta: p.ruta,
                                        ruta_mostrada: p.ruta_mostrada,
                                        texto: p.texto.to_string(),
                                        linea: p.cursor.linea,
                                        columna: p.cursor.columna,
                                    })
                                    .collect();
                                let _ = carpeta.escribir(respaldos);
                            }
                            Mensaje::Terminar => break,
                        }
                    }
                    carpeta.terminar();
                })
                .ok()?;
            Some((enviar, hilo))
        });
        Self { hilo, escrita: Vec::new(), vista: Vec::new(), vista_desde: Instant::now(), pendiente_desde: None }
    }

    /// Si el bucle tiene que mirar en el tick: hay algo modificado, o
    /// quedaron respaldos escritos que habrá que borrar al guardar.
    pub fn necesita_tick(&self, layout: &PanelLayout) -> bool {
        self.hilo.is_some() && (!self.escrita.is_empty() || layout.documentos_modificados() > 0)
    }

    /// En cada tick: escribe si los buffers modificados cambiaron y ya
    /// tocaba. Devuelve si escribió (para guardar también la sesión).
    pub fn tick(&mut self, layout: &PanelLayout) -> bool {
        let Some((enviar, _)) = &self.hilo else { return false };
        let modificados: Vec<_> = layout.documentos().into_iter().filter(|d| d.editor.buffer().modificado()).collect();
        let revisiones: Vec<u64> = modificados.iter().map(|d| d.editor.buffer().revision()).collect();
        let ahora = Instant::now();
        if revisiones == self.escrita {
            self.pendiente_desde = None;
            self.vista = revisiones;
            return false;
        }
        if revisiones != self.vista {
            self.vista = revisiones.clone();
            self.vista_desde = ahora;
            self.pendiente_desde.get_or_insert(ahora);
        }
        let tocaba = self.vista_desde.elapsed() >= PAUSA
            || self.pendiente_desde.is_some_and(|desde| desde.elapsed() >= DEMORA_MAXIMA);
        if !tocaba {
            return false;
        }
        let pendientes = modificados
            .iter()
            .map(|d| {
                let buffer = d.editor.buffer();
                Pendiente {
                    ruta: buffer.ruta().map(|r| r.display().to_string()),
                    ruta_mostrada: d.ruta_mostrada.clone(),
                    texto: buffer.rope().clone(),
                    cursor: d.editor.cursor(),
                }
            })
            .collect();
        let _ = enviar.send(Mensaje::Escribir(pendientes));
        self.escrita = revisiones;
        self.pendiente_desde = None;
        true
    }

    /// Salida normal (también "salir sin guardar": lo descartado a
    /// propósito no se ofrece recuperar): borra los respaldos.
    pub fn terminar(&mut self) {
        if let Some((enviar, hilo)) = self.hilo.take() {
            let _ = enviar.send(Mensaje::Terminar);
            let _ = hilo.join();
        }
    }
}

/// Lo que se muestra en el diálogo por cada archivo recuperable: la ruta
/// relativa a la carpeta del proyecto si está adentro (`Ctrl+P` abre con
/// rutas absolutas, que no entran en el recuadro).
pub fn nombres(huerfanos: &[Huerfano]) -> Vec<String> {
    let proyecto = std::env::current_dir().and_then(std::fs::canonicalize).ok();
    let mut nombres: Vec<String> = Vec::new();
    for respaldo in huerfanos.iter().flat_map(|h| &h.respaldos) {
        let relativa = respaldo.ruta.as_ref().and_then(|ruta| {
            let canonica = std::fs::canonicalize(ruta).ok()?;
            Some(canonica.strip_prefix(proyecto.as_ref()?).ok()?.display().to_string())
        });
        let nombre = relativa.unwrap_or_else(|| respaldo.ruta_mostrada.clone());
        if !nombres.contains(&nombre) {
            nombres.push(nombre);
        }
    }
    nombres
}

/// `Enter` en el diálogo: abre cada archivo (o activa su pestaña) con el
/// texto del respaldo como cambio sin guardar — un solo paso de deshacer
/// vuelve a lo que hay en disco — y el cursor donde estaba. Un
/// "[Sin nombre]" o un archivo que ya no existe se abren en una pestaña
/// sin nombre. Después borra los respaldos: lo recuperado vuelve a estar
/// en buffers modificados, que se respaldan de nuevo desde este proceso.
pub fn recuperar(huerfanos: Vec<Huerfano>, layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let mut recuperados = 0;
    // Del más viejo al más nuevo: si el mismo archivo aparece dos veces,
    // gana el último.
    for huerfano in &huerfanos {
        for respaldo in &huerfano.respaldos {
            let existe = respaldo.ruta.as_ref().filter(|r| Path::new(r).is_file());
            let abierto = existe.is_some_and(|ruta| {
                abrir_ruta_desde_explorador(layout, &mut estado.foco, PathBuf::from(ruta), &estado.config.editor);
                layout.editor_activo().buffer().ruta().is_some_and(|r| mismo_archivo(r, Path::new(ruta)))
            });
            if !abierto {
                let mut editor = Editor::nuevo();
                if estado.config.editor.modo_vim {
                    editor.entrar_modo_normal();
                }
                let nombre = match &respaldo.ruta {
                    Some(_) => format!("[Recuperado] {}", respaldo.ruta_mostrada),
                    None => "[Recuperado]".to_string(),
                };
                layout.abrir_en_activo(editor, nombre);
            }
            let editor = layout.editor_activo_mut();
            if editor.buffer().a_texto() != respaldo.texto {
                let fin = editor.buffer().len_bytes();
                editor.reemplazar_rango_bytes(0, fin, &respaldo.texto);
            }
            let cursor = Cursor { linea: respaldo.linea, columna: respaldo.columna };
            editor.fijar_seleccion(cursor, cursor);
            recuperados += 1;
        }
    }
    for huerfano in huerfanos {
        huerfano.borrar();
    }
    let s = if recuperados == 1 { "" } else { "s" };
    layout.panel_activo_mut().mensaje_estado =
        Some(format!("Recuperado{s} {recuperados} archivo{s} con cambios sin guardar"));
}
