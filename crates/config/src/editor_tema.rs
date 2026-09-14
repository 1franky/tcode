//! Editor visual de tema (`Ctrl+K Ctrl+P`, PLAN.md §7 "Personalización de
//! tema"): edita cada color de un tema por código hex, elegido de una
//! paleta predefinida, o ajustando HSL con flechas — los tres métodos de
//! entrada que menciona el plan. Preview en vivo (quien llama reconstruye
//! la `Paleta` desde `tema()` tras cada cambio — este módulo no sabe nada
//! de `ratatui`) y persistencia en la copia editable del tema
//! (`<id>-mio.toml`, la misma que crea `duplicar_tema_para_editar`,
//! sección "Temas" del panel de administración).
//!
//! Solo el ajuste HSL previsualiza en vivo tecla a tecla (cada flecha
//! recalcula el color y lo aplica de inmediato a `tema()`, aunque recién
//! se guarda a disco al confirmar) — hex y paleta predefinida son
//! "elegís y confirmás", sin preview mientras se decide, para no
//! necesitar revertir nada si se cancela a mitad de camino. Tampoco hay
//! "navegación en árbol" real para la lista de campos — es una lista
//! plana de los ~29 campos de color, agrupados visualmente por etiqueta
//! pero sin plegar/expandir.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::color::{analizar_color_hex, formatear_color_hex, hsl_a_rgb, rgb_a_hsl};
use crate::config::directorio_temas_usuario;
use crate::tema::{
    cargar_tema, duplicar_tema_para_editar, guardar_tema, tema_por_defecto, EstiloToken, ResultadoDuplicarTema, Tema,
};

type Obtener = fn(&Tema) -> Option<String>;
type Fijar = fn(&mut Tema, String);

/// Un campo de color editable: su etiqueta para mostrar, y cómo leer/
/// escribir su valor hex sobre un `Tema`. `fn` en vez de closures porque
/// ninguno captura nada — son proyecciones puras a un campo, así que la
/// lista entera puede construirse sin asignar en el heap más que el
/// `Vec` mismo.
pub struct CampoColor {
    pub etiqueta: &'static str,
    obtener: Obtener,
    fijar: Fijar,
}

impl CampoColor {
    /// Color actual en hex, o `None` si el tema no define ese campo
    /// todavía (los de `syntax`/`diagnostics`/`git`/`search` son
    /// opcionales — ver PLAN.md §7).
    pub fn valor_actual(&self, tema: &Tema) -> Option<String> {
        (self.obtener)(tema)
    }

    fn aplicar(&self, tema: &mut Tema, nuevo_hex: String) {
        (self.fijar)(tema, nuevo_hex)
    }
}

fn estilo_con_nuevo_color(actual: Option<&EstiloToken>, nuevo_hex: String) -> EstiloToken {
    match actual {
        // Preserva bold/italic si ya estaban — cambiar el color de un
        // token no debería perderle el estilo.
        Some(EstiloToken::ConEstilo { style, .. }) => EstiloToken::ConEstilo { fg: nuevo_hex, style: style.clone() },
        _ => EstiloToken::Color(nuevo_hex),
    }
}

fn hex_de_estilo(e: &EstiloToken) -> String {
    match e {
        EstiloToken::Color(hex) => hex.clone(),
        EstiloToken::ConEstilo { fg, .. } => fg.clone(),
    }
}

