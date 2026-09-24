//! Compatibilidad con temas en formato Helix (PLAN.md §7 "Compartir
//! temas": "parser tolerante que acepta temas en formato Helix";
//! BACKLOG.md P3 #13). Un tema de Helix que el usuario deja en su carpeta
//! de temas se convierte al vuelo a un [`Tema`] de `tcode` al cargarlo —
//! el archivo original nunca se reescribe (duplicarlo o abrirlo en el
//! editor visual produce una copia `-mio` ya en formato `tcode`, ver
//! `duplicar_tema_para_editar`).
//!
//! ## Los dos esquemas, campo por campo
//!
//! Helix no tiene secciones fijas: cada clave de primer nivel es un
//! *scope* jerárquico (`"ui.cursor.primary"`, `"constant.numeric"`...)
//! cuyo valor es un color suelto (= `fg`) o una tabla `{ fg, bg,
//! modifiers, underline = { color, style } }`. Un color puede ser
//! `#rrggbb`, un nombre de la tabla `[palette]` o un nombre ANSI
//! (`red`, `light-blue`...). `inherits = "otro"` hereda todas las
//! claves del padre (el hijo pisa scopes enteros; las paletas se mezclan
//! entrada por entrada, y los estilos del padre ven la paleta del hijo).
//! Al buscar un scope que no existe, Helix sube por la jerarquía
//! (`constant.numeric` → `constant`) — acá se hace lo mismo.
//!
//! `tcode` tiene un conjunto fijo y chico de colores. El mapeo elegido
//! (primera fuente que exista; "respaldo" = el tema base de `tcode`, solo
//! cuando `inherits` apunta a un padre que no está en la carpeta):
//!
//! | `tcode`                     | Helix                                           | si falta                        |
//! |-----------------------------|-------------------------------------------------|---------------------------------|
//! | `ui.background`             | `ui.background`.bg                              | fondo del tema base             |
//! | `ui.foreground`             | `ui.text`.fg                                    | texto del tema base             |
//! | `ui.cursor`                 | `ui.cursor.primary` / `ui.cursor` (bg; fg si `reversed`) | respaldo → texto       |
//! | `ui.selection`              | `ui.selection.primary` / `ui.selection`.bg      | respaldo → 25% texto sobre fondo |
//! | `ui.line_number`            | `ui.linenr`.fg                                  | respaldo → 45% texto sobre fondo |
//! | `ui.line_number_active`     | `ui.linenr.selected`.fg (exacto)                | respaldo → texto                |
//! | `ui.current_line`           | `ui.cursorline.primary` / `ui.cursorline`.bg    | respaldo → 6% texto sobre fondo |
//! | `statusbar.background`      | `ui.statusline`.bg                              | respaldo → 8% texto sobre fondo |
//! | `statusbar.foreground`      | `ui.statusline`.fg                              | respaldo → texto                |
//! | `statusbar.modo_insertar`   | `ui.statusline.insert`.bg (exacto)              | respaldo → sin definir          |
//! | `statusbar.modo_seleccion`  | `ui.statusline.select`.bg (exacto)              | respaldo → sin definir          |
//! | `syntax.keyword`            | `keyword`, `keyword.control`                    | respaldo → sin definir (= texto) |
//! | `syntax.string`             | `string`                                        | idem                            |
//! | `syntax.number`             | `constant.numeric` (→ `constant`)               | idem                            |
//! | `syntax.comment`            | `comment`, `comment.line`                       | idem                            |
//! | `syntax.function`           | `function`, `function.method`                   | idem                            |
//! | `syntax.type`               | `type`, `type.builtin`                          | idem                            |
//! | `syntax.variable`           | `variable`                                      | idem                            |
//! | `syntax.constant`           | `constant`, `constant.builtin`                  | idem                            |
//! | `syntax.operator`           | `operator`, `keyword.operator`                  | idem                            |
//! | `diagnostics.*`             | `error`/`warning`/`info`/`hint`.fg, si no el color del `underline` de `diagnostic.<nivel>` | respaldo → colores ANSI de la `Paleta` |
//! | `git.added/modified/deleted`| `diff.plus`/`diff.delta`/`diff.minus` (`.gutter` si existe) | respaldo → ANSI     |
//! | `search.coincidencia_actual`| `ui.highlight`.bg, si no el color de `warning`  | respaldo → ANSI                 |
//! | `search.otras_coincidencias`| (Helix no tiene) = el color de selección        | —                               |
//! | `name`                      | (Helix no tiene) = `<archivo> (Helix)`          | —                               |
//! | `type`                      | (Helix no tiene) = luminancia del fondo         | —                               |
//!
//! En los tokens de sintaxis solo se conservan los modificadores `bold`
//! e `italic` (los únicos que `EstiloToken` sabe representar) y solo el
//! `fg` — un scope con solo `bg` se ignora. Las derivaciones "N% texto
//! sobre fondo" siguen el mismo criterio que la regla vertical de la
//! `Paleta` (`crates/ui`): mezclar los dos colores que cualquier tema
//! tiene en vez de inventar un color fijo que quede mal en la mitad de
//! los temas. "(exacto)" = sin subir por la jerarquía: `ui.linenr.
//! selected` caería a `ui.linenr` (el gris apagado de las demás líneas)
//! y `ui.statusline.insert` al fondo de la propia statusbar, que no
//! distinguirían nada.
//!
//! Todo lo demás (scopes de markup, `ui.menu`, `ui.popup`, `ui.virtual.*`,
//! `diff.*` fuera del gutter, modificadores como `underlined`/`dim`/
//! `crossed_out`, estilos de subrayado) se ignora sin fallar.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use toml::{Table, Value};

