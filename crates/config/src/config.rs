use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::tema::TEMA_POR_DEFECTO;

/// Ajustes generales del editor, persistidos en `config.toml`. Corresponde
/// a las secciones "Editor" e "Interfaz" del panel de administración
/// (PLAN.md §5) — ese panel visual llega en M4; por ahora el archivo se
/// edita a mano o se regenera con los valores por defecto.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub editor: ConfigEditor,
    pub interfaz: ConfigInterfaz,
    pub lenguajes: ConfigLenguajes,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigEditor {
    pub tamano_tabulacion: usize,
    pub usar_espacios: bool,
    pub ajuste_linea: bool,
    pub numeros_de_linea: bool,
    /// Modo VIM (M5, alcance "lo esencial" — sin operadores combinables
    /// como `dw`, sin conteos numéricos, sin `:`): modos Normal/Insertar
    /// con `Esc`/`i`/`a`/`o`, movimientos `hjkl`/`0`/`$`/`gg`/`G`, y
    /// `x`/`dd`/`yy`/`p`/`u`. Apagado por defecto — no cambia el
    /// comportamiento de nadie que no lo prenda a propósito, ni acá ni en
    /// la sección "Editor" del panel de administración.
    pub modo_vim: bool,
    /// Regla vertical / guía de columna (BACKLOG.md P1 #5): marca una
    /// columna fija de la vista de código con un fondo distinto, para
    /// usarla como guía de ancho de línea (80/100/120...). `None` =
    /// apagada (default) — un solo campo en vez de un booleano +
    /// número separados porque el propio valor ya expresa "prendida en
    /// esta columna" o "apagada" sin un segundo estado que pueda quedar
    /// inconsistente (p. ej. "prendida" pero con la columna en 0).
    pub columna_regla: Option<usize>,
}

impl Default for ConfigEditor {
    fn default() -> Self {
        Self {
            tamano_tabulacion: 4,
            usar_espacios: true,
            ajuste_linea: false,
            numeros_de_linea: true,
            modo_vim: false,
            columna_regla: None,
        }
    }
}

/// Corresponde a la sección "Interfaz" del panel de administración
/// (PLAN.md §5.5). Los `statusbar_*` son los elementos "marcar/
/// desmarcar" que menciona el plan — salvo la rama git, que todavía no
/// existe como feature (no tiene sentido un toggle para algo que nunca
/// se muestra); densidad de UI y mostrar/ocultar tabs/breadcrumbs quedan
/// para cuando esos widgets existan.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigInterfaz {
    pub tema: String,
    pub mostrar_statusbar: bool,
    pub statusbar_posicion_cursor: bool,
    pub statusbar_codificacion: bool,
    pub statusbar_eol: bool,
    pub statusbar_lenguaje: bool,
    pub statusbar_diagnosticos: bool,
    pub statusbar_modo: bool,
}

impl Default for ConfigInterfaz {
    fn default() -> Self {
        Self {
            tema: TEMA_POR_DEFECTO.to_string(),
            mostrar_statusbar: true,
            statusbar_posicion_cursor: true,
            statusbar_codificacion: true,
            statusbar_eol: true,
            statusbar_lenguaje: true,
            statusbar_diagnosticos: true,
            statusbar_modo: true,
        }
    }
}

/// Comando + argumentos configurados a mano para el LSP de un lenguaje
/// (PLAN.md §5.3: "Configurar comando, argumentos y variables de
/// entorno") — sobreescribe lo que `tcode_lsp::comando_para` trae fijo
/// para ese lenguaje (que puede ser nada, como todos salvo Python por
/// ahora). Variables de entorno quedan fuera de esta pieza: por ahora
/// solo comando + argumentos, que ya es lo que hace falta para apuntar
/// a `rust-analyzer`/`gopls`/`clangd`/etc. sin recompilar.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ComandoLsp {
    pub comando: String,
    pub argumentos: Vec<String>,
}

