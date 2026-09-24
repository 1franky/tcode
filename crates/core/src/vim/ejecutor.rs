//! Ejecución de los comandos del modo Normal/Visual de VIM sobre un
//! `Editor`: recibe cada tecla, la acumula en `EstadoVim` hasta que
//! `gramatica::analizar` dice que forma un comando, y lo aplica. Todas las
//! modificaciones pasan por los métodos públicos del `Editor`
//! (`reemplazar_rango_bytes`, `aplicar_ediciones`, `insertar_texto`...),
//! así respetan plegado, revisión e historial; cada comando compuesto es
//! UNA edición (`3dd`, `J`, `>>`) o un grupo de deshacer (`cw` + lo
//! tipeado hasta `Esc`), así `u` lo deshace en un paso.

use crate::cursor::Cursor;
use crate::editor::{Editor, Modo};
use crate::estado_vim::{CambioRepetible, EstadoVim, InicioInsercion};

use super::gramatica::{analizar, Accion, Analisis, Comando, Movimiento, Objetivo, Operador, TipoComando};
use super::movimientos::{self, alcance, rango_de_movimiento, rango_de_objeto, rango_entre, Lector, Rango};

/// Lo que el ejecutor necesita saber de la config del editor.
#[derive(Debug, Clone)]
pub struct OpcionesVim {
    /// Lo que agrega un nivel de `>>` (`"    "` o `"\t"`).
    pub indentacion: String,
    /// Cuántos espacios saca como mucho un `<<`.
    pub ancho_tabulacion: usize,
}

impl Default for OpcionesVim {
    fn default() -> Self {
        Self { indentacion: "    ".to_string(), ancho_tabulacion: 4 }
    }
}

/// Procesa una tecla (carácter sin modificadores) en modo Normal o
/// Visual. Devuelve un aviso para la barra de estado, si hay alguno
/// ("Nada para repetir"...).
pub fn ejecutar_tecla(editor: &mut Editor, vim: &mut EstadoVim, c: char, opciones: &OpcionesVim) -> Option<String> {
    let visual = es_visual(editor.modo());
    vim.agregar_tecla(c);
    let comando = match analizar(vim.teclas_pendientes(), visual) {
        Analisis::Incompleto => return None,
        Analisis::Invalido => {
            vim.limpiar_pendiente();
            return None;
        }
        Analisis::Completo(comando) => comando,
    };
    vim.limpiar_pendiente();
    let aviso = if visual { ejecutar_visual(editor, vim, comando, opciones) } else { ejecutar_comando(editor, vim, comando, opciones) };
    terminar_tecla(editor, vim);
    aviso
}

fn es_visual(modo: Modo) -> bool {
    matches!(modo, Modo::Visual | Modo::VisualLinea)
}

/// Después de cada comando: en Normal, el cursor nunca queda después del
/// último carácter (`Editor::entrar_modo_normal` lo recorta); en Visual,
/// se vuelve a dibujar la selección desde el ancla.
fn terminar_tecla(editor: &mut Editor, vim: &mut EstadoVim) {
    match editor.modo() {
        Modo::Normal => editor.entrar_modo_normal(),
        Modo::Visual | Modo::VisualLinea => refrescar_visual(editor, vim),
        Modo::Insertar => {}
    }
}

/// `Esc` en Normal o Visual: descarta el comando a medio escribir y, en
/// Visual, vuelve a Normal sin selección.
pub fn cancelar(editor: &mut Editor, vim: &mut EstadoVim) {
    vim.limpiar_pendiente();
    if es_visual(editor.modo()) {
        vim.ancla_visual = None;
        editor.colapsar_cursores();
        editor.entrar_modo_normal();
    }
}

/// `Esc` en Insertar (con el modo VIM prendido): graba lo tipeado para
/// `.` si la inserción la empezó un comando VIM, cierra el grupo de
/// deshacer, mueve el cursor una posición a la izquierda (como VIM: queda
/// sobre el último carácter escrito) y vuelve a Normal.
pub fn salir_de_insertar(editor: &mut Editor, vim: &mut EstadoVim) {
    if let Some(inicio) = vim.insercion.take() {
        let texto = texto_insertado(editor, inicio);
        if let (Some(cambio), false) = (vim.ultimo_cambio.as_mut(), vim.repitiendo) {
            cambio.texto_insertado = texto;
        }
    }
    editor.colapsar_cursores();
    if editor.cursor().columna > 0 {
        editor.mover_izquierda();
    }
    editor.entrar_modo_normal();
}

fn texto_insertado(editor: &Editor, inicio: InicioInsercion) -> Option<String> {
    let buffer = editor.buffer();
    let largo = buffer.len_bytes();
    let crecimiento = largo.checked_sub(inicio.largo)?;
    let cursor = buffer.offset_byte(editor.cursor().linea, editor.cursor().columna);
    if cursor != inicio.offset + crecimiento {
        return None;
    }
    buffer.a_texto().get(inicio.offset..cursor).map(str::to_string)
}

fn offset(editor: &Editor, p: Cursor) -> usize {
    editor.buffer().offset_byte(p.linea, p.columna)
}

fn ir_a(editor: &mut Editor, p: Cursor) {
    let o = offset(editor, p);
    editor.mover_cursor_a_byte(o);
}

fn largo_linea(editor: &Editor, l: usize) -> usize {
    editor.buffer().longitud_visible_linea(l)
}

fn primer_no_blanco(editor: &Editor, l: usize) -> Cursor {
    let mut lector = Lector::nuevo(editor.buffer());
    Cursor { linea: l, columna: lector.primer_no_blanco(l) }
}