use crate::color::{analizar_color_hex, formatear_color_hex};
use crate::tema::{
    tema_embebido, EstiloToken, Tema, TemaBusqueda, TemaDiagnosticos, TemaGit, TemaSintaxis, TemaStatusbar, TemaUi,
};

type Rgb = (u8, u8, u8);

/// Límite de saltos de `inherits` — defensa contra una cadena rota o un
/// ciclo que se escape de la lista de visitados (no debería pasar).
const PROFUNDIDAD_MAXIMA_HERENCIA: usize = 8;

/// Claves de una tabla que la hacen *estilo* (y no un grupo de scopes
/// anidados escrito sin comillas, ver [`aplanar`]).
const CLAVES_DE_ESTILO: &[&str] = &["fg", "bg", "modifiers", "underline", "underline_color", "underline_style"];

/// Primeros segmentos de scope habituales de Helix: con que aparezca uno
/// solo como clave de primer nivel ya se reconoce el formato, aunque el
/// archivo no use `[palette]`, `inherits` ni claves con punto.
const SCOPES_RAIZ: &[&str] = &[
    "ui", "keyword", "string", "comment", "function", "type", "constant", "variable", "operator", "diff", "error",
    "warning", "info", "hint", "diagnostic", "markup", "attribute", "punctuation", "label", "namespace", "special",
    "tag", "constructor",
];

/// ¿Esta tabla TOML (un archivo de tema ya parseado) es un tema de
/// Helix? Un tema propio de `tcode` siempre declara `name`/`type` o una
/// sección `[ui]` con `background` como string; si no es eso, alcanza con
/// cualquier rasgo típico de Helix (`inherits`, `[palette]`, una clave
/// de primer nivel con punto, o un scope conocido).
pub(crate) fn es_formato_helix(tabla: &Table) -> bool {
    let tiene_str = |clave: &str| tabla.get(clave).is_some_and(Value::is_str);
    let ui_propio = matches!(tabla.get("ui"), Some(Value::Table(ui)) if ui.get("background").is_some_and(Value::is_str));
    if (tiene_str("name") && tiene_str("type")) || ui_propio {
        return false;
    }
    tabla.contains_key("inherits")
        || tabla.contains_key("palette")
        || tabla.keys().any(|k| k.contains('.') || SCOPES_RAIZ.contains(&k.as_str()))
}

/// Convierte el texto de un tema de Helix (`id` = nombre del archivo sin
/// `.toml`) a un [`Tema`]. `dir` es la carpeta donde buscar el padre de
/// `inherits` (`<dir>/<padre>.toml`, la misma carpeta de temas del
/// usuario) — si no está ahí (p. ej. un tema que hereda de uno que viene
/// dentro de Helix), se usa el tema base de `tcode` "oscuro" o "claro"
/// según la luminancia del fondo para todo lo que el hijo no defina.
pub(crate) fn convertir_tema_helix(texto: &str, id: &str, dir: &Path) -> Result<Tema> {
    let tabla: Table = toml::from_str(texto).with_context(|| format!("el tema de Helix '{id}' tiene TOML inválido"))?;
    let leer_padre = |nombre: &str| std::fs::read_to_string(dir.join(format!("{nombre}.toml"))).ok();
    Ok(convertir_tabla(tabla, id, &leer_padre))
}