/// Corresponde a la sección "Lenguajes / LSP" del panel de
/// administración (PLAN.md §5.3).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ConfigLenguajes {
    /// Ids de lenguaje (`tcode_syntax::Lenguaje::id`, ej. `"python"`)
    /// cuyo LSP no se lanza aunque haya uno configurado, aun si el
    /// archivo activo es de ese lenguaje.
    pub lsp_deshabilitado: Vec<String>,
    /// Comando personalizado por lenguaje — si un id no está acá, se
    /// usa el que trae `tcode_lsp::comando_para` (si alguno).
    pub lsp_comando: HashMap<String, ComandoLsp>,
}

impl ConfigLenguajes {
    pub fn lsp_habilitado(&self, id_lenguaje: &str) -> bool {
        !self.lsp_deshabilitado.iter().any(|l| l == id_lenguaje)
    }

    /// Habilita el LSP de `id_lenguaje` si estaba deshabilitado, o
    /// viceversa.
    pub fn alternar_lsp(&mut self, id_lenguaje: &str) {
        if let Some(pos) = self.lsp_deshabilitado.iter().position(|l| l == id_lenguaje) {
            self.lsp_deshabilitado.remove(pos);
        } else {
            self.lsp_deshabilitado.push(id_lenguaje.to_string());
        }
    }

    pub fn comando_configurado(&self, id_lenguaje: &str) -> Option<&ComandoLsp> {
        self.lsp_comando.get(id_lenguaje)
    }

    /// Guarda (o reemplaza) el comando personalizado de `id_lenguaje`.
    /// `linea` es la línea completa tal como se escribió ("comando arg1
    /// arg2 ..."), separada por espacios en blanco — el primer token es
    /// el comando, el resto son argumentos. `None` si `linea` está vacía
    /// (nada para guardar).
    pub fn fijar_comando_desde_linea(&mut self, id_lenguaje: &str, linea: &str) -> Option<()> {
        let mut tokens = linea.split_whitespace();
        let comando = tokens.next()?.to_string();
        let argumentos = tokens.map(str::to_string).collect();
        self.lsp_comando.insert(id_lenguaje.to_string(), ComandoLsp { comando, argumentos });
        Some(())
    }

    /// Quita el comando personalizado de `id_lenguaje` — vuelve a usar
    /// el que trae `tcode_lsp::comando_para`, si alguno.
    pub fn quitar_comando(&mut self, id_lenguaje: &str) {
        self.lsp_comando.remove(id_lenguaje);
    }
}

/// Directorio de configuración de tcode. En M1 solo se resuelve el modo
/// "usuario" (vía el directorio de config estándar del SO); el modo
/// portable de Windows (buscar `config.toml` junto al ejecutable) es
/// trabajo de M5 (PLAN.md §10).
pub fn directorio_config() -> PathBuf {
    dirs::config_dir()
        .map(|base| base.join("tcode"))
        .unwrap_or_else(|| PathBuf::from(".tcode"))
}

pub fn directorio_temas_usuario() -> PathBuf {
    directorio_config().join("themes")
}

pub fn ruta_config() -> PathBuf {
    directorio_config().join("config.toml")
}

/// Carga `config.toml`. Si todavía no existe, crea uno con los valores por
/// defecto (best-effort: si el sistema de archivos no lo permite, sigue
/// igual con los valores en memoria) y lo devuelve.
pub fn cargar() -> Result<Config> {
    let ruta = ruta_config();
    if !ruta.exists() {
        let config = Config::default();
        let _ = guardar(&config);
        return Ok(config);
    }
    let texto = std::fs::read_to_string(&ruta)
        .with_context(|| format!("no se pudo leer '{}'", ruta.display()))?;
    toml::from_str(&texto).with_context(|| format!("'{}' tiene TOML inválido", ruta.display()))
}

pub fn guardar(config: &Config) -> Result<()> {
    let dir = directorio_config();
    std::fs::create_dir_all(&dir).with_context(|| format!("no se pudo crear '{}'", dir.display()))?;
    let texto = toml::to_string_pretty(config).context("no se pudo serializar la configuración")?;
    let ruta = ruta_config();
    std::fs::write(&ruta, texto).with_context(|| format!("no se pudo escribir '{}'", ruta.display()))
}

