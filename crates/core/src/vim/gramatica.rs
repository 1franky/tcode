//! Gramática de los comandos del modo Normal/Visual de VIM: `[conteo]
//! movimiento`, `[conteo] acción` y `[conteo] operador [conteo]
//! (movimiento | objeto de texto | el mismo operador)`. Puro: recibe las
//! teclas acumuladas hasta ahora y dice si ya forman un comando completo,
//! si falta alguna tecla o si no significan nada — sin tocar ningún
//! `Editor` (eso es `vim::ejecutor`). Así se testea la gramática sola.

/// Un movimiento del cursor (lo que va solo o detrás de un operador).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movimiento {
    Izquierda,
    Derecha,
    Arriba,
    Abajo,
    /// `0`
    InicioLinea,
    /// `^`
    PrimerNoBlanco,
    /// `$`
    FinLinea,
    /// `w`/`W` (`grande` = WORD: todo lo que no es blanco)
    PalabraSiguiente { grande: bool },
    /// `b`/`B`
    PalabraAnterior { grande: bool },
    /// `e`/`E`
    FinPalabra { grande: bool },
    /// `gg` (con conteo, a esa línea)
    InicioArchivo,
    /// `G` (con conteo, a esa línea)
    FinArchivo,
    /// `f`/`t`/`F`/`T` + carácter
    BuscarCaracter(BusquedaCaracter),
    /// `;` (`inversa: false`) y `,` (`inversa: true`): repiten la última
    /// búsqueda de carácter — el ejecutor la resuelve con la guardada en
    /// `EstadoVim`.
    RepetirBusqueda { inversa: bool },
    /// `%`
    ParejaCorchete,
    /// `}`
    ParrafoSiguiente,
    /// `{`
    ParrafoAnterior,
}

/// `f{c}`/`t{c}`/`F{c}`/`T{c}`: `hasta` es `t`/`T` (se detiene un
/// carácter antes), `atras` es `F`/`T`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusquedaCaracter {
    pub caracter: char,
    pub hasta: bool,
    pub atras: bool,
}

