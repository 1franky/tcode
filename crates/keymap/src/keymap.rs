use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::combinacion::{parsear_atajo, Combinacion};

/// Directorio de configuración de tcode. Reutiliza la misma resolución que
/// `tcode-config` (mismo `config.toml`), en vez de duplicarla.
fn directorio_config() -> PathBuf {
    tcode_config::directorio_config()
}

/// Forma cruda del archivo `keymap.toml` (PLAN.md §4): una tabla por
/// ámbito, cada una mapeando el texto del atajo al nombre del comando en
/// español. En M1 solo existe la vista de código, así que únicamente
/// `[global]` y `[editor]` se resuelven a atajos activos. Un `keymap.toml`
/// con secciones `[markdown]`/`[csv]` (como el `default.toml` embebido, que
/// las deja vacías a modo de plantilla) sigue cargando sin error gracias al
/// comportamiento por defecto de `serde` de ignorar tablas no declaradas
/// aquí — no hace falta declararlas para eso.
#[derive(Debug, Clone, Deserialize, Default)]
struct KeymapCrudo {
    #[serde(default)]
    global: HashMap<String, String>,
    #[serde(default)]
    editor: HashMap<String, String>,
}

/// Keymap ya resuelto: secuencia de combinaciones -> nombre de comando.
/// Solo `[global]` y `[editor]` se resuelven a `Combinacion` en M1;
/// `[markdown]`/`[csv]` se validan como TOML (vía `KeymapCrudo`) pero se
/// descartan hasta que esas vistas existan (M3) — no hay nada que
/// conservar todavía, así que no se guardan en esta estructura.
pub struct Keymap {
    atajos: HashMap<Vec<Combinacion>, String>,
}

impl Keymap {
    fn desde_crudo(crudo: KeymapCrudo) -> Result<Self> {
        let mut atajos = HashMap::new();
        for (texto, comando) in crudo.global.iter().chain(crudo.editor.iter()) {
            let secuencia = parsear_atajo(texto)
                .with_context(|| format!("atajo inválido '{texto}' -> '{comando}'"))?;
            atajos.insert(secuencia, comando.clone());
        }
        Ok(Self { atajos })
    }

    /// Busca el comando exacto para una secuencia de combinaciones.
    pub fn buscar(&self, secuencia: &[Combinacion]) -> Option<&str> {
        self.atajos.get(secuencia).map(String::as_str)
    }

    /// `true` si `secuencia` es un prefijo estricto de algún atajo más
    /// largo (es decir, hace falta seguir esperando teclas de un chord).
    pub fn es_prefijo(&self, secuencia: &[Combinacion]) -> bool {
        self.atajos
            .keys()
            .any(|clave| clave.len() > secuencia.len() && clave.starts_with(secuencia))
    }

    pub fn num_atajos(&self) -> usize {
        self.atajos.len()
    }

    /// Todas las secuencias de teclas asignadas a `comando` (normalmente
    /// una sola, pero nada impide tener más de un atajo para el mismo
    /// comando). Se usa para mostrar el atajo junto al comando en la
    /// paleta de comandos (`Ctrl+Shift+P`, M2).
    pub fn atajos_para(&self, comando: &str) -> Vec<&[Combinacion]> {
        self.atajos
            .iter()
            .filter(|(_, c)| c.as_str() == comando)
            .map(|(secuencia, _)| secuencia.as_slice())
            .collect()
    }
}

const KEYMAP_POR_DEFECTO: &str = include_str!("../../../runtime/keymaps/default.toml");

/// Carga el keymap por defecto embebido en el binario.
pub fn keymap_por_defecto() -> Keymap {
    parsear_keymap(KEYMAP_POR_DEFECTO).expect("el keymap embebido por defecto debe ser válido")
}

fn parsear_keymap(texto: &str) -> Result<Keymap> {
    let crudo: KeymapCrudo = toml::from_str(texto)?;
    Keymap::desde_crudo(crudo)
}

pub fn ruta_keymap_usuario() -> PathBuf {
    directorio_config().join("keymap.toml")
}

