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

### 1. El explorador de archivos es de solo lectura

`crates/fs/src/explorador.rs` no tiene ningún método para crear,
renombrar, borrar o mover un archivo/carpeta — solo navegar (`Enter`
abre o expande/colapsa). No hay "nuevo archivo", "nueva carpeta",
"renombrar" ni "eliminar" en ningún lado de la app, ni siquiera como
comando de paleta.

**Alcance**: cada operación es chica por separado (crear/renombrar son
básicamente un prompt de texto tipo "Guardar como" + una llamada a
`std::fs`; borrar necesita confirmación, capaz con el mismo patrón de
doble-confirmación que ya usa `Ctrl+Q` con cambios sin guardar). Juntas
son una pieza mediana. Candidato a dividir en 2-3 PRs (crear
archivo/carpeta primero, renombrar después, borrar al final — es el más
delicado, mejor no apurarlo).

---

## P1 — Gaps reales de alcance acotado

### 2. El selector de temas no puede importar un tema de tercero

`TEMAS_EMBEBIDOS` (`crates/config/src/tema.rs`) es un array fijo de 13
entradas — nada escanea `directorio_temas_usuario()`
(`~/.config/tcode/themes/`, o el equivalente portable de Windows)
buscando archivos `.toml` nuevos que no correspondan a ninguno de esos
13 ids. Si alguien te pasa un tema de un lenguaje/tema builder externo y
lo dejás en esa carpeta con OTRO nombre, la app puede *usarlo* si lo
escribís a mano en `config.toml` (`tema = "nombre-que-sea"`) y recargás
con `Ctrl+K Ctrl+L`, pero **nunca aparece en el selector** (`Ctrl+K
Ctrl+T`) — rompe justo el flujo "importar y elegir desde la lista" que
describe `PLAN.md` §7 ("Compartir temas").

**Alcance**: `EstadoSelectorTema::temas_filtrados` (o quien arma la
lista) tendría que hacer `read_dir` sobre el directorio de usuario,
parsear el `name`/`type`/`alto_contraste` de cada `.toml` que no
coincida con ningún id embebido, y agregarlo a la lista como
`InfoTema` "dinámico" (probablemente necesita que `InfoTema` deje de
ser `&'static str` puro, o una segunda estructura paralela — repasar al
encarar esto). Compatibilidad con el formato Helix (mencionada en el
mismo punto del plan) es un problema aparte y más grande; no incluirla
acá.

### 3. LSP: sin variables de entorno por comando

`ComandoLsp` (`crates/config/src/config.rs`) tiene `comando` +
`argumentos`, nada de `env` — PLAN.md §5.3 pedía las tres. Sirve para
casos reales (un LSP que necesita `JAVA_HOME`, o una versión de Node
distinta vía `PATH` acotado a esa sesión).

**Alcance**: agregar `env: HashMap<String, String>` a `ComandoLsp`
(con su UI de edición — probablemente una fila más en el prompt `c` de
"Lenguajes / LSP", o un modal aparte tipo `CLAVE=valor` por línea) y
pasarlo a `Command::envs(...)` en `Cliente::lanzar`
(`crates/lsp/src/cliente.rs`). Acotado, pero la parte de UI para
editar un mapa clave-valor (no un string plano) es la parte con más
decisiones de diseño.

### 4. "Ver logs del LSP" (`Ctrl+K R`, ya implementado) es una foto, no en vivo

