//! Terminal integrada de `tcode` (BACKLOG.md P3 #26): una shell corriendo
//! en una pseudo-terminal (`portable-pty`: PTY en Unix, ConPTY en
//! Windows) cuya salida se interpreta con `vt100` — secuencias de escape,
//! colores, cursor, pantalla alternativa (`vim`, `htop`, `less`) e
//! historial — en una grilla de celdas que la UI dibuja.
//!
//! Sin nada de UI ni de teclado: `app` traduce las teclas a bytes
//! ([`SesionTerminal::escribir`]) y `tcode-ui` dibuja
//! [`SesionTerminal::pantalla`]. La salida de la shell la lee un hilo
//! aparte y la manda por un canal de `tokio`, así el bucle principal la
//! espera en su `select!` sin sondear ([`SesionTerminal::siguiente`]).

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

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
    receptor: UnboundedReceiver<Vec<u8>>,
    /// El nombre de la shell (para el título del panel).
    pub nombre: String,
    terminada: bool,
}

impl SesionTerminal {
    /// Lanza `shell` en `carpeta` con una pantalla de `filas` x
    /// `columnas`. `TERM=xterm-256color` (lo que `vt100` interpreta).
    pub fn lanzar(shell: &str, carpeta: &Path, filas: u16, columnas: u16) -> Result<Self> {
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
        let (emisor, receptor) = unbounded_channel();
        std::thread::Builder::new()
            .name("tcode-terminal".to_string())
            .spawn(move || {
                let mut buffer = vec![0u8; 16 * 1024];
                loop {
                    match lector.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if emisor.send(buffer[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
            .context("no se pudo crear el hilo de la terminal")?;
        let nombre = Path::new(shell).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Self {
            parser: vt100::Parser::new(filas, columnas, LINEAS_HISTORIAL),
            escritor,
            master: par.master,
            hijo,
            receptor,
            nombre,
            terminada: false,
        })
    }

    /// Espera la próxima salida de la shell y la interpreta (junto con
    /// todo lo que ya esté esperando). `false` si la shell terminó. Para
    /// el `select!` del bucle principal.
    pub async fn siguiente(&mut self) -> bool {
        match self.receptor.recv().await {
            Some(datos) => {
                self.procesar(&datos);
                self.recibir_pendiente();
                true
            }
            None => {
                self.terminada = true;
                false
            }
        }
    }

    /// Interpreta lo que ya llegó, sin esperar. Devuelve si hubo algo.
    pub fn recibir_pendiente(&mut self) -> bool {
        let mut hubo = false;
        loop {
            match self.receptor.try_recv() {
                Ok(datos) => {
                    self.procesar(&datos);
                    hubo = true;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.terminada = true;
                    break;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
            }
        }
        hubo
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// Una shell de verdad: lo que se le escribe se ejecuta y la salida
    /// aparece en la pantalla interpretada; `exit` la termina.
    #[tokio::test]
    async fn una_shell_ejecuta_comandos_y_termina() {
        let dir = std::env::temp_dir();
        let mut sesion = SesionTerminal::lanzar("/bin/sh", &dir, 10, 40).unwrap();
        sesion.escribir(b"echo hola$((1+2))\r");
        let mut texto = String::new();
        for _ in 0..50 {
            tokio::time::timeout(std::time::Duration::from_millis(100), sesion.siguiente()).await.ok();
            texto = sesion.pantalla().contents();
            if texto.contains("hola3") {
                break;
            }
        }
        assert!(texto.contains("hola3"), "salida: {texto:?}");
        sesion.redimensionar(5, 20);
        assert_eq!(sesion.pantalla().size(), (5, 20));
        sesion.escribir(b"exit\r");
        for _ in 0..50 {
            if !tokio::time::timeout(std::time::Duration::from_millis(100), sesion.siguiente()).await.unwrap_or(true) {
                break;
            }
        }
        assert!(sesion.terminada());
    }
}
