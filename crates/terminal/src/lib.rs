//! Terminal integrada de `tcode` (BACKLOG.md P3 #26): una shell corriendo
//! en una pseudo-terminal (`portable-pty`: PTY en Unix, ConPTY en
//! Windows) cuya salida se interpreta con `vt100` — secuencias de escape,
//! colores, cursor, pantalla alternativa (`vim`, `htop`, `less`) e
//! historial — en una grilla de celdas que la UI dibuja.
//!
//! Sin nada de UI ni de teclado: `app` traduce las teclas a bytes
//! ([`SesionTerminal::escribir`]) y `tcode-ui` dibuja
//! [`SesionTerminal::pantalla`]. Puede haber varias a la vez
//! ([`Terminales`], una por pestaña): la salida de cada shell la lee un
//! hilo aparte y la manda, con el id de su sesión, por UN canal de
//! `tokio` compartido, así el bucle principal espera a todas en su
//! `select!` sin sondear ([`Terminales::esperar`]).

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// Lo que manda el hilo lector de una sesión: su id y lo que leyó, o
/// `None` cuando la shell terminó.
type Salida = (u64, Option<Vec<u8>>);

/// Líneas que se guardan arriba de la pantalla para volver a verlas con
/// la rueda.
pub const LINEAS_HISTORIAL: usize = 5000;

/// La shell a lanzar: `$SHELL` (o `/bin/sh`) en Unix; `%COMSPEC%` (o
/// `cmd.exe`) en Windows.
pub fn shell_por_defecto() -> String {
    if cfg!(windows) {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/sh".to_string())
    }
}

/// Una shell corriendo, con su pantalla.
pub struct SesionTerminal {
    parser: vt100::Parser,
    escritor: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    hijo: Box<dyn Child + Send + Sync>,
    id: u64,
    /// El nombre de la shell (para el título del panel).
    pub nombre: String,
    terminada: bool,
}

