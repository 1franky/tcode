//! Editor visual de tema (`Ctrl+K Ctrl+P`, PLAN.md §7 "Personalización de
//! tema"): edita por código hex cada color de un tema, con preview en
//! vivo (quien llama reconstruye la `Paleta` desde `tema()` tras cada
//! cambio — este módulo no sabe nada de `ratatui`) y persistencia
//! inmediata en la copia editable del tema (`<id>-mio.toml`, la misma que
//! crea `duplicar_tema_para_editar`, sección "Temas" del panel de
//! administración).
//!
//! Alcance de esta pieza: solo edición por texto hex (`#rrggbb`). Elegir
//! de una paleta predefinida o ajustar HSL con flechas, que PLAN.md §7
//! también menciona, quedan para una pieza aparte. Tampoco hay
//! "navegación en árbol" real — es una lista plana de los ~29 campos de
//! color, agrupados visualmente por etiqueta pero sin plegar/expandir.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::color::analizar_color_hex;
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

/// Estado del editor visual de tema. Sin dependencias de terminal/
/// `ratatui` — el crate `ui` lo dibuja, `app` decide qué tecla llega
/// acá y reconstruye la `Paleta` de preview tras cada cambio aplicado.
pub struct EstadoEditorTema {
    activo: bool,
    tema: Tema,
    ruta_archivo: PathBuf,
    id_tema: String,
    campo: usize,
    editando: bool,
    buffer_hex: String,
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
            editando: false,
            buffer_hex: String::new(),
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
        self.editando = false;
        self.buffer_hex.clear();
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

    pub fn editando(&self) -> bool {
        self.editando
    }

    pub fn buffer_hex(&self) -> &str {
        &self.buffer_hex
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

    /// `Enter` sobre la fila seleccionada: entra en modo edición con el
    /// valor actual precargado (sin el `#`, para escribir menos).
    pub fn iniciar_edicion(&mut self) {
        let campos = campos_color();
        let actual = campos.get(self.campo).and_then(|c| c.valor_actual(&self.tema)).unwrap_or_default();
        self.buffer_hex = actual.trim_start_matches('#').to_string();
        self.editando = true;
        self.mensaje = None;
    }

    pub fn cancelar_edicion(&mut self) {
        self.editando = false;
        self.buffer_hex.clear();
    }

    pub fn escribir_hex(&mut self, c: char) {
        // Los códigos hex son cortos (6 caracteres) — recortar evita que
        // se pueda escribir un valor imposible de validar como color.
        if self.buffer_hex.len() < 6 {
            self.buffer_hex.push(c);
        }
    }

    pub fn borrar_hex(&mut self) {
        self.buffer_hex.pop();
    }

    /// `Enter` con la edición en curso: valida el hex escrito, lo aplica
    /// al campo seleccionado y persiste el tema completo a disco de
    /// inmediato. Si el hex no es válido, deja la edición abierta con un
    /// mensaje de error en vez de aplicar cualquier cosa.
    pub fn confirmar_edicion(&mut self) {
        let hex = format!("#{}", self.buffer_hex);
        if let Err(e) = analizar_color_hex(&hex) {
            self.mensaje = Some(format!("Color inválido: {e}"));
            return;
        }

        let campos = campos_color();
        if let Some(campo) = campos.get(self.campo) {
            campo.aplicar(&mut self.tema, hex);
        }
        self.editando = false;
        self.buffer_hex.clear();

        match guardar_tema(&self.tema, &self.ruta_archivo) {
            Ok(()) => self.mensaje = Some("Guardado".to_string()),
            Err(e) => self.mensaje = Some(format!("No se pudo guardar: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
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
    // de quien corra `cargo test`. Se verifica a mano en tmux.
}
