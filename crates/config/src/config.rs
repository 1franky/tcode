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
}

impl Default for ConfigEditor {
    fn default() -> Self {
        Self {
            tamano_tabulacion: 4,
            usar_espacios: true,
            ajuste_linea: false,
            numeros_de_linea: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigInterfaz {
    pub tema: String,
}

impl Default for ConfigInterfaz {
    fn default() -> Self {
        Self {
            tema: TEMA_POR_DEFECTO.to_string(),
        }
    }
}

/// Corresponde a la sección "Lenguajes / LSP" del panel de
/// administración (PLAN.md §5.3). Por ahora solo guarda qué lenguajes
/// tienen su LSP deshabilitado a propósito — configurar comando,
/// argumentos y variables de entorno por lenguaje queda para una pieza
/// aparte de M4.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ConfigLenguajes {
    /// Ids de lenguaje (`tcode_syntax::Lenguaje::id`, ej. `"python"`)
    /// cuyo LSP no se lanza aunque haya uno configurado, aun si el
    /// archivo activo es de ese lenguaje.
    pub lsp_deshabilitado: Vec<String>,
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
            },
            interfaz: ConfigInterfaz {
                tema: "claro".into(),
            },
            lenguajes: ConfigLenguajes {
                lsp_deshabilitado: vec!["python".to_string()],
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
}