impl SesionTerminal {
    /// Lanza `shell` en `carpeta` con una pantalla de `filas` x
    /// `columnas`. `TERM=xterm-256color` (lo que `vt100` interpreta).
    fn lanzar(shell: &str, carpeta: &Path, filas: u16, columnas: u16, id: u64, emisor: UnboundedSender<Salida>) -> Result<Self> {
        let (filas, columnas) = (filas.max(2), columnas.max(10));
        let par = native_pty_system()
            .openpty(PtySize { rows: filas, cols: columnas, pixel_width: 0, pixel_height: 0 })
            .context("no se pudo abrir una pseudo-terminal")?;
        let mut comando = CommandBuilder::new(shell);
        comando.cwd(carpeta);
        comando.env("TERM", "xterm-256color");
        comando.env("COLORTERM", "truecolor");
        let hijo = par.slave.spawn_command(comando).with_context(|| format!("no se pudo lanzar '{shell}'"))?;
        // El extremo esclavo ya lo tiene la shell: soltarlo acá hace que
        // el lector vea EOF cuando la shell termina.
        drop(par.slave);
        let mut lector = par.master.try_clone_reader().context("no se pudo leer la terminal")?;
        let escritor = par.master.take_writer().context("no se pudo escribir en la terminal")?;
        std::thread::Builder::new()
            .name("tcode-terminal".to_string())
            .spawn(move || {
                let mut buffer = vec![0u8; 16 * 1024];
                loop {
                    match lector.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if emisor.send((id, Some(buffer[..n].to_vec()))).is_err() {
                                return;
                            }
                        }
                    }
                }
                let _ = emisor.send((id, None));
            })
            .context("no se pudo crear el hilo de la terminal")?;
        let nombre = Path::new(shell).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Self {
            parser: vt100::Parser::new(filas, columnas, LINEAS_HISTORIAL),
            escritor,
            master: par.master,
            hijo,
            id,
            nombre,
            terminada: false,
        })
    }

    fn procesar(&mut self, datos: &[u8]) {
        self.parser.process(datos);
    }

    /// Si la shell terminó (`exit`, se cerró sola).
    pub fn terminada(&self) -> bool {
        self.terminada
    }

    /// Manda bytes a la shell (teclas ya traducidas, texto pegado). Vuelve
    /// la vista al final del historial, como cualquier terminal.
    pub fn escribir(&mut self, bytes: &[u8]) {
        self.parser.set_scrollback(0);
        let _ = self.escritor.write_all(bytes);
        let _ = self.escritor.flush();
    }

    /// Ajusta la pseudo-terminal y la pantalla a `filas` x `columnas` (la
    /// shell recibe `SIGWINCH`). No hace nada si ya tenía ese tamaño.
    pub fn redimensionar(&mut self, filas: u16, columnas: u16) {
        let (filas, columnas) = (filas.max(2), columnas.max(10));
        if self.parser.screen().size() == (filas, columnas) {
            return;
        }
        self.parser.set_size(filas, columnas);
        let _ = self.master.resize(PtySize { rows: filas, cols: columnas, pixel_width: 0, pixel_height: 0 });
    }

    pub fn pantalla(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Rueda del mouse: `delta` filas hacia el historial (positivo =
    /// hacia arriba). `vt100` recorta al historial que haya.
    pub fn desplazar_historial(&mut self, delta: isize) {
        let actual = self.parser.screen().scrollback() as isize;
        self.parser.set_scrollback((actual + delta).max(0) as usize);
    }

    /// Termina la shell (al cerrar la terminal o salir de `tcode`).
    pub fn cerrar(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
    }
}

impl Drop for SesionTerminal {
    fn drop(&mut self) {
        if !self.terminada {
            self.cerrar();
        }
    }
}

/// Las terminales abiertas (pestañas del panel) y cuál se ve.
pub struct Terminales {
    sesiones: Vec<SesionTerminal>,
    activa: usize,
    emisor: UnboundedSender<Salida>,
    receptor: UnboundedReceiver<Salida>,
    siguiente_id: u64,
}

impl Default for Terminales {
    fn default() -> Self {
        let (emisor, receptor) = unbounded_channel();
        Self { sesiones: Vec::new(), activa: 0, emisor, receptor, siguiente_id: 1 }
    }
}

/// Qué pasó mientras se esperaba ([`Terminales::esperar`]).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Novedades {
    /// Cuántas shells terminaron (sus pestañas ya se cerraron).
    pub cerradas: usize,
}

impl Terminales {
    pub fn vacia(&self) -> bool {
        self.sesiones.is_empty()
    }

    pub fn cantidad(&self) -> usize {
        self.sesiones.len()
    }

    pub fn indice_activa(&self) -> usize {
        self.activa
    }

    pub fn activa(&self) -> Option<&SesionTerminal> {
        self.sesiones.get(self.activa)
    }

    pub fn activa_mut(&mut self) -> Option<&mut SesionTerminal> {
        self.sesiones.get_mut(self.activa)
    }

    /// Los nombres de las shells, en orden (para las pestañas).
    pub fn nombres(&self) -> impl Iterator<Item = &str> {
        self.sesiones.iter().map(|s| s.nombre.as_str())
    }

    /// Lanza una shell nueva y la deja activa.
    pub fn abrir(&mut self, shell: &str, carpeta: &Path, filas: u16, columnas: u16) -> Result<()> {
        let id = self.siguiente_id;
        self.siguiente_id += 1;
        let sesion = SesionTerminal::lanzar(shell, carpeta, filas, columnas, id, self.emisor.clone())?;
        self.sesiones.push(sesion);
        self.activa = self.sesiones.len() - 1;
        Ok(())
    }

    /// Activa la siguiente (`adelante`) o la anterior, dando la vuelta.
    pub fn cambiar(&mut self, adelante: bool) {
        let n = self.sesiones.len();
        if n > 1 {
            self.activa = if adelante { (self.activa + 1) % n } else { (self.activa + n - 1) % n };
        }
    }

