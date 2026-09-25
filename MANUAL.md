# Manual de uso de `tcode`

Guía práctica para gente que ya instaló `tcode` y quiere usarlo — para el
diseño interno, arquitectura y roadmap del proyecto ver [PLAN.md](./PLAN.md).

## Índice

- [Primeros pasos](#primeros-pasos)
- [Atajos esenciales](#atajos-esenciales)
- [Multi-cursor y selección](#multi-cursor-y-selección)
- [Edición de líneas](#edición-de-líneas)
- [Copiar, cortar y pegar](#copiar-cortar-y-pegar)
- [Búsqueda y reemplazo](#búsqueda-y-reemplazo)
- [Plegado de bloques](#plegado-de-bloques)
- [Paneles divididos (splits)](#paneles-divididos-splits)
- [Pestañas de archivos abiertos](#pestañas-de-archivos-abiertos)
- [Explorador de archivos](#explorador-de-archivos)
- [Modo VIM opcional](#modo-vim-opcional)
- [Paleta de comandos y buscador de archivos](#paleta-de-comandos-y-buscador-de-archivos)
- [Vistas especiales: Markdown y CSV](#vistas-especiales-markdown-y-csv)
- [Temas](#temas)
- [Panel de administración](#panel-de-administración)
- [LSP: autocompletado y diagnósticos](#lsp-autocompletado-y-diagnósticos)
- [Personalizar atajos](#personalizar-atajos)
- [Dónde vive la configuración](#dónde-vive-la-configuración)
- [Config por proyecto](#config-por-proyecto)

## Primeros pasos

```bash
tcode                # abre un buffer vacío
tcode archivo.rs      # abre (o crea) ese archivo
tcode --version       # o -v — confirma qué versión quedó instalada
```

`tcode` arranca directo en modo edición: no hay un modo "normal" separado
como en Vim — se escribe y se navega con las flechas/`Home`/`End` desde el
primer momento, con atajos estilo VSCode. `Ctrl+Q` sale — si hay cambios
sin guardar en cualquier archivo abierto (también en pestañas o paneles
que no se están viendo), la primera vez no sale: avisa en la barra de
estado cuántos archivos tienen cambios, y hace falta presionarlo una
segunda vez seguida para salir de verdad; cualquier otra tecla en el
medio cancela esa confirmación pendiente.

## Atajos esenciales

| Atajo | Acción |
|---|---|
| `Ctrl+S` | Guardar (si el buffer no tiene nombre todavía, abre "Guardar como") |
| `Ctrl+Shift+S` (o `Ctrl+K S`) | Guardar como... — elegir/cambiar la ruta del archivo |
| `Ctrl+Z` / `Ctrl+Y` | Deshacer / Rehacer |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copiar / cortar (la selección, o la línea si no hay) / pegar, con el portapapeles del sistema (ver [Copiar, cortar y pegar](#copiar-cortar-y-pegar)) |
| `Ctrl+Q` | Salir |
| `Ctrl+PageDown` / `Ctrl+PageUp` | Pestaña siguiente / anterior (ver [Pestañas](#pestañas-de-archivos-abiertos)) |
| `Ctrl+W` | Cerrar la pestaña activa |
| `Tab` / `Shift+Tab` | Indentar / desindentar |
| `Ctrl+A` | Seleccionar todo |
| `Ctrl+G` | Ir a línea (`n` o `n:col`) |
| `Ctrl+/` (o `Ctrl+K Ctrl+C`) | Comentar/descomentar las líneas del cursor o la selección (ver [Edición de líneas](#edición-de-líneas)) |
| `Alt+↑` / `Alt+↓` | Mover la línea (o las de la selección) arriba/abajo |
| `Shift+Alt+↓` (o `Ctrl+Shift+D`, `Ctrl+K Ctrl+D`) | Duplicar la línea (o las de la selección) debajo |
| `Ctrl+B` | Mostrar/ocultar el explorador de archivos lateral |
| `Ctrl+K J` | Salto rápido en el explorador (etiquetas de una tecla) |
| `Ctrl+P` | Buscar archivo por nombre (difuso) |
| `Ctrl+K .` o `Ctrl+Shift+O` | Ir a un símbolo del archivo (funciones, clases...; ver [Breadcrumbs](#breadcrumbs)) |
| `Ctrl+Shift+P` o `F1` | Paleta de comandos (buscar cualquier acción por nombre) |
| `Ctrl+F` / `Ctrl+H` | Buscar / Buscar y reemplazar en el archivo |
| `Ctrl+Shift+F` (o `Ctrl+K B`) | Buscar (y reemplazar) en todo el proyecto (ver [Buscar en todo el proyecto](#buscar-en-todo-el-proyecto)) |
| `Ctrl+,` o `Ctrl+K A` | Panel de administración |
| `Ctrl+K R` | Ver logs del LSP del lenguaje del archivo activo |
| `F12` (o `Ctrl+K D`) / `Alt+←` (o `Ctrl+K H`) | Ir a la definición / volver (ver [LSP](#navegación-autocompletado-y-renombrar)) |
| `Shift+F12` (o `Ctrl+K U`) | Buscar referencias |
| `Ctrl+K I` | Tipo y documentación del símbolo bajo el cursor (hover) |
| `Ctrl+Espacio` (o `Ctrl+K Espacio`) | Autocompletar |
| `F2` (o `Ctrl+K Shift+R`) | Renombrar símbolo (en la tabla de un CSV, `F2` edita la celda) |
| `Ctrl+K Ctrl+T` | Selector de temas (con preview en vivo) |
| `Ctrl+K Ctrl+L` | Recargar `config.toml`/`keymap.toml` sin reiniciar |

"Guardar como" (y "Guardar" sobre un buffer nuevo, que abre el mismo
prompt) no tiene selector de archivos — es un campo de texto donde se
escribe la ruta destino a mano, con `Enter` para confirmar y `Esc` para
cancelar; también está en la paleta de comandos ("Archivo: Guardar
como...").

Varios atajos con `Ctrl+Shift+<letra>` o símbolos de control (`Ctrl+\`,
`Ctrl+,`) no llegan igual en todos los emuladores de terminal — el keymap
por defecto siempre trae una alternativa con `Ctrl+K <letra>` que funciona
en cualquier terminal clásica (ver la tabla completa en
[`runtime/keymaps/default.toml`](./runtime/keymaps/default.toml), o la
sección "Atajos de teclado" del panel de administración, que muestra
todas las combinaciones activas).

## Multi-cursor y selección

`Shift`+flechas (`Left`/`Right`/`Up`/`Down`/`Home`/`End`) selecciona
texto de la forma de toda la vida: extiende la selección desde donde
estaba el cursor al primer `Shift`+movimiento, sin colapsarla en cada
tecla — mover el cursor sin `Shift` después sí la colapsa, como en
cualquier editor. Funciona igual con varios cursores activos a la vez,
cada uno extendiendo su propia selección de forma independiente.

| Atajo | Acción |
|---|---|
| `Ctrl+D` | Agrega la siguiente ocurrencia de la palabra/selección actual como otro cursor |
| `Ctrl+Shift+L` (o `Ctrl+K L`) | Selecciona todas las ocurrencias a la vez |
| `Ctrl+Alt+↑` / `Ctrl+Alt+↓` | Agrega un cursor en la línea de arriba/abajo, misma columna |

Con varios cursores activos, escribir/borrar/mover el cursor afecta a
todos a la vez — igual que en VSCode o Sublime Text.

## Edición de líneas

| Atajo | Acción |
|---|---|
| `Ctrl+/` (o `Ctrl+K Ctrl+C`) | Comentar / descomentar |
| `Alt+↑` / `Alt+↓` (o `Ctrl+K Shift+↑` / `Ctrl+K Shift+↓`) | Mover línea(s) arriba / abajo |
| `Shift+Alt+↓` (o `Ctrl+Shift+D`, `Ctrl+K Ctrl+D`) | Duplicar línea(s) debajo |
| `Ctrl+A` | Seleccionar todo |
| `Ctrl+G` | Ir a línea: escribir `42` o `42:7` (línea:columna) y `Enter` |

- **Sobre qué líneas actúan**: la del cursor, o todas las que toca la
  selección. Si la selección termina al principio de una línea (lo que
  deja `Shift+↓`), esa línea no cuenta. Con varios cursores, cada uno
  actúa sobre las suyas, y todo se deshace con un solo `Ctrl+Z`.
- **Comentar** usa el comentario del lenguaje según la extensión: `//`
  (Rust, JS/TS, Go, C/C++, Java...), `#` (Python, Ruby, shell, TOML,
  YAML, `Makefile`...), `--` (SQL, Lua), `;`, `%`... El prefijo se
  alinea a la sangría mínima del bloque. Si todas las líneas ya están
  comentadas, las descomenta; si no, las comenta todas. Las líneas en
  blanco no se tocan. HTML, Markdown y XML usan `<!-- ... -->` y CSS
  `/* ... */`, envolviendo **cada línea** por separado. En texto plano,
  JSON o un archivo sin nombre no hace nada y lo avisa en la barra.
- **Mover líneas** respeta el plegado: pasar sobre un bloque plegado lo
  salta entero, y mover la cabecera de un bloque plegado mueve el bloque
  completo, que sigue plegado. En la primera o la última línea no hace
  nada.
- **Duplicar** deja el cursor (y la selección) en la copia de abajo, así
  que repetirlo sigue duplicando.
- **Ir a línea** muestra el rango válido en el título; un número mayor
  va a la última línea, y una columna más allá del final va al final de
  esa línea.
- **Terminales sin protocolo Kitty**: `Ctrl+/` suele llegar como
  `Ctrl+7` o `Ctrl+_`, y las tres funcionan. `Ctrl+Shift+D` llega como
  `Ctrl+D` (agregar ocurrencia): usá `Shift+Alt+↓` o `Ctrl+K Ctrl+D`.
  En macOS, `Alt+↑/↓` necesitan que la terminal mande Option como Meta;
  si no, están los chords con `Ctrl+K` y la paleta ("Editor: ...").
- En modo VIM, los atajos con `Ctrl`/`Alt` funcionan igual en modo
  Normal.

## Copiar, cortar y pegar

| Atajo | Acción |
|---|---|
| `Ctrl+C` | Copia la selección al portapapeles del sistema. Sin selección, copia la línea entera |
| `Ctrl+X` | Igual que `Ctrl+C`, y además borra lo copiado (un solo `Ctrl+Z` lo devuelve) |
| `Ctrl+V` | Pega desde el portapapeles del sistema |

- **Con varios cursores**: se copian las selecciones de todos, en orden y
  unidas por saltos de línea. Si ninguno tiene selección, se copia la
  línea de cada cursor.
- **Líneas enteras, como en VSCode**: lo copiado sin selección, pegado
  con `Ctrl+V`, se inserta **arriba** de la línea del cursor, no en el
  medio.
- **La barra de estado avisa** cuántas líneas se copiaron o cortaron
  ("Copiado: 3 líneas"). Si agrega "(solo dentro de tcode)", lo copiado
  no salió de tcode: el portapapeles está desactivado, o no hubo manera
  de llegar al del sistema.
- **Pegar desde la terminal sigue funcionando**: `Cmd+V` en macOS,
  `Ctrl+Shift+V` en la mayoría de las terminales de Linux, o el clic del
  medio. En esos casos es la terminal la que manda el texto
  (bracketed paste). `Ctrl+V` es útil cuando la terminal no hace
  bracketed paste, o dentro de tmux sin configurar. Si no puede leer el
  portapapeles del sistema, pega lo último que se copió dentro de tcode.
- También están en la paleta de comandos ("Editor: Copiar al
  portapapeles", etc.).

**Cómo llega al portapapeles del sistema.** tcode usa dos caminos a la
vez, y se puede elegir en `Ctrl+,` → Editor → "Portapapeles del sistema"
(`portapapeles` en `[editor]`):

| Modo | Al copiar | Al pegar con `Ctrl+V` |
|---|---|---|
| Automático (por defecto, `automatico`) | OSC 52 + herramienta del sistema | Herramienta del sistema |
| Solo OSC 52 (`solo_osc52`) | OSC 52 | Lo último copiado en tcode |
| Solo sistema (`solo_sistema`) | Herramienta del sistema | Herramienta del sistema |
| Desactivado (`desactivado`) | Nada (solo queda dentro de tcode) | Lo último copiado en tcode |

- **OSC 52** es una secuencia de escape que le pide a la *terminal* que
  guarde el texto en su portapapeles.
  - Funciona también **por SSH**: se llena el portapapeles de tu máquina,
    no el del servidor.
  - La soportan casi todas las terminales modernas: kitty, WezTerm,
    Alacritty, iTerm2 (hay que habilitar "Applications in terminal may
    access clipboard"), Windows Terminal, foot y Ghostty.
  - **Terminal.app de macOS no la soporta.** Ahí se usa `pbcopy`.
  - Los textos de más de ~75 KB no se mandan por OSC 52.
- **Herramientas del sistema**: en macOS, `pbcopy`/`pbpaste`; en Wayland,
  `wl-copy`/`wl-paste`; en X11, `xclip` o `xsel`; en Windows y WSL,
  `clip.exe` y PowerShell `Get-Clipboard`. Se usa la primera que esté
  instalada. Si alguna se cuelga, tcode la corta en medio segundo, así
  que no congela el editor.
- **Dentro de tmux**, para que OSC 52 llegue a la terminal de afuera,
  hace falta esta línea en `~/.tmux.conf`:

  ```
  set -g set-clipboard on
  ```

  Sin ella, en la misma máquina igual funciona por `pbcopy`/`wl-copy`/
  `xclip`. Por SSH + tmux, en cambio, hace falta.
- **La lectura por OSC 52 no se usa.** La mayoría de las terminales la
  bloquean por seguridad.

## Búsqueda y reemplazo

`Ctrl+F` abre la barra de búsqueda; `Ctrl+H` abre además el campo de
reemplazo. Con la barra abierta:

| Atajo | Acción |
|---|---|
| `F3` / `Shift+F3` | Ir a la siguiente / anterior coincidencia |
| `Alt+R` | Alternar modo regex |
| `Alt+C` | Alternar sensibilidad a mayúsculas |
| `Alt+W` | Alternar "palabra completa" |
| `Enter` (en el campo de reemplazo) | Reemplazar la coincidencia actual |

`F3`/`Shift+F3` también funcionan con la barra cerrada, repitiendo la
última búsqueda — igual que en VSCode.

### Buscar en todo el proyecto

`Ctrl+Shift+F` (o `Ctrl+K B`, "Buscar", en terminales sin el protocolo de
teclado de Kitty, donde `Ctrl+Shift+F` llega como `Ctrl+F`; también en la
paleta: "Buscar: En todo el proyecto") abre una vista casi a pantalla
completa con tres campos — **Buscar**, **Reemplazar** y **Archivos** — y
la lista de resultados agrupada por archivo, con el número de línea y la
coincidencia resaltada. Busca mientras escribís: los resultados van
apareciendo a medida que se encuentran (la búsqueda corre de fondo, el
editor nunca se congela) y cada tecla nueva cancela la búsqueda anterior.

| Tecla | Acción |
|---|---|
| `Tab` | Pasar al campo siguiente (Buscar -> Reemplazar -> Archivos) |
| `↑` / `↓`, `PageUp` / `PageDown` | Moverse por los resultados |
| `Enter` | Abrir el archivo del resultado (en una pestaña) con el cursor en la coincidencia |
| `Alt+R` / `Alt+C` / `Alt+W` | Regex / mayúsculas / palabra completa (las mismas de `Ctrl+F`) |
| `Alt+Enter` (o `Ctrl+Alt+Enter`) | Reemplazar todo (pide confirmación con `y`) |
| `Esc` | Cerrar (cancela la búsqueda si seguía) |

- **Qué se busca**: los archivos del proyecto (la carpeta de `Ctrl+P`),
  respetando `.gitignore` y `.ignore` (aunque la carpeta no sea un repo
  git), sin archivos ni carpetas ocultos, sin `target/` ni
  `node_modules/`. Se saltean los binarios, los que no son UTF-8 y los de
  más de 4 MB. Los archivos que tenés abiertos se buscan sobre lo que ves
  en el editor (con los cambios sin guardar), no sobre el disco.
- **Archivos**: globs separados por coma, con la sintaxis de
  `.gitignore`: `*.rs` o `src/**` solo buscan ahí; con `!` adelante
  excluyen (`!tests, !*.md`).
- **Tope**: a las 5000 coincidencias la búsqueda se corta y lo avisa;
  afiná la consulta o el filtro.
- `Enter` esconde la vista sin perderla: `Ctrl+Shift+F` de nuevo vuelve a
  la misma lista con la misma selección, para ir al siguiente resultado.
  La lista no se actualiza sola si después editás: cualquier cambio en
  la consulta, el filtro o las opciones vuelve a buscar.
- **Reemplazar todo** usa el texto del campo Reemplazar tal cual (igual
  que `Ctrl+H`: `$1` no se expande). Antes de hacer nada muestra cuántas
  coincidencias en cuántos archivos y espera `y` (cualquier otra tecla
  cancela). Solo se puede con la búsqueda terminada y sin haber llegado
  al tope. Cada archivo se vuelve a buscar en el momento de reemplazar,
  así que un archivo que cambió después de la búsqueda no se rompe.
  Después:
  - los archivos **abiertos** en alguna pestaña se cambian en el editor,
    sin guardar: se revisan y se guardan con `Ctrl+S`, y un `Ctrl+Z` en
    esa pestaña deshace todo el reemplazo de ese archivo;
  - los archivos **cerrados** se escriben directo a disco, de forma
    segura (a un temporal en la misma carpeta y después se reemplaza el
    original: nunca queda a medio escribir; los finales de línea CRLF se
    conservan). Esto **no** se deshace con `Ctrl+Z` — si el proyecto está
    en git, `git diff`/`git checkout` son la red de seguridad.

## Plegado de bloques

Plegar oculta las líneas de un bloque (el cuerpo de una función, clase o
`impl`, un `if`/`for`, un objeto o array, un comentario de bloque...): su
primera línea queda visible con un marcador ` ... ` al final, y la línea
de cierre (`}`, `end`, `</div>`) queda debajo, igual que en VSCode.

| Atajo | Alternativa | Acción |
|---|---|---|
| `Ctrl+Shift+[` | `Ctrl+K [` | Plegar el bloque más interno que contiene al cursor (repetirlo pliega hacia afuera) |
| `Ctrl+Shift+]` | `Ctrl+K ]` | Desplegar el bloque del cursor |
| `Ctrl+K Ctrl+0` | `Ctrl+K 0` | Plegar todo |
| `Ctrl+K Ctrl+J` | | Desplegar todo |

Las alternativas son para terminales sin el protocolo de teclado de
Kitty, donde `Ctrl+Shift+[` llega como `Esc` y `Ctrl+0` como un `0`
suelto. Los cuatro comandos también están en la paleta (`Ctrl+Shift+P`,
"Plegado: ...").

- Las flechas saltan los bloques plegados; `Shift`+flecha selecciona el
  bloque entero.
- Editar dentro de un bloque plegado (o borrarlo con una selección) lo
  despliega; editar más arriba lo corre junto con su código. Escribir en
  la primera línea del bloque no lo despliega.
- Buscar (`Ctrl+F`/`F3`) una coincidencia que está adentro de un bloque
  plegado lo despliega.
- Qué se puede plegar sale del árbol de sintaxis (tree-sitter) en todos
  los lenguajes con resaltado, salvo Markdown, que no tiene plegado. En
  archivos sin lenguaje reconocido (texto plano, YAML, TOML...) se pliega
  por indentación: una línea seguida de otras más indentadas.
- El plegado es de cada documento abierto y **se recuerda entre
  sesiones**: al cerrar la pestaña (`Ctrl+W`), el panel (`Ctrl+K F`) o
  tcode (`Ctrl+Q`), los bloques plegados de cada archivo se guardan, y
  al volver a abrirlo aparecen plegados igual. Si el archivo cambió por
  fuera mientras tanto (otro editor, `git checkout`...), arranca todo
  desplegado en vez de plegar líneas equivocadas; lo mismo si se cerró
  descartando cambios sin guardar (se conserva lo que había guardado de
  antes). Se guarda en un archivo de estado aparte de la configuración
  — `~/.local/state/tcode/estado/pliegues.toml` en Linux,
  `~/Library/Application Support/tcode/estado/pliegues.toml` en macOS,
  `%LOCALAPPDATA%\tcode\estado\pliegues.toml` en Windows —, con los
  últimos 200 archivos; se puede borrar sin problema.

## Paneles divididos (splits)

| Atajo | Acción |
|---|---|
| `Ctrl+\` (o `Ctrl+K \`) | Dividir el panel activo verticalmente |
| `Ctrl+K Ctrl+\` (o `Ctrl+K -`) | Dividir horizontalmente |
| `Ctrl+1` / `Ctrl+2` / `Ctrl+3` (o `Ctrl+K 1`/`2`/`3`) | Saltar al panel 1/2/3 |
| `Ctrl+K F` | Cerrar el panel activo (con todas sus pestañas; si alguna tiene cambios sin guardar, pide repetirlo) |
| `F11` (o `Ctrl+K G`) | Maximizar/restaurar el panel activo |

Cada panel tiene sus propias pestañas (ver
[Pestañas](#pestañas-de-archivos-abiertos)), y cada archivo abierto su
propio cursor y estado de vista (tabla CSV, preview Markdown) — son
independientes entre sí. Un panel recién dividido arranca con un
"[Sin nombre]" vacío, que el primer archivo que se abra ahí reemplaza.

**Maximizar** (`F11`, la "pantalla completa" de tcode) funciona como el
zoom de tmux: el panel activo ocupa toda el área de edición y la barra de
estado muestra `[MAX]`; al volver a apretarlo los paneles vuelven
exactamente como estaban. Cambiar de panel, dividir o cerrar mientras
está maximizado sale del maximizado primero. Con un solo panel no hace
nada. En muchas terminales (y en macOS, que lo usa para "mostrar
escritorio") `F11` no llega a tcode: `Ctrl+K G` hace lo mismo. El tamaño
de letra y la pantalla completa de la ventana los maneja el emulador de
terminal, no tcode.

### Modo zen (`Ctrl+K Z`)

Oculta de un golpe todo lo que no es código — explorador, barra de
pestañas, breadcrumbs y barra de estado — y otra vez `Ctrl+K Z` lo deja
todo como estaba, incluido el foco (si estabas en el explorador, volvés
ahí). Es solo para la sesión: no cambia ninguna opción de la
configuración ni se recuerda al volver a abrir tcode. La paleta, el buscador de archivos, la búsqueda y demás
ventanas flotantes funcionan igual en zen. Lo que necesita ver el
explorador (`Ctrl+B`, `Ctrl+K J`, crear/renombrar) sale del zen primero.
Se combina con maximizar: zen + `F11` deja solo el panel activo en toda
la pantalla.

### Breadcrumbs

Arriba del código de cada panel (debajo de la barra de
[pestañas](#pestañas-de-archivos-abiertos)) hay una fila con dónde está
el cursor: la ruta del archivo relativa a la raíz del proyecto (la carpeta del repo
git que lo contiene, o el directorio desde el que abriste tcode si no hay
repo) seguida de los símbolos que encierran al cursor, del más externo al
más interno:

```
crates > core > src > editor.rs > impl Editor > fn insertar_texto
```

Reconoce funciones, métodos, clases, structs, `impl`, traits,
interfaces, enums, módulos y namespaces en Rust, Python,
JavaScript/TypeScript, Go, Java, C/C++, C#, Kotlin, Ruby y PHP, y los
encabezados en Markdown (`# Manual > ## Atajos`). En los demás archivos
(texto plano, HTML, CSS, SQL...) y en la vista de tabla CSV muestra solo
la ruta. Si no entra en el ancho del panel, se recorta de a poco: primero
las carpetas del medio (`..`), después los símbolos de afuera, y por
último el final del símbolo más interno — el nombre del archivo y el
símbolo más interno siempre quedan a la vista. Se apaga en `Ctrl+,` →
Interfaz → "Mostrar breadcrumbs" (viene prendido) y se oculta en modo
zen. Muestra siempre la ubicación del documento de la pestaña activa.

**Ir a un símbolo** (`Ctrl+K .`, o `Ctrl+Shift+O` en terminales con el
protocolo de teclado de Kitty; en la paleta: "Ir: Símbolo del
archivo"): abre una lista con todos los símbolos del archivo (los mismos
que muestra el breadcrumb: funciones, métodos, clases, `impl`...), en el
orden del archivo, indentados según su anidamiento y con su número de
línea. Arranca posicionada en el símbolo donde está el cursor. Escribir
filtra (difuso, como `Ctrl+P`, pero sin reordenar), `↑`/`↓` + `Enter`
salta al símbolo — desplegando el bloque si estaba plegado — y `Esc`
cierra sin moverse. En archivos sin símbolos la lista lo avisa. No hace
nada en la vista de tabla CSV ni con el foco en el explorador.

## Pestañas de archivos abiertos

Abrir un archivo (buscador `Ctrl+P`, explorador, salto rápido) lo agrega
como pestaña nueva en el panel activo, justo a la derecha de la que se
estaba viendo, en vez de reemplazarla. Si ese archivo ya estaba abierto
en ese panel, solo se va a su pestaña (sin volver a leerlo de disco ni
perder lo que tenga sin guardar). Cada pestaña conserva todo lo suyo al
cambiar: cursor, scroll, deshacer/rehacer, pliegues, diagnósticos del
LSP, marcas de git y vista Markdown/CSV.

| Atajo | Acción |
|---|---|
| `Ctrl+PageDown` (o `Ctrl+K PageDown`) | Pestaña siguiente (de la última vuelve a la primera) |
| `Ctrl+PageUp` (o `Ctrl+K PageUp`) | Pestaña anterior |
| `Alt+1` ... `Alt+9` | Ir a la pestaña 1...9 del panel activo |
| `Ctrl+W` | Cerrar la pestaña activa |

La barra de pestañas va arriba del código de cada panel: el nombre del
archivo (con la carpeta delante si hay dos con el mismo nombre en el
panel, p. ej. `a/mod.rs` y `b/mod.rs`), un `*` si tiene cambios sin
guardar, y la activa resaltada (en negrita en el panel con el foco). Si no
entran todas, la barra se corre para que siempre se vea la activa, con
`<`/`>` en los bordes indicando que hay más de ese lado. Se muestra
también con una sola pestaña (así se ve qué archivo tiene cada panel);
se puede ocultar en `Ctrl+,` → Interfaz → "Mostrar pestañas" — los
atajos siguen andando igual sin la barra. El modo zen también la oculta.

- **Cerrar con cambios sin guardar**: `Ctrl+W` no cierra, avisa en la
  barra de estado; otro `Ctrl+W` seguido cierra descartando los cambios.
  Cualquier otra tecla en el medio cancela.
- **Cerrar la última pestaña de un panel**: si hay más paneles, se cierra
  el panel; si es el único, queda un "[Sin nombre]" vacío.
- Algunas terminales (GNOME Terminal, Windows Terminal) usan
  `Ctrl+PageDown`/`Ctrl+PageUp` para sus propias pestañas y no se los
  pasan a tcode: `Ctrl+K PageDown`/`Ctrl+K PageUp` hacen lo mismo en
  cualquier terminal. `Alt+<número>` necesita que la terminal mande
  `Alt`/`Option` como Meta (en macOS: iTerm2 → Profiles → Keys → "Left
  Option key: Esc+"; Terminal.app → "Usar Option como tecla Meta"); si
  no, "Pestañas: Ir a la pestaña N" está en la paleta (`F1`).
- Guardado automático "al perder foco": cambiar de pestaña cuenta como
  perder el foco, y se guardan también las pestañas que no se ven.
- LSP: hay un servidor por lenguaje, compartido por todas las pestañas y
  paneles. Alternar entre un `.py` y un `.rs` (o un `.txt`) no reinicia
  ninguno, y una pestaña de fondo sigue recibiendo sus diagnósticos. El
  servidor de un lenguaje se cierra al cerrar la última pestaña de ese
  lenguaje.

## Explorador de archivos

`Ctrl+B` lo muestra/oculta a la izquierda. Con el foco ahí: `↑`/`↓` mueve
la selección, `Enter` sobre una carpeta la expande/colapsa, `Enter` sobre
un archivo lo abre y devuelve el foco al editor. `Esc` devuelve el foco al
editor sin cerrar el explorador.

### Salto rápido (`Ctrl+K J`)

Con muchos archivos visibles, `Ctrl+K J` muestra una etiqueta de una sola
tecla (`1`, `2`, `3`... y después `a`, `b`, `c`...) junto a cada fila:
tipear la que corresponde abre ese archivo directamente (o expande esa
carpeta), sin navegar con las flechas — funciona incluso con el
explorador oculto (lo muestra y le da el foco solo). `Esc` cancela sin
saltar a ningún lado.

No existe una versión "mantener Alt/Cmd presionado" al estilo de una app
nativa: ninguna terminal entrega un evento cuando se sostiene solo una
tecla modificadora (sin otra tecla acompañándola), sin importar cuál se
elija — es una limitación del protocolo de teclado de cualquier
terminal, no algo particular de `tcode`.

### Crear, renombrar y borrar

| Atajo | Acción |
|---|---|
| `Ctrl+K N` | Nuevo archivo (dentro de la carpeta seleccionada, o de la que contiene al archivo seleccionado) |
| `Ctrl+K C` | Nueva carpeta (mismo destino que "Nuevo archivo") |
| `Ctrl+K M` | Renombrar la selección actual (precargado con el nombre actual) |
| `Delete` (con el explorador enfocado) | Borrar la selección actual — siempre pide confirmación primero |

Los tres primeros son globales: si el explorador está oculto, lo
muestran y le dan el foco antes de abrir el prompt, igual que `Ctrl+K
J`. Borrar es irreversible (no hay papelera de reciclaje) — el prompt de
confirmación solo acepta `y`/`Y`; cualquier otra tecla, incluido `Enter`,
cancela sin tocar el disco.

## Modo VIM opcional

Apagado por defecto: `tcode` sigue funcionando exactamente igual que
siempre (atajos estilo VSCode/Helix). Quien quiera movimientos al estilo
VIM lo prende en el panel de administración (`Ctrl+K A`, sección
"Editor" → "Modo VIM") o a mano en `config.toml`
(`[editor]` / `modo_vim = true`) — el cambio surte efecto de inmediato
sobre el panel activo, sin reiniciar ni reabrir el archivo.

Hay modo Normal, Insertar, Visual (`v`) y Visual por líneas (`V`) — la
barra de estado muestra cuál. Los comandos siguen la gramática de VIM:
`[conteo] operador [conteo] movimiento-u-objeto` (`d3w`, `2dd`, `ci(`,
`y$`...), `[conteo] movimiento` (`5j`) o `[conteo] comando` (`3x`). Lo que
se lleva escrito de un comando a medias (`d2`, `ci`) se ve en la barra de
estado; `Esc` lo cancela.

| Movimiento | Qué hace |
|---|---|
| `h` / `j` / `k` / `l` | Izquierda / abajo / arriba / derecha (`h`/`l` no cambian de línea) |
| `0` / `^` / `$` | Inicio de línea / primer carácter no blanco / fin de línea |
| `w` / `b` / `e` | Próxima palabra / palabra anterior / fin de palabra |
| `W` / `B` / `E` | Lo mismo con palabras separadas solo por blancos |
| `gg` / `G` / `{n}G` | Inicio del archivo / fin del archivo / línea `n` |
| `f{c}` / `t{c}` | Hasta el carácter `c` en la línea / justo antes de él |
| `F{c}` / `T{c}` | Lo mismo hacia atrás |
| `;` / `,` | Repetir el último `f`/`t`/`F`/`T` / al revés |
| `%` | Al paréntesis, llave o corchete que corresponde |
| `{` / `}` | Párrafo anterior / siguiente (líneas vacías) |

| Operador (seguido de movimiento u objeto; repetido, línea entera) | Qué hace |
|---|---|
| `d` (`dd`) | Borrar (queda en el registro) |
| `c` (`cc`) | Cambiar: borrar y entrar a Insertar |
| `y` (`yy`) | Copiar al registro |
| `>` / `<` (`>>` / `<<`) | Indentar / desindentar líneas (con los espacios o el tab de la config) |

| Objeto de texto (tras un operador, o en Visual) | Qué cubre |
|---|---|
| `iw` / `aw` (`iW` / `aW`) | La palabra / la palabra con sus espacios |
| `i"` / `a"` (también `'` y `` ` ``) | Lo de adentro de las comillas / con las comillas |
| `i(` / `a(` (también `ib`, `i)`) | Lo de adentro de los paréntesis / con los paréntesis |
| `i{` / `a{` (también `iB`), `i[` / `a[`, `i<` / `a<` | Igual con llaves, corchetes y `<>`; multilínea |

| Comando | Qué hace |
|---|---|
| `i` / `a` | Insertar antes / después del cursor |
| `I` / `A` | Insertar al principio (primer no blanco) / al final de la línea |
| `o` / `O` | Abrir una línea debajo / arriba (con la misma indentación) e insertar |
| `x` / `X` | Borrar el carácter bajo el cursor / el anterior |
| `s` / `S` | Cambiar el carácter / la línea entera |
| `D` / `C` / `Y` | `d$` / `c$` / `yy` |
| `r{c}` | Reemplazar el carácter (con conteo, varios) por `c` |
| `J` | Unir con la línea siguiente |
| `~` | Alternar mayúscula/minúscula |
| `p` / `P` | Pegar el registro después / antes (debajo / arriba si son líneas) |
| `u` | Deshacer (comparte historial con `Ctrl+Z`) |
| `.` | Repetir el último cambio, incluido el texto tipeado |
| `v` / `V` | Modo Visual por caracteres / por líneas |
| `Esc` | En Insertar, volver a Normal; en Visual, salir sin hacer nada |

En Visual, los movimientos y objetos de texto extienden la selección; `o`
cambia de extremo; `d`/`x`, `y`, `c`/`s`, `>`/`<`, `J` y `~` operan sobre
ella.

| Línea de comandos (`:`) | Qué hace |
|---|---|
| `:w` | Guardar (mismo camino que `Ctrl+S`, formatea si corresponde) |
| `:q` / `:q!` | Cerrar la pestaña (con cambios, pide repetir `:q`) / sin preguntar; en la última pestaña sale de tcode |
| `:wq` / `:x` | Guardar y cerrar (`:x` solo guarda si hay cambios) |
| `:qa` / `:qa!` | Salir de tcode (avisa si hay cambios) / sin preguntar |
| `:{n}` | Ir a la línea `n` |
| `:e <ruta>` | Abrir un archivo en una pestaña nueva |
| `:s/a/b/` / `:%s/a/b/g` | Reemplazar en la línea / en todo el archivo (`g`: todas por línea, `i`: sin distinguir mayúsculas; patrón regex de Rust) |

La línea `:` aparece al pie de la pantalla; `↑`/`↓` recorren los
comandos ya usados en la sesión, `Esc` la cierra.

Cada cambio compuesto se deshace de una vez: `3dd`, `J`, `:%s`, y también
`cw` + lo que se escribió hasta `Esc`. El registro sin nombre es uno solo
para toda la app, no por panel — yanquear en un archivo y pegar en otro
funciona, igual que en VIM real — y recuerda si guardó líneas enteras o
caracteres sueltos. El resto de atajos de tcode (flechas, `Ctrl+S`,
`Ctrl+B`, splits, etc.) siguen andando igual en cualquier modo.

Como en VIM real, en modo Normal el cursor nunca queda "después" del
último carácter de una línea no vacía (`$`/`l` se detienen sobre él), y
al salir de Insertar vuelve un lugar a la izquierda.

**Portapapeles del sistema en modo VIM.** `y`/`d`/`c`/`x` y `p` siguen
usando el registro interno, así que yanquear no pisa lo que tenías
copiado de otra app. Tres maneras de usar el portapapeles del sistema:

- **`"+` (o `"*`) delante de un comando**: `"+yy` y `"+y` en Visual
  copian al portapapeles. `"+p`/`"+P` pegan desde el portapapeles, sin
  tocar el registro sin nombre.
- **Sincronizar siempre**: prendiendo `Ctrl+,` → Editor → "VIM: registro
  = portapapeles" (`vim_sincronizar_portapapeles = true`), todo lo que
  yanqueás o borrás va al portapapeles, y `p` pega desde ahí. Equivale
  al `clipboard=unnamedplus` de Neovim.
- **`Ctrl+C`/`Ctrl+X`/`Ctrl+V`**, que andan igual en cualquier modo.

El texto que llega de otra app se pega por líneas si termina en salto
de línea.

No hay otros registros con nombre (`"a`, etc.; tcode avisa), ni marcas,
macros o búsqueda con `/` (para buscar, `Ctrl+F` sigue funcionando); `.` no repite operaciones hechas en Visual.
Ver PRUEBAS.md para la lista completa de limitaciones.

## Paleta de comandos y buscador de archivos

- **`Ctrl+Shift+P`/`F1`**: paleta de comandos — buscá cualquier acción por
  nombre en español ("Selección: Seleccionar todas las ocurrencias",
  "Tema: Seleccionar", etc.) en vez de memorizar el atajo.
- **`Ctrl+P`**: buscador difuso de archivos del proyecto — escribí
  fragmentos del nombre/ruta, no hace falta el camino completo ni el
  orden exacto de las letras.

Ambos se cierran con `Esc` y se navegan con `↑`/`↓` + `Enter`.

## Vistas especiales: Markdown y CSV

- **Markdown** (`.md`): `Ctrl+K V` alterna entre ver solo la fuente y
  verla dividida junto al preview renderizado; `Ctrl+Shift+V` alterna
  directo entre solo fuente y **solo** preview (a pantalla completa). El
  preview resalta encabezados, listas, tablas, código con sintaxis
  coloreada y más.
- **CSV/TSV** (`.csv`/`.tsv`): se abre directo como una tabla con
  columnas alineadas y encabezado fijo; `Ctrl+K T` alterna a texto plano
  y viceversa. En modo tabla, `↑↓←→` mueven la celda seleccionada,
  `Tab`/`Shift+Tab` saltan a la celda siguiente/anterior (en vez de
  indentar) y `Enter`/`F2` empiezan a editar la celda actual.

En modo tabla también podés ordenar, filtrar, insertar/eliminar filas y
columnas y ajustar el ancho de las columnas. Todos son chords con
`Ctrl+K` (y están en la paleta como "CSV: ..."); fuera de la vista de
tabla no hacen nada.

| Atajo | Acción |
|---|---|
| `Ctrl+K O` | Ordenar las filas por la columna seleccionada; repetirlo alterna ascendente/descendente. Detecta sola si la columna es numérica (si no, ordena como texto sin distinguir mayúsculas ni tildes). El encabezado no se mueve y las celdas vacías quedan al final |
| `Ctrl+K /` | Filtrar: muestra solo las filas cuya celda en la columna seleccionada contiene el texto escrito (sin distinguir mayúsculas ni tildes). Una barra al pie indica el filtro activo |
| `Esc` (con un filtro activo) | Quitar el filtro (también: `Enter` con el prompt de filtro vacío, o "CSV: Quitar filtro") |
| `Ctrl+K ↓` / `Ctrl+K ↑` | Insertar una fila vacía debajo / arriba de la seleccionada |
| `Ctrl+K →` / `Ctrl+K ←` | Insertar una columna vacía a la derecha / izquierda de la seleccionada |
| `Ctrl+K E` / `Ctrl+K Shift+E` | Eliminar la fila / la columna seleccionada |
| `Ctrl+K Shift+→` / `Ctrl+K Shift+←` | Ensanchar / angostar la columna seleccionada (de a 2) |
| `Ctrl+K W` | Volver la columna seleccionada a su ancho automático |

Ordenar e insertar/eliminar **modifican el archivo** (cada una se
deshace con un solo `Ctrl+Z`); filtrar y el ancho de columna son solo de
vista y no cambian nada en disco. Con un filtro activo, editar una celda
edita la fila correcta del archivo; insertar una fila quita el filtro
primero (si no, la fila nueva, vacía, quedaría oculta). Ordenar conserva
el texto original de cada fila tal cual; insertar/eliminar una
**columna** reescribe todas las filas con el quoting mínimo necesario
(se citan solo las celdas con el delimitador, comillas o saltos de
línea).

## Temas

`tcode` trae 13 temas incluidos (Dracula, Monokai, One Dark, Nord,
Gruvbox Dark, Tokyo Night, Catppuccin Mocha, Solarized Dark/Light,
GitHub Light, "Alto contraste", más dos temas simples "Oscuro"/"Claro"
de las primeras versiones).

- **`Ctrl+K Ctrl+T`**: selector de temas con **preview en vivo** —
  navegá la lista con `↑`/`↓` y el editor se recolorea al instante;
  `Enter` confirma, `Esc` vuelve al tema anterior. `Tab` cicla el filtro
  `Todos` → `Oscuro` → `Claro` → `Alto contraste` → `Todos`.
- **"Alto contraste"**: negro puro + colores primarios saturados, sin
  tonos intermedios en ningún lado — pensado para quien necesita la
  máxima diferencia perceptible entre elementos (al estilo de los temas
  "High Contrast" de VS Code/Windows), no para verse "lindo".
- **`Ctrl+K Ctrl+P`** (o `Ctrl+K P`): editor visual de tema — ajustá
  cualquiera de los ~29 colores del tema activo por código hex, eligiendo
  de una paleta predefinida, o afinando tono/saturación/luminosidad
  (HSL) con las flechas. Los cambios se ven en vivo sobre un panel de
  ejemplo antes de guardar.
- Para editar un tema de fondo hasta el final (o compartirlo): la fila
  "Duplicar tema activo" de la sección "Temas" del panel de
  administración lo copia a
  `~/.config/tcode/themes/<tema>-mio.toml` (o el directorio de config
  equivalente de tu SO) — un archivo TOML plano, fácil de editar a mano
  o pasarle a alguien más.
- **Importar un tema de otra persona**: dejá su archivo `.toml` en esa
  misma carpeta de temas (con cualquier nombre de archivo que no sea uno
  de los 13 incluidos) y va a aparecer solo en el selector (`Ctrl+K
  Ctrl+T`), con su nombre y filtro (claro/oscuro/alto contraste) reales
  — no hace falta reiniciar `tcode` ni editar `config.toml` a mano.
- **Usar un tema de Helix**: los `.toml` de tema del editor Helix
  (`runtime/themes/` de su repo, o cualquiera de la comunidad) también
  sirven tal cual: copialo a esa misma carpeta de temas y aparece en el
  selector como "<archivo> (Helix)", con el filtro claro/oscuro según su
  color de fondo. `tcode` lo convierte al vuelo (resuelve `[palette]`,
  los colores ANSI por nombre y `inherits` si el tema padre también está
  en la carpeta; si no está, lo que falte sale del tema "Oscuro" o
  "Claro" de `tcode`) y nunca modifica el archivo. Helix tiene muchos
  más colores que `tcode`: se usan el fondo, el texto, cursor,
  selección, números de línea, línea actual, statusbar, los tokens de
  sintaxis principales, diagnósticos y los colores de diff; el resto
  (menús, popups, markup, subrayados) se ignora. Para retocarlo,
  "Duplicar tema activo" o `Ctrl+K Ctrl+P` crean una copia
  `<archivo>-mio.toml` ya en formato `tcode`.

## Panel de administración

`Ctrl+,` (o `Ctrl+K A`) abre una vista a pantalla completa con 5
secciones navegables desde la barra lateral (`↑`/`↓` + `Enter`/`→` para
entrar, `Tab` para volver a la barra, `Ctrl+F` para buscar cualquier
opción por nombre):

| Sección | Qué permite |
|---|---|
| **Atajos de teclado** | Ver/rebindear cualquier atajo (`Enter` sobre un comando y presionar la nueva combinación), con detección de conflictos resaltada en rojo. `Backspace` restablece uno solo al valor por defecto; hay una fila para restablecer todos. También exportar/importar el `keymap.toml` activo a/desde un archivo fijo (ver [Personalizar atajos](#personalizar-atajos)). |
| **Temas** | Elegir tema (abre el selector con preview) y duplicar el activo para editarlo. |
| **Lenguajes / LSP** | Habilitar/deshabilitar el servidor LSP de cada lenguaje, ver si el binario está en el `PATH` y el estado de la sesión de cada lenguaje (Conectado/Iniciando/Error/Inactivo). `c` sobre una fila edita el comando+argumentos a mano (ver [LSP](#lsp-autocompletado-y-diagnósticos)); `Backspace` quita ese override. |
| **Editor** | Tamaño de tabulación, espacios vs. tabs, ajuste de línea, números de línea, indicadores de git, modo VIM, regla vertical, guardado automático, portapapeles del sistema (ver [Copiar, cortar y pegar](#copiar-cortar-y-pegar)). |
| **Interfaz** | Mostrar/ocultar la barra de estado y cada uno de sus elementos (posición del cursor, codificación, fin de línea, lenguaje, diagnósticos, modo), la barra de pestañas y los [breadcrumbs](#breadcrumbs) de arriba del código. |

Todos los cambios se aplican y persisten al instante en `config.toml`, sin
tocar ni reiniciar nada más.

**Ajuste de línea** (sección "Editor"): con el toggle activo, cualquier
línea más ancha que la terminal se parte en varias filas de pantalla en
vez de recortarse — sin buscar el espacio más cercano (ajuste por
carácter, no por palabra). `↑`/`↓`/`Home`/`End` se siguen moviendo por
línea lógica completa, no por fila de pantalla. No tiene efecto en la
mitad "fuente" de la vista Markdown dividida (`Ctrl+K V`) — esa mitad
comparte el desplazamiento vertical con el preview de al lado, que no
sabe de filas partidas; funciona normal viendo el mismo archivo sin
dividir.

Con archivos muy grandes y MUCHOS errores de sintaxis para su lenguaje
(p. ej. código de otro lenguaje guardado con la extensión equivocada),
volver a analizar el archivo después de cada tecla puede tardar más de
un segundo: en ese caso `tcode` corta el análisis a los 250 ms, sigue
con los colores anteriores y lo reintenta cada 2 segundos — tipear
sigue fluido, y los colores de lo recién editado pueden quedar
aproximados un momento.

**Regla vertical** (sección "Editor"): marca una columna fija del código
con un fondo distinto — la guía de ancho de línea de siempre (80/100/
120...). Apagada por defecto; `→`/`Enter` sobre la fila la prende en la
columna 80, `←`/`→` la ajustan de a uno (entre 20 y 300), y bajarla por
debajo de 20 la apaga de nuevo. Es relativa a la fila de pantalla, no a
la línea lógica: con ajuste de línea activo se ve en la misma columna en
todas las filas de una línea partida. El color se calcula a partir del
tema activo (no hace falta que un tema lo declare para que se vea bien).

**Indicadores de git** (sección "Editor", prendidos por defecto): si el
archivo está trackeado en un repo git, una columna angosta entre los
números de línea y el código marca qué cambió respecto del último commit
(`HEAD`), y se actualiza mientras se escribe (no hace falta guardar):

| Marca | Significa | Color (sección `[git]` del tema) |
|---|---|---|
| `+` | Línea agregada | `added` |
| `~` | Línea modificada | `modified` |
| `-` | Justo antes de esta línea había líneas que se borraron (al final del archivo, va en la última línea) | `deleted` |

Detalles:

- Hace falta el comando `git` instalado y en el `PATH`; `tcode` lo usa
  una vez al abrir cada archivo para leer su versión en `HEAD` (en
  segundo plano, sin frenar nada). Sin `git`, o con un archivo fuera de
  un repo o todavía sin trackear (nunca commiteado), simplemente no hay
  columna ni marcas.
- La versión de referencia se vuelve a leer al abrir el archivo, al
  guardar (`Ctrl+S`) y sola cuando cambia `HEAD`: tras un commit,
  checkout o reset hecho desde otra terminal, las marcas se ponen al día
  en un par de segundos sin tocar nada (tcode mira cada 2 segundos las
  fechas de `.git/HEAD`, de la rama y del índice — sin lanzar procesos —,
  y también al volver a la terminal si esta avisa del foco; en tmux hace
  falta `set -g focus-events on`). Con archivos fuera de un repo no se
  revisa nada.
- Con los números de línea apagados, la columna de git se sigue viendo
  sola (si el archivo está en un repo); para ocultarla está este mismo
  toggle.

**Guardado automático** (sección "Editor"): `Nunca` (por defecto), `Al
perder foco` o `Cada N segundos` — `←`/`→`/`Enter` recorren los tres, y
la fila de abajo ajusta los segundos de a 5 (entre 5 y 600, 30 por
defecto). Solo guarda archivos con nombre y con cambios; un "[Sin
nombre]" hay que guardarlo a mano la primera vez (`Ctrl+S`). "Al perder
foco" guarda al cambiar de panel, al abrir otro archivo en el mismo
panel, al pasar al explorador y — si la terminal avisa de eso — al
cambiar a otra ventana (en tmux hace falta `set -g focus-events on`).
Si un guardado falla (sin permiso de escritura, carpeta borrada...) el
motivo aparece en la barra de estado como `ERROR: ...` y los cambios
siguen en el editor; se reintenta en el próximo guardado. En
`config.toml`: `guardado_automatico = "nunca" | "al_perder_foco" |
"cada_n_segundos"` y `segundos_guardado_automatico = 30` bajo `[editor]`.

## LSP: autocompletado y diagnósticos

`tcode` no instala servidores LSP por vos — si el binario correspondiente
está en el `PATH`, se lanza solo al abrir un archivo de ese lenguaje y los
diagnósticos (errores/avisos) aparecen subrayados en el código y resumidos
en la barra de estado.

Hay un servidor por lenguaje, corriendo en paralelo: con un `.py` y un
`.rs` abiertos (en pestañas del mismo panel o en paneles distintos) corren
los dos a la vez, cada uno con todos los archivos de su lenguaje abiertos
— no solo el visible —, así que cambiar de pestaña no los reinicia y los
errores de una pestaña de fondo ya están al volver a ella. El servidor de
un lenguaje se lanza con el primer archivo de ese lenguaje y se cierra al
cerrar el último. Si uno no arranca o se cae, los demás siguen
funcionando; su fila en "Lenguajes / LSP" dice "Error" y `Ctrl+K R`
muestra por qué. No se reintenta solo: se vuelve a lanzar al cambiar su
comando, al deshabilitarlo y volver a habilitarlo, o al cerrar todos sus
archivos y abrir uno de nuevo.

Comando por defecto conocido para estos lenguajes (instalá el paquete
correspondiente para que funcione):

| Lenguaje | Servidor LSP por defecto |
|---|---|
| Python | `pyright-langserver --stdio` |
| TypeScript | `typescript-language-server --stdio` |
| C / C++ | `clangd` |
| Ruby | `solargraph stdio` |
| PHP | `intelephense --stdio` |
| Kotlin | `kotlin-language-server` |
| HTML | `vscode-html-language-server --stdio` |
| CSS | `vscode-css-language-server --stdio` |
| SQL | `sqls` |

Rust, JavaScript, Go, Markdown, Java y C# tienen resaltado de sintaxis
completo pero **sin comando por defecto** — Java (`jdtls`) y C#
(`omnisharp`) necesitan un directorio de proyecto como argumento que no
hay forma de adivinar de antemano, y los otros simplemente no tienen uno
todavía. Para cualquiera de estos (o para apuntar a un comando distinto
del que trae por defecto, como una versión instalada en otra ruta):
sección "Lenguajes / LSP" del panel de administración, `c` sobre la fila
del lenguaje, escribí el comando completo con sus argumentos y `Enter`.
Si ya hay una sesión para ese lenguaje, se relanza sola con el comando
nuevo (solo esa: los servidores de otros lenguajes siguen como estaban).

Ese mismo campo acepta variables de entorno propias del servidor: antes
del comando, escribí `VAR=valor` (una o más, separadas por espacio) y
después un `--` suelto, por ejemplo
`RUST_LOG=debug -- rust-analyzer --stdio`. Se suman al entorno que
`tcode` ya tiene (no lo reemplazan), y la fila muestra
`[+N var(s) de entorno]` junto al comando cuando hay alguna configurada.
Volver a editar la fila con `c` precarga la línea completa, variables
incluidas.

Si el servidor lo soporta (pyright, por ejemplo), `tcode` le manda solo
lo que cambió en cada edición en vez del archivo entero — con archivos
de miles de líneas, tipear no se frena por el LSP.

### Navegación, autocompletado y renombrar

Si el servidor del lenguaje lo soporta (lo anuncia al arrancar; si no,
el atajo solo deja un aviso corto en la barra de estado, como "el LSP no
soporta renombrar"):

| Atajo | Acción |
|---|---|
| `F12` o `Ctrl+K D` | **Ir a la definición** del símbolo bajo el cursor. Si está en otro archivo, se abre en una pestaña nueva (o se activa la que ya lo tenía). Con varias definiciones, lista para elegir (se filtra escribiendo, `Enter` salta). |
| `Alt+←` o `Ctrl+K H` | **Volver** a donde estaba el cursor antes del último salto (se recuerdan los últimos 50; también cuenta saltar desde las referencias). En macOS, `Alt+←` necesita que la terminal mande Option como Meta. |
| `Shift+F12` o `Ctrl+K U` | **Buscar referencias**: lista de `archivo:línea  código` (incluida la declaración), filtrable, `Enter` salta. |
| `Ctrl+K I` | **Hover**: tipo y documentación del símbolo bajo el cursor en un recuadro debajo (el markdown se muestra como texto, hasta 20 líneas). Se cierra con cualquier tecla. |
| `Ctrl+Espacio` o `Ctrl+K Espacio` | **Autocompletar** a mano. |
| `F2` o `Ctrl+K Shift+R` | **Renombrar símbolo**: pide el nombre nuevo (precargado con el actual), `Enter` confirma. |

**Autocompletado**: además de a mano, la lista aparece sola al tipear un
carácter de disparo del servidor (el `.` de un método, `::` en Rust) o
al hacer una pausa corta en medio de un nombre. Sale debajo del cursor
con el nombre, el tipo (`fn`, `método`, `var`, `clase`...) y la firma;
seguir escribiendo la filtra, `↑`/`↓` eligen, `Tab` o `Enter` aceptan y
`Esc` la cierra (cualquier otra tecla que no sea parte del nombre
también). Aceptar reemplaza lo escrito de la palabra por el item (más un
`use`/`import` si el servidor lo agrega) en un solo paso: un `Ctrl+Z` lo
deshace entero. Sin la lista abierta, `Tab` indenta como siempre. Los
snippets se insertan como texto plano: `tcode` no tiene saltos entre
placeholders, así que `foo(${1:a})` queda `foo(a)` y el cursor al final
(igual, `tcode` le pide al servidor texto sin snippets, y rust-analyzer y
pyright lo respetan). Solo con un cursor (no con multi-cursor) y, en modo
VIM, en modo Insertar. Pedir la lista nunca frena el tipeo: se pide en
segundo plano y, si llega cuando ya se siguió escribiendo otra cosa, se
descarta.

**Renombrar** aplica los cambios del servidor en todos los archivos que
toque: los que ya están abiertos (en cualquier pestaña o panel) se editan
ahí, un paso de deshacer por archivo; los que no, **se abren en pestañas
nuevas con los cambios sin guardar** — `tcode` nunca escribe al disco un
renombrado sin que se vea: revisalos y guardá (o deshacé) cada uno. La
barra de estado dice cuántos cambios hubo y cuántos archivos se abrieron.
Si el servidor pide además crear, renombrar o borrar archivos (renombrar
un módulo, por ejemplo), no se aplica nada. Si el archivo cambió mientras
se esperaba la respuesta, tampoco: hay que pedirlo de nuevo.

Todas están también en la paleta de comandos (categoría "LSP"). El
servidor recibe como carpeta del proyecto el directorio desde el que se
lanzó `tcode` (pyright, por ejemplo, la necesita para renombrar en más de
un archivo).

### Formatear al guardar

Apagado por defecto para todos los lenguajes. En "Lenguajes / LSP" del
panel de administración, `f` sobre la fila de un lenguaje lo prende: cada
`Ctrl+S` (y "Guardar como") formatea el archivo antes de escribirlo. Hay
dos formas de formatear, con esta prioridad:

1. **Formateador externo** (si el lenguaje tiene uno configurado): un
   programa que recibe el texto por stdin y devuelve el formateado por
   stdout. `e` sobre la fila lo edita, con la misma sintaxis que el
   comando LSP (`VAR=valor -- comando args...`); una línea vacía lo
   quita. En los argumentos, `{archivo}` se reemplaza por la ruta
   absoluta del archivo. Se lanza con la carpeta del archivo como
   directorio de trabajo, así encuentra su propia config
   (`rustfmt.toml`, `pyproject.toml`...). Ejemplos:

   | Lenguaje | Formateador |
   |---|---|
   | Rust | `rustfmt --emit stdout --edition 2021` |
   | Python | `black -q -` o `ruff format -` |
   | JS/TS/CSS/HTML | `prettier --stdin-filepath {archivo}` |
   | Go | `gofmt` |

2. **El LSP** del lenguaje (`textDocument/formatting`), si no hay
   formateador externo y el servidor lo soporta.

En los dos casos el resultado se aplica como **un solo paso de
deshacer** (`Ctrl+Z` vuelve al texto sin formatear) y solo se tocan las
partes que cambian: el cursor y los bloques plegados se quedan sobre el
mismo código. Formatear **nunca impide guardar**: si el formateador no
está instalado, termina con error, no devuelve nada o tarda más de 3
segundos (se lo corta), el archivo se guarda tal cual y la barra de
estado dice por qué (`Sin formatear: 'black' falló (código 123): ...`,
con la primera línea de su stderr). El guardado automático no formatea.

En `config.toml`:

```toml
[lenguajes]
formatear_al_guardar = ["rust", "python"]

[lenguajes.formateador.rust]
comando = "rustfmt"
argumentos = ["--emit", "stdout", "--edition", "2021"]
```

Límite: los argumentos se separan por espacios, sin comillas (igual que
el comando LSP); para algo más complejo, apuntá a un script propio.

**`Ctrl+K R`** ("LSP: Ver logs de la sesión activa" en la paleta):
muestra lo que el servidor del lenguaje del archivo activo escribió en su
stderr — útil para entender
por qué no conecta o se comporta raro, más allá del estado "Conectado"/
"Iniciando…"/"Inactivo". Se actualiza en vivo mientras está abierto (lo
más nuevo arriba); escribir en el campo de arriba filtra las líneas por
texto sin cortar la actualización. `↓`/`↑`/`RePág`/`AvPág` recorren la
lista resaltando una fila, que se queda quieta sobre su línea aunque
lleguen nuevas; `↑` desde la primera fila vuelve a seguir lo último. La
mayoría de los servidores reales se quedan en silencio mientras todo
funciona bien, así que ver "sin logs" con una sesión conectada es lo
normal, no un signo de que algo esté mal.

## Personalizar atajos

Todos los atajos viven en `keymap.toml` (formato TOML plano, ver
[`runtime/keymaps/default.toml`](./runtime/keymaps/default.toml) como
referencia completa). Tres formas de cambiarlos:

1. **Desde la UI** (la más simple): sección "Atajos de teclado" del panel
   de administración, `Enter` sobre un comando y presionar la nueva
   combinación — se guarda al instante.
2. **Editando el archivo a mano**: copiá el `keymap.toml` de referencia a
   tu [directorio de configuración](#dónde-vive-la-configuración),
   editalo, y `Ctrl+K Ctrl+L` para recargarlo sin reiniciar.
3. **Exportar/importar**: desde la misma sección del panel, "Exportar
   atajos a archivo" guarda el keymap activo en
   `keymap-exportado.toml` (mismo directorio que `config.toml`) — llevalo
   a otra máquina, renombralo a `keymap-importar.toml` en el directorio
   de config de destino, y "Importar atajos desde archivo" lo carga ahí.

## Dónde vive la configuración

`tcode` usa el directorio de configuración estándar de tu sistema
operativo:

| SO | Directorio |
|---|---|
| Linux | `~/.config/tcode/` |
| macOS | `~/Library/Application Support/tcode/` |
| Windows | `%APPDATA%\tcode\` |

Ahí adentro: `config.toml` (ajustes de editor/interfaz/lenguajes),
`keymap.toml` (si existe — si no, se usan los atajos por defecto
embebidos) y `themes/` (temas propios o duplicados para editar). Todos se
crean automáticamente con valores por defecto la primera vez que hacen
falta — nunca hay que crearlos a mano para empezar a usar `tcode`.

## Config por proyecto

Además de la global, cada proyecto puede tener su propia
`.tcode/config.toml` con **solo** lo que quiera cambiar respecto de tu
config — por ejemplo, tabulación de 2 y sin números de línea en un repo
en particular:

```toml
[editor]
tamano_tabulacion = 2
numeros_de_linea = false
```

- **Dónde se busca**: al arrancar, desde la carpeta del archivo que
  abriste (o la carpeta actual si no abriste ninguno) hacia arriba,
  hasta la raíz del repo git (la carpeta con `.git`) o la del disco. Se
  usa la primera que aparezca. `config.recargar` (`Ctrl+K Ctrl+L`) la
  vuelve a buscar y leer.
- **Cómo se mezcla**: clave por clave. Las secciones (`[editor]`,
  `[interfaz]`, `[lenguajes]`...) se combinan con las de tu config
  global; un valor suelto o una lista del proyecto reemplaza al de la
  global.
- **El panel de administración (`Ctrl+,`) sigue editando solo tu config
  global**: nunca copia valores del proyecto a ella. Si hay una config
  de proyecto activa, el panel lo avisa arriba (con su ruta y qué
  claves pisa), y las filas pisadas muestran el valor en uso como
  `[proyecto: ...]` al lado del valor de tu global.
- **Seguridad**: las claves que ejecutan programas — comandos de LSP
  (`[lenguajes.lsp_comando.*]`, con sus variables de entorno) y
  formateadores externos (`[lenguajes.formateador.*]`) — **se ignoran**
  mientras no marques el proyecto como confiable, para que abrir un repo
  ajeno nunca ejecute un programa que eligió otra persona. Al arrancar,
  la barra de estado avisa si se ignoró alguna, y la cabecera del panel
  de administración dice cuáles. Un proyecto tampoco puede volver a
  habilitar un LSP que tengas deshabilitado en tu global (sí puede
  deshabilitar otros).
- **Confiar en un proyecto**: paleta (`Ctrl+Shift+P`/`F1`) → "Proyecto:
  Confiar en este proyecto". Desde ese momento se aplican sus comandos;
  la cabecera del panel lo muestra como "Proyecto CONFIABLE". La
  confianza se guarda en **tu** config global (sección `[confianza]`,
  nunca en el proyecto) con la ruta de la carpeta del proyecto y un
  hash (SHA-256) del contenido de su `.tcode/config.toml`: **si ese
  archivo cambia** (por ejemplo, un `git pull` que trae otro comando),
  el proyecto vuelve a ser no confiable al reabrirlo o con
  `config.recargar`, hasta que confíes de nuevo. "Proyecto: Revocar
  confianza" la quita. Un `.tcode/config.toml` no puede escribir la
  sección `[confianza]` (se ignora siempre).
- **Errores**: si el archivo tiene un error (TOML mal formado o un valor
  del tipo equivocado), se ignora entero y se sigue con tu config
  global; el motivo aparece en la cabecera del panel de administración.
  Las claves que `tcode` no conoce se ignoran y se listan ahí mismo.