Nota, no gap nuevo: `PLAN.md` §5.3 pedía "logs en tiempo real"; lo que
hay (PR #78) es un snapshot al momento de abrir — cerrar y volver a
abrir trae lo último, pero no se actualiza solo mientras está abierto.
Decisión consciente de alcance en su momento (evita la complejidad de
una vista que recibe actualizaciones de una tarea de fondo mientras el
usuario está tipeando un filtro). Si en algún momento hace falta de
verdad ver un log mientras se reproduce un problema en curso, esta es
la pieza para revisarla — hasta entonces, cerrar/reabrir alcanza.

### 5. Scroll-follow real en overlays y en el explorador

Bug latente compartido por varios lugares, notado en piezas anteriores
pero nunca resuelto de raíz:

- `tcode_ui::overlay::dibujar` (paleta de comandos, buscador de
  archivos, selector de temas, visor de logs de LSP) usa
  `ratatui::widgets::List` **sin `ListState`** — siempre renderiza desde
  el ítem 0, recortado a lo que entra en el alto disponible. Con más
  resultados de los que entran en pantalla, mover la selección hacia
  abajo con `↓` la deja resaltando una fila que nunca se dibuja.
- `crates/ui/src/panel_archivos.rs` (explorador) tiene el mismo
  problema: sin `ListState`, un árbol más alto que el panel deja la
  fila seleccionada invisible al bajar lo suficiente.

**Alcance**: un solo arreglo en `overlay::dibujar` (agregar `ListState`
y `render_stateful_widget`, calculando el offset para mantener
`seleccion` visible) resuelve los 4 usos de una vez. El explorador
necesita el mismo tratamiento por separado (arma su propia `List` sin
pasar por `overlay::dibujar`). No es viese ni difícil, pero toca 2
archivos con bastante superficie de uso — probar con cuidado que no
rompa el resaltado de fila actual que ya funciona.

---

## P2 — Del plan original, alcance grande o valor dudoso

### 6. Guardado automático

`PLAN.md` §5 "Editor": nunca / al perder foco / cada N segundos. No
implementado. Alcance mediano: un campo de config + lógica de timer
(perder foco es fácil de detectar — cambio de panel/archivo activo;
"cada N segundos" necesita un tick periódico en el loop de eventos, que
hoy es puramente reactivo a `tokio::select!` entre teclado y LSP —
agregar un `tokio::time::interval` al select).

### 7. Formatear al guardar (vía LSP)

`PLAN.md` §5 "Editor": on/off por lenguaje. No implementado — no hay
ninguna llamada a `textDocument/formatting` en `crates/lsp`/`app/src/
lsp.rs` hoy. Alcance grande: nuevo método LSP, aplicar el `TextEdit[]`
resultante al buffer antes de escribir a disco, manejar el caso "el LSP
no soporta formatting" o "tardó demasiado" sin bloquear el guardado.

### 8. Indicadores de git en el gutter

Colores `TemaGit` (`added`/`modified`/`deleted`) ya existen en cada
tema, sin conectar a nada (confirmado: cero integración con git en todo
el repo). Implica correr `git diff`/leer el índice para saber qué
líneas cambiaron respecto al último commit — trabajo real, y una
dependencia nueva (`git2` o invocar el binario `git`). Grande.

### 9. Code folding (plegado de bloques)

`PLAN.md` §4 lo lista con atajos propios (`Ctrl+Shift+[`/`]`, `Ctrl+K
Ctrl+0`/`Ctrl+K Ctrl+J`) — cero implementación. Necesita: queries de
plegado de tree-sitter por lenguaje (no todas las gramáticas ya
embebidas las traen listas), estado de "qué rangos están plegados" por
buffer, e integrar ese estado con el scroll/gutter de `vista_codigo`
(ya bastante compleja desde el ajuste de línea/reflow). Una de las
piezas más grandes de todo este documento — considerar dividirla en
sub-piezas (soporte para 2-3 lenguajes primero, resto incremental, como
se hizo con LSP).

### 10. Config por proyecto (`.tcode/config.toml` con override)

`PLAN.md` §12, decisión abierta #5: "sí a ambas, con override" — nunca
se implementó, solo existe config global de usuario. Alcance: al
arrancar, buscar `.tcode/config.toml` subiendo desde el directorio del
archivo abierto (o el cwd) hasta la raíz de git o el filesystem, y
mezclarlo sobre la config global (probablemente campo por campo, no
todo-o-nada). Mediano — la parte de "mezclar dos `Config` parciales"
requiere pensar bien las reglas de merge.

### 11. CSV: funciones que quedaron fuera de M3

Ya documentado en `PRUEBAS.md` (sección "M3 — Vista CSV/TSV"), sigue
pendiente: ordenar por columna, filtrar por columna, insertar/eliminar
filas y columnas, resize manual de ancho de columna (hoy es automático
según contenido, con el scroll horizontal de PR #71). Ninguna es
urgente; se usan mucho menos que ver/editar celdas, que ya funciona.

---

## P3 — Bloqueado o reconsiderar si debería estar en el plan

### 12. Densidad de UI / tabs / breadcrumbs

`PLAN.md` §5 "Interfaz". Genuinamente bloqueado: tcode no tiene ningún
concepto de "pestaña de archivo abierto" (solo splits) ni de
breadcrumb (ruta + jerarquía de símbolos arriba del código) — haría
falta diseñar y construir esos widgets desde cero antes de que
"mostrar/ocultar" tenga algo que mostrar. No es una pieza chica
camuflada de toggle; es un feature nuevo con ese toggle como
consecuencia menor.

### 13. Zoom (`Ctrl++`/`Ctrl+-`/`Ctrl+0`) y pantalla completa (`F11`)

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

### 14. Modo zen

`PLAN.md` §4, `Ctrl+K Z`. Distinto de "Mostrar barra de estado" (ya
existe, pero es un toggle persistente en config, no un atajo rápido
para ocultar TODO — explorador, statusbar — de un solo golpe y
mostrarlo de nuevo igual de rápido). Factible y chico si se quiere: un
booleano de sesión (no persistido) que la UI consulta para saltearse
explorador/statusbar sin importar su config normal.

### 15. Compatibilidad de temas con formato Helix

`PLAN.md` §7 "Compartir temas": "parser tolerante que acepta temas en
formato Helix". No implementado. Depende de qué tan distinto es el
esquema TOML de Helix del propio de `tcode` (no investigado a fondo
todavía) — antes de estimar esfuerzo real haría falta comparar ambos
formatos campo por campo.

---

## Hecho recientemente (para no reabrir por error)

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