/// Los ~29 campos de color de PLAN.md §7, agrupados por sección igual
/// que el plan (UI del editor, statusbar, sintaxis, diagnósticos, git,
/// búsqueda/multi-cursor).
pub fn campos_color() -> Vec<CampoColor> {
    vec![
        // --- UI del editor ---
        CampoColor { etiqueta: "UI: Fondo", obtener: |t| Some(t.ui.background.clone()), fijar: |t, v| t.ui.background = v },
        CampoColor { etiqueta: "UI: Texto", obtener: |t| Some(t.ui.foreground.clone()), fijar: |t, v| t.ui.foreground = v },
        CampoColor { etiqueta: "UI: Cursor", obtener: |t| Some(t.ui.cursor.clone()), fijar: |t, v| t.ui.cursor = v },
        CampoColor { etiqueta: "UI: Selección", obtener: |t| Some(t.ui.selection.clone()), fijar: |t, v| t.ui.selection = v },
        CampoColor {
            etiqueta: "UI: Número de línea",
            obtener: |t| Some(t.ui.line_number.clone()),
            fijar: |t, v| t.ui.line_number = v,
        },
        CampoColor {
            etiqueta: "UI: Número de línea (activa)",
            obtener: |t| Some(t.ui.line_number_active.clone()),
            fijar: |t, v| t.ui.line_number_active = v,
        },
        CampoColor {
            etiqueta: "UI: Línea actual",
            obtener: |t| Some(t.ui.current_line.clone()),
            fijar: |t, v| t.ui.current_line = v,
        },
        // --- Statusbar ---
        CampoColor {
            etiqueta: "Statusbar: Fondo",
            obtener: |t| Some(t.statusbar.background.clone()),
            fijar: |t, v| t.statusbar.background = v,
        },
        CampoColor {
            etiqueta: "Statusbar: Texto",
            obtener: |t| Some(t.statusbar.foreground.clone()),
            fijar: |t, v| t.statusbar.foreground = v,
        },
        CampoColor {
            etiqueta: "Statusbar: Modo insertar",
            obtener: |t| t.statusbar.modo_insertar.clone(),
            fijar: |t, v| t.statusbar.modo_insertar = Some(v),
        },
        CampoColor {
            etiqueta: "Statusbar: Modo selección",
            obtener: |t| t.statusbar.modo_seleccion.clone(),
            fijar: |t, v| t.statusbar.modo_seleccion = Some(v),
        },
        // --- Sintaxis ---
        CampoColor {
            etiqueta: "Sintaxis: Palabra clave",
            obtener: |t| t.syntax.keyword.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.keyword = Some(estilo_con_nuevo_color(t.syntax.keyword.as_ref(), v)),
        },
        CampoColor {
            etiqueta: "Sintaxis: Cadena de texto",
            obtener: |t| t.syntax.string.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.string = Some(estilo_con_nuevo_color(t.syntax.string.as_ref(), v)),
        },
        CampoColor {
            etiqueta: "Sintaxis: Número",
            obtener: |t| t.syntax.number.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.number = Some(estilo_con_nuevo_color(t.syntax.number.as_ref(), v)),
        },
        CampoColor {
            etiqueta: "Sintaxis: Comentario",
            obtener: |t| t.syntax.comment.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.comment = Some(estilo_con_nuevo_color(t.syntax.comment.as_ref(), v)),
        },
        CampoColor {
            etiqueta: "Sintaxis: Función",
            obtener: |t| t.syntax.function.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.function = Some(estilo_con_nuevo_color(t.syntax.function.as_ref(), v)),
        },
        CampoColor {
            etiqueta: "Sintaxis: Tipo",
            obtener: |t| t.syntax.tipo.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.tipo = Some(estilo_con_nuevo_color(t.syntax.tipo.as_ref(), v)),
        },
        CampoColor {
            etiqueta: "Sintaxis: Variable",
            obtener: |t| t.syntax.variable.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.variable = Some(estilo_con_nuevo_color(t.syntax.variable.as_ref(), v)),
        },
        CampoColor {
            etiqueta: "Sintaxis: Constante",
            obtener: |t| t.syntax.constant.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.constant = Some(estilo_con_nuevo_color(t.syntax.constant.as_ref(), v)),
        },
        CampoColor {
            etiqueta: "Sintaxis: Operador",
            obtener: |t| t.syntax.operator.as_ref().map(hex_de_estilo),
            fijar: |t, v| t.syntax.operator = Some(estilo_con_nuevo_color(t.syntax.operator.as_ref(), v)),
        },
        // --- Diagnósticos ---
        CampoColor {
            etiqueta: "Diagnósticos: Error",
            obtener: |t| t.diagnostics.error.clone(),
            fijar: |t, v| t.diagnostics.error = Some(v),
        },
        CampoColor {
            etiqueta: "Diagnósticos: Advertencia",
            obtener: |t| t.diagnostics.warning.clone(),
            fijar: |t, v| t.diagnostics.warning = Some(v),
        },
        CampoColor {
            etiqueta: "Diagnósticos: Información",
            obtener: |t| t.diagnostics.info.clone(),
            fijar: |t, v| t.diagnostics.info = Some(v),
        },
        CampoColor {
            etiqueta: "Diagnósticos: Sugerencia",
            obtener: |t| t.diagnostics.hint.clone(),
            fijar: |t, v| t.diagnostics.hint = Some(v),
        },
        // --- Git ---
        CampoColor { etiqueta: "Git: Añadido", obtener: |t| t.git.added.clone(), fijar: |t, v| t.git.added = Some(v) },
        CampoColor {
            etiqueta: "Git: Modificado",
            obtener: |t| t.git.modified.clone(),
            fijar: |t, v| t.git.modified = Some(v),
        },
        CampoColor {
            etiqueta: "Git: Eliminado",
            obtener: |t| t.git.deleted.clone(),
            fijar: |t, v| t.git.deleted = Some(v),
        },
        // --- Búsqueda y multi-cursor ---
        CampoColor {
            etiqueta: "Búsqueda: Coincidencia actual",
            obtener: |t| t.search.coincidencia_actual.clone(),
            fijar: |t, v| t.search.coincidencia_actual = Some(v),
        },
        CampoColor {
            etiqueta: "Búsqueda: Otras coincidencias",
            obtener: |t| t.search.otras_coincidencias.clone(),
            fijar: |t, v| t.search.otras_coincidencias = Some(v),
        },
    ]
}