impl BusquedaCaracter {
    /// La misma búsqueda en la dirección contraria (`,`).
    pub fn invertida(self) -> Self {
        Self { atras: !self.atras, ..self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operador {
    Borrar,
    Cambiar,
    Copiar,
    Indentar,
    Desindentar,
}

impl Operador {
    fn desde(c: char) -> Option<Self> {
        Some(match c {
            'd' => Self::Borrar,
            'c' => Self::Cambiar,
            'y' => Self::Copiar,
            '>' => Self::Indentar,
            '<' => Self::Desindentar,
            _ => return None,
        })
    }
}

/// Qué delimita un objeto de texto (`iw`, `a"`, `i(`...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoObjeto {
    Palabra,
    PalabraGrande,
    /// `"`, `'` o `` ` ``: mismo carácter de los dos lados, en la línea.
    Comillas(char),
    /// Par de apertura/cierre (`()`, `{}`, `[]`, `<>`), anidable y
    /// multilínea.
    Par(char, char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjetoTexto {
    /// `i` (solo lo de adentro) vs. `a` (con los delimitadores/espacios).
    pub interior: bool,
    pub tipo: TipoObjeto,
}

impl ObjetoTexto {
    fn desde(interior: bool, c: char) -> Option<Self> {
        let tipo = match c {
            'w' => TipoObjeto::Palabra,
            'W' => TipoObjeto::PalabraGrande,
            '"' | '\'' | '`' => TipoObjeto::Comillas(c),
            '(' | ')' | 'b' => TipoObjeto::Par('(', ')'),
            '{' | '}' | 'B' => TipoObjeto::Par('{', '}'),
            '[' | ']' => TipoObjeto::Par('[', ']'),
            '<' | '>' => TipoObjeto::Par('<', '>'),
            _ => return None,
        };
        Some(Self { interior, tipo })
    }
}

/// Sobre qué actúa un operador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Objetivo {
    Movimiento(Movimiento),
    Objeto(ObjetoTexto),
    /// El operador repetido (`dd`, `cc`, `yy`, `>>`, `<<`): líneas enteras.
    Lineas,
}

/// Comandos de una tecla (o dos, `r{c}`) que no son movimientos ni
/// operadores combinables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accion {
    /// `x`
    BorrarCaracter,
    /// `X`
    BorrarCaracterAtras,
    /// `s`
    SustituirCaracter,
    /// `S`
    SustituirLinea,
    /// `D`
    BorrarHastaFin,
    /// `C`
    CambiarHastaFin,
    /// `Y`
    CopiarLinea,
    /// `p`
    PegarDespues,
    /// `P`
    PegarAntes,
    /// `u`
    Deshacer,
    /// `r{c}`
    Reemplazar(char),
    /// `J`
    Unir,
    /// `~`
    AlternarMayuscula,
    /// `i`
    Insertar,
    /// `I`
    InsertarInicio,
    /// `a`
    Agregar,
    /// `A`
    AgregarFin,
    /// `o`
    AbrirAbajo,
    /// `O`
    AbrirArriba,
    /// `.`
    Repetir,
    /// `v`
    Visual,
    /// `V`
    VisualLinea,
    /// `:`
    LineaComando,
    /// `o` en modo Visual: intercambia ancla y cursor.
    IntercambiarExtremos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoComando {
    Mover(Movimiento),
    Operar(Operador, Objetivo),
    /// Operador aplicado a la selección del modo Visual (`d`, `y`, `c`,
    /// `>`, `<` en Visual, más `x`/`s` como sinónimos).
    OperarSeleccion(Operador),
    /// `J` en Visual: une las líneas seleccionadas.
    UnirSeleccion,
    /// `~` en Visual.
    AlternarMayusculaSeleccion,
    /// `iw`/`a(`... en Visual: extiende la selección al objeto.
    SeleccionarObjeto(ObjetoTexto),
    Accion(Accion),
}

/// Un comando completo. `conteo` es el producto de los dos conteos
/// posibles (`2d3w` = 6), `None` si no se escribió ninguno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comando {
    pub conteo: Option<usize>,
    pub tipo: TipoComando,
}

impl Comando {
    /// El conteo efectivo (1 si no se escribió).
    pub fn veces(&self) -> usize {
        self.conteo.unwrap_or(1).max(1)
    }

    /// Si es un cambio repetible con `.` (modifica el texto y no es
    /// deshacer/repetir). Los del modo Visual quedan afuera (limitación
    /// documentada: `.` no repite operaciones sobre una selección).
    pub fn es_repetible(&self) -> bool {
        match self.tipo {
            TipoComando::Operar(op, _) => op != Operador::Copiar,
            TipoComando::Accion(a) => !matches!(
                a,
                Accion::CopiarLinea
                    | Accion::Deshacer
                    | Accion::Repetir
                    | Accion::Visual
                    | Accion::VisualLinea
                    | Accion::LineaComando
                    | Accion::IntercambiarExtremos
            ),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Analisis {
    Completo(Comando),
    /// Faltan teclas (`d`, `3`, `g`, `f`, `di`...).
    Incompleto,
    /// Estas teclas no forman ningún comando: se descartan.
    Invalido,
}

/// Lee un conteo (`[1-9][0-9]*`) al principio de `teclas`; devuelve el
/// número (si había) y cuántas teclas consumió. Un `0` suelto no es
/// conteo, es el movimiento "inicio de línea".
fn leer_conteo(teclas: &[char]) -> (Option<usize>, usize) {
    let mut n: Option<usize> = None;
    let mut i = 0;
    while let Some(&c) = teclas.get(i) {
        let Some(d) = c.to_digit(10) else { break };
        if d == 0 && n.is_none() {
            break;
        }
        // Tope generoso: un conteo absurdo no puede desbordar.
        n = Some((n.unwrap_or(0).saturating_mul(10).saturating_add(d as usize)).min(99_999));
        i += 1;
    }
    (n, i)
}

fn multiplicar(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(1).saturating_mul(b.unwrap_or(1)).min(99_999)),
    }
}

/// Resultado de leer un movimiento desde el principio de `teclas`.
enum LecturaMovimiento {
    Si(Movimiento),
    Falta,
    No,
}

fn leer_movimiento(teclas: &[char]) -> LecturaMovimiento {
    use LecturaMovimiento::*;
    let Some(&c) = teclas.first() else { return Falta };
    let simple = match c {
        'h' => Some(Movimiento::Izquierda),
        'l' | ' ' => Some(Movimiento::Derecha),
        'k' => Some(Movimiento::Arriba),
        'j' => Some(Movimiento::Abajo),
        '0' => Some(Movimiento::InicioLinea),
        '^' => Some(Movimiento::PrimerNoBlanco),
        '$' => Some(Movimiento::FinLinea),
        'w' => Some(Movimiento::PalabraSiguiente { grande: false }),
        'W' => Some(Movimiento::PalabraSiguiente { grande: true }),
        'b' => Some(Movimiento::PalabraAnterior { grande: false }),
        'B' => Some(Movimiento::PalabraAnterior { grande: true }),
        'e' => Some(Movimiento::FinPalabra { grande: false }),
        'E' => Some(Movimiento::FinPalabra { grande: true }),
        'G' => Some(Movimiento::FinArchivo),
        ';' => Some(Movimiento::RepetirBusqueda { inversa: false }),
        ',' => Some(Movimiento::RepetirBusqueda { inversa: true }),
        '%' => Some(Movimiento::ParejaCorchete),
        '}' => Some(Movimiento::ParrafoSiguiente),
        '{' => Some(Movimiento::ParrafoAnterior),
        _ => None,
    };
    if let Some(m) = simple {
        return if teclas.len() == 1 { Si(m) } else { No };
    }
    match c {
        'g' => match teclas.get(1) {
            None => Falta,
            Some('g') if teclas.len() == 2 => Si(Movimiento::InicioArchivo),
            _ => No,
        },
        'f' | 't' | 'F' | 'T' => match teclas.get(1) {
            None => Falta,
            Some(&caracter) if teclas.len() == 2 => Si(Movimiento::BuscarCaracter(BusquedaCaracter {
                caracter,
                hasta: c == 't' || c == 'T',
                atras: c == 'F' || c == 'T',
            })),
            _ => No,
        },
        _ => No,
    }
}

/// Lee un objeto de texto (`iw`, `a"`...) desde el principio de `teclas`.
fn leer_objeto(teclas: &[char]) -> Option<Result<ObjetoTexto, ()>> {
    let interior = match teclas.first()? {
        'i' => true,
        'a' => false,
        _ => return None,
    };
    match teclas.get(1) {
        None => Some(Err(())),
        Some(&c) if teclas.len() == 2 => ObjetoTexto::desde(interior, c).map(Ok),
        _ => None,
    }
}

/// Interpreta las teclas acumuladas en modo Normal (`visual: false`) o
/// Visual (`visual: true`). Ver `Analisis`.
pub fn analizar(teclas: &[char], visual: bool) -> Analisis {
    let (conteo, n) = leer_conteo(teclas);
    let resto = &teclas[n..];
    let Some(&c) = resto.first() else { return Analisis::Incompleto };
    let completo = |tipo| Analisis::Completo(Comando { conteo, tipo });

    if visual {
        let op = match c {
            'd' | 'x' => Some(Operador::Borrar),
            'c' | 's' => Some(Operador::Cambiar),
            'y' => Some(Operador::Copiar),
            '>' => Some(Operador::Indentar),
            '<' => Some(Operador::Desindentar),
            _ => None,
        };
        if let Some(op) = op {
            return if resto.len() == 1 { completo(TipoComando::OperarSeleccion(op)) } else { Analisis::Invalido };
        }
        if resto.len() == 1 {
            let tipo = match c {
                'J' => Some(TipoComando::UnirSeleccion),
                '~' => Some(TipoComando::AlternarMayusculaSeleccion),
                'o' => Some(TipoComando::Accion(Accion::IntercambiarExtremos)),
                'v' => Some(TipoComando::Accion(Accion::Visual)),
                'V' => Some(TipoComando::Accion(Accion::VisualLinea)),
                _ => None,
            };
            if let Some(tipo) = tipo {
                return completo(tipo);
            }
        }
        if let Some(objeto) = leer_objeto(resto) {
            return match objeto {
                Ok(o) => completo(TipoComando::SeleccionarObjeto(o)),
                Err(()) => Analisis::Incompleto,
            };
        }
        return match leer_movimiento(resto) {
            LecturaMovimiento::Si(m) => completo(TipoComando::Mover(m)),
            LecturaMovimiento::Falta => Analisis::Incompleto,
            LecturaMovimiento::No => Analisis::Invalido,
        };
    }

    if let Some(op) = Operador::desde(c) {
        let despues = &resto[1..];
        let (conteo2, n2) = leer_conteo(despues);
        let objetivo = &despues[n2..];
        let conteo = multiplicar(conteo, conteo2);
        let operar = |o| Analisis::Completo(Comando { conteo, tipo: TipoComando::Operar(op, o) });
        let Some(&c2) = objetivo.first() else { return Analisis::Incompleto };
        if c2 == c {
            return if objetivo.len() == 1 { operar(Objetivo::Lineas) } else { Analisis::Invalido };
        }
        if let Some(objeto) = leer_objeto(objetivo) {
            return match objeto {
                Ok(o) => operar(Objetivo::Objeto(o)),
                Err(()) => Analisis::Incompleto,
            };
        }
        return match leer_movimiento(objetivo) {
            LecturaMovimiento::Si(m) => operar(Objetivo::Movimiento(m)),
            LecturaMovimiento::Falta => Analisis::Incompleto,
            LecturaMovimiento::No => Analisis::Invalido,
        };
    }

    if c == 'r' {
        return match resto.get(1) {
            None => Analisis::Incompleto,
            Some(&x) if resto.len() == 2 => completo(TipoComando::Accion(Accion::Reemplazar(x))),
            _ => Analisis::Invalido,
        };
    }

    if resto.len() == 1 {
        let accion = match c {
            'x' => Some(Accion::BorrarCaracter),
            'X' => Some(Accion::BorrarCaracterAtras),
            's' => Some(Accion::SustituirCaracter),
            'S' => Some(Accion::SustituirLinea),
            'D' => Some(Accion::BorrarHastaFin),
            'C' => Some(Accion::CambiarHastaFin),
            'Y' => Some(Accion::CopiarLinea),
            'p' => Some(Accion::PegarDespues),
            'P' => Some(Accion::PegarAntes),
            'u' => Some(Accion::Deshacer),
            'J' => Some(Accion::Unir),
            '~' => Some(Accion::AlternarMayuscula),
            'i' => Some(Accion::Insertar),
            'I' => Some(Accion::InsertarInicio),
            'a' => Some(Accion::Agregar),
            'A' => Some(Accion::AgregarFin),
            'o' => Some(Accion::AbrirAbajo),
            'O' => Some(Accion::AbrirArriba),
            '.' => Some(Accion::Repetir),
            'v' => Some(Accion::Visual),
            'V' => Some(Accion::VisualLinea),
            ':' => Some(Accion::LineaComando),
            _ => None,
        };
        if let Some(a) = accion {
            return completo(TipoComando::Accion(a));
        }
    }

    match leer_movimiento(resto) {
        LecturaMovimiento::Si(m) => completo(TipoComando::Mover(m)),
        LecturaMovimiento::Falta => Analisis::Incompleto,
        LecturaMovimiento::No => Analisis::Invalido,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(teclas: &str) -> Analisis {
        analizar(&teclas.chars().collect::<Vec<_>>(), false)
    }

    fn visual(teclas: &str) -> Analisis {
        analizar(&teclas.chars().collect::<Vec<_>>(), true)
    }

    fn cmd(conteo: Option<usize>, tipo: TipoComando) -> Analisis {
        Analisis::Completo(Comando { conteo, tipo })
    }

    #[test]
    fn movimientos_simples_y_con_conteo() {
        assert_eq!(normal("j"), cmd(None, TipoComando::Mover(Movimiento::Abajo)));
        assert_eq!(normal("3j"), cmd(Some(3), TipoComando::Mover(Movimiento::Abajo)));
        assert_eq!(normal("12w"), cmd(Some(12), TipoComando::Mover(Movimiento::PalabraSiguiente { grande: false })));
        assert_eq!(normal("0"), cmd(None, TipoComando::Mover(Movimiento::InicioLinea)));
        // Un 0 después de otros dígitos es parte del conteo.
        assert_eq!(normal("10j"), cmd(Some(10), TipoComando::Mover(Movimiento::Abajo)));
        assert_eq!(normal("5G"), cmd(Some(5), TipoComando::Mover(Movimiento::FinArchivo)));
    }

    #[test]
    fn prefijos_incompletos() {
        for t in ["3", "d", "d3", "g", "f", "dt", "di", "2d", "r", "ya"] {
            assert_eq!(normal(t), Analisis::Incompleto, "{t}");
        }
    }

    #[test]
    fn secuencias_invalidas() {
        for t in ["gx", "dz", "dq", "Q", "diz", "yx"] {
            assert_eq!(normal(t), Analisis::Invalido, "{t}");
        }
    }

    #[test]
    fn gg_y_busqueda_de_caracter() {
        assert_eq!(normal("gg"), cmd(None, TipoComando::Mover(Movimiento::InicioArchivo)));
        assert_eq!(
            normal("tx"),
            cmd(
                None,
                TipoComando::Mover(Movimiento::BuscarCaracter(BusquedaCaracter { caracter: 'x', hasta: true, atras: false }))
            )
        );
        assert_eq!(
            normal("2F("),
            cmd(
                Some(2),
                TipoComando::Mover(Movimiento::BuscarCaracter(BusquedaCaracter { caracter: '(', hasta: false, atras: true }))
            )
        );
    }

    #[test]
    fn operadores_con_movimiento_objeto_o_repetidos() {
        assert_eq!(normal("dd"), cmd(None, TipoComando::Operar(Operador::Borrar, Objetivo::Lineas)));
        assert_eq!(normal("3dd"), cmd(Some(3), TipoComando::Operar(Operador::Borrar, Objetivo::Lineas)));
        assert_eq!(
            normal("2d3w"),
            cmd(Some(6), TipoComando::Operar(Operador::Borrar, Objetivo::Movimiento(Movimiento::PalabraSiguiente { grande: false })))
        );
        assert_eq!(normal("y$"), cmd(None, TipoComando::Operar(Operador::Copiar, Objetivo::Movimiento(Movimiento::FinLinea))));
        assert_eq!(
            normal("ciw"),
            cmd(None, TipoComando::Operar(Operador::Cambiar, Objetivo::Objeto(ObjetoTexto { interior: true, tipo: TipoObjeto::Palabra })))
        );
        assert_eq!(
            normal("da("),
            cmd(
                None,
                TipoComando::Operar(Operador::Borrar, Objetivo::Objeto(ObjetoTexto { interior: false, tipo: TipoObjeto::Par('(', ')') }))
            )
        );
        assert_eq!(
            normal("ci\""),
            cmd(
                None,
                TipoComando::Operar(Operador::Cambiar, Objetivo::Objeto(ObjetoTexto { interior: true, tipo: TipoObjeto::Comillas('"') }))
            )
        );
        assert_eq!(normal(">>"), cmd(None, TipoComando::Operar(Operador::Indentar, Objetivo::Lineas)));
        assert_eq!(normal("dgg"), cmd(None, TipoComando::Operar(Operador::Borrar, Objetivo::Movimiento(Movimiento::InicioArchivo))));
        assert_eq!(
            normal("dfx"),
            cmd(
                None,
                TipoComando::Operar(
                    Operador::Borrar,
                    Objetivo::Movimiento(Movimiento::BuscarCaracter(BusquedaCaracter { caracter: 'x', hasta: false, atras: false }))
                )
            )
        );
    }

    #[test]
    fn acciones() {
        assert_eq!(normal("x"), cmd(None, TipoComando::Accion(Accion::BorrarCaracter)));
        assert_eq!(normal("5x"), cmd(Some(5), TipoComando::Accion(Accion::BorrarCaracter)));
        assert_eq!(normal("ra"), cmd(None, TipoComando::Accion(Accion::Reemplazar('a'))));
        assert_eq!(normal("4p"), cmd(Some(4), TipoComando::Accion(Accion::PegarDespues)));
        assert_eq!(normal(":"), cmd(None, TipoComando::Accion(Accion::LineaComando)));
        assert_eq!(normal("i"), cmd(None, TipoComando::Accion(Accion::Insertar)));
    }

    #[test]
    fn modo_visual() {
        assert_eq!(visual("d"), cmd(None, TipoComando::OperarSeleccion(Operador::Borrar)));
        assert_eq!(visual("x"), cmd(None, TipoComando::OperarSeleccion(Operador::Borrar)));
        assert_eq!(visual(">"), cmd(None, TipoComando::OperarSeleccion(Operador::Indentar)));
        assert_eq!(visual("3j"), cmd(Some(3), TipoComando::Mover(Movimiento::Abajo)));
        assert_eq!(visual("i"), Analisis::Incompleto);
        assert_eq!(
            visual("iw"),
            cmd(None, TipoComando::SeleccionarObjeto(ObjetoTexto { interior: true, tipo: TipoObjeto::Palabra }))
        );
        assert_eq!(visual("o"), cmd(None, TipoComando::Accion(Accion::IntercambiarExtremos)));
        assert_eq!(visual("p"), Analisis::Invalido);
    }

    #[test]
    fn repetibles() {
        let c = |t: &str| match normal(t) {
            Analisis::Completo(c) => c,
            otro => panic!("{t}: {otro:?}"),
        };
        assert!(c("dw").es_repetible());
        assert!(c("x").es_repetible());
        assert!(c("o").es_repetible());
        assert!(!c("yy").es_repetible());
        assert!(!c("j").es_repetible());
        assert!(!c("u").es_repetible());
        assert!(!c(".").es_repetible());
    }
}