    /// Termina la shell activa y cierra su pestaña.
    pub fn cerrar_activa(&mut self) {
        if self.activa < self.sesiones.len() {
            let mut sesion = self.sesiones.remove(self.activa);
            sesion.cerrar();
            self.activa = self.activa.min(self.sesiones.len().saturating_sub(1));
        }
    }

    /// Todas toman el tamaño del panel (también las que no se ven, así al
    /// volver a ellas ya están bien).
    pub fn redimensionar(&mut self, filas: u16, columnas: u16) {
        for sesion in &mut self.sesiones {
            sesion.redimensionar(filas, columnas);
        }
    }

    /// Espera la próxima salida de cualquier shell y la interpreta, junto
    /// con todo lo que ya esté esperando; cierra las pestañas de las que
    /// terminaron. Para el `select!` del bucle principal (solo con alguna
    /// abierta: si no, no llega nada nunca).
    pub async fn esperar(&mut self) -> Novedades {
        let mut novedades = Novedades::default();
        if let Some(salida) = self.receptor.recv().await {
            self.procesar(salida, &mut novedades);
        }
        while let Ok(salida) = self.receptor.try_recv() {
            self.procesar(salida, &mut novedades);
        }
        novedades
    }

    fn procesar(&mut self, (id, datos): Salida, novedades: &mut Novedades) {
        let Some(i) = self.sesiones.iter().position(|s| s.id == id) else { return };
        match datos {
            Some(datos) => self.sesiones[i].procesar(&datos),
            None => {
                let mut sesion = self.sesiones.remove(i);
                sesion.terminada = true;
                if i < self.activa || self.activa >= self.sesiones.len() {
                    self.activa = self.activa.saturating_sub(1);
                }
                novedades.cerradas += 1;
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    async fn esperar_hasta(terminales: &mut Terminales, condicion: impl Fn(&Terminales) -> bool) {
        for _ in 0..50 {
            if condicion(terminales) {
                return;
            }
            let _ = tokio::time::timeout(std::time::Duration::from_millis(100), terminales.esperar()).await;
        }
    }

    /// Dos shells de verdad: cada una ejecuta lo suyo y muestra su salida;
    /// `exit` cierra solo la suya.
    #[tokio::test]
    async fn varias_shells_con_un_solo_canal() {
        let dir = std::env::temp_dir();
        let mut terminales = Terminales::default();
        terminales.abrir("/bin/sh", &dir, 10, 40).unwrap();
        terminales.abrir("/bin/sh", &dir, 10, 40).unwrap();
        assert_eq!((terminales.cantidad(), terminales.indice_activa()), (2, 1));
        terminales.activa_mut().unwrap().escribir(b"echo segunda$((1+1))\r");
        terminales.cambiar(true);
        assert_eq!(terminales.indice_activa(), 0);
        terminales.activa_mut().unwrap().escribir(b"echo primera$((0+1))\r");
        let contiene = |t: &Terminales, i: usize, texto: &str| t.sesiones[i].pantalla().contents().contains(texto);
        esperar_hasta(&mut terminales, |t| contiene(t, 0, "primera1") && contiene(t, 1, "segunda2")).await;
        assert!(contiene(&terminales, 0, "primera1") && !contiene(&terminales, 0, "segunda2"));
        assert!(contiene(&terminales, 1, "segunda2"));
        terminales.redimensionar(5, 20);
        assert_eq!(terminales.activa().unwrap().pantalla().size(), (5, 20));
        // `exit` en la primera: queda la segunda, activa.
        terminales.activa_mut().unwrap().escribir(b"exit\r");
        esperar_hasta(&mut terminales, |t| t.cantidad() == 1).await;
        assert_eq!(terminales.cantidad(), 1);
        assert!(contiene(&terminales, 0, "segunda2"));
        terminales.cerrar_activa();
        assert!(terminales.vacia());
    }
}
