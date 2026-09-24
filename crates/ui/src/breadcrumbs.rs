//! Breadcrumbs (BACKLOG.md P3 #10, PLAN.md §5 "Interfaz"): una fila
//! arriba del código de cada panel con dónde está el cursor — la ruta del
//! archivo relativa a la raíz del proyecto y la jerarquía de símbolos que
//! lo contienen: `crates > core > editor.rs > impl Editor > fn insertar`.
//!
//! Es un bloque independiente: [`dibujar`] pinta en el `Rect` de una fila
//! que recibe, y `paneles::dibujar_panel` es el único lugar que le
//! reserva esa fila recortando el área del contenido — así otra franja
//! arriba del código (p. ej. una barra de pestañas) se apila al lado sin
//! tocar nada de acá. No es interactivo.
//!
//! Costo por frame: nada O(archivo). La ruta se resuelve (tocando disco)
//! solo cuando cambia la del panel ([`CacheRuta`]); los símbolos salen
//! del árbol incremental que ya mantiene el `Resaltador`, que con la
//! revisión del buffer sin cambios no re-parsea ni pide el texto, y con
//! la misma posición devuelve la consulta anterior sin recorrer nada
//! (`Resaltador::simbolos_en`).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_core::Editor;
use tcode_syntax::{Lenguaje, Resaltador, Simbolo};

use crate::Paleta;

/// Separador entre segmentos. ASCII a propósito (no `›` ni `▸`): un
/// carácter de ancho "ambiguo" corre de lugar todo lo que sigue en
/// Windows Terminal (ver `BORDE_ASCII` en `lib.rs`).
const SEPARADOR: &str = " > ";

/// Marca de lo que se elidió para que la línea entre en el ancho.
const ELIPSIS: &str = "..";

/// Partes de la ruta ya resueltas para la `ruta_mostrada` de un panel
/// (vive en `EstadoUi`): resolverlas sube por el disco buscando la raíz
/// del repo, así que se hace una vez por ruta y no en cada frame. Queda
/// invalidada sola si la ruta cambia ("Guardar como", abrir otro
/// archivo en el panel).
#[derive(Default)]
pub(crate) struct CacheRuta {
    ruta: String,
    partes: Vec<String>,
}

impl CacheRuta {
    fn partes(&mut self, editor: &Editor, ruta_mostrada: &str) -> &[String] {
        if self.ruta != ruta_mostrada || self.partes.is_empty() {
            self.ruta = ruta_mostrada.to_string();
            self.partes = match editor.buffer().ruta() {
                Some(ruta) => tcode_fs::partes_ruta_en_proyecto(ruta),
                // Buffer sin archivo: lo que muestra la statusbar, tal
                // cual — salvo el panel nuevo de un split, que no trae
                // ninguna ruta y dejaría la fila en blanco.
                None if ruta_mostrada.is_empty() => vec!["[Sin nombre]".to_string()],
                None => vec![ruta_mostrada.to_string()],
            };
        }
        &self.partes
    }
}

/// Qué es cada segmento, para colorearlo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clase {
    Carpeta,
    Archivo,
    Simbolo,
    Elipsis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Segmento {
    texto: String,
    clase: Clase,
}

impl Segmento {
    fn nuevo(texto: impl Into<String>, clase: Clase) -> Self {
        Self { texto: texto.into(), clase }
    }
}

/// Dibuja el breadcrumb del panel en `area` (una fila). `con_simbolos` es
/// `false` para vistas donde la posición del cursor en el texto no tiene
/// sentido (la tabla CSV): solo la ruta.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dibujar(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    ruta_mostrada: &str,
    cache: &mut CacheRuta,
    resaltador: &mut Resaltador,
    paleta: &Paleta,
    con_simbolos: bool,
) {
    let partes = cache.partes(editor, ruta_mostrada).to_vec();
    let simbolos = if con_simbolos { simbolos_del_cursor(editor, ruta_mostrada, resaltador) } else { Vec::new() };
    let etiquetas: Vec<String> = simbolos.iter().map(Simbolo::etiqueta).collect();
    // Un espacio de margen a cada lado, igual que la statusbar.
    let ancho = (area.width as usize).saturating_sub(2);
    let segmentos = componer(&partes, &etiquetas, ancho);

    let base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let tenue = base.fg(paleta.numero_linea);
    let mut spans = vec![Span::styled(" ", base)];
    for (i, segmento) in segmentos.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(SEPARADOR, tenue));
        }
        let estilo = match segmento.clase {
            Clase::Carpeta | Clase::Elipsis => tenue,
            Clase::Archivo => base.add_modifier(Modifier::BOLD),
            Clase::Simbolo => base.patch(paleta.estilo_sintaxis("function")),
        };
        spans.push(Span::styled(segmento.texto.clone(), estilo));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(base), area);
}

