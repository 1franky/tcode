//! Blame en línea (BACKLOG.md P2 #25): con `editor.blame_en_linea`
//! prendido, al final de la línea del cursor aparece atenuado quién la
//! cambió por última vez, cuándo y con qué commit
//! (`PanelEditor::anotacion`, que dibuja `vista_codigo`).
//!
//! `git blame` se corre en un hilo aparte (`tcode_fs::blame_linea`) y
//! solo cuando el cursor se queda quieto [`PAUSA`] en una línea: moverse
//! con las flechas no lanza un proceso por tecla. Lo vigila el tick del
//! bucle principal mientras el documento activo está en un repo.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

use tcode_config::Config;
use tcode_fs::{blame_linea, texto_blame, InfoBlame};
use tcode_ui::Layout as PanelLayout;

/// Cuánto tiene que quedarse quieto el cursor antes de pedir el blame.
const PAUSA: Duration = Duration::from_millis(300);

/// Qué línea de qué documento, con qué texto (la revisión del buffer).
type Clave = (String, usize, u64);

#[derive(Default)]
pub struct EstadoBlame {
    /// La línea actual del cursor y desde cuándo está ahí.
    clave: Option<Clave>,
    desde: Option<Instant>,
    /// El blame pedido y todavía sin respuesta.
    pedido: Option<(Clave, Receiver<Option<InfoBlame>>)>,
    /// La línea cuya anotación ya se muestra (o se intentó mostrar).
    mostrado: Option<Clave>,
    /// Si hay alguna anotación puesta (para sacarla al apagar).
    hay_anotacion: bool,
}

/// La clave del documento activo, si está trackeado en git.
fn clave_activa(layout: &PanelLayout) -> Option<(Clave, PathBuf)> {
    let panel = layout.panel_activo();
    let ruta = panel.editor.buffer().ruta()?.to_path_buf();
    if !panel.git.tiene_base() {
        return None;
    }
    let clave = (panel.ruta_mostrada.clone(), panel.editor.cursor().linea, panel.editor.buffer().revision());
    Some((clave, ruta))
}

impl EstadoBlame {
    /// Si el tick tiene que correr: prendido y con algo que hacer.
    pub fn necesita_tick(&self, layout: &PanelLayout, config: &Config) -> bool {
        if !config.editor.blame_en_linea {
            return self.hay_anotacion;
        }
        self.pedido.is_some() || layout.panel_activo().git.en_repo()
    }

    /// Un tick: saca la anotación si el cursor se movió, pide el blame si
    /// se quedó quieto, y la pone si llegó. Devuelve si hay que redibujar.
    pub fn tick(&mut self, layout: &mut PanelLayout, config: &Config) -> bool {
        if !config.editor.blame_en_linea {
            return self.limpiar(layout);
        }
        let actual = clave_activa(layout);
        let mut cambio = false;
        if actual.as_ref().map(|(c, _)| c) != self.clave.as_ref() {
            self.clave = actual.as_ref().map(|(c, _)| c.clone());
            self.desde = Some(Instant::now());
            self.pedido = None;
            self.mostrado = None;
            cambio |= self.limpiar(layout);
        }
        let Some((clave, ruta)) = actual else { return cambio };

        if self.pedido.is_none() && self.mostrado.is_none() && self.desde.is_some_and(|d| d.elapsed() >= PAUSA) {
            let (emisor, receptor) = mpsc::channel();
            let contenido = layout.editor_activo().buffer().a_texto();
            let linea = clave.1;
            std::thread::spawn(move || {
                let _ = emisor.send(blame_linea(&ruta, linea, &contenido));
            });
            self.pedido = Some((clave.clone(), receptor));
        }

        if let Some((pedida, receptor)) = &self.pedido {
            match receptor.try_recv() {
                Ok(info) => {
                    if *pedida == clave {
                        let ahora = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        layout.panel_activo_mut().anotacion = info.map(|i| (clave.1, texto_blame(&i, ahora)));
                        self.hay_anotacion = true;
                        cambio = true;
                    }
                    self.mostrado = Some(pedida.clone());
                    self.pedido = None;
                }
                Err(TryRecvError::Disconnected) => self.pedido = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        cambio
    }

    /// Saca las anotaciones de todos los documentos. Devuelve si había.
    fn limpiar(&mut self, layout: &mut PanelLayout) -> bool {
        if !std::mem::take(&mut self.hay_anotacion) {
            return false;
        }
        for panel in layout.paneles_mut() {
            panel.anotacion = None;
        }
        true
    }
}