/// Carga el keymap activo: el de usuario (`~/.config/tcode/keymap.toml` o
/// equivalente) si existe, si no el embebido por defecto. Es lo que hace
/// posible editar atajos sin recompilar — el editor visual del panel de
/// administración (PLAN.md §5) llega en M4.
pub fn cargar() -> Result<Keymap> {
    let ruta = ruta_keymap_usuario();
    if !ruta.exists() {
        return Ok(keymap_por_defecto());
    }
    let texto = std::fs::read_to_string(&ruta)
        .with_context(|| format!("no se pudo leer '{}'", ruta.display()))?;
    parsear_keymap(&texto).with_context(|| format!("'{}' tiene TOML inválido", ruta.display()))
}

/// Un atajo es conflictivo si su secuencia exacta es también el prefijo de
/// otro atajo más largo: el resolvedor siempre dispara el más corto de
/// inmediato, dejando el más largo inalcanzable. Base para el detector de
/// conflictos en tiempo real del editor de atajos (PLAN.md §5, UI en M4).
pub struct Conflicto {
    pub atajo_corto: Vec<Combinacion>,
    pub comando_corto: String,
    pub atajo_bloqueado: Vec<Combinacion>,
    pub comando_bloqueado: String,
}

pub fn detectar_conflictos(keymap: &Keymap) -> Vec<Conflicto> {
    let mut conflictos = Vec::new();
    for (secuencia, comando) in &keymap.atajos {
        for (otra, otro_comando) in &keymap.atajos {
            if otra.len() > secuencia.len() && otra.starts_with(secuencia.as_slice()) {
                conflictos.push(Conflicto {
                    atajo_corto: secuencia.clone(),
                    comando_corto: comando.clone(),
                    atajo_bloqueado: otra.clone(),
                    comando_bloqueado: otro_comando.clone(),
                });
            }
        }
    }
    conflictos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinacion::parsear_combinacion;

    #[test]
    fn el_keymap_por_defecto_carga_y_no_esta_vacio() {
        let keymap = keymap_por_defecto();
        assert!(keymap.num_atajos() > 0);
        let guardar = vec![parsear_combinacion("Ctrl+S").unwrap()];
        assert_eq!(keymap.buscar(&guardar), Some("archivo.guardar"));
    }

    #[test]
    fn atajos_para_encuentra_la_secuencia_de_un_comando() {
        let keymap = keymap_por_defecto();
        let atajos = keymap.atajos_para("archivo.guardar");
        assert_eq!(atajos.len(), 1);
        assert_eq!(atajos[0], [parsear_combinacion("Ctrl+S").unwrap()]);
        assert!(keymap.atajos_para("comando.inexistente").is_empty());
    }

    #[test]
    fn el_keymap_por_defecto_no_tiene_conflictos() {
        let conflictos = detectar_conflictos(&keymap_por_defecto());
        assert!(
            conflictos.is_empty(),
            "el keymap por defecto no debería tener atajos ambiguos"
        );
    }

    #[test]
    fn detecta_un_conflicto_de_prefijo() {
        let crudo: KeymapCrudo = toml::from_str(
            r#"
            [global]
            "Ctrl+K" = "comando.a"
            "Ctrl+K Ctrl+O" = "comando.b"
            "#,
        )
        .unwrap();
        let keymap = Keymap::desde_crudo(crudo).unwrap();
        let conflictos = detectar_conflictos(&keymap);
        assert_eq!(conflictos.len(), 1);
        assert_eq!(conflictos[0].comando_corto, "comando.a");
        assert_eq!(conflictos[0].comando_bloqueado, "comando.b");
    }

    #[test]
    fn seccion_markdown_no_rompe_la_carga_ni_genera_atajos_activos() {
        // Válido como TOML y no revienta la carga, pero tampoco genera
        // atajos activos todavía: esa vista no existe hasta M3.
        let crudo: KeymapCrudo = toml::from_str(
            r#"
            [markdown]
            "Ctrl+K V" = "markdown.alternar_preview"
            "#,
        )
        .unwrap();
        let keymap = Keymap::desde_crudo(crudo).unwrap();
        assert_eq!(keymap.num_atajos(), 0);
    }
}