/// Colores conocidos para "elegir de una paleta predefinida" (PLAN.md
/// §7) — 20 colores planos, de los que se suelen usar en editores/UIs,
/// con nombre en español para poder buscarlos por lo que son en vez de
/// tener que reconocerlos por el hex.
pub const PALETA_PREDEFINIDA: &[(&str, &str)] = &[
    ("Rojo", "#e74c3c"),
    ("Rojo oscuro", "#c0392b"),
    ("Naranja", "#e67e22"),
    ("Amarillo", "#f1c40f"),
    ("Dorado", "#f39c12"),
    ("Verde", "#2ecc71"),
    ("Verde oscuro", "#27ae60"),
    ("Turquesa", "#1abc9c"),
    ("Cian", "#00bcd4"),
    ("Azul", "#3498db"),
    ("Azul oscuro", "#2980b9"),
    ("Violeta", "#8e44ad"),
    ("Morado", "#9b59b6"),
    ("Rosa", "#fd79a8"),
    ("Marrón", "#795548"),
    ("Beige", "#d2b48c"),
    ("Blanco", "#ffffff"),
    ("Gris claro", "#bdc3c7"),
    ("Gris oscuro", "#7f8c8d"),
    ("Negro", "#000000"),
];

/// Cuál de los tres componentes de HSL está enfocado — `←`/`→` cambia
/// cuál, `↑`/`↓` ajusta el valor del que esté enfocado (PLAN.md §7:
/// "ajustar HSL con flechas").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponenteHsl {
    Matiz,
    Saturacion,
    Luminosidad,
}

impl ComponenteHsl {
    fn siguiente(&self) -> Self {
        match self {
            ComponenteHsl::Matiz => ComponenteHsl::Saturacion,
            ComponenteHsl::Saturacion => ComponenteHsl::Luminosidad,
            ComponenteHsl::Luminosidad => ComponenteHsl::Matiz,
        }
    }

    fn anterior(&self) -> Self {
        match self {
            ComponenteHsl::Matiz => ComponenteHsl::Luminosidad,
            ComponenteHsl::Saturacion => ComponenteHsl::Matiz,
            ComponenteHsl::Luminosidad => ComponenteHsl::Saturacion,
        }
    }
}