/// Núcleo de [`convertir_tema_helix`], con la lectura del padre
/// inyectada para poder testear la herencia sin tocar disco.
fn convertir_tabla(tabla: Table, id: &str, leer_padre: &dyn Fn(&str) -> Option<String>) -> Tema {
    let mut visitados = vec![id.to_string()];
    let (fusionada, padre_faltante) = resolver_herencia(aplanar(tabla), leer_padre, &mut visitados);
    let resolutor = Resolutor::nuevo(fusionada);
    resolutor.a_tema(id, padre_faltante)
}

/// Resuelve `inherits` recursivamente (el padre a su vez puede heredar).
/// Devuelve la tabla ya mezclada y si en algún punto de la cadena faltó
/// un padre (no existe en la carpeta, no parsea, no es formato Helix, o
/// hay un ciclo).
fn resolver_herencia(mut tabla: Table, leer_padre: &dyn Fn(&str) -> Option<String>, visitados: &mut Vec<String>) -> (Table, bool) {
    let Some(padre) = tabla.remove("inherits") else { return (tabla, false) };
    let Some(padre) = padre.as_str().map(str::to_string) else { return (tabla, true) };
    if visitados.contains(&padre) || visitados.len() > PROFUNDIDAD_MAXIMA_HERENCIA {
        return (tabla, true);
    }
    visitados.push(padre.clone());
    let tabla_padre = leer_padre(&padre).and_then(|t| toml::from_str::<Table>(&t).ok()).filter(es_formato_helix);
    let Some(tabla_padre) = tabla_padre else { return (tabla, true) };
    let (base, faltante) = resolver_herencia(aplanar(tabla_padre), leer_padre, visitados);
    (fusionar(base, tabla), faltante)
}

/// Mezcla igual que Helix: cada scope del hijo reemplaza entero al del
/// padre; `[palette]` se mezcla entrada por entrada.
fn fusionar(mut base: Table, hijo: Table) -> Table {
    for (clave, valor) in hijo {
        match (clave.as_str(), valor, base.get_mut("palette")) {
            ("palette", Value::Table(paleta_hijo), Some(Value::Table(paleta_base))) => paleta_base.extend(paleta_hijo),
            (_, valor, _) => {
                base.insert(clave, valor);
            }
        }
    }
    base
}

/// Tolerancia a claves sin comillas: `ui.background = { bg = "..." }`
/// (sin comillas) en TOML es una tabla `ui` anidada, no el scope
/// `"ui.background"`. Cualquier tabla de primer nivel que no sea un
/// estilo (no tiene ninguna de [`CLAVES_DE_ESTILO`]) se aplana a scopes
/// con punto. `palette` e `inherits` quedan tal cual.
fn aplanar(tabla: Table) -> Table {
    fn aplanar_en(prefijo: &str, valor: Value, destino: &mut Table) {
        match valor {
            Value::Table(t) if !t.is_empty() && !t.keys().any(|k| CLAVES_DE_ESTILO.contains(&k.as_str())) => {
                for (clave, v) in t {
                    aplanar_en(&format!("{prefijo}.{clave}"), v, destino);
                }
            }
            otro => {
                destino.insert(prefijo.to_string(), otro);
            }
        }
    }

    let mut destino = Table::new();
    for (clave, valor) in tabla {
        if clave == "palette" || clave == "inherits" {
            destino.insert(clave, valor);
        } else {
            aplanar_en(&clave, valor, &mut destino);
        }
    }
    destino
}

/// Colores ANSI con nombre que Helix acepta sin declararlos en
/// `[palette]`, con los valores RGB por defecto de xterm (en Helix
/// dependen de la paleta de la terminal; acá hace falta un `#rrggbb`
/// fijo). `gray` es el "gris oscuro" (ANSI 8) y `light-gray` el gris
/// claro (ANSI 7), igual que en Helix.
fn color_ansi(nombre: &str) -> Option<Rgb> {
    Some(match nombre {
        "black" => (0x00, 0x00, 0x00),
        "red" => (0xcd, 0x00, 0x00),
        "green" => (0x00, 0xcd, 0x00),
        "yellow" => (0xcd, 0xcd, 0x00),
        "blue" => (0x00, 0x00, 0xee),
        "magenta" => (0xcd, 0x00, 0xcd),
        "cyan" => (0x00, 0xcd, 0xcd),
        "gray" => (0x7f, 0x7f, 0x7f),
        "light-red" => (0xff, 0x00, 0x00),
        "light-green" => (0x00, 0xff, 0x00),
        "light-yellow" => (0xff, 0xff, 0x00),
        "light-blue" => (0x5c, 0x5c, 0xff),
        "light-magenta" => (0xff, 0x00, 0xff),
        "light-cyan" => (0x00, 0xff, 0xff),
        "light-gray" => (0xe5, 0xe5, 0xe5),
        "white" => (0xff, 0xff, 0xff),
        _ => return None,
    })
}

