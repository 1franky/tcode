use anyhow::{Context, Result};
use serde::Deserialize;

use crate::color::analizar_color_hex;
use crate::config::directorio_temas_usuario;

/// Un color de sintaxis puede definirse como un string simple
/// (`string = "#a6e3a1"`) o como una tabla con estilo adicional
/// (`keyword = { fg = "#cba6f7", style = "bold" }`), tal como describe
/// PLAN.md §7.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EstiloToken {
    Color(String),
    ConEstilo {
        fg: String,
        #[serde(default)]
        style: Option<String>,
    },
}

impl EstiloToken {
    pub fn color_rgb(&self) -> Result<(u8, u8, u8)> {
        let hex = match self {
            EstiloToken::Color(c) => c,
            EstiloToken::ConEstilo { fg, .. } => fg,
        };
        analizar_color_hex(hex)
    }

    pub fn negrita(&self) -> bool {
        matches!(self, EstiloToken::ConEstilo { style: Some(s), .. } if s.contains("bold"))
    }

    pub fn cursiva(&self) -> bool {
        matches!(self, EstiloToken::ConEstilo { style: Some(s), .. } if s.contains("italic"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemaUi {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
    pub line_number: String,
    pub line_number_active: String,
    pub current_line: String,
}

impl TemaUi {
    pub fn background_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.background)
    }
    pub fn foreground_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.foreground)
    }
    pub fn cursor_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.cursor)
    }
    pub fn seleccion_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.selection)
    }
    pub fn linea_actual_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.current_line)
    }
    pub fn numero_linea_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.line_number)
    }
    pub fn numero_linea_activa_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.line_number_active)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemaStatusbar {
    pub background: String,
    pub foreground: String,
    #[serde(default)]
    pub modo_insertar: Option<String>,
    #[serde(default)]
    pub modo_seleccion: Option<String>,
}

impl TemaStatusbar {
    pub fn background_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.background)
    }
    pub fn foreground_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.foreground)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TemaSintaxis {
    pub keyword: Option<EstiloToken>,
    pub string: Option<EstiloToken>,
    pub number: Option<EstiloToken>,
    pub comment: Option<EstiloToken>,
    pub function: Option<EstiloToken>,
    #[serde(rename = "type")]
    pub tipo: Option<EstiloToken>,
    pub variable: Option<EstiloToken>,
    pub constant: Option<EstiloToken>,
    pub operator: Option<EstiloToken>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TemaDiagnosticos {
    pub error: Option<String>,
    pub warning: Option<String>,
    pub info: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TemaGit {
    pub added: Option<String>,
    pub modified: Option<String>,
    pub deleted: Option<String>,
}

/// Un tema completo, tal como se define en `runtime/themes/*.toml`
/// (PLAN.md §7). `syntax` todavía no se usa (el resaltado con tree-sitter
/// llega en una pieza aparte de M1); se parsea desde ya para no tener que
/// cambiar el formato del archivo cuando se conecte.
#[derive(Debug, Clone, Deserialize)]
pub struct Tema {
    pub name: String,
    #[serde(rename = "type")]
    pub tipo: String,
    pub ui: TemaUi,
    pub statusbar: TemaStatusbar,
    #[serde(default)]
    pub syntax: TemaSintaxis,
    #[serde(default)]
    pub diagnostics: TemaDiagnosticos,
    #[serde(default)]
    pub git: TemaGit,
}

const TEMA_DRACULA: &str = include_str!("../../../runtime/themes/dracula.toml");
const TEMA_OSCURO: &str = include_str!("../../../runtime/themes/oscuro.toml");
const TEMA_CLARO: &str = include_str!("../../../runtime/themes/claro.toml");

/// Nombre del tema usado si la config no especifica uno, o si el
/// especificado no se encuentra.
pub const TEMA_POR_DEFECTO: &str = "dracula";

fn tema_embebido(nombre: &str) -> Option<&'static str> {
    match nombre {
        "dracula" => Some(TEMA_DRACULA),
        "oscuro" => Some(TEMA_OSCURO),
        "claro" => Some(TEMA_CLARO),
        _ => None,
    }
}

/// Carga un tema por nombre: primero busca un archivo de usuario en
/// `~/.config/tcode/themes/<nombre>.toml` (o el directorio portable en
/// Windows — ver M5), y si no existe recurre a los 3 temas básicos
/// embebidos en el binario. El resto de los 10+ temas de PLAN.md §7 llega
/// en M4.
pub fn cargar_tema(nombre: &str) -> Result<Tema> {
    let ruta_usuario = directorio_temas_usuario().join(format!("{nombre}.toml"));
    let texto = if ruta_usuario.exists() {
        std::fs::read_to_string(&ruta_usuario)
            .with_context(|| format!("no se pudo leer el tema '{}'", ruta_usuario.display()))?
    } else if let Some(embebido) = tema_embebido(nombre) {
        embebido.to_string()
    } else {
        anyhow::bail!("tema '{nombre}' no encontrado (ni de usuario ni embebido)");
    };

    toml::from_str(&texto).with_context(|| format!("el tema '{nombre}' tiene TOML inválido"))
}

/// Carga el tema por defecto. Si falla (no debería: es contenido embebido
/// validado en tests), entra en pánico — sin tema no hay UI que dibujar.
pub fn tema_por_defecto() -> Tema {
    cargar_tema(TEMA_POR_DEFECTO).expect("el tema embebido por defecto debe ser válido")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_tres_temas_embebidos_parsean_y_tienen_colores_validos() {
        for nombre in ["dracula", "oscuro", "claro"] {
            let tema = cargar_tema(nombre).unwrap_or_else(|e| panic!("tema '{nombre}': {e}"));
            assert!(!tema.name.is_empty());
            tema.ui.background_rgb().unwrap();
            tema.ui.foreground_rgb().unwrap();
            tema.statusbar.background_rgb().unwrap();
            tema.statusbar.foreground_rgb().unwrap();
        }
    }

    #[test]
    fn tema_desconocido_da_error() {
        assert!(cargar_tema("no-existe-este-tema").is_err());
    }

    #[test]
    fn estilo_token_distingue_color_simple_de_color_con_estilo() {
        #[derive(Deserialize)]
        struct Envoltorio {
            v: EstiloToken,
        }

        let simple: Envoltorio = toml::from_str("v = \"#ffffff\"").unwrap();
        assert!(!simple.v.negrita());
        assert_eq!(simple.v.color_rgb().unwrap(), (255, 255, 255));

        let con_estilo: Envoltorio =
            toml::from_str("v = { fg = \"#000000\", style = \"bold\" }").unwrap();
        assert!(con_estilo.v.negrita());
        assert_eq!(con_estilo.v.color_rgb().unwrap(), (0, 0, 0));
    }
}