/// Recarga la config desde disco reemplazando `actual` en el sitio — esto
/// es lo que permite aplicar cambios sin reiniciar el editor
/// (`config.recargar`, PLAN.md §4). No decide qué hacer si cambió el tema;
/// quien llama compara `actual.interfaz.tema` antes/después y actúa.
pub fn recargar(actual: &mut Config) -> Result<()> {
    *actual = cargar()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_por_defecto_tiene_valores_razonables() {
        let config = Config::default();
        assert_eq!(config.editor.tamano_tabulacion, 4);
        assert!(config.editor.usar_espacios);
        assert!(config.editor.numeros_de_linea);
        assert_eq!(config.interfaz.tema, TEMA_POR_DEFECTO);
    }

    #[test]
    fn round_trip_serializar_y_parsear() {
        let original = Config {
            editor: ConfigEditor {
                tamano_tabulacion: 2,
                usar_espacios: false,
                ajuste_linea: true,
                numeros_de_linea: false,
                modo_vim: true,
                columna_regla: Some(80),
            },
            interfaz: ConfigInterfaz {
                tema: "claro".into(),
                mostrar_statusbar: false,
                statusbar_posicion_cursor: false,
                statusbar_codificacion: true,
                statusbar_eol: false,
                statusbar_lenguaje: true,
                statusbar_diagnosticos: false,
                statusbar_modo: true,
            },
            lenguajes: ConfigLenguajes {
                lsp_deshabilitado: vec!["python".to_string()],
                lsp_comando: HashMap::from([(
                    "rust".to_string(),
                    ComandoLsp { comando: "rust-analyzer".to_string(), argumentos: vec![] },
                )]),
            },
        };
        let texto = toml::to_string_pretty(&original).unwrap();
        let recuperado: Config = toml::from_str(&texto).unwrap();
        assert_eq!(recuperado, original);
    }

    #[test]
    fn toml_parcial_completa_con_valores_por_defecto() {
        // Un config.toml que solo fija el tema debe heredar el resto de
        // los valores por defecto gracias a #[serde(default)].
        let config: Config = toml::from_str("[interfaz]\ntema = \"claro\"\n").unwrap();
        assert_eq!(config.interfaz.tema, "claro");
        assert_eq!(config.editor.tamano_tabulacion, 4);
    }

    #[test]
    fn todos_los_lenguajes_empiezan_habilitados() {
        let lenguajes = ConfigLenguajes::default();
        assert!(lenguajes.lsp_habilitado("python"));
    }

    #[test]
    fn alternar_lsp_deshabilita_y_vuelve_a_habilitar() {
        let mut lenguajes = ConfigLenguajes::default();
        lenguajes.alternar_lsp("python");
        assert!(!lenguajes.lsp_habilitado("python"));
        assert!(lenguajes.lsp_habilitado("rust")); // no afecta a otros lenguajes

        lenguajes.alternar_lsp("python");
        assert!(lenguajes.lsp_habilitado("python"));
    }

    #[test]
    fn fijar_comando_desde_linea_separa_comando_y_argumentos() {
        let mut lenguajes = ConfigLenguajes::default();
        lenguajes.fijar_comando_desde_linea("rust", "rust-analyzer --stdio --log-file /tmp/ra.log").unwrap();

        let comando = lenguajes.comando_configurado("rust").unwrap();
        assert_eq!(comando.comando, "rust-analyzer");
        assert_eq!(comando.argumentos, vec!["--stdio", "--log-file", "/tmp/ra.log"]);
    }

    #[test]
    fn fijar_comando_desde_linea_vacia_no_guarda_nada() {
        let mut lenguajes = ConfigLenguajes::default();
        assert!(lenguajes.fijar_comando_desde_linea("rust", "   ").is_none());
        assert!(lenguajes.comando_configurado("rust").is_none());
    }

    #[test]
    fn quitar_comando_borra_solo_ese_lenguaje() {
        let mut lenguajes = ConfigLenguajes::default();
        lenguajes.fijar_comando_desde_linea("rust", "rust-analyzer").unwrap();
        lenguajes.fijar_comando_desde_linea("go", "gopls").unwrap();

        lenguajes.quitar_comando("rust");
        assert!(lenguajes.comando_configurado("rust").is_none());
        assert!(lenguajes.comando_configurado("go").is_some());
    }
}