/// Un scope de Helix ya resuelto a colores concretos.
#[derive(Debug, Default, Clone, Copy)]
struct Estilo {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    subrayado: Option<Rgb>,
    negrita: bool,
    cursiva: bool,
    invertido: bool,
}

impl Estilo {
    /// Color de texto efectivo (`reversed` intercambia fg y bg).
    fn fg_efectivo(&self) -> Option<Rgb> {
        if self.invertido { self.bg } else { self.fg }
    }

    fn bg_efectivo(&self) -> Option<Rgb> {
        if self.invertido { self.fg } else { self.bg }
    }
}

struct Resolutor {
    scopes: Table,
    paleta: HashMap<String, Rgb>,
}

impl Resolutor {
    fn nuevo(mut tabla: Table) -> Self {
        // Una entrada de paleta puede ser hex o, tolerando, un nombre ANSI.
        let paleta = match tabla.remove("palette") {
            Some(Value::Table(p)) => p
                .into_iter()
                .filter_map(|(nombre, v)| {
                    let texto = v.as_str()?;
                    let rgb = analizar_color_hex(texto).ok().or_else(|| color_ansi(texto))?;
                    Some((nombre, rgb))
                })
                .collect(),
            _ => HashMap::new(),
        };
        Self { scopes: tabla, paleta }
    }

    /// Un valor de color: primero la `[palette]` (que puede pisar incluso
    /// los nombres ANSI, como hace `onedark` con `red`), después hex, y
    /// por último los nombres ANSI. Cualquier otra cosa (`default`,
    /// `reset`, índices numéricos, basura) queda sin color.
    fn color(&self, valor: &Value) -> Option<Rgb> {
        let texto = valor.as_str()?.trim();
        self.paleta.get(texto).copied().or_else(|| analizar_color_hex(texto).ok()).or_else(|| color_ansi(texto))
    }

    fn estilo_exacto(&self, scope: &str) -> Option<Estilo> {
        match self.scopes.get(scope)? {
            valor @ Value::String(_) => Some(Estilo { fg: self.color(valor), ..Estilo::default() }),
            Value::Table(t) => {
                let modificadores: Vec<&str> =
                    t.get("modifiers").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                let subrayado = match t.get("underline") {
                    Some(Value::Table(u)) => u.get("color").and_then(|c| self.color(c)),
                    _ => t.get("underline_color").and_then(|c| self.color(c)),
                };
                Some(Estilo {
                    fg: t.get("fg").and_then(|c| self.color(c)),
                    bg: t.get("bg").and_then(|c| self.color(c)),
                    subrayado,
                    negrita: modificadores.contains(&"bold"),
                    cursiva: modificadores.contains(&"italic"),
                    invertido: modificadores.contains(&"reversed"),
                })
            }
            _ => None,
        }
    }

    /// Primer estilo, subiendo por la jerarquía de `scope` como Helix
    /// (`a.b.c` → `a.b` → `a`), del que `extraer` saque algo.
    fn buscar<T>(&self, scope: &str, extraer: impl Fn(&Estilo) -> Option<T>) -> Option<T> {
        std::iter::successors(Some(scope), |s| s.rsplit_once('.').map(|(padre, _)| padre))
            .find_map(|s| self.estilo_exacto(s).as_ref().and_then(&extraer))
    }

    fn fg(&self, scopes: &[&str]) -> Option<Rgb> {
        scopes.iter().find_map(|s| self.buscar(s, Estilo::fg_efectivo))
    }

    fn bg(&self, scopes: &[&str]) -> Option<Rgb> {
        scopes.iter().find_map(|s| self.buscar(s, Estilo::bg_efectivo))
    }

    fn bg_exacto(&self, scope: &str) -> Option<Rgb> {
        self.estilo_exacto(scope).and_then(|e| e.bg_efectivo())
    }

