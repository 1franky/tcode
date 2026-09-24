# Manual de uso de `tcode`

Guía práctica para gente que ya instaló `tcode` y quiere usarlo — para el
diseño interno, arquitectura y roadmap del proyecto ver [PLAN.md](./PLAN.md).

## Índice

- [Primeros pasos](#primeros-pasos)
- [Atajos esenciales](#atajos-esenciales)
- [Multi-cursor y selección](#multi-cursor-y-selección)
- [Búsqueda y reemplazo](#búsqueda-y-reemplazo)
- [Plegado de bloques](#plegado-de-bloques)
- [Paneles divididos (splits)](#paneles-divididos-splits)
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
sin guardar, la primera vez no hace nada visible (queda pendiente de
confirmación) y hace falta presionarlo una segunda vez seguida para
confirmar y salir de verdad; cualquier otra tecla en el medio cancela esa
confirmación pendiente.

## Atajos esenciales

| Atajo | Acción |
|---|---|
| `Ctrl+S` | Guardar (si el buffer no tiene nombre todavía, abre "Guardar como") |
| `Ctrl+Shift+S` (o `Ctrl+K S`) | Guardar como... — elegir/cambiar la ruta del archivo |
| `Ctrl+Z` / `Ctrl+Y` | Deshacer / Rehacer |
| `Ctrl+Q` | Salir |
| `Tab` / `Shift+Tab` | Indentar / desindentar |
| `Ctrl+B` | Mostrar/ocultar el explorador de archivos lateral |
| `Ctrl+K J` | Salto rápido en el explorador (etiquetas de una tecla) |
| `Ctrl+P` | Buscar archivo por nombre (difuso) |
| `Ctrl+Shift+P` o `F1` | Paleta de comandos (buscar cualquier acción por nombre) |
| `Ctrl+F` / `Ctrl+H` | Buscar / Buscar y reemplazar en el archivo |
| `Ctrl+,` o `Ctrl+K A` | Panel de administración |
| `Ctrl+K R` | Ver logs de la sesión LSP activa |
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
- El plegado es de cada panel y no se guarda: al volver a abrir el
  archivo arranca todo desplegado.

## Paneles divididos (splits)

| Atajo | Acción |
|---|---|
| `Ctrl+\` (o `Ctrl+K \`) | Dividir el panel activo verticalmente |
| `Ctrl+K Ctrl+\` (o `Ctrl+K -`) | Dividir horizontalmente |
| `Ctrl+1` / `Ctrl+2` / `Ctrl+3` (o `Ctrl+K 1`/`2`/`3`) | Saltar al panel 1/2/3 |
| `Ctrl+K F` | Cerrar el panel activo |

Cada panel tiene su propio archivo abierto, cursor y estado de vista
(tabla CSV, preview Markdown) — son independientes entre sí.

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

Alcance de esta primera entrega (se puede ampliar más adelante): modos
Normal/Insertar, movimientos básicos y los comandos de una/dos teclas más
usados. Sin operadores combinables (`dw`, `d$`), sin conteos numéricos
(`3dd`), sin modo Visual, sin `:`.

| Tecla (modo Normal) | Acción |
|---|---|
| `h` / `j` / `k` / `l` | Mover el cursor izquierda/abajo/arriba/derecha |
| `0` / `$` | Inicio / fin de la línea |
| `gg` / `G` | Inicio / fin del archivo |
| `i` | Entrar a Insertar en la posición actual |
| `a` | Entrar a Insertar una posición a la derecha (al final de línea, después del último carácter) |
| `o` | Abrir una línea nueva debajo y entrar a Insertar ahí |
| `x` | Borrar el carácter bajo el cursor |
| `dd` | Borrar la línea completa (queda en el registro) |
| `yy` | Copiar la línea completa al registro, sin borrar nada |
| `p` | Pegar el registro como una línea nueva debajo de la actual |
| `u` | Deshacer (comparte historial con `Ctrl+Z`) |
| `Esc` (en Insertar) | Volver a Normal |

El registro sin nombre (lo que dejan `dd`/`yy`, lo que pega `p`) es uno
solo para toda la app, no por panel — yanquear en un archivo y pegar en
otro funciona, igual que en VIM real. El resto de atajos de tcode
(flechas, `Ctrl+S`, `Ctrl+B`, splits, etc.) siguen andando igual estando
en cualquiera de los dos modos: el modo VIM solo cambia qué significa un
carácter suelto sin modificador.

Una diferencia con Insertar (y con el resto de editores no-VIM): en modo
Normal el cursor nunca queda "después" del último carácter de una línea
no vacía — como en VIM real, `$`/`l` se detienen justo sobre el último
carácter, no después. `a`/`A` siguen permitiendo escribir al final,
como es de esperar.

Limitación conocida: un panel nuevo por `Ctrl+\` (split) siempre arranca
en Insertar, incluso con el modo VIM prendido — abrir un archivo ahí (o
en el explorador/buscador de archivos) sí respeta la config.

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
| **Lenguajes / LSP** | Habilitar/deshabilitar el servidor LSP de cada lenguaje, ver si el binario está en el `PATH` y el estado de la sesión activa (Conectado/Iniciando/Inactivo). `c` sobre una fila edita el comando+argumentos a mano (ver [LSP](#lsp-autocompletado-y-diagnósticos)); `Backspace` quita ese override. |
| **Editor** | Tamaño de tabulación, espacios vs. tabs, ajuste de línea, números de línea, indicadores de git, modo VIM, regla vertical, guardado automático. |
| **Interfaz** | Mostrar/ocultar la barra de estado y cada uno de sus elementos (posición del cursor, codificación, fin de línea, lenguaje, diagnósticos, modo). |

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
- La versión de referencia se vuelve a leer al abrir el archivo y al
  guardar (`Ctrl+S`). Tras un commit hecho desde otra terminal, las marcas
  se ponen al día con el próximo `Ctrl+S` (aunque no haya cambios).
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
Si ya hay una sesión activa para ese lenguaje, se relanza sola con el
comando nuevo.

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

**`Ctrl+K R`** ("LSP: Ver logs de la sesión activa" en la paleta):
muestra lo que el servidor escribió en su stderr — útil para entender
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
- **Seguridad**: una config de proyecto **no puede** definir comandos de
  LSP (`[lenguajes.lsp_comando.*]`, con sus variables de entorno) — se
  ignoran, para que abrir un repo ajeno nunca ejecute un programa que
  eligió otra persona. Tampoco puede volver a habilitar un LSP que
  tengas deshabilitado en tu global (sí puede deshabilitar otros).
- **Errores**: si el archivo tiene un error (TOML mal formado o un valor
  del tipo equivocado), se ignora entero y se sigue con tu config
  global; el motivo aparece en la cabecera del panel de administración.
  Las claves que `tcode` no conoce se ignoran y se listan ahí mismo.