/// Los tres métodos de entrada de color de PLAN.md §7, más "ninguno" (la
/// lista de campos sin ninguna edición en curso).
#[derive(Debug, Clone, PartialEq)]
pub enum ModoEdicion {
    Ninguno,
    /// Código hex escrito a mano, sin el `#` — se valida y aplica recién
    /// al confirmar, nunca mientras se escribe.
    Hex(String),
    /// Índice seleccionado en `PALETA_PREDEFINIDA` — igual que `Hex`, se
    /// aplica recién al confirmar.
    Paleta(usize),
    /// A diferencia de `Hex`/`Paleta`, cada ajuste de flecha se aplica
    /// EN VIVO a `tema()` (sin guardar a disco todavía) — por eso hace
    /// falta guardar el hex original, para poder revertir si se cancela.
    Hsl { h: u16, s: u8, l: u8, foco: ComponenteHsl, hex_original: String },
}

/// Estado del editor visual de tema. Sin dependencias de terminal/
/// `ratatui` — el crate `ui` lo dibuja, `app` decide qué tecla llega
/// acá y reconstruye la `Paleta` de preview tras cada cambio aplicado.
pub struct EstadoEditorTema {
    activo: bool,
    tema: Tema,
    ruta_archivo: PathBuf,
    id_tema: String,
    campo: usize,
    modo: ModoEdicion,
    mensaje: Option<String>,
}

impl EstadoEditorTema {
    /// Inactivo hasta el primer `abrir` — mismo patrón que
    /// `EstadoSelectorTema`/`EstadoPanelAdmin`: siempre existe (no es un
    /// `Option` en `EstadoApp`), con un `Tema` de relleno que nunca se ve
    /// porque `ui` solo lo dibuja si `activo()`.
    pub fn nueva() -> Self {
        Self {
            activo: false,
            tema: tema_por_defecto(),
            ruta_archivo: PathBuf::new(),
            id_tema: String::new(),
            campo: 0,
            modo: ModoEdicion::Ninguno,
            mensaje: None,
        }
    }

    /// Abre el editor sobre una copia editable de `id_tema_actual`: si
    /// ese id ya termina en `-mio` (ya es una copia editable, quizás de
    /// una sesión anterior) se usa tal cual; si no, se duplica primero
    /// (o se reusa la copia si ya existía de antes — nunca se pisa una
    /// personalización previa, ver `duplicar_tema_para_editar`). Si algo
    /// falla (el tema base no existe, no se puede escribir el archivo),
    /// el editor queda como estaba — nunca a mitad de abrir.
    pub fn abrir(&mut self, id_tema_actual: &str) -> Result<()> {
        let id_tema = if id_tema_actual.ends_with("-mio") {
            id_tema_actual.to_string()
        } else {
            let ruta = match duplicar_tema_para_editar(id_tema_actual)? {
                ResultadoDuplicarTema::Creado(ruta) | ResultadoDuplicarTema::YaExistia(ruta) => ruta,
            };
            ruta.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
                .with_context(|| format!("ruta de tema inválida: {}", ruta.display()))?
        };

        let tema = cargar_tema(&id_tema)?;
        let ruta_archivo = directorio_temas_usuario().join(format!("{id_tema}.toml"));

        self.activo = true;
        self.tema = tema;
        self.ruta_archivo = ruta_archivo;
        self.id_tema = id_tema;
        self.campo = 0;
        self.modo = ModoEdicion::Ninguno;
        self.mensaje = None;
        Ok(())
    }

    pub fn activo(&self) -> bool {
        self.activo
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
    }

    pub fn tema(&self) -> &Tema {
        &self.tema
    }

    /// El id de la copia editable (`<original>-mio`) — quien llama lo
    /// usa para dejarlo como tema activo en `config.toml`, así el
    /// preview en vivo sigue viéndose después de cerrar el editor.
    pub fn id_tema(&self) -> &str {
        &self.id_tema
    }

    pub fn campo(&self) -> usize {
        self.campo
    }

    pub fn modo(&self) -> &ModoEdicion {
        &self.modo
    }

    pub fn mensaje(&self) -> Option<&str> {
        self.mensaje.as_deref()
    }

    pub fn mover_arriba(&mut self) {
        self.campo = self.campo.saturating_sub(1);
    }

    pub fn mover_abajo(&mut self) {
        let total = campos_color().len();
        if total > 0 {
            self.campo = (self.campo + 1).min(total - 1);
        }
    }