    /// Color de un diagnóstico: el `fg` del scope suelto (`error`) o, si
    /// no, el color del subrayado de `diagnostic.error`.
    fn diagnostico(&self, nivel: &str) -> Option<Rgb> {
        let scope_diagnostico = format!("diagnostic.{nivel}");
        self.fg(&[nivel]).or_else(|| self.buscar(&scope_diagnostico, |e| e.subrayado))
    }

    fn token(&self, scopes: &[&str]) -> Option<EstiloToken> {
        let estilo = scopes.iter().find_map(|s| self.buscar(s, |e| e.fg_efectivo().map(|fg| (fg, *e))))?;
        let (fg, estilo) = estilo;
        let fg = formatear_color_hex(fg.0, fg.1, fg.2);
        let modificadores: Vec<&str> =
            [(estilo.negrita, "bold"), (estilo.cursiva, "italic")].into_iter().filter(|(si, _)| *si).map(|(_, m)| m).collect();
        Some(if modificadores.is_empty() {
            EstiloToken::Color(fg)
        } else {
            EstiloToken::ConEstilo { fg, style: Some(modificadores.join(" ")) }
        })
    }

    fn a_tema(&self, id: &str, padre_faltante: bool) -> Tema {
        let fondo_helix = self.bg(&["ui.background"]);
        let texto_helix = self.fg(&["ui.text"]);

        // Tema base: por la luminancia del fondo si hay; si el tema deja
        // el fondo transparente (`"ui.background" = {}`, común en Helix
        // para usar el de la terminal), por la del texto (texto claro =
        // tema oscuro); sin ninguno de los dos, oscuro.
        let es_claro = match (fondo_helix, texto_helix) {
            (Some(f), _) => luminancia(f) > 0.5,
            (None, Some(t)) => luminancia(t) <= 0.5,
            (None, None) => false,
        };
        let base = tema_base(es_claro);
        let respaldo = padre_faltante.then_some(&base);

        let hex = |c: Rgb| formatear_color_hex(c.0, c.1, c.2);
        let rgb_de = |s: &str| analizar_color_hex(s).ok();
        let fondo = fondo_helix.or_else(|| rgb_de(&base.ui.background)).unwrap_or((0, 0, 0));
        let texto = texto_helix.or_else(|| rgb_de(&base.ui.foreground)).unwrap_or((255, 255, 255));
        let mezcla = |t: f32| hex(mezclar(fondo, texto, t));

        // Helix → si no, el tema base (solo con padre faltante) → si no,
        // el derivado.
        let elegir = |helix: Option<Rgb>, del_base: fn(&Tema) -> String, derivado: String| {
            helix.map(hex).or_else(|| respaldo.map(del_base)).unwrap_or(derivado)
        };
        let elegir_opcional = |helix: Option<Rgb>, del_base: fn(&Tema) -> Option<String>| {
            helix.map(hex).or_else(|| respaldo.and_then(del_base))
        };
        let token = |scopes: &[&str], del_base: fn(&Tema) -> Option<EstiloToken>| {
            self.token(scopes).or_else(|| respaldo.and_then(del_base))
        };

        let seleccion = elegir(self.bg(&["ui.selection.primary", "ui.selection"]), |t| t.ui.selection.clone(), mezcla(0.25));
        let advertencia = elegir_opcional(self.diagnostico("warning"), |t| t.diagnostics.warning.clone());

        Tema {
            name: format!("{id} (Helix)"),
            tipo: if es_claro { "light" } else { "dark" }.to_string(),
            alto_contraste: false,
            ui: TemaUi {
                background: hex(fondo),
                foreground: hex(texto),
                cursor: elegir(self.bg(&["ui.cursor.primary", "ui.cursor"]), |t| t.ui.cursor.clone(), hex(texto)),
                selection: seleccion.clone(),
                line_number: elegir(self.fg(&["ui.linenr"]), |t| t.ui.line_number.clone(), mezcla(0.45)),
                line_number_active: elegir(
                    self.estilo_exacto("ui.linenr.selected").and_then(|e| e.fg_efectivo()),
                    |t| t.ui.line_number_active.clone(),
                    hex(texto),
                ),
                current_line: elegir(
                    self.bg(&["ui.cursorline.primary", "ui.cursorline"]),
                    |t| t.ui.current_line.clone(),
                    mezcla(0.06),
                ),
            },
            statusbar: TemaStatusbar {
                background: elegir(self.bg(&["ui.statusline"]), |t| t.statusbar.background.clone(), mezcla(0.08)),
                foreground: elegir(self.fg(&["ui.statusline"]), |t| t.statusbar.foreground.clone(), hex(texto)),
                modo_insertar: elegir_opcional(self.bg_exacto("ui.statusline.insert"), |t| t.statusbar.modo_insertar.clone()),
                modo_seleccion: elegir_opcional(self.bg_exacto("ui.statusline.select"), |t| t.statusbar.modo_seleccion.clone()),
            },
            syntax: TemaSintaxis {
                keyword: token(&["keyword", "keyword.control"], |t| t.syntax.keyword.clone()),
                string: token(&["string"], |t| t.syntax.string.clone()),
                number: token(&["constant.numeric"], |t| t.syntax.number.clone()),
                comment: token(&["comment", "comment.line"], |t| t.syntax.comment.clone()),
                function: token(&["function", "function.method"], |t| t.syntax.function.clone()),
                tipo: token(&["type", "type.builtin"], |t| t.syntax.tipo.clone()),
                variable: token(&["variable"], |t| t.syntax.variable.clone()),
                constant: token(&["constant", "constant.builtin"], |t| t.syntax.constant.clone()),
                operator: token(&["operator", "keyword.operator"], |t| t.syntax.operator.clone()),
            },
            diagnostics: TemaDiagnosticos {
                error: elegir_opcional(self.diagnostico("error"), |t| t.diagnostics.error.clone()),
                warning: advertencia.clone(),
                info: elegir_opcional(self.diagnostico("info"), |t| t.diagnostics.info.clone()),
                hint: elegir_opcional(self.diagnostico("hint"), |t| t.diagnostics.hint.clone()),
            },
            git: TemaGit {
                added: elegir_opcional(self.fg(&["diff.plus.gutter"]), |t| t.git.added.clone()),
                modified: elegir_opcional(self.fg(&["diff.delta.gutter"]), |t| t.git.modified.clone()),
                deleted: elegir_opcional(self.fg(&["diff.minus.gutter"]), |t| t.git.deleted.clone()),
            },
            search: TemaBusqueda {
                coincidencia_actual: self.bg(&["ui.highlight"]).map(hex).or(advertencia),
                otras_coincidencias: Some(seleccion),
            },
        }
    }
}