/// Símbolos que contienen al cursor principal. La posición que se
/// consulta es la del cursor, pero nunca antes del primer carácter no
/// blanco de su línea ni después del último: con el cursor en la
/// indentación de
/// `    fn f() {` (o en una línea en blanco dentro de un cuerpo) el
/// breadcrumb ya muestra `fn f`, como se espera, en vez del contenedor
/// de afuera. Columna en bytes, como la cuenta tree-sitter.
fn simbolos_del_cursor(editor: &Editor, ruta: &str, resaltador: &mut Resaltador) -> Vec<Simbolo> {
    let Some(lenguaje) = Lenguaje::detectar_por_extension(ruta) else {
        return Vec::new();
    };
    let buffer = editor.buffer();
    let cursor = editor.cursor();
    let linea = buffer.linea_texto(cursor.linea);
    let primer_no_blanco = linea.len() - linea.trim_start().len();
    // Inicio del último carácter no blanco: con el cursor al final de
    // `}` (el cierre de un bloque) la posición cae justo DESPUÉS del
    // nodo, y tree-sitter devolvería el contenedor de afuera.
    let ultimo_no_blanco = linea.trim_end().char_indices().last().map_or(primer_no_blanco, |(i, _)| i);
    let columna_cursor = linea.char_indices().nth(cursor.columna).map_or(linea.len(), |(i, _)| i);
    let columna = columna_cursor.min(ultimo_no_blanco).max(primer_no_blanco);
    resaltador.simbolos_en(ruta, lenguaje, Some(buffer.revision()), || buffer.a_texto(), cursor.linea, columna)
}

fn ancho_de(segmentos: &[Segmento]) -> usize {
    let textos: usize = segmentos.iter().map(|s| Span::raw(s.texto.as_str()).width()).sum();
    textos + SEPARADOR.len() * segmentos.len().saturating_sub(1)
}