    fn valor_actual_campo(&self) -> Option<String> {
        campos_color().get(self.campo).and_then(|c| c.valor_actual(&self.tema))
    }

    fn aplicar_sin_guardar(&mut self, hex: String) {
        if let Some(campo) = campos_color().get(self.campo) {
            campo.aplicar(&mut self.tema, hex);
        }
    }

    fn aplicar_y_guardar(&mut self, hex: String) {
        self.aplicar_sin_guardar(hex);
        match guardar_tema(&self.tema, &self.ruta_archivo) {
            Ok(()) => self.mensaje = Some("Guardado".to_string()),
            Err(e) => self.mensaje = Some(format!("No se pudo guardar: {e}")),
        }
    }

    /// `Enter` sobre la fila seleccionada, método hex: entra en modo
    /// edición con el valor actual precargado (sin el `#`, para escribir
    /// menos).
    pub fn iniciar_edicion_hex(&mut self) {
        let actual = self.valor_actual_campo().unwrap_or_default();
        self.modo = ModoEdicion::Hex(actual.trim_start_matches('#').to_string());
        self.mensaje = None;
    }

    /// Método paleta predefinida: arranca sobre la primera fila.
    pub fn iniciar_paleta(&mut self) {
        self.modo = ModoEdicion::Paleta(0);
        self.mensaje = None;
    }

    /// Método HSL: convierte el color actual del campo a HSL para
    /// arrancar desde ahí, no desde cero.
    pub fn iniciar_hsl(&mut self) {
        let actual = self.valor_actual_campo().unwrap_or_default();
        let (r, g, b) = analizar_color_hex(&actual).unwrap_or((0, 0, 0));
        let (h, s, l) = rgb_a_hsl(r, g, b);
        self.modo = ModoEdicion::Hsl { h, s, l, foco: ComponenteHsl::Matiz, hex_original: actual };
        self.mensaje = None;
    }

    /// `Esc`: cancela cualquiera de los tres modos sin guardar nada.
    /// Solo HSL necesita revertir algo de verdad — `Hex`/`Paleta` nunca
    /// tocan `tema()` hasta que se confirman.
    pub fn cancelar_edicion(&mut self) {
        if let ModoEdicion::Hsl { hex_original, .. } = &self.modo {
            let original = hex_original.clone();
            self.aplicar_sin_guardar(original);
        }
        self.modo = ModoEdicion::Ninguno;
    }

    pub fn escribir_hex(&mut self, c: char) {
        if let ModoEdicion::Hex(buffer) = &mut self.modo {
            // Los códigos hex son cortos (6 caracteres) — recortar evita
            // que se pueda escribir un valor imposible de validar.
            if buffer.len() < 6 {
                buffer.push(c);
            }
        }
    }

    pub fn borrar_hex(&mut self) {
        if let ModoEdicion::Hex(buffer) = &mut self.modo {
            buffer.pop();
        }
    }

    /// `Enter` con la edición hex en curso: valida el código escrito, lo
    /// aplica al campo seleccionado y persiste el tema completo a disco
    /// de inmediato. Si el hex no es válido, deja la edición abierta con
    /// un mensaje de error en vez de aplicar cualquier cosa.
    pub fn confirmar_hex(&mut self) {
        let ModoEdicion::Hex(buffer) = &self.modo else { return };
        let hex = format!("#{buffer}");
        if let Err(e) = analizar_color_hex(&hex) {
            self.mensaje = Some(format!("Color inválido: {e}"));
            return;
        }
        self.aplicar_y_guardar(hex);
        self.modo = ModoEdicion::Ninguno;
    }

    pub fn mover_paleta_arriba(&mut self) {
        if let ModoEdicion::Paleta(seleccion) = &mut self.modo {
            *seleccion = seleccion.saturating_sub(1);
        }
    }

    pub fn mover_paleta_abajo(&mut self) {
        if let ModoEdicion::Paleta(seleccion) = &mut self.modo {
            *seleccion = (*seleccion + 1).min(PALETA_PREDEFINIDA.len().saturating_sub(1));
        }
    }

