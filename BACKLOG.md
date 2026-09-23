# BACKLOG.md — trabajo pendiente de `tcode`

Lista priorizada de lo que falta, resultado de una revisión del código
real (no de memoria ni de lo que dice `PLAN.md` de memoria) hecha el
2026-09-18 contra el estado de `develop` en ese momento (post v0.6.0 +
`Ctrl+K R` de logs LSP, PR #78). Complementa a los otros dos documentos
del repo, cada uno con un propósito distinto:

- **[PLAN.md](./PLAN.md)** — diseño original y arquitectura; de ahí sale
  la numeración `§N` que se usa acá para ubicar cada ítem en el plan de
  origen.
- **[PRUEBAS.md](./PRUEBAS.md)** — checklist de pruebas manuales de lo
  YA implementado; se actualiza pieza por pieza a medida que algo se
  mergea.
- **Este archivo** — lo que falta, priorizado. Se actualiza cuando se
  cierra un ítem (mover a "Hecho" o borrar) o cuando aparece uno nuevo
  (bug encontrado, pieza de `PLAN.md` que se decide encarar).

No es una promesa de orden estricto — es la mejor estimación de qué
conviene encarar primero, para no tener que redescubrir el análisis cada
vez que se pregunta "¿qué hay pendiente?". Cualquier pieza nueva sigue el
flujo de ramas de siempre (rama `feature/<slug>` desde `develop` → PR →
nunca commitear directo a `develop`/`main`).

---

## Cómo priorizar acá

Cuatro niveles:

- **P0 — Gap sorprendente.** Algo que cualquier usuario esperaría que ya
  existiera, y no existe. Se nota enseguida al usar el editor.
- **P1 — Gap real de alcance acotado.** Falta, importa, y el trabajo
  para cerrarlo es predecible (no requiere inventar arquitectura nueva).
- **P2 — Del plan original, alcance grande o de valor más dudoso.**
  Legítimo, pero o es mucho trabajo, o antes hay que decidir si de
  verdad tiene sentido en una TUI.
- **P3 — Bloqueado, o mejor sacarlo del plan.** No se puede encarar
  todavía (falta un prerrequisito), o el análisis de esta revisión
  sugiere que no aplica a un editor de terminal y debería reconsiderarse
  en vez de dejarlo como "pendiente" para siempre.

---

## P0 — Gaps sorprendentes

Ninguno pendiente por ahora — el último (pegar texto grande / teclas
repetidas lento) se cerró, ver "Hecho recientemente".

---

## P1 — Gaps reales de alcance acotado

### 14. Rendimiento con archivos de miles de líneas al EDITAR

Lo que quedó después del arreglo de pegado (PR #89). Medido el
2026-09-23 en tmux (160x50, release) con 10.000 líneas: pegar, moverse
y saltar de punta a punta ya es instantáneo (pegar 10k líneas: 0,05 s
en `.txt`, 0,17 s en `.rs`, 0,47 s en `.py` con pyright; 1000 flechas
seguidas: < 0,1 s), pero la latencia de UNA tecla que edita crece con
el tamaño del archivo:

| 10.000 líneas            | flecha | tipear 1 carácter |
|--------------------------|--------|-------------------|
| `.txt` (sin resaltado)   | ~19 ms | ~20 ms            |
| `.rs`                    | ~17 ms | ~69 ms            |
| `.py` real con pyright   | ~19 ms | ~64 ms            |

(Con 500 líneas no se nota.) Tres costos que hoy son O(archivo) por
frame o por edición, de mayor a menor impacto:

- **tree-sitter no incremental**: cada edición re-parsea el archivo
  entero (`tcode_syntax::Resaltador`; la cache de PR #89 solo evita
  re-parsear cuando el texto no cambió). La solución de fondo es
  guardar el `Tree` y usar `tree.edit()` + parseo incremental, y
  resaltar solo el rango visible.
- **`Buffer::lineas_texto()` y las filas visuales** se recalculan sobre
  todo el archivo en cada frame (`vista_codigo::dibujar`) — explica el
  piso de ~20 ms incluso en `.txt`. Debería trabajar solo con las
  líneas visibles leyendo del `Rope`.
- **LSP con sync completo**: `didChange` manda el texto entero una vez
  por frame con cambios (`lsp.rs`, `sincronizar_contenido`); la
  sincronización incremental por rangos lo reduciría a lo editado.

Ojo con un caso patológico: un archivo con MUCHOS errores de sintaxis
para su lenguaje (p. ej. código Rust guardado como `.py`) llegó a
~400 ms por tecla — la recuperación de errores de tree-sitter es cara;
el parseo incremental también lo mitiga.

Nota aparte (no es de tcode, no hace falta arreglarlo): en una ráfaga
artificial de cientos de secuencias de escape de una sola vez (`tmux
send-keys` con 1000 flechas en un llamado), crossterm puede recibir un
`ESC` al final de una lectura parcial y entregarlo como tecla `Esc`,
dejando `[B` como texto. Pasa igual con la versión anterior a PR #89;
con teclado real o pegando texto no se da. Para benchmarks, mandar las
teclas en tandas de ~50.

### 2. "Ver logs del LSP" (`Ctrl+K R`, ya implementado) es una foto, no en vivo

Nota, no gap nuevo: `PLAN.md` §5.3 pedía "logs en tiempo real"; lo que
hay (PR #78) es un snapshot al momento de abrir — cerrar y volver a
abrir trae lo último, pero no se actualiza solo mientras está abierto.
Decisión consciente de alcance en su momento (evita la complejidad de
una vista que recibe actualizaciones de una tarea de fondo mientras el
usuario está tipeando un filtro). Si en algún momento hace falta de
verdad ver un log mientras se reproduce un problema en curso, esta es
la pieza para revisarla — hasta entonces, cerrar/reabrir alcanza.

---

## P2 — Del plan original, alcance grande o valor dudoso

### 4. Guardado automático

`PLAN.md` §5 "Editor": nunca / al perder foco / cada N segundos. No
implementado. Alcance mediano: un campo de config + lógica de timer
(perder foco es fácil de detectar — cambio de panel/archivo activo;
"cada N segundos" necesita un tick periódico en el loop de eventos, que
hoy es puramente reactivo a `tokio::select!` entre teclado y LSP —
agregar un `tokio::time::interval` al select).

### 5. Formatear al guardar (vía LSP)

`PLAN.md` §5 "Editor": on/off por lenguaje. No implementado — no hay
ninguna llamada a `textDocument/formatting` en `crates/lsp`/`app/src/
lsp.rs` hoy. Alcance grande: nuevo método LSP, aplicar el `TextEdit[]`
resultante al buffer antes de escribir a disco, manejar el caso "el LSP
no soporta formatting" o "tardó demasiado" sin bloquear el guardado.

### 6. Indicadores de git en el gutter

Colores `TemaGit` (`added`/`modified`/`deleted`) ya existen en cada
tema, sin conectar a nada (confirmado: cero integración con git en todo
el repo). Implica correr `git diff`/leer el índice para saber qué
líneas cambiaron respecto al último commit — trabajo real, y una
dependencia nueva (`git2` o invocar el binario `git`). Grande.

### 7. Code folding (plegado de bloques)

`PLAN.md` §4 lo lista con atajos propios (`Ctrl+Shift+[`/`]`, `Ctrl+K
Ctrl+0`/`Ctrl+K Ctrl+J`) — cero implementación. Necesita: queries de
plegado de tree-sitter por lenguaje (no todas las gramáticas ya
embebidas las traen listas), estado de "qué rangos están plegados" por
buffer, e integrar ese estado con el scroll/gutter de `vista_codigo`
(ya bastante compleja desde el ajuste de línea/reflow). Una de las
piezas más grandes de todo este documento — considerar dividirla en
sub-piezas (soporte para 2-3 lenguajes primero, resto incremental, como
se hizo con LSP).

### 8. Config por proyecto (`.tcode/config.toml` con override)

`PLAN.md` §12, decisión abierta #5: "sí a ambas, con override" — nunca
se implementó, solo existe config global de usuario. Alcance: al
arrancar, buscar `.tcode/config.toml` subiendo desde el directorio del
archivo abierto (o el cwd) hasta la raíz de git o el filesystem, y
mezclarlo sobre la config global (probablemente campo por campo, no
todo-o-nada). Mediano — la parte de "mezclar dos `Config` parciales"
requiere pensar bien las reglas de merge.

### 9. CSV: funciones que quedaron fuera de M3

Ya documentado en `PRUEBAS.md` (sección "M3 — Vista CSV/TSV"), sigue
pendiente: ordenar por columna, filtrar por columna, insertar/eliminar
filas y columnas, resize manual de ancho de columna (hoy es automático
según contenido, con el scroll horizontal de PR #71). Ninguna es
urgente; se usan mucho menos que ver/editar celdas, que ya funciona.

---

## P3 — Bloqueado o reconsiderar si debería estar en el plan

### 10. Densidad de UI / tabs / breadcrumbs

`PLAN.md` §5 "Interfaz". Genuinamente bloqueado: tcode no tiene ningún
concepto de "pestaña de archivo abierto" (solo splits) ni de
breadcrumb (ruta + jerarquía de símbolos arriba del código) — haría
falta diseñar y construir esos widgets desde cero antes de que
"mostrar/ocultar" tenga algo que mostrar. No es una pieza chica
camuflada de toggle; es un feature nuevo con ese toggle como
consecuencia menor.

### 11. Zoom (`Ctrl++`/`Ctrl+-`/`Ctrl+0`) y pantalla completa (`F11`)

`PLAN.md` §4 "Zoom y vista". Sospecha fuerte de que **no aplica a una
TUI**: el tamaño de fuente en una terminal lo controla el emulador de
terminal (Ctrl+/Ctrl- de iTerm2/Windows Terminal/etc.), no el proceso
que corre adentro — `tcode` no tiene forma de "agrandar la letra" sin
control sobre la terminal misma. Pantalla completa es lo mismo: la
controla la terminal (o el gestor de ventanas), no la app. Antes de
tratar esto como "falta implementar", vale la pena decidir
explícitamente si se saca del plan (lo más probable) o si hay alguna
interpretación razonable para una terminal (¿"zoom" como ajustar
cuántas columnas/filas usa el layout interno, sin tocar la fuente
real?) que valga la pena.

### 12. Modo zen

`PLAN.md` §4, `Ctrl+K Z`. Distinto de "Mostrar barra de estado" (ya
existe, pero es un toggle persistente en config, no un atajo rápido
para ocultar TODO — explorador, statusbar — de un solo golpe y
mostrarlo de nuevo igual de rápido). Factible y chico si se quiere: un
booleano de sesión (no persistido) que la UI consulta para saltearse
explorador/statusbar sin importar su config normal.

### 13. Compatibilidad de temas con formato Helix

`PLAN.md` §7 "Compartir temas": "parser tolerante que acepta temas en
formato Helix". No implementado. Depende de qué tan distinto es el
esquema TOML de Helix del propio de `tcode` (no investigado a fondo
todavía) — antes de estimar esfuerzo real haría falta comparar ambos
formatos campo por campo.

---

## Hecho recientemente (para no reabrir por error)

**2026-09-23 — rendimiento al pegar y con teclas repetidas** (PR #89)
— era el P0 reportado usando el editor de verdad: pegar ~500 líneas
tardaba 104,6 s y mantener una flecha congelaba la pantalla. Bracketed
paste (`Editor::insertar_texto`, un solo paso de deshacer), el bucle
principal procesa todos los eventos encolados antes de dibujar,
`sincronizar_lsp` una vez por frame, cache en `Resaltador` y búsqueda
binaria de tokens por línea. Ahora: 500 líneas en 0,02 s, 10.000 en
< 0,5 s; lo que queda para archivos grandes está en el P1 #14.

**2026-09-21, dos P1 más (misma verificación independiente):**
- **Scroll-follow real en overlays y en el explorador** (PR #86) — era
  el P1 #3. `ListState` en `overlay::dibujar` (paleta, buscador, selector
  de temas, visor de logs LSP) y su equivalente en `panel_archivos.rs`;
  la fila seleccionada ya no queda fuera de pantalla con listas largas.
- **Variables de entorno por comando LSP** (PR #87) — era el P1 #1.
  `ComandoLsp::env` (`BTreeMap`), sintaxis `VAR=valor -- comando` en el
  mismo campo de edición de "Lenguajes / LSP" (retrocompatible);
  `Cliente::lanzar` las pasa con `Command::envs` (aditivo al entorno
  heredado).

**2026-09-21, dos piezas más en paralelo (mismo criterio: agentes en
worktrees aislados, revisadas y mergeadas después de verificación
independiente — build/test/clippy propios + merge de prueba contra el
`develop` combinado + tmux):**
- **Explorador: crear, renombrar y borrar** (PR #84) — era el único P0
  que quedaba. `Explorador::crear_archivo`/`crear_carpeta`/
  `renombrar_seleccion`/`borrar_seleccion`, refrescando solo la carpeta
  contenedora. `Ctrl+K N`/`Ctrl+K C`/`Ctrl+K M` + `Delete` (con
  confirmación explícita `y`/`Y`, ninguna otra tecla por defecto —
  acción irreversible). Verificado a mano en tmux incluido el borrado
  recursivo de una carpeta con contenido real.
- **Selector de temas: importar temas de terceros** (PR #83) — era el
  P1 #2. `descubrir_temas_usuario()` escanea la carpeta de temas del
  usuario; `InfoTemaListado` (owned) en paralelo a `InfoTema`
  (`&'static str`, sin tocar), sin duplicar embebidos ni copias `-mio`.
  De paso corrigió que "Duplicar tema activo" perdía el nombre cuando el
  tema activo era de terceros.

**2026-09-18, dos piezas en paralelo (agentes en worktrees aislados,
revisadas y mergeadas después de verificación independiente contra el
`develop` combinado):**
- **Selección de texto con `Shift+flechas`/`Home`/`End`** (PR #80) — era
  el P0 #1 de este documento. `Editor::extender_cada_cursor` (hermana de
  `mover_cada_cursor` que no colapsa `ancla`), 6 comandos
  `cursor.seleccionar_*` nuevos. `Shift+Ctrl+Home/End` (selección hasta
  inicio/fin de archivo) sigue sin implementar, era alcance
  deliberadamente afuera de esta entrega.
- **Regla vertical / guía de columna** (PR #81) — era el P1 #5.
  `config.editor.columna_regla: Option<usize>` + fila en el panel de
  administración; color derivado del tema activo (`background`/
  `foreground`), sin tocar los 13 archivos de tema existentes.

Referencia rápida de lo que esta misma revisión confirmó que SÍ está
resuelto, para no re-preguntarse "¿esto ya existe?": los 13 lenguajes
objetivo con resaltado + LSP donde hay uno conocido, multi-cursor
(`Ctrl+D`/`Ctrl+Shift+L`/`Ctrl+Alt+↑↓`), ajuste de línea real (reflow),
vista Markdown/CSV, selector de temas con preview en vivo + 3 filtros
(claro/oscuro/alto contraste), editor visual de tema (hex/paleta/HSL),
export/import de keymap, comando LSP personalizado por lenguaje + ver
logs, modo VIM opcional (alcance acotado), salto rápido del explorador
(`Ctrl+K J`), scroll horizontal en CSV, y los 3 bugs de Windows
(codepage, símbolos de ancho ambiguo, CRLF).