/// Luminancia relativa aproximada (0 = negro, 1 = blanco), sin
/// linealizar gamma — alcanza para decidir claro/oscuro.
fn luminancia((r, g, b): Rgb) -> f32 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

/// `t` de `hacia` mezclado sobre `desde` (0 = `desde`, 1 = `hacia`).
fn mezclar(desde: Rgb, hacia: Rgb, t: f32) -> Rgb {
    let canal = |a: u8, b: u8| (a as f32 * (1.0 - t) + b as f32 * t).round() as u8;
    (canal(desde.0, hacia.0), canal(desde.1, hacia.1), canal(desde.2, hacia.2))
}

/// "oscuro" o "claro" embebidos — directo del binario, nunca de la
/// carpeta del usuario (un `oscuro.toml` de usuario en formato Helix
/// volvería a entrar acá).
fn tema_base(es_claro: bool) -> Tema {
    let id = if es_claro { "claro" } else { "oscuro" };
    let texto = tema_embebido(id).expect("los temas base oscuro/claro están embebidos");
    toml::from_str(texto).expect("los temas base embebidos son válidos (validado en tests)")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONEDARK: &str = include_str!("../tests/fixtures/helix/onedark.toml");
    const GRUVBOX: &str = include_str!("../tests/fixtures/helix/gruvbox.toml");
    const GRUVBOX_DARK_HARD: &str = include_str!("../tests/fixtures/helix/gruvbox_dark_hard.toml");
    const ONELIGHT: &str = include_str!("../tests/fixtures/helix/onelight.toml");

    fn convertir(texto: &str, id: &str) -> Tema {
        convertir_tabla(toml::from_str(texto).unwrap(), id, &|_| None)
    }

    /// Lector de padres sobre las fixtures, como si estuvieran todas en
    /// la misma carpeta de temas.
    fn leer_fixture(nombre: &str) -> Option<String> {
        match nombre {
            "onedark" => Some(ONEDARK.to_string()),
            "gruvbox" => Some(GRUVBOX.to_string()),
            _ => None,
        }
    }

    #[test]
    fn detecta_formato_helix_y_no_confunde_los_temas_propios() {
        for texto in [ONEDARK, GRUVBOX, GRUVBOX_DARK_HARD, ONELIGHT] {
            assert!(es_formato_helix(&toml::from_str(texto).unwrap()));
        }
        for info in crate::tema::TEMAS_EMBEBIDOS {
            let tabla: Table = toml::from_str(tema_embebido(info.id).unwrap()).unwrap();
            assert!(!es_formato_helix(&tabla), "'{}' no es Helix", info.id);
        }
        // Tema propio incompleto (sin name/type) tampoco se confunde.
        assert!(!es_formato_helix(&toml::from_str("[ui]\nbackground = \"#000000\"").unwrap()));
        assert!(!es_formato_helix(&toml::from_str("otra_cosa = 1").unwrap()));
    }

    #[test]
    fn onedark_resuelve_paleta_ui_y_sintaxis() {
        let tema = convertir(ONEDARK, "onedark");
        assert_eq!(tema.name, "onedark (Helix)");
        assert_eq!(tema.tipo, "dark");
        assert_eq!(tema.ui.background, "#282c34");
        assert_eq!(tema.ui.foreground, "#abb2bf");
        // `ui.cursor.primary` es `reversed` sin bg: el cursor se ve con
        // el color del fg.
        assert_eq!(tema.ui.cursor, "#abb2bf");
        assert_eq!(tema.ui.selection, "#3e4452");
        assert_eq!(tema.ui.current_line, "#2c323c");
        assert_eq!(tema.ui.line_number, "#4b5263");
        assert_eq!(tema.ui.line_number_active, "#abb2bf");
        assert_eq!(tema.statusbar.background, "#2c323c");
        assert_eq!(tema.statusbar.modo_insertar.as_deref(), Some("#98c379"));
        assert_eq!(tema.statusbar.modo_seleccion.as_deref(), Some("#c678dd"));
        let keyword = tema.syntax.keyword.as_ref().unwrap();
        assert_eq!(keyword.color_rgb().unwrap(), (0xc6, 0x78, 0xdd));
        let comment = tema.syntax.comment.as_ref().unwrap();
        assert!(comment.cursiva() && !comment.negrita());
        // `constant.numeric` definido aparte de `constant`.
        assert_eq!(tema.syntax.number.as_ref().unwrap().color_rgb().unwrap(), (0xd1, 0x9a, 0x66));
        assert_eq!(tema.syntax.constant.as_ref().unwrap().color_rgb().unwrap(), (0x56, 0xb6, 0xc2));
        assert_eq!(tema.diagnostics.error.as_deref(), Some("#e06c75"));
        assert_eq!(tema.git.added.as_deref(), Some("#98c379"));
        assert_eq!(tema.git.modified.as_deref(), Some("#d19a66"));
        assert_eq!(tema.search.otras_coincidencias.as_deref(), Some("#3e4452"));
    }

    #[test]
    fn un_tema_helix_claro_queda_como_light() {
        let tema = convertir(ONELIGHT, "onelight");
        assert_eq!(tema.tipo, "light");
        assert_eq!(tema.ui.background, "#fafafa");
    }

    #[test]
    fn inherits_en_la_misma_carpeta_mezcla_la_paleta_del_hijo_sobre_el_padre() {
        let tabla: Table = toml::from_str(GRUVBOX_DARK_HARD).unwrap();
        let tema = convertir_tabla(tabla, "gruvbox_dark_hard", &leer_fixture);
        // `bg0` pisado por el hijo: el `ui.background` del PADRE lo ve.
        assert_eq!(tema.ui.background, "#1d2021");
        // Lo demás viene del padre tal cual.
        let padre = convertir(GRUVBOX, "gruvbox");
        assert_eq!(tema.ui.foreground, padre.ui.foreground);
        assert_eq!(tema.syntax.keyword.unwrap().color_rgb().unwrap(), padre.syntax.keyword.unwrap().color_rgb().unwrap());
    }

    #[test]
    fn inherits_hacia_un_padre_ausente_usa_el_tema_base_de_tcode() {
        let texto = r##"
inherits = "catppuccin_mocha"
"ui.background" = { bg = "base" }
"keyword" = { fg = "mauve", modifiers = ["italic", "underlined"] }
[palette]
base = "#303446"
mauve = "#ca9ee6"
"##;
        let tema = convertir(texto, "catppuccin_frappe");
        assert_eq!(tema.tipo, "dark");
        assert_eq!(tema.ui.background, "#303446");
        let oscuro = tema_base(false);
        // Lo no definido sale del tema base "oscuro", no derivado.
        assert_eq!(tema.ui.foreground, oscuro.ui.foreground);
        assert_eq!(tema.ui.selection, oscuro.ui.selection);
        assert_eq!(tema.diagnostics.error, oscuro.diagnostics.error);
        assert_eq!(tema.syntax.string.unwrap().color_rgb().unwrap(), oscuro.syntax.string.unwrap().color_rgb().unwrap());
        // Lo definido por el hijo gana; `underlined` se ignora.
        let keyword = tema.syntax.keyword.unwrap();
        assert_eq!(keyword.color_rgb().unwrap(), (0xca, 0x9e, 0xe6));
        assert!(keyword.cursiva() && !keyword.negrita());
    }

    #[test]
    fn herencia_ciclica_no_se_cuelga() {
        let a = "inherits = \"b\"\n\"ui.text\" = \"#ffffff\"";
        let b = "inherits = \"a\"\n\"ui.background\" = { bg = \"#101010\" }";
        let leer = |n: &str| match n {
            "a" => Some(a.to_string()),
            "b" => Some(b.to_string()),
            _ => None,
        };
        let tema = convertir_tabla(toml::from_str(a).unwrap(), "a", &leer);
        assert_eq!(tema.ui.background, "#101010");
        assert_eq!(tema.ui.foreground, "#ffffff");
    }

    #[test]
    fn colores_ansi_y_valores_desconocidos_no_rompen_nada() {
        let texto = r##"
"ui.background" = { bg = "black" }
"ui.text" = "light-gray"
"string" = "light-green"
"comment" = { fg = "no-existe", modifiers = ["dim", "crossed_out"] }
"keyword" = { bg = "red" }
"ui.virtual.inlay-hint" = { fg = "reset" }
"markup.heading.1" = 42
"rainbow" = ["red", "yellow"]
"##;
        let tema = convertir(texto, "ansi");
        assert_eq!(tema.ui.background, "#000000");
        assert_eq!(tema.ui.foreground, "#e5e5e5");
        assert_eq!(tema.syntax.string.unwrap().color_rgb().unwrap(), (0x00, 0xff, 0x00));
        // Color que no resuelve y scope con solo bg: sin definir (= texto).
        assert!(tema.syntax.comment.is_none());
        assert!(tema.syntax.keyword.is_none());
        // Sin padre faltante: lo que no hay se deriva del fondo/texto.
        assert_eq!(tema.ui.current_line, formatear_color_hex(14, 14, 14));
        assert!(tema.diagnostics.error.is_none());
    }

    #[test]
    fn claves_sin_comillas_se_aplanan_a_scopes() {
        let texto = "[ui.background]\nbg = \"#123456\"\n[ui.linenr]\nfg = \"#654321\"\n";
        assert!(es_formato_helix(&toml::from_str(texto).unwrap()));
        let tema = convertir(texto, "sin-comillas");
        assert_eq!(tema.ui.background, "#123456");
        assert_eq!(tema.ui.line_number, "#654321");
    }

    #[test]
    fn fondo_transparente_elige_el_base_por_el_texto() {
        let tema = convertir("\"ui.background\" = {}\n\"ui.text\" = \"#202020\"", "transparente");
        assert_eq!(tema.tipo, "light");
        assert_eq!(tema.ui.background, tema_base(true).ui.background);
    }

    #[test]
    fn diagnosticos_caen_al_color_del_subrayado() {
        let texto = "\"ui.background\" = { bg = \"#000000\" }\n\"diagnostic.warning\" = { underline = { color = \"#ffaa00\", style = \"curl\" } }";
        let tema = convertir(texto, "subrayado");
        assert_eq!(tema.diagnostics.warning.as_deref(), Some("#ffaa00"));
        // Sin `ui.highlight`, la coincidencia actual usa el de warning.
        assert_eq!(tema.search.coincidencia_actual.as_deref(), Some("#ffaa00"));
    }

    #[test]
    fn el_tema_convertido_sobrevive_un_round_trip_en_formato_tcode() {
        let tema = convertir(ONEDARK, "onedark");
        let texto = toml::to_string_pretty(&tema).unwrap();
        let tabla: Table = toml::from_str(&texto).unwrap();
        assert!(!es_formato_helix(&tabla));
        let recuperado: Tema = toml::from_str(&texto).unwrap();
        assert_eq!(recuperado.ui.background, tema.ui.background);
    }
}