    /// `Enter` con la paleta predefinida abierta: aplica el color
    /// seleccionado y persiste, igual que `confirmar_hex`.
    pub fn confirmar_paleta(&mut self) {
        let ModoEdicion::Paleta(seleccion) = &self.modo else { return };
        if let Some((_, hex)) = PALETA_PREDEFINIDA.get(*seleccion) {
            let hex = hex.to_string();
            self.aplicar_y_guardar(hex);
        }
        self.modo = ModoEdicion::Ninguno;
    }

    /// `←`/`→` con el ajuste HSL abierto: cambia cuál de los tres
    /// componentes va a modificar `ajustar_hsl`.
    pub fn mover_foco_hsl(&mut self, hacia_adelante: bool) {
        if let ModoEdicion::Hsl { foco, .. } = &mut self.modo {
            *foco = if hacia_adelante { foco.siguiente() } else { foco.anterior() };
        }
    }

    /// `↑`/`↓` con el ajuste HSL abierto: suma `delta` (negativo para
    /// bajar) al componente enfocado — matiz da la vuelta en 360,
    /// saturación/luminosidad se recortan a 0..100 — y aplica el color
    /// resultante EN VIVO a `tema()` (sin guardar a disco todavía; eso
    /// es `confirmar_hsl`), para que cada tecla se vea reflejada de
    /// inmediato.
    pub fn ajustar_hsl(&mut self, delta: i32) {
        let ModoEdicion::Hsl { h, s, l, foco, .. } = &mut self.modo else { return };
        match foco {
            ComponenteHsl::Matiz => *h = (*h as i32 + delta).rem_euclid(360) as u16,
            ComponenteHsl::Saturacion => *s = (*s as i32 + delta).clamp(0, 100) as u8,
            ComponenteHsl::Luminosidad => *l = (*l as i32 + delta).clamp(0, 100) as u8,
        }
        let (h, s, l) = (*h, *s, *l);
        let (r, g, b) = hsl_a_rgb(h, s, l);
        self.aplicar_sin_guardar(formatear_color_hex(r, g, b));
    }

