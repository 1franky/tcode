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

Ninguno pendiente — los que había (#14 rendimiento con archivos grandes,
#2 logs del LSP en vivo) se cerraron, ver "Hecho recientemente".

---

## P2 — Del plan original, alcance grande o valor dudoso

Ninguno pendiente — #4 a #9 se cerraron el 2026-09-23, ver "Hecho
recientemente". Limitaciones conocidas que quedaron de esas piezas (no
son gaps nuevos, anotadas para no redescubrirlas):

- **Plegado (#7)**: Markdown no pliega (a propósito); una línea
  modificada que queda dentro de un bloque plegado no se marca con git en
  la cabecera del pliegue; los pliegues no se guardan entre sesiones.
- **Git en el gutter (#6)**: la base de `HEAD` se relee al abrir y al
  guardar, no sola — un commit hecho desde otra terminal se ve recién en
  el próximo `Ctrl+S`. Archivos sin trackear no se marcan.
- **Config por proyecto (#8)**: la carpeta de búsqueda se fija al
  arrancar (abrir después un archivo de otro repo no cambia la config);
  un proyecto no puede apagar un valor opcional que la global prende
  (TOML no tiene `null`); los errores del TOML del proyecto solo se ven
  en la cabecera del panel `Ctrl+,`. Por seguridad no puede definir
  comandos LSP; un mecanismo de "confiar en este proyecto" quedó fuera.
- **Guardado automático (#4)**: nunca formatea (solo `Ctrl+S`, mismo
  criterio que VSCode con autoguardado por demora); "al perder foco" en
  tmux requiere `set -g focus-events on`.
- **Formatear al guardar (#5)**: solo vía LSP (`textDocument/formatting`);
  no hay formateadores externos por lenguaje.
- **CSV (#9)**: insertar/eliminar columnas re-serializa con quoting mínimo
  (pierde líneas en blanco entre filas); ordenar texto pliega tildes y ñ
  sin collation completa.
- **Rendimiento (#14)**: en un frame con edición, el texto se sigue
  copiando una vez para el resaltador y otra para el LSP (evitarlo del
  todo requiere que el `Buffer` registre las ediciones). En archivos con
  muchos errores de sintaxis el re-parseo tiene un tope de 250 ms: los
  colores de lo recién editado quedan aproximados hasta el reintento.

---

## P3 — Bloqueado o reconsiderar si debería estar en el plan

Ninguno pendiente — #10 a #13 se cerraron el 2026-09-24, ver "Hecho
recientemente". Decisión explícita que sale del plan: el **zoom de
fuente** (`Ctrl++`/`Ctrl+-`/`Ctrl+0`, PLAN.md §4) no se implementa — en
una TUI el tamaño de letra lo controla el emulador de terminal, no el
proceso que corre adentro. "Pantalla completa" (`F11`) sí, reinterpretada
como maximizar el panel activo.

Limitaciones conocidas de estas piezas (no son gaps nuevos):

- **Pestañas (#10)**: sigue habiendo un solo cliente LSP — pasar a una
  pestaña de otro lenguaje (o sin lenguaje) lo relanza al volver. "Guardar
  como" hacia un archivo ya abierto en otra pestaña deja dos pestañas del
  mismo archivo. Un hilo de git inactivo por pestaña con repo.
- **Breadcrumbs (#10)**: no son interactivos (no se navega desde ahí); en
  Python, una línea en blanco al final de un `def` muestra solo el
  contenedor de afuera (tree-sitter no la incluye en el bloque).
- **Zen / maximizar (#11/#12)**: en zen + maximizado no se ve `[MAX]` (no
  hay statusbar); si la terminal o el sistema se comen `F11`, queda
  `Ctrl+K G`.
- **Temas Helix (#13)**: los scopes que tcode no puede representar
  (`ui.menu`, `ui.popup`, `ui.virtual.*`, `markup.*`, sub-scopes finos) se
  ignoran; un tema que hereda de uno incluido en Helix pero ausente
  localmente se completa con el Oscuro/Claro de tcode; los nombres ANSI
  usan valores fijos de xterm, no la paleta de la terminal.

---

## Hecho recientemente (para no reabrir por error)

**2026-09-24 — todo P3 cerrado** (4 agentes en paralelo, integrados de a
uno a `develop` con verificación independiente — tests, clippy, tmux):
- **Modo zen + maximizar panel** (PR #105, P3 #12 y #11): `Ctrl+K Z` oculta
  todo lo que no es código (sesión, no config; punto único `Cromo::nuevo`
  en `crates/ui/src/lib.rs`); `F11`/`Ctrl+K G` maximiza el panel activo
  del split como `prefix z` de tmux.
- **Temas en formato Helix** (PR #106, P3 #13): parser tolerante en
  `crates/config/src/helix.rs` — `[palette]`, `inherits`, scopes
  jerárquicos; aparecen en `Ctrl+K Ctrl+T` como "(Helix)"; editarlos o
  duplicarlos produce una copia en formato tcode.
- **Pestañas** (PR #107, P3 #10): varios documentos por panel con estado
  propio; `Ctrl+PageUp`/`PageDown`, `Ctrl+W`, `Alt+1…9`. De paso: la
  sesión LSP sigue al documento activo (antes mandaba el texto de otro
  archivo con el URI del primero), y los avisos transitorios de la
  statusbar van pegados a la ruta (en terminales angostas la confirmación
  de `Ctrl+W` quedaba recortada).
- **Breadcrumbs** (PR #108, P3 #10): ruta relativa al proyecto + símbolos
  que contienen al cursor, desde el árbol incremental del resaltador; 11
  lenguajes con símbolos.

**2026-09-23 — todo P1 y P2 cerrado en paralelo** (7 agentes en worktrees
aislados; cada rama verificada de forma independiente — tests, clippy,
tmux — e integrada de a una a `develop`, resolviendo conflictos):
- **P2 #7 Plegado de bloques** (PR #94): rangos por tree-sitter
  (reusando el árbol del resaltador) en todos los lenguajes con gramática
  salvo Markdown, por indentación en el resto; `Ctrl+Shift+[`/`]`,
  `Ctrl+K Ctrl+0`/`Ctrl+K Ctrl+J` (+ alternativas `Ctrl+K [`/`]`/`0`).
- **P2 #9 CSV** (PR #95): ordenar, filtrar, insertar/eliminar filas y
  columnas, ancho manual (chords `Ctrl+K` en la vista de tabla).
- **P2 #5 Formatear al guardar** (PR #97): vía LSP, por lenguaje, apagado
  por defecto, una sola edición deshacible, nunca bloquea el guardado.
- **P2 #4 Guardado automático + P1 #2 logs del LSP en vivo** (PR #98):
  nunca / al perder foco / cada N segundos; el visor `Ctrl+K R` se
  actualiza solo. Tick en el bucle solo cuando hace falta.
- **P2 #8 Config por proyecto** (PR #99): `.tcode/config.toml` mezclado
  campo por campo sobre la global; el panel edita solo la global; sin
  comandos LSP desde el proyecto (seguridad).
- **P1 #14 Rendimiento, segunda parte** (PR #100): LSP incremental,
  revisión del `Buffer`, ajuste de línea sin recorrer el archivo, tope de
  250 ms al re-parsear archivos llenos de errores (~1,3 s → ~7 ms/tecla).
- **P2 #6 Git en el gutter** (PR #101): `+`/`~`/`-` en vivo respecto de
  `HEAD`, diff en un hilo aparte, sin dependencias nuevas.
- De paso (PR #96): el test del resaltado incremental ahora compara solo
  sobre código válido — con errores de sintaxis el árbol incremental
  puede diferir legítimamente del de parsear de cero.

**2026-09-23 — rendimiento al editar archivos grandes** (P1 #14, primera
parte). Medido por dentro del proceso con 10.000 líneas, frame al tipear:
`.rs` 45,5 → 4,4 ms, `.py` 39,9 → 5,3 ms; al moverse, ~5-6 → ~2,5 ms.
- **Resaltado incremental**: `tcode_syntax::Resaltador` deja
  `tree-sitter-highlight` (que siempre parseaba el archivo entero) por
  un motor propio sobre `tree-sitter`: guarda el árbol de cada documento,
  deduce la edición por prefijo/sufijo común, re-parsea de forma
  incremental y consulta solo el rango visible
  (`Resaltador::resaltar_documento`). Mismo algoritmo de resolución de
  capturas, verificado con un test que compara token por token contra
  `tree-sitter-highlight` en los 16 lenguajes, otro de 360 ediciones
  pseudoaleatorias contra parsear de cero, y capturas de pantalla con
  color idénticas a v0.7.0 (con y sin ajuste de línea).
- **Solo las líneas visibles**: sin ajuste de línea, `vista_codigo` lee
  del buffer únicamente las filas en pantalla (`Buffer::linea_texto`)
  en vez de copiar todas las líneas en cada frame.

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