fn indentacion_de(editor: &Editor, l: usize) -> String {
    editor.buffer().linea_texto(l).chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// Rango de bytes `[inicio, fin)` de un `Rango` (las líneas enteras, con
/// su salto de línea, si es por líneas).
fn bytes(editor: &Editor, r: Rango) -> (usize, usize) {
    let buffer = editor.buffer();
    if r.lineal {
        let inicio = buffer.inicio_byte_linea(r.inicio.linea);
        let fin = if r.fin.linea + 1 < buffer.num_lineas() { buffer.inicio_byte_linea(r.fin.linea + 1) } else { buffer.len_bytes() };
        (inicio, fin)
    } else {
        (offset(editor, r.inicio), offset(editor, r.fin))
    }
}

fn texto_entre(editor: &Editor, inicio: usize, fin: usize) -> String {
    editor.buffer().a_texto().get(inicio..fin).unwrap_or_default().to_string()
}

/// Si el comando termina en Insertar (para abrir el grupo de deshacer
/// ANTES de su primera edición).
fn entra_a_insertar(comando: &Comando) -> bool {
    match comando.tipo {
        TipoComando::Operar(op, _) => op == Operador::Cambiar,
        TipoComando::Accion(a) => matches!(
            a,
            Accion::SustituirCaracter
                | Accion::SustituirLinea
                | Accion::CambiarHastaFin
                | Accion::Insertar
                | Accion::InsertarInicio
                | Accion::Agregar
                | Accion::AgregarFin
                | Accion::AbrirAbajo
                | Accion::AbrirArriba
        ),
        _ => false,
    }
}

/// Pasa a Insertar en la posición actual del cursor, anotando dónde
/// empezó la inserción (ver `InicioInsercion`).
fn empezar_insercion(editor: &mut Editor, vim: &mut EstadoVim) {
    editor.entrar_modo_insertar();
    let offset = offset(editor, editor.cursor());
    vim.insercion = Some(InicioInsercion { offset, largo: editor.buffer().len_bytes() });
}

fn ejecutar_comando(editor: &mut Editor, vim: &mut EstadoVim, comando: Comando, opciones: &OpcionesVim) -> Option<String> {
    if comando.es_repetible() && !vim.repitiendo {
        vim.ultimo_cambio = Some(CambioRepetible { comando, texto_insertado: None });
    }
    if entra_a_insertar(&comando) {
        editor.abrir_grupo_deshacer();
    }
    let veces = comando.veces();
    let cursor = editor.cursor();
    match comando.tipo {
        TipoComando::Mover(m) => mover(editor, vim, m, veces, comando.conteo.is_some()),
        TipoComando::Operar(op, objetivo) => {
            let rango = match objetivo {
                Objetivo::Lineas => {
                    let ultima = editor.buffer().num_lineas().saturating_sub(1);
                    Some(Rango { inicio: cursor, fin: Cursor { linea: (cursor.linea + veces - 1).min(ultima), columna: 0 }, lineal: true })
                }
                Objetivo::Objeto(o) => rango_de_objeto(&mut Lector::nuevo(editor.buffer()), cursor, o),
                Objetivo::Movimiento(m) => rango_de_movimiento_resuelto(editor, vim, m, veces, comando.conteo.is_some(), op),
            };
            if let Some(rango) = rango {
                aplicar_operador(editor, vim, op, rango, opciones);
            }
            None
        }
        TipoComando::Accion(a) => ejecutar_accion(editor, vim, a, comando, opciones),
        // Solo existen en modo Visual.
        _ => None,
    }
}

/// `;`/`,` resueltos contra la última búsqueda `f`/`t` (y `f`/`t`
/// normales guardados como la nueva última búsqueda).
fn resolver_busqueda(vim: &mut EstadoVim, m: Movimiento) -> Option<(Movimiento, bool)> {
    match m {
        Movimiento::RepetirBusqueda { inversa } => {
            let b = vim.ultima_busqueda?;
            Some((Movimiento::BuscarCaracter(if inversa { b.invertida() } else { b }), true))
        }
        Movimiento::BuscarCaracter(b) => {
            vim.ultima_busqueda = Some(b);
            Some((m, false))
        }
        _ => Some((m, false)),
    }
}

fn destino(editor: &Editor, vim: &mut EstadoVim, m: Movimiento, veces: usize, explicito: bool) -> Option<(Cursor, Movimiento)> {
    let (m, repitiendo) = resolver_busqueda(vim, m)?;
    let mut lector = Lector::nuevo(editor.buffer());
    let desde = editor.cursor();
    let p = match (m, repitiendo) {
        (Movimiento::BuscarCaracter(b), true) => movimientos::buscar_caracter(&mut lector, desde, b, veces, true)?,
        _ => movimientos::calcular(&mut lector, desde, m, veces, explicito)?,
    };
    Some((p, m))
}

fn rango_de_movimiento_resuelto(
    editor: &Editor,
    vim: &mut EstadoVim,
    m: Movimiento,
    veces: usize,
    explicito: bool,
    op: Operador,
) -> Option<Rango> {
    let desde = editor.cursor();
    if matches!(m, Movimiento::RepetirBusqueda { .. } | Movimiento::BuscarCaracter(_)) {
        let (p, resuelto) = destino(editor, vim, m, veces, explicito)?;
        return Some(rango_entre(&mut Lector::nuevo(editor.buffer()), desde, p, alcance(resuelto)));
    }
    rango_de_movimiento(&mut Lector::nuevo(editor.buffer()), desde, m, veces, explicito, op == Operador::Cambiar)
}

/// Mueve el cursor. `h`/`j`/`k`/`l`/`0`/`$` usan los movimientos del
/// `Editor` (los mismos de las flechas, que saben saltar bloques
/// plegados); el resto se calcula y se salta a la posición.
fn mover(editor: &mut Editor, vim: &mut EstadoVim, m: Movimiento, veces: usize, explicito: bool) -> Option<String> {
    let cursor = editor.cursor();
    match m {
        Movimiento::Abajo => (0..veces).for_each(|_| editor.mover_abajo()),
        Movimiento::Arriba => (0..veces).for_each(|_| editor.mover_arriba()),
        Movimiento::Izquierda => {
            for _ in 0..veces.min(cursor.columna) {
                editor.mover_izquierda();
            }
        }
        Movimiento::Derecha => {
            let largo = largo_linea(editor, cursor.linea);
            for _ in 0..veces.min(largo.saturating_sub(cursor.columna + 1)) {
                editor.mover_derecha();
            }
        }
        Movimiento::InicioLinea => editor.inicio_linea(),
        Movimiento::FinLinea => {
            for _ in 1..veces {
                editor.mover_abajo();
            }
            editor.fin_linea();
        }
        _ => {
            if let Some((p, _)) = destino(editor, vim, m, veces, explicito) {
                ir_a(editor, p);
            }
        }
    }
    None
}

fn aplicar_operador(editor: &mut Editor, vim: &mut EstadoVim, op: Operador, rango: Rango, opciones: &OpcionesVim) {
    let (inicio, fin) = bytes(editor, rango);
    match op {
        Operador::Copiar => {
            vim.fijar_registro(texto_entre(editor, inicio, fin), rango.lineal);
            if rango.lineal {
                let columna = editor.cursor().columna.min(largo_linea(editor, rango.inicio.linea));
                ir_a(editor, Cursor { linea: rango.inicio.linea, columna });
            } else {
                ir_a(editor, rango.inicio);
            }
        }
        Operador::Borrar => {
            vim.fijar_registro(texto_entre(editor, inicio, fin), rango.lineal);
            if rango.lineal {
                borrar_lineas(editor, rango.inicio.linea, inicio, fin);
            } else {
                editor.reemplazar_rango_bytes(inicio, fin, "");
            }
        }
        Operador::Cambiar => {
            vim.fijar_registro(texto_entre(editor, inicio, fin), rango.lineal);
            if rango.lineal {
                // Como `cc`: las líneas se reemplazan por una sola vacía
                // con la indentación de la primera.
                let indentacion = indentacion_de(editor, rango.inicio.linea);
                let fin_contenido = offset(editor, Cursor { linea: rango.fin.linea, columna: largo_linea(editor, rango.fin.linea) });
                editor.reemplazar_rango_bytes(inicio, fin_contenido, &indentacion);
            } else {
                editor.reemplazar_rango_bytes(inicio, fin, "");
            }
            empezar_insercion(editor, vim);
        }
        Operador::Indentar | Operador::Desindentar => {
            indentar(editor, rango.inicio.linea.min(rango.fin.linea), rango.inicio.linea.max(rango.fin.linea), op == Operador::Indentar, opciones);
        }
    }
}

/// Borra las líneas `[inicio, fin)` (bytes, ya con su salto). Si llegan
/// hasta el final del archivo y hay líneas antes, se come también el
/// salto de la anterior (como VIM: `dd` en la última línea no deja una
/// línea vacía colgando).
fn borrar_lineas(editor: &mut Editor, primera: usize, inicio: usize, fin: usize) {
    let buffer = editor.buffer();
    let hasta_el_final = fin == buffer.len_bytes() && !texto_entre(editor, inicio, fin).ends_with('\n');
    let (inicio, linea_destino) =
        if hasta_el_final && primera > 0 { (inicio - 1, primera - 1) } else { (inicio, primera) };
    editor.reemplazar_rango_bytes(inicio, fin, "");
    let linea = linea_destino.min(editor.buffer().num_lineas().saturating_sub(1));
    let destino = primer_no_blanco(editor, linea);
    ir_a(editor, destino);
}

fn indentar(editor: &mut Editor, primera: usize, ultima: usize, adentro: bool, opciones: &OpcionesVim) {
    let mut ediciones = Vec::new();
    for l in primera..=ultima {
        let inicio = editor.buffer().inicio_byte_linea(l);
        let texto = editor.buffer().linea_texto(l);
        if adentro {
            if !texto.is_empty() {
                ediciones.push((inicio..inicio, opciones.indentacion.clone()));
            }
        } else {
            let quitar = if texto.starts_with('\t') {
                1
            } else {
                texto.chars().take(opciones.ancho_tabulacion.max(1)).take_while(|c| *c == ' ').count()
            };
            if quitar > 0 {
                ediciones.push((inicio..inicio + quitar, String::new()));
            }
        }
    }
    editor.aplicar_ediciones(&ediciones);
    let destino = primer_no_blanco(editor, primera);
    ir_a(editor, destino);
}

fn ejecutar_accion(editor: &mut Editor, vim: &mut EstadoVim, accion: Accion, comando: Comando, opciones: &OpcionesVim) -> Option<String> {
    let veces = comando.veces();
    let cursor = editor.cursor();
    let largo = largo_linea(editor, cursor.linea);
    let en_linea = |desde: usize, hasta: usize| Rango {
        inicio: Cursor { linea: cursor.linea, columna: desde },
        fin: Cursor { linea: cursor.linea, columna: hasta },
        lineal: false,
    };
    let hasta_fin = |editor: &Editor| {
        let ultima = (cursor.linea + veces - 1).min(editor.buffer().num_lineas().saturating_sub(1));
        Rango { inicio: cursor, fin: Cursor { linea: ultima, columna: largo_linea(editor, ultima) }, lineal: false }
    };
    let lineas = |editor: &Editor| {
        let ultima = (cursor.linea + veces - 1).min(editor.buffer().num_lineas().saturating_sub(1));
        Rango { inicio: cursor, fin: Cursor { linea: ultima, columna: 0 }, lineal: true }
    };
    match accion {
        Accion::BorrarCaracter => {
            if largo > 0 {
                aplicar_operador(editor, vim, Operador::Borrar, en_linea(cursor.columna, (cursor.columna + veces).min(largo)), opciones);
            }
        }
        Accion::BorrarCaracterAtras => {
            if cursor.columna > 0 {
                aplicar_operador(editor, vim, Operador::Borrar, en_linea(cursor.columna.saturating_sub(veces), cursor.columna), opciones);
            }
        }
        Accion::SustituirCaracter => {
            let rango = en_linea(cursor.columna.min(largo), (cursor.columna + veces).min(largo));
            aplicar_operador(editor, vim, Operador::Cambiar, rango, opciones);
        }
        Accion::SustituirLinea => aplicar_operador(editor, vim, Operador::Cambiar, lineas(editor), opciones),
        Accion::BorrarHastaFin => {
            let r = hasta_fin(editor);
            aplicar_operador(editor, vim, Operador::Borrar, r, opciones);
        }
        Accion::CambiarHastaFin => {
            let r = hasta_fin(editor);
            aplicar_operador(editor, vim, Operador::Cambiar, r, opciones);
        }
        Accion::CopiarLinea => aplicar_operador(editor, vim, Operador::Copiar, lineas(editor), opciones),
        Accion::PegarDespues => pegar(editor, vim, veces, true),
        Accion::PegarAntes => pegar(editor, vim, veces, false),
        Accion::Deshacer => (0..veces).for_each(|_| editor.deshacer()),
        Accion::Reemplazar(c) => {
            if cursor.columna + veces <= largo {
                let inicio = offset(editor, cursor);
                let fin = offset(editor, Cursor { linea: cursor.linea, columna: cursor.columna + veces });
                editor.reemplazar_rango_bytes(inicio, fin, &c.to_string().repeat(veces));
                ir_a(editor, Cursor { linea: cursor.linea, columna: cursor.columna + veces - 1 });
            }
        }
        Accion::Unir => unir(editor, cursor.linea, veces.max(2)),
        Accion::AlternarMayuscula => {
            if largo > 0 {
                let fin = (cursor.columna + veces).min(largo);
                alternar_mayusculas(editor, en_linea(cursor.columna, fin));
                ir_a(editor, Cursor { linea: cursor.linea, columna: fin.min(largo - 1) });
            }
        }
        Accion::Insertar => empezar_insercion(editor, vim),
        Accion::InsertarInicio => {
            let destino = if largo == 0 { cursor } else { primer_no_blanco(editor, cursor.linea) };
            let destino = if editor.buffer().linea_texto(cursor.linea).trim().is_empty() { Cursor { linea: cursor.linea, columna: largo } } else { destino };
            ir_a(editor, destino);
            empezar_insercion(editor, vim);
        }
        Accion::Agregar => {
            ir_a(editor, Cursor { linea: cursor.linea, columna: (cursor.columna + 1).min(largo) });
            empezar_insercion(editor, vim);
        }
        Accion::AgregarFin => {
            ir_a(editor, Cursor { linea: cursor.linea, columna: largo });
            empezar_insercion(editor, vim);
        }
        Accion::AbrirAbajo => {
            let indentacion = indentacion_de(editor, cursor.linea);
            let fin = offset(editor, Cursor { linea: cursor.linea, columna: largo });
            editor.reemplazar_rango_bytes(fin, fin, &format!("\n{indentacion}"));
            empezar_insercion(editor, vim);
        }
        Accion::AbrirArriba => {
            let indentacion = indentacion_de(editor, cursor.linea);
            let inicio = editor.buffer().inicio_byte_linea(cursor.linea);
            editor.reemplazar_rango_bytes(inicio, inicio, &format!("{indentacion}\n"));
            ir_a(editor, Cursor { linea: cursor.linea, columna: indentacion.chars().count() });
            empezar_insercion(editor, vim);
        }
        Accion::Repetir => return repetir(editor, vim, comando.conteo, opciones),
        Accion::Visual | Accion::VisualLinea => {
            vim.ancla_visual = Some(cursor);
            editor.entrar_modo_visual(accion == Accion::VisualLinea);
        }
        Accion::LineaComando => vim.linea_comando.abrir(),
        Accion::IntercambiarExtremos => {}
    }
    None
}

/// `p`/`P` con conteo: el registro repetido `veces`, debajo/arriba de la
/// línea si es por líneas, después/antes del cursor si es por caracteres.
fn pegar(editor: &mut Editor, vim: &EstadoVim, veces: usize, despues: bool) {
    if vim.registro().is_empty() {
        return;
    }
    let contenido = vim.registro().repeat(veces);
    let cursor = editor.cursor();
    let buffer = editor.buffer();
    if vim.registro_lineal() {
        let (punto, texto, linea_destino) = if !despues {
            (buffer.inicio_byte_linea(cursor.linea), contenido, cursor.linea)
        } else if cursor.linea + 1 < buffer.num_lineas() {
            (buffer.inicio_byte_linea(cursor.linea + 1), contenido, cursor.linea + 1)
        } else {
            // Última línea (sin salto propio): el salto va antes.
            let sin_final = contenido.strip_suffix('\n').unwrap_or(&contenido);
            (buffer.len_bytes(), format!("\n{sin_final}"), cursor.linea + 1)
        };
        editor.reemplazar_rango_bytes(punto, punto, &texto);
        let destino = primer_no_blanco(editor, linea_destino);
        ir_a(editor, destino);
    } else {
        let largo = largo_linea(editor, cursor.linea);
        let columna = if despues && largo > 0 { cursor.columna + 1 } else { cursor.columna };
        let punto = offset(editor, Cursor { linea: cursor.linea, columna });
        editor.reemplazar_rango_bytes(punto, punto, &contenido);
        if contenido.contains('\n') {
            editor.mover_cursor_a_byte(punto);
        } else {
            // Sobre el último carácter pegado, como VIM.
            ir_a(editor, Cursor { linea: cursor.linea, columna: columna + contenido.chars().count() - 1 });
        }
    }
}

/// `J`: une `cantidad` líneas desde `primera` en una edición, poniendo
/// un espacio entre cada una (salvo si la siguiente está vacía o empieza
/// con `)`) y sacando la indentación de las que se suben.
fn unir(editor: &mut Editor, primera: usize, cantidad: usize) {
    let num = editor.buffer().num_lineas();
    if primera + 1 >= num {
        return;
    }
    let ultima = (primera + cantidad - 1).min(num - 1);
    let mut resultado = editor.buffer().linea_texto(primera);
    let mut columna_union = 0;
    for l in primera + 1..=ultima {
        let siguiente = editor.buffer().linea_texto(l);
        let recortada = siguiente.trim_start();
        let base = resultado.trim_end_matches([' ', '\t']).to_string();
        let espacio = !recortada.is_empty() && !recortada.starts_with(')') && !base.is_empty();
        resultado = base;
        columna_union = resultado.chars().count();
        if espacio {
            resultado.push(' ');
        }
        resultado.push_str(recortada);
    }
    let inicio = editor.buffer().inicio_byte_linea(primera);
    let fin = offset(editor, Cursor { linea: ultima, columna: largo_linea(editor, ultima) });
    editor.reemplazar_rango_bytes(inicio, fin, &resultado);
    ir_a(editor, Cursor { linea: primera, columna: columna_union });
}

fn alternar_mayusculas(editor: &mut Editor, rango: Rango) {
    let (inicio, fin) = bytes(editor, rango);
    let texto = texto_entre(editor, inicio, fin);
    let alternado: String = texto
        .chars()
        .flat_map(|c| if c.is_uppercase() { c.to_lowercase().collect::<Vec<_>>() } else { c.to_uppercase().collect() })
        .collect();
    if alternado != texto {
        editor.reemplazar_rango_bytes(inicio, fin, &alternado);
    }
}

/// `.`: vuelve a ejecutar el último cambio (con `conteo` en vez del
/// original, si se escribió uno) y, si entraba a Insertar, vuelve a
/// escribir lo que se tipeó y sale — todo como un solo paso de deshacer.
fn repetir(editor: &mut Editor, vim: &mut EstadoVim, conteo: Option<usize>, opciones: &OpcionesVim) -> Option<String> {
    let Some(mut cambio) = vim.ultimo_cambio.clone() else { return Some("Nada para repetir".to_string()) };
    if conteo.is_some() {
        cambio.comando.conteo = conteo;
        if let Some(ultimo) = vim.ultimo_cambio.as_mut() {
            ultimo.comando.conteo = conteo;
        }
    }
    vim.repitiendo = true;
    editor.abrir_grupo_deshacer();
    ejecutar_comando(editor, vim, cambio.comando, opciones);
    if editor.modo() == Modo::Insertar {
        if let Some(texto) = &cambio.texto_insertado {
            editor.insertar_texto(texto);
        }
        salir_de_insertar(editor, vim);
    }
    editor.cerrar_grupo_deshacer();
    vim.insercion = None;
    vim.repitiendo = false;
    None
}

/// Extremos ordenados de la selección Visual: (principio, fin), ambos
/// incluidos.
fn extremos_visual(editor: &Editor, vim: &EstadoVim) -> (Cursor, Cursor) {
    let cursor = editor.cursor();
    let ancla = vim.ancla_visual.unwrap_or(cursor);
    if (ancla.linea, ancla.columna) <= (cursor.linea, cursor.columna) {
        (ancla, cursor)
    } else {
        (cursor, ancla)
    }
}

fn ejecutar_visual(editor: &mut Editor, vim: &mut EstadoVim, comando: Comando, opciones: &OpcionesVim) -> Option<String> {
    let lineal = editor.modo() == Modo::VisualLinea;
    if vim.ancla_visual.is_none() {
        vim.ancla_visual = Some(editor.cursor());
    }
    match comando.tipo {
        TipoComando::Mover(m) => {
            mover(editor, vim, m, comando.veces(), comando.conteo.is_some());
        }
        TipoComando::SeleccionarObjeto(o) => {
            let cursor = editor.cursor();
            if let Some(r) = rango_de_objeto(&mut Lector::nuevo(editor.buffer()), cursor, o) {
                if r.lineal {
                    vim.ancla_visual = Some(Cursor { linea: r.inicio.linea, columna: 0 });
                    let l = r.fin.linea;
                    ir_a(editor, Cursor { linea: l, columna: largo_linea(editor, l).saturating_sub(1) });
                } else if r.fin != r.inicio {
                    vim.ancla_visual = Some(r.inicio);
                    let fin = offset(editor, r.fin);
                    let (linea, columna) = editor.buffer().linea_columna_desde_byte(fin.saturating_sub(1));
                    ir_a(editor, Cursor { linea, columna });
                }
            }
        }
        TipoComando::Accion(Accion::IntercambiarExtremos) => {
            let cursor = editor.cursor();
            if let Some(ancla) = vim.ancla_visual.replace(cursor) {
                ir_a(editor, ancla);
            }
        }
        TipoComando::Accion(a @ (Accion::Visual | Accion::VisualLinea)) => {
            let pedido_lineal = a == Accion::VisualLinea;
            if pedido_lineal == lineal {
                cancelar(editor, vim);
            } else {
                editor.entrar_modo_visual(pedido_lineal);
            }
        }
        TipoComando::OperarSeleccion(op) => {
            let (a, b) = extremos_visual(editor, vim);
            let rango = if lineal {
                Rango { inicio: a, fin: b, lineal: true }
            } else {
                let fin = Cursor { linea: b.linea, columna: (b.columna + 1).min(largo_linea(editor, b.linea)) };
                Rango { inicio: a, fin, lineal: false }
            };
            salir_de_visual(editor, vim);
            if op == Operador::Cambiar {
                editor.abrir_grupo_deshacer();
            }
            aplicar_operador(editor, vim, op, rango, opciones);
        }
        TipoComando::UnirSeleccion => {
            let (a, b) = extremos_visual(editor, vim);
            salir_de_visual(editor, vim);
            unir(editor, a.linea, (b.linea - a.linea + 1).max(2));
        }
        TipoComando::AlternarMayusculaSeleccion => {
            let (a, b) = extremos_visual(editor, vim);
            let rango = if lineal {
                Rango { inicio: a, fin: b, lineal: true }
            } else {
                Rango { inicio: a, fin: Cursor { linea: b.linea, columna: (b.columna + 1).min(largo_linea(editor, b.linea)) }, lineal: false }
            };
            salir_de_visual(editor, vim);
            alternar_mayusculas(editor, rango);
            ir_a(editor, a);
        }
        _ => {}
    }
    None
}

fn salir_de_visual(editor: &mut Editor, vim: &mut EstadoVim) {
    vim.ancla_visual = None;
    editor.colapsar_cursores();
    editor.entrar_modo_normal();
}

/// Vuelve a fijar la selección visible del modo Visual desde el ancla
/// hasta el cursor, los dos incluidos (la selección del `Editor` es
/// semiabierta: el carácter bajo el cursor lo marca el cursor mismo).
/// En Visual por líneas se extiende desde el borde de la línea del ancla.
pub fn refrescar_visual(editor: &mut Editor, vim: &mut EstadoVim) {
    let mut cursor = editor.cursor();
    let largo = largo_linea(editor, cursor.linea);
    cursor.columna = cursor.columna.min(largo.saturating_sub(1));
    let ancla = *vim.ancla_visual.get_or_insert(cursor);
    let adelante = (cursor.linea, cursor.columna) >= (ancla.linea, ancla.columna);
    let ancla_visible = if editor.modo() == Modo::VisualLinea {
        if adelante {
            Cursor { linea: ancla.linea, columna: 0 }
        } else {
            Cursor { linea: ancla.linea, columna: largo_linea(editor, ancla.linea) }
        }
    } else if adelante {
        ancla
    } else {
        Cursor { linea: ancla.linea, columna: (ancla.columna + 1).min(largo_linea(editor, ancla.linea)) }
    };
    editor.fijar_seleccion(ancla_visible, cursor);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arma un editor en modo Normal con `texto`, donde `|` marca el
    /// cursor (se saca del texto).
    fn editor_con(texto: &str) -> Editor {
        let posicion = texto.find('|').expect("falta el cursor `|`");
        let limpio = texto.replacen('|', "", 1);
        let mut editor = Editor::nuevo();
        editor.insertar_texto(&limpio);
        editor.mover_cursor_a_byte(posicion);
        editor.entrar_modo_normal();
        editor
    }

    /// Texto del editor con `|` en el cursor.
    fn con_cursor(editor: &Editor) -> String {
        let mut texto = editor.buffer().a_texto();
        let c = editor.cursor();
        texto.insert(editor.buffer().offset_byte(c.linea, c.columna), '|');
        texto
    }

    /// Tipea `teclas` como las recibiría el bucle principal: `\x1b` es
    /// `Esc`; en Insertar, cualquier otro carácter se inserta tal cual.
    fn tipear(editor: &mut Editor, vim: &mut EstadoVim, teclas: &str) {
        let opciones = OpcionesVim::default();
        for c in teclas.chars() {
            match (editor.modo(), c) {
                (Modo::Insertar, '\x1b') => salir_de_insertar(editor, vim),
                (Modo::Insertar, c) => editor.insertar_char(c),
                (_, '\x1b') => cancelar(editor, vim),
                (_, c) => {
                    ejecutar_tecla(editor, vim, c, &opciones);
                }
            }
        }
    }

    fn correr(inicial: &str, teclas: &str) -> (String, EstadoVim, Editor) {
        let mut editor = editor_con(inicial);
        let mut vim = EstadoVim::nuevo();
        tipear(&mut editor, &mut vim, teclas);
        (con_cursor(&editor), vim, editor)
    }

    /// La tabla principal: texto inicial (con `|` en el cursor) + teclas
    /// → texto final (con `|` en el cursor).
    #[test]
    fn tabla_de_comandos() {
        let casos: &[(&str, &str, &str)] = &[
            // Movimientos básicos y conteos.
            ("|ab\ncd", "l", "a|b\ncd"),
            ("|ab\ncd", "j", "ab\n|cd"),
            ("|a\nb\nc\nd", "3j", "a\nb\nc\n|d"),
            ("a\nb\nc\n|d", "2k", "a\n|b\nc\nd"),
            ("|abc", "5l", "ab|c"),
            ("a|bc", "h", "|abc"),
            ("|abc\nx", "h", "|abc\nx"),
            ("|hola", "$", "hol|a"),
            ("ho|la", "0", "|hola"),
            ("  h|ola", "^", "  |hola"),
            ("|a\nb\nc", "G", "a\nb\n|c"),
            ("a\nb\n|c", "gg", "|a\nb\nc"),
            ("|a\n  b\nc", "2G", "a\n  |b\nc"),
            ("|foo bar baz", "w", "foo |bar baz"),
            ("|foo bar baz", "2w", "foo bar |baz"),
            ("foo bar |baz", "b", "foo |bar baz"),
            ("|foo bar", "e", "fo|o bar"),
            ("|a.b c", "W", "a.b |c"),
            ("|a,b,c", "f,", "a|,b,c"),
            ("|a,b,c", "2f,", "a,b|,c"),
            ("|a,b,c", "t,", "|a,b,c"),
            ("|a,b,c", "f,;", "a,b|,c"),
            ("|a,b,c", "f,;,", "a|,b,c"),
            ("a,b,|c", "F,", "a,b|,c"),
            ("|f(a(b))", "%", "f(a(b)|)"),
            ("|a\nb\n\nc", "}", "a\nb\n|\nc"),
            // Borrados.
            ("|hola", "x", "|ola"),
            ("|hola", "3x", "|a"),
            ("ho|la", "X", "h|la"),
            ("ho|la", "10x", "h|o"),
            ("|", "x", "|"),
            ("uno\n|dos\ntres", "dd", "uno\n|tres"),
            ("|a\nb\nc\nd", "3dd", "|d"),
            ("|a\nb\nc\nd", "d2j", "|d"),
            ("uno\n|dos", "dd", "|uno"),
            ("|uno", "dd", "|"),
            ("|foo bar", "dw", "|bar"),
            ("|foo bar baz", "d2w", "|baz"),
            ("|foo bar baz", "2dw", "|baz"),
            ("foo |bar\nbaz", "dw", "foo| \nbaz"),
            ("fo|o bar", "d$", "f|o"),
            ("fo|o bar", "D", "f|o"),
            ("|foo bar", "de", "| bar"),
            ("foo |bar", "db", "|bar"),
            ("|a,b,c", "dt,", "|,b,c"),
            ("|a,b,c", "df,", "|b,c"),
            ("a\n|b\nc", "dG", "|a"),
            ("a\n|b\nc", "dgg", "|c"),
            ("|a\nb\n\nc", "d}", "|\nc"),
            ("|f(a) x", "d%", "| x"),
            // Objetos de texto.
            ("uno d|os tres", "diw", "uno | tres"),
            ("uno d|os tres", "daw", "uno |tres"),
            ("x = \"ho|la\";", "di\"", "x = \"|\";"),
            // Sin blancos después, `a"` se come los de antes (como VIM).
            ("x = \"ho|la\";", "da\"", "x =|;"),
            ("f(a, |b)", "di(", "f(|)"),
            ("f(a, |b)", "da(", "|f"),
            ("f(a, |b)", "dib", "f(|)"),
            ("fn x() {\n    |a;\n    b;\n}", "di{", "fn x() {\n|}"),
            ("{ a |b }", "da{", "|"),
            // Cambios (entran a Insertar y lo tipeado termina en Esc).
            ("|foo bar", "cwxy\x1b", "x|y bar"),
            ("fo|o bar", "cwX\x1b", "fo|X bar"),
            ("|foo bar baz", "c2wX\x1b", "|X baz"),
            ("uno d|os tres", "ciwX\x1b", "uno |X tres"),
            ("f(|a, b)", "ci(z\x1b", "f(|z)"),
            ("fo|o bar", "CX\x1b", "fo|X"),
            ("fo|o bar", "c$X\x1b", "fo|X"),
            ("  fo|o\nbar", "ccX\x1b", "  |X\nbar"),
            ("  fo|o\nbar", "SX\x1b", "  |X\nbar"),
            ("|hola", "sX\x1b", "|Xola"),
            ("|hola", "2sX\x1b", "|Xla"),
            ("|hola", "rx", "|xola"),
            ("|hola", "3rx", "xx|xa"),
            ("|hola", "5rx", "|hola"),
            ("|hOla", "~", "H|Ola"),
            ("|hOla", "3~", "HoL|a"),
            // Inserción.
            ("ho|la", "iX\x1b", "ho|Xla"),
            ("ho|la", "aX\x1b", "hol|Xa"),
            ("  ho|la", "IX\x1b", "  |Xhola"),
            ("ho|la", "AX\x1b", "hola|X"),
            ("  ho|la\nb", "oX\x1b", "  hola\n  |X\nb"),
            ("a\n  ho|la", "OX\x1b", "a\n  |X\n  hola"),
            // Unir e indentar.
            ("|uno\n  dos\ntres", "J", "uno| dos\ntres"),
            ("|uno\n  dos\ntres", "3J", "uno dos| tres"),
            ("|f(\n)", "J", "f(|)"),
            ("|a\nb", ">>", "    |a\nb"),
            ("|a\nb", "2>>", "    |a\n    b"),
            ("      |a", "<<", "  |a"),
            ("\t|a", "<<", "|a"),
            ("|a\nb\nc", ">j", "    |a\n    b\nc"),
            // Registro y pegado.
            ("|uno\ndos\ntres", "yyjp", "uno\ndos\n|uno\ntres"),
            ("|uno\ndos\ntres", "yyjP", "uno\n|uno\ndos\ntres"),
            ("|uno\ndos", "yy3p", "uno\n|uno\nuno\nuno\ndos"),
            ("uno\n|dos", "yyp", "uno\ndos\n|dos"),
            ("|ab cd", "dwp", "cab| d"),
            ("|ab cd", "yeP", "a|bab cd"),
            ("|ab cd", "yw$p", "ab cdab| "),
            ("|ab", "x2p", "ba|a"),
            ("|a\nb", "ddp", "b\n|a"),
            ("foo |bar", "y0P", "foo| foo bar"),
            // Deshacer: una operación compuesta es un solo paso.
            ("|a\nb\nc\nd", "3ddu", "|a\nb\nc\nd"),
            ("|foo bar", "cwxyz\x1bu", "|foo bar"),
            ("|a", "oX\x1bu", "|a"),
            ("|hola", "xxuu", "|hola"),
            // Repetir con `.`.
            ("|a b c d", "dw.", "|c d"),
            ("|a b c d", "dw2.", "|d"),
            ("|a\nb\nc", "dd.", "|c"),
            ("|foo foo", "cwbar\x1bw.", "bar ba|r"),
            ("|x", "ahi\x1b.", "xhih|i"),
            ("|a\nb", "A;\x1bj.", "a;\nb|;"),
            ("|ab", "x.", "|"),
            ("|a\nb\nc", "dd.u", "|b\nc"),
        ];
        let mut fallas = Vec::new();
        for (inicial, teclas, esperado) in casos {
            let (obtenido, _, editor) = correr(inicial, teclas);
            if obtenido != *esperado || editor.modo() != Modo::Normal {
                fallas.push(format!("{inicial:?} + {teclas:?}: esperado {esperado:?}, obtenido {obtenido:?} ({:?})", editor.modo()));
            }
        }
        assert!(fallas.is_empty(), "\n{}", fallas.join("\n"));
    }

    #[test]
    fn el_registro_distingue_lineas_de_caracteres() {
        let (_, vim, _) = correr("|uno\ndos", "yy");
        assert_eq!((vim.registro(), vim.registro_lineal()), ("uno\n", true));
        let (_, vim, _) = correr("uno\n|dos", "dd");
        assert_eq!((vim.registro(), vim.registro_lineal()), ("dos\n", true));
        let (_, vim, _) = correr("|foo bar", "dw");
        assert_eq!((vim.registro(), vim.registro_lineal()), ("foo ", false));
        let (_, vim, _) = correr("|hola", "x");
        assert_eq!((vim.registro(), vim.registro_lineal()), ("h", false));
        let (_, vim, _) = correr("|a\nb\nc", "yj");
        assert_eq!((vim.registro(), vim.registro_lineal()), ("a\nb\n", true));
    }

    #[test]
    fn comandos_a_medias_quedan_pendientes_y_los_invalidos_se_descartan() {
        let mut editor = editor_con("|uno\ndos");
        let mut vim = EstadoVim::nuevo();
        tipear(&mut editor, &mut vim, "d2");
        assert_eq!(vim.teclas_pendientes(), &['d', '2']);
        tipear(&mut editor, &mut vim, "z");
        assert!(vim.teclas_pendientes().is_empty());
        assert_eq!(editor.buffer().a_texto(), "uno\ndos");
        // `Esc` también cancela.
        tipear(&mut editor, &mut vim, "d\x1bj");
        assert_eq!(editor.buffer().a_texto(), "uno\ndos");
        assert_eq!(editor.cursor().linea, 1);
    }

    #[test]
    fn un_movimiento_que_falla_cancela_el_operador() {
        let (texto, _, _) = correr("|abc", "dfz");
        assert_eq!(texto, "|abc");
        let (texto, _, _) = correr("|abc", "ci(X\x1b");
        // Sin paréntesis no hay nada que cambiar ni se entra a Insertar
        // (la `X` y el `Esc` quedan como comandos de Normal: `X` en la
        // columna 0 no hace nada).
        assert_eq!(texto, "|abc");
    }

    #[test]
    fn dos_puntos_abre_la_linea_de_comandos() {
        let (_, vim, _) = correr("|abc", ":");
        assert!(vim.linea_comando.activa());
    }

    #[test]
    fn modo_visual_por_caracteres() {
        let casos: &[(&str, &str, &str)] = &[
            ("|hola mundo", "vlld", "|a mundo"),
            ("hola |mundo", "vhhd", "hol|undo"),
            ("|hola mundo", "vey$p", "hola mundohol|a"),
            ("|hola mundo", "vecX\x1b", "|X mundo"),
            ("|ab\ncd", "vjd", "|d"),
            ("uno d|os tres", "viwd", "uno | tres"),
            ("f(a, |b)", "vi(d", "f(|)"),
            ("|hola", "vl~", "|HOla"),
            ("|ab", "vlohd", "|"),
            ("|abc", "vl\x1bx", "a|c"),
            ("|a\nb", "vJ", "a| b"),
            ("|a\nb", "vj>", "    |a\n    b"),
        ];
        let mut fallas = Vec::new();
        for (inicial, teclas, esperado) in casos {
            let (obtenido, _, editor) = correr(inicial, teclas);
            if obtenido != *esperado || editor.modo() != Modo::Normal {
                fallas.push(format!("{inicial:?} + {teclas:?}: esperado {esperado:?}, obtenido {obtenido:?} ({:?})", editor.modo()));
            }
        }
        assert!(fallas.is_empty(), "\n{}", fallas.join("\n"));
    }

    #[test]
    fn modo_visual_por_lineas() {
        let casos: &[(&str, &str, &str)] = &[
            ("a\n|b\nc\nd", "Vjd", "a\n|d"),
            ("a\nb\n|c\nd", "Vkd", "a\n|d"),
            ("|a\nb\nc", "Vjy", "|a\nb\nc"),
            ("|a\nb\nc", "VjyGp", "a\nb\nc\n|a\nb"),
            ("  |a\nb", "VcX\x1b", "  |X\nb"),
            ("|a\nb", "Vj>", "    |a\n    b"),
            ("|a\nb\nc", "vVjd", "|c"),
        ];
        let mut fallas = Vec::new();
        for (inicial, teclas, esperado) in casos {
            let (obtenido, _, editor) = correr(inicial, teclas);
            if obtenido != *esperado || editor.modo() != Modo::Normal {
                fallas.push(format!("{inicial:?} + {teclas:?}: esperado {esperado:?}, obtenido {obtenido:?} ({:?})", editor.modo()));
            }
        }
        assert!(fallas.is_empty(), "\n{}", fallas.join("\n"));
    }

    #[test]
    fn visual_muestra_la_seleccion_y_esc_la_quita() {
        let mut editor = editor_con("|hola mundo");
        let mut vim = EstadoVim::nuevo();
        tipear(&mut editor, &mut vim, "vll");
        assert_eq!(editor.modo(), Modo::Visual);
        let c = editor.cursores()[0];
        assert_eq!((c.ancla.columna, c.cursor.columna), (0, 2));
        tipear(&mut editor, &mut vim, "\x1b");
        assert_eq!(editor.modo(), Modo::Normal);
        assert!(!editor.cursores()[0].tiene_seleccion());
        tipear(&mut editor, &mut vim, "V");
        assert_eq!(editor.modo(), Modo::VisualLinea);
        tipear(&mut editor, &mut vim, "V");
        assert_eq!(editor.modo(), Modo::Normal);
    }

    #[test]
    fn punto_sin_cambio_previo_avisa() {
        let mut editor = editor_con("|a");
        let mut vim = EstadoVim::nuevo();
        let aviso = ejecutar_tecla(&mut editor, &mut vim, '.', &OpcionesVim::default());
        assert_eq!(aviso.as_deref(), Some("Nada para repetir"));
    }

    #[test]
    fn indentar_usa_la_unidad_de_la_config() {
        let mut editor = editor_con("|a");
        let mut vim = EstadoVim::nuevo();
        let opciones = OpcionesVim { indentacion: "\t".to_string(), ancho_tabulacion: 4 };
        ejecutar_tecla(&mut editor, &mut vim, '>', &opciones);
        ejecutar_tecla(&mut editor, &mut vim, '>', &opciones);
        assert_eq!(editor.buffer().a_texto(), "\ta");
    }
}