    /// `Enter` con el ajuste HSL abierto: el color ya está aplicado en
    /// vivo (`ajustar_hsl` lo hizo en cada flecha) — solo falta
    /// persistirlo a disco.
    pub fn confirmar_hsl(&mut self) {
        match guardar_tema(&self.tema, &self.ruta_archivo) {
            Ok(()) => self.mensaje = Some("Guardado".to_string()),
            Err(e) => self.mensaje = Some(format!("No se pudo guardar: {e}")),
        }
        self.modo = ModoEdicion::Ninguno;
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn campos_color_tiene_una_etiqueta_unica_por_campo() {
        let campos = campos_color();
        assert!(!campos.is_empty());
        let mut etiquetas: Vec<&str> = campos.iter().map(|c| c.etiqueta).collect();
        etiquetas.sort_unstable();
        etiquetas.dedup();
        assert_eq!(etiquetas.len(), campos.len(), "hay etiquetas repetidas en campos_color");
    }

    #[test]
    fn todos_los_campos_de_un_tema_completo_devuelven_algun_valor() {
        // Los 12 temas embebidos definen los 4 grupos opcionales
        // (syntax/diagnostics/git/search) completos — así que sobre
        // cualquiera de ellos ningún campo debería devolver `None`.
        let tema = cargar_tema("dracula").unwrap();
        for campo in campos_color() {
            assert!(campo.valor_actual(&tema).is_some(), "'{}' no debería ser None en dracula", campo.etiqueta);
        }
    }

    #[test]
    fn aplicar_un_color_de_sintaxis_preserva_el_estilo_bold() {
        let mut tema = cargar_tema("dracula").unwrap();
        assert!(tema.syntax.keyword.as_ref().unwrap().negrita());

        let campos = campos_color();
        let campo_keyword = campos.iter().find(|c| c.etiqueta == "Sintaxis: Palabra clave").unwrap();
        campo_keyword.aplicar(&mut tema, "#123456".to_string());

        let actualizado = tema.syntax.keyword.as_ref().unwrap();
        assert!(actualizado.negrita(), "cambiar el color no debería perder el bold");
        assert_eq!(actualizado.color_rgb().unwrap(), (0x12, 0x34, 0x56));
    }

    // `EstadoEditorTema::abrir` (que duplica sobre el directorio de temas
    // REAL del usuario vía `directorio_config()`, sin forma de
    // inyectar una ruta distinta) no tiene test unitario a propósito —
    // mismo criterio que `duplicar_tema_para_editar` y `config::
    // cargar`/`guardar`, que tampoco lo tienen: ejercitarlo escribiría
    // de verdad en `~/.config/tcode/themes/` (o el equivalente del SO)
    // de quien corra `cargo test`. Se verifica a mano en tmux. El resto
    // de los métodos sí se pueden probar construyendo el struct a mano
    // (los tests están en el mismo módulo, así que los campos privados
    // son visibles) con una ruta de archivo en un directorio temporal —
    // `sufijo` distingue el directorio de cada test porque corren en
    // paralelo.
    fn estado_de_prueba(sufijo: &str) -> (EstadoEditorTema, PathBuf) {
        let ruta = std::env::temp_dir()
            .join(format!("tcode-test-editor-tema-{sufijo}-{}", std::process::id()))
            .join("prueba.toml");
        let estado = EstadoEditorTema {
            activo: true,
            tema: cargar_tema("dracula").unwrap(),
            ruta_archivo: ruta.clone(),
            id_tema: "dracula-mio".to_string(),
            campo: 0, // "UI: Fondo"
            modo: ModoEdicion::Ninguno,
            mensaje: None,
        };
        (estado, ruta)
    }

    fn limpiar(ruta: &Path) {
        if let Some(dir) = ruta.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// `iniciar_edicion_hex` precarga el valor actual (6 caracteres, ya
    /// al tope) — `escribir_hex` no reemplaza el buffer, así que hay que
    /// borrarlo antes de poder escribir algo nuevo. Mismo comportamiento
    /// ya verificado a mano en tmux en la pieza anterior.
    fn vaciar_buffer_hex(estado: &mut EstadoEditorTema) {
        for _ in 0..6 {
            estado.borrar_hex();
        }
    }

    #[test]
    fn editar_por_hex_aplica_y_guarda() {
        let (mut estado, ruta) = estado_de_prueba("hex-ok");
        estado.iniciar_edicion_hex();
        vaciar_buffer_hex(&mut estado);
        for c in "123456".chars() {
            estado.escribir_hex(c);
        }
        estado.confirmar_hex();

        assert_eq!(estado.tema().ui.background, "#123456");
        assert_eq!(estado.modo(), &ModoEdicion::Ninguno);
        assert_eq!(estado.mensaje(), Some("Guardado"));
        let guardado = cargar_tema_desde_ruta(&ruta);
        assert_eq!(guardado.ui.background, "#123456");

        limpiar(&ruta);
    }

    #[test]
    fn editar_por_hex_invalido_no_aplica_nada() {
        let (mut estado, ruta) = estado_de_prueba("hex-invalido");
        let original = estado.tema().ui.background.clone();
        estado.iniciar_edicion_hex();
        vaciar_buffer_hex(&mut estado);
        for c in "12".chars() {
            estado.escribir_hex(c); // muy corto: 2 en vez de 6
        }
        estado.confirmar_hex();

        assert_eq!(estado.tema().ui.background, original, "no debería haber cambiado nada");
        assert!(estado.mensaje().unwrap().starts_with("Color inválido"));
        assert!(!ruta.exists(), "no debería haberse guardado ningún archivo");

        limpiar(&ruta);
    }

    #[test]
    fn borrar_hex_funciona_sobre_el_buffer_en_edicion() {
        let (mut estado, ruta) = estado_de_prueba("hex-borrar");
        estado.iniciar_edicion_hex();
        vaciar_buffer_hex(&mut estado);
        for c in "abcdef".chars() {
            estado.escribir_hex(c);
        }
        estado.borrar_hex();
        estado.borrar_hex();
        let ModoEdicion::Hex(buffer) = estado.modo() else { panic!("debería seguir en modo Hex") };
        assert_eq!(buffer, "abcd");
        limpiar(&ruta);
    }

    #[test]
    fn elegir_de_la_paleta_predefinida_aplica_el_color_seleccionado() {
        let (mut estado, ruta) = estado_de_prueba("paleta");
        estado.iniciar_paleta();
        estado.mover_paleta_abajo();
        estado.mover_paleta_abajo();
        estado.confirmar_paleta();

        assert_eq!(estado.tema().ui.background, PALETA_PREDEFINIDA[2].1);
        assert_eq!(estado.modo(), &ModoEdicion::Ninguno);
        limpiar(&ruta);
    }

    #[test]
    fn paleta_no_se_va_antes_de_la_primera_ni_despues_de_la_ultima() {
        let (mut estado, ruta) = estado_de_prueba("paleta-limites");
        estado.iniciar_paleta();
        estado.mover_paleta_arriba();
        assert_eq!(estado.modo(), &ModoEdicion::Paleta(0));

        for _ in 0..(PALETA_PREDEFINIDA.len() + 5) {
            estado.mover_paleta_abajo();
        }
        assert_eq!(estado.modo(), &ModoEdicion::Paleta(PALETA_PREDEFINIDA.len() - 1));
        limpiar(&ruta);
    }

    #[test]
    fn ajustar_hsl_aplica_en_vivo_sin_guardar_todavia() {
        let (mut estado, ruta) = estado_de_prueba("hsl-vivo");
        estado.iniciar_hsl();
        estado.ajustar_hsl(10); // sube el matiz

        // Se aplicó en memoria...
        assert_ne!(estado.tema().ui.background, "#282a36"); // el fondo original de dracula
        // ...pero todavía no se guardó nada a disco.
        assert!(!ruta.exists());
        limpiar(&ruta);
    }

    #[test]
    fn cancelar_edicion_hsl_revierte_al_color_original() {
        let (mut estado, ruta) = estado_de_prueba("hsl-cancelar");
        let original = estado.tema().ui.background.clone();
        estado.iniciar_hsl();
        estado.ajustar_hsl(50);
        assert_ne!(estado.tema().ui.background, original);

        estado.cancelar_edicion();
        assert_eq!(estado.tema().ui.background, original);
        assert_eq!(estado.modo(), &ModoEdicion::Ninguno);
        limpiar(&ruta);
    }

    #[test]
    fn confirmar_hsl_guarda_el_color_ya_aplicado() {
        let (mut estado, ruta) = estado_de_prueba("hsl-confirmar");
        estado.iniciar_hsl();
        estado.ajustar_hsl(30);
        let esperado = estado.tema().ui.background.clone();
        estado.confirmar_hsl();

        assert_eq!(estado.modo(), &ModoEdicion::Ninguno);
        assert_eq!(estado.mensaje(), Some("Guardado"));
        let guardado = cargar_tema_desde_ruta(&ruta);
        assert_eq!(guardado.ui.background, esperado);
        limpiar(&ruta);
    }

    #[test]
    fn mover_foco_hsl_cicla_los_tres_componentes_en_ambas_direcciones() {
        let (mut estado, ruta) = estado_de_prueba("hsl-foco");
        estado.iniciar_hsl();
        let foco = |e: &EstadoEditorTema| match e.modo() {
            ModoEdicion::Hsl { foco, .. } => *foco,
            _ => panic!("debería seguir en modo Hsl"),
        };

        assert_eq!(foco(&estado), ComponenteHsl::Matiz);
        estado.mover_foco_hsl(true);
        assert_eq!(foco(&estado), ComponenteHsl::Saturacion);
        estado.mover_foco_hsl(true);
        assert_eq!(foco(&estado), ComponenteHsl::Luminosidad);
        estado.mover_foco_hsl(true);
        assert_eq!(foco(&estado), ComponenteHsl::Matiz);
        estado.mover_foco_hsl(false);
        assert_eq!(foco(&estado), ComponenteHsl::Luminosidad);
        limpiar(&ruta);
    }

    fn cargar_tema_desde_ruta(ruta: &Path) -> Tema {
        let texto = std::fs::read_to_string(ruta).unwrap();
        toml::from_str(&texto).unwrap()
    }
}