/// Arma los segmentos (carpetas, archivo, símbolos) recortando con
/// elegancia si no entran en `ancho`: lo que más importa es el símbolo
/// más interno y el nombre del archivo, así que se elide en este orden
/// hasta que entre —
///
/// 1. las carpetas del medio (queda la primera, `..` y las más cercanas
///    al archivo), después todas (`..`);
/// 2. los símbolos de afuera (`archivo > .. > fn interna`);
/// 3. la marca `..` de las carpetas;
/// 4. el final del símbolo más interno (`fn insertar_te..`).
///
/// Si ni así entra (el nombre del archivo solo ya no entra), el
/// `Paragraph` lo corta por la derecha.
fn componer(partes_ruta: &[String], simbolos: &[String], ancho: usize) -> Vec<Segmento> {
    let (archivo, carpetas) = match partes_ruta.split_last() {
        Some((archivo, carpetas)) => (archivo.as_str(), carpetas),
        None => ("", &[][..]),
    };
    let armar = |carpetas: Vec<Segmento>, simbolos: Vec<Segmento>| -> Vec<Segmento> {
        let mut todo = carpetas;
        todo.push(Segmento::nuevo(archivo, Clase::Archivo));
        todo.extend(simbolos);
        todo
    };
    let todos_los_simbolos: Vec<Segmento> = simbolos.iter().map(|s| Segmento::nuevo(s.as_str(), Clase::Simbolo)).collect();
    let n = carpetas.len();

    // 1. Carpetas: todas, después eliding de a una las del medio.
    for elididas in 0..=n {
        let carpetas_mostradas: Vec<Segmento> = if elididas == 0 {
            carpetas.iter().map(|c| Segmento::nuevo(c.as_str(), Clase::Carpeta)).collect()
        } else if elididas < n {
            let mut v = vec![Segmento::nuevo(carpetas[0].as_str(), Clase::Carpeta), Segmento::nuevo(ELIPSIS, Clase::Elipsis)];
            v.extend(carpetas[1 + elididas..].iter().map(|c| Segmento::nuevo(c.as_str(), Clase::Carpeta)));
            v
        } else {
            vec![Segmento::nuevo(ELIPSIS, Clase::Elipsis)]
        };
        let candidato = armar(carpetas_mostradas, todos_los_simbolos.clone());
        if ancho_de(&candidato) <= ancho {
            return candidato;
        }
    }

    // 2. Símbolos de afuera, de a uno, siempre dejando el más interno.
    let marca_carpetas = || if n > 0 { vec![Segmento::nuevo(ELIPSIS, Clase::Elipsis)] } else { Vec::new() };
    let m = todos_los_simbolos.len();
    for elididos in 1..m {
        let mut simbolos_mostrados = vec![Segmento::nuevo(ELIPSIS, Clase::Elipsis)];
        simbolos_mostrados.extend(todos_los_simbolos[elididos..].iter().cloned());
        let candidato = armar(marca_carpetas(), simbolos_mostrados);
        if ancho_de(&candidato) <= ancho {
            return candidato;
        }
    }

    // 3. Sin la marca de carpetas: `archivo > .. > interno`.
    let mut simbolos_mostrados = Vec::new();
    if m > 1 {
        simbolos_mostrados.push(Segmento::nuevo(ELIPSIS, Clase::Elipsis));
    }
    simbolos_mostrados.extend(todos_los_simbolos.last().cloned());
    let mut candidato = armar(Vec::new(), simbolos_mostrados);

    // 4. Se acorta el símbolo más interno terminándolo en `..`, en vez de
    //    que el corte seco del borde lo deje a mitad de palabra sin aviso
    //    (el nombre del archivo nunca: sin símbolos, lo corta el borde).
    let sobra = ancho_de(&candidato).saturating_sub(ancho);
    if sobra > 0 {
        if let Some(ultimo) = candidato.last_mut().filter(|s| s.clase == Clase::Simbolo) {
            let largo = ultimo.texto.chars().count();
            let queda = largo.saturating_sub(sobra + ELIPSIS.len());
            // Con menos de 3 caracteres no se reconoce nada: se deja
            // entero y lo corta el borde.
            if queda >= 3 {
                ultimo.texto = ultimo.texto.chars().take(queda).collect::<String>() + ELIPSIS;
            }
        }
    }
    candidato
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texto(segmentos: &[Segmento]) -> String {
        segmentos.iter().map(|s| s.texto.as_str()).collect::<Vec<_>>().join(SEPARADOR)
    }

    fn partes(ruta: &str) -> Vec<String> {
        ruta.split('/').map(String::from).collect()
    }

    fn simbolos(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn si_entra_se_muestra_todo() {
        let segmentos = componer(&partes("crates/core/editor.rs"), &simbolos(&["impl Editor", "fn insertar"]), 200);
        assert_eq!(texto(&segmentos), "crates > core > editor.rs > impl Editor > fn insertar");
        assert_eq!(segmentos[2].clase, Clase::Archivo);
        assert_eq!(segmentos[4].clase, Clase::Simbolo);
    }

    #[test]
    fn primero_se_eliden_las_carpetas_del_medio() {
        let ruta = partes("crates/core/src/modulo/editor.rs");
        let sim = simbolos(&["impl Editor", "fn insertar"]);
        let completo = texto(&componer(&ruta, &sim, 500));
        // Justo un carácter menos que el completo: sale la primera
        // carpeta del medio.
        let recortado = texto(&componer(&ruta, &sim, completo.len() - 1));
        assert_eq!(recortado, "crates > .. > src > modulo > editor.rs > impl Editor > fn insertar");
        let mas = texto(&componer(&ruta, &sim, "crates > .. > modulo > editor.rs > impl Editor > fn insertar".len()));
        assert_eq!(mas, "crates > .. > modulo > editor.rs > impl Editor > fn insertar");
        let sin_carpetas = texto(&componer(&ruta, &sim, ".. > editor.rs > impl Editor > fn insertar".len()));
        assert_eq!(sin_carpetas, ".. > editor.rs > impl Editor > fn insertar");
    }

    #[test]
    fn despues_los_simbolos_de_afuera_y_por_ultimo_la_marca_de_carpetas() {
        let ruta = partes("crates/core/editor.rs");
        let sim = simbolos(&["mod a", "impl Editor", "fn insertar"]);
        let objetivo = ".. > editor.rs > .. > impl Editor > fn insertar";
        assert_eq!(texto(&componer(&ruta, &sim, objetivo.len())), objetivo);
        let objetivo = ".. > editor.rs > .. > fn insertar";
        assert_eq!(texto(&componer(&ruta, &sim, objetivo.len())), objetivo);
        // Ni eso entra: archivo y símbolo más interno siempre quedan, el
        // símbolo acortado con `..`.
        let objetivo = "editor.rs > .. > fn inser..";
        assert_eq!(texto(&componer(&ruta, &sim, objetivo.len())), objetivo);
        // Si no entra ni el archivo, se deja tal cual (lo corta el borde).
        assert_eq!(texto(&componer(&ruta, &sim, 5)), "editor.rs > .. > fn insertar");
    }

    #[test]
    fn sin_simbolos_ni_carpetas() {
        assert_eq!(texto(&componer(&partes("notas.txt"), &[], 80)), "notas.txt");
        assert_eq!(texto(&componer(&partes("a/b/notas.txt"), &[], 3)), "notas.txt");
    }
}
