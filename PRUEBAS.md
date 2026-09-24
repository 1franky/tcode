# Plan de pruebas manuales — tcode

Checklist para probar `tcode` de punta a punta antes de liberar una nueva
versión. Cubre todo lo implementado hasta la fecha: M0-M4 completos y
liberados, más piezas post-M4 (Guardar como, LSP robusto, ajuste de
línea, manual de uso, fixes de Windows, salto rápido del explorador,
scroll horizontal en CSV, modo VIM opcional, tema de alto contraste —
ver [PLAN.md](./PLAN.md) §11 para el detalle de cada milestone). Cada
sección se va agregando/actualizando pieza por pieza, a medida que se
mergea a `develop` — no es un documento que se escribe una sola vez.

No hace falta correrlo entero en cada versión — como mínimo, correr la
sección de la pieza que cambió más el bug conocido de Windows. Antes de
mergear `develop` → `main` y taggear, conviene pasar al menos una vez por
todo, en la plataforma donde se vaya a usar principalmente.

## Cómo instalar la versión a probar

- **Linux/macOS**: `curl -fsSL https://raw.githubusercontent.com/1franky/tcode/main/install/linux.sh | bash`
- **Windows**: `irm https://raw.githubusercontent.com/1franky/tcode/main/install/windows.ps1 | iex`
- **Desde código fuente** (para probar `develop` antes de liberar): `cargo build --release` y usar `target/release/tcode`.

Para cada prueba, anotar: ✅ funcionó / ❌ falló (con qué se rompió) / ⚠️
funcionó pero con algo raro. Sistema operativo y terminal usados importan
mucho en este proyecto — anotarlos siempre (ver la sección de Windows más
abajo, es donde más problemas aparecieron).

---

## M0 — Fundamentos

- [ ] Abrir `tcode` sin argumentos: arranca con un buffer vacío ("[Sin nombre]").
- [ ] Abrir `tcode ruta/a/archivo.txt`: carga el contenido correcto.
- [ ] Escribir texto, moverse con las flechas, `Home`/`End`, `Ctrl+Home`/`Ctrl+End`.
- [ ] `Enter` inserta un salto de línea real (no rompe el archivo).
- [ ] `Backspace`/`Delete` borran correctamente, incluida la fusión de líneas al borrar al inicio/fin de línea.
- [ ] `Ctrl+Z`/`Ctrl+Y` (deshacer/rehacer) varias veces seguidas, en ambas direcciones.
- [ ] `Ctrl+S` guarda; volver a abrir el archivo y confirmar que el contenido persistió.
- [ ] Escribir caracteres UTF-8 (acentos, ñ, emoji) y guardar — no se corrompen.
- [ ] Barra de estado inferior: `Ln`/`Col` correctos, cuenta total de líneas, marca `*` cuando hay cambios sin guardar.

### Selección de texto con `Shift`+flechas

Mecanismo de selección básico que faltaba por completo (`BACKLOG.md`
P0): hasta esta pieza la única forma de seleccionar texto era
multi-cursor (`Ctrl+D`/`Ctrl+Shift+L`). `Shift+Ctrl+Home`/`Shift+Ctrl+End`
(selección hasta inicio/fin de archivo) quedan fuera de esta entrega a
propósito.

- [ ] `Shift+Right`/`Shift+Left` repetido: extiende la selección un carácter a la vez, sin importar cuántas veces se presione — el fondo de selección (color `ui.selection` del tema activo) cubre el rango correcto.
- [ ] `Shift+Down`/`Shift+Up`: extiende la selección por línea completa, respetando dónde arrancó (el "ancla" no se mueve nunca, solo el extremo activo).
- [ ] `Shift+End` y `Shift+Home` desde el medio de una línea: seleccionan hasta el fin/inicio de esa línea respectivamente.
- [ ] Mover el cursor SIN `Shift` (una flecha sola) inmediatamente después de una selección con `Shift`: la colapsa por completo, igual que en cualquier editor — no debe quedar ningún resto de selección.
- [ ] Escribir un carácter (o `Backspace`) con una selección hecha con `Shift` activa: reemplaza/borra todo el rango seleccionado, mismo comportamiento que ya existía para selecciones de multi-cursor.
- [ ] Con más de un cursor activo (`Ctrl+D` dos veces para tener 2): `Shift+Right` extiende la selección de AMBOS cursores a la vez, cada uno de forma independiente desde su propia posición.
- [ ] Repetir la prueba visual con el tema "oscuro" (no Dracula — ahí `selection` y `current_line` comparten color, puede parecer que no pasa nada aunque esté funcionando).

## M1 — Configuración, temas, atajos, sintaxis, explorador

- [ ] `~/.config/tcode/config.toml` (o el equivalente portable en Windows) se crea solo la primera vez, con valores razonables.
- [ ] Cambiar `tema` en `config.toml` a `"oscuro"` o `"claro"`, `Ctrl+K Ctrl+L` (recargar config): el tema cambia en caliente sin reiniciar.
- [ ] Editar `keymap.toml` de usuario (copiarlo del embebido), cambiar un atajo, `Ctrl+K Ctrl+L`: el nuevo atajo funciona sin reiniciar (recién a partir de M4 pieza "editor de atajos" esto quedó realmente implementado — antes de esa pieza `Ctrl+K Ctrl+L` solo recargaba `config.toml`/tema, no el keymap, aunque este checklist ya lo daba por hecho).
- [ ] Abrir un archivo `.rs`, `.py`, `.js`, `.go` y `.md`: resaltado de sintaxis correcto (palabras clave, strings, comentarios, números).
- [ ] Abrir un archivo de una extensión no soportada: se ve como texto plano sin colorear, sin romperse.
- [ ] `Ctrl+B`: abre/cierra el explorador de archivos lateral.
- [ ] Con el explorador enfocado: `↑`/`↓` mueve la selección, `Enter` sobre una carpeta la expande/colapsa, `Enter` sobre un archivo lo abre en el editor y devuelve el foco a este último.
- [ ] `Esc` con el explorador visible: devuelve el foco al editor sin cerrar el explorador.

### Salto rápido en el explorador (`Ctrl+K J`)

No se puede detectar de forma confiable sostener Alt/Cmd solos en
ninguna terminal (ninguna combinación de solo modificadores llega a la
aplicación — ni siquiera `Windows+Alt` o `Option+Command`, ver la
discusión en el PR): por eso el trigger es un atajo normal, al estilo
Vimium/`vim-easymotion`, no una tecla sostenida.

- [ ] Con el explorador visible y varios archivos/carpetas listados, `Ctrl+K J`: aparece una etiqueta de una tecla (`1`, `2`, `3`... y después `a`, `b`, `c`...) junto a cada fila, con fondo resaltado.
- [ ] Tipear la etiqueta de un archivo: lo abre directamente en el editor y devuelve el foco a este — sin haber navegado ahí con las flechas.
- [ ] Tipear la etiqueta de una carpeta: la expande/colapsa (igual que `Enter`) y sale del modo salto, sin abrir nada.
- [ ] Tipear una tecla que no le toca a ninguna fila visible (p. ej. `z` con solo 3 archivos): no pasa nada, las etiquetas se quedan mostradas esperando una válida.
- [ ] `Esc` en modo salto: cierra el modo sin saltar a ningún lado, las filas vuelven a verse normales.
- [ ] Invocar "Ver: Saltar a un archivo" desde la paleta de comandos (`F1`) con el explorador **oculto**: lo muestra, le da el foco y activa el modo salto directamente — no hace falta abrirlo a mano primero.
- [ ] Con más archivos visibles que letras del alfabeto (36 — dígitos + minúsculas, poco común pero posible con una terminal muy alta): las filas de más allá de la 36 quedan sin etiqueta, pero se pueden seguir navegando con las flechas como siempre.

### Crear, renombrar y borrar desde el explorador (BACKLOG.md P0, "explorador de solo lectura")

Hasta esta pieza el explorador era de solo lectura — no había forma de
crear, renombrar ni borrar nada desde la app. `Ctrl+K N`/`Ctrl+K C`/
`Ctrl+K M` (nuevo archivo/carpeta/renombrar) son globales, como
`Ctrl+K J`: si el explorador está oculto, lo muestran y le dan el foco
antes de abrir el prompt. Borrar (`Delete`, con el explorador enfocado)
siempre pide confirmación primero — es destructivo e irreversible, no
pasa por ninguna papelera de reciclaje.

- [ ] `Ctrl+K N` con una carpeta seleccionada: abre "Nuevo archivo" vacío; escribir un nombre y `Enter` lo crea DENTRO de esa carpeta (aunque esté colapsada) — confirmar en disco y, al expandirla, que aparece en el árbol.
- [ ] `Ctrl+K N` con un archivo seleccionado (no una carpeta): el nuevo archivo se crea en la carpeta que lo CONTIENE, no adentro de él (los archivos no tienen "adentro").
- [ ] `Ctrl+K C`: igual que `Ctrl+K N` pero crea una carpeta (`std::fs::create_dir`, no recursivo — si la carpeta padre no existe, es un error).
- [ ] Escribir un nombre que ya existe en el destino (archivo o carpeta) y `Enter`: el prompt queda abierto con "ya existe '…'" en vez de cerrarse o sobreescribir nada.
- [ ] Dejar el campo vacío (o solo espacios) y `Enter`: no crea nada, muestra "el nombre no puede estar vacío".
- [ ] `Esc` en cualquier momento del prompt: cierra sin crear nada.
- [ ] `Ctrl+K M` sobre una fila seleccionada: abre "Renombrar" PRECARGADO con el nombre actual (no vacío) — ajustarlo y `Enter` renombra en disco (`std::fs::rename`, misma carpeta contenedora — esto es renombrar, no mover a otro lado) y el árbol refleja el cambio al instante.
- [ ] Renombrar a un nombre que ya existe en la misma carpeta: falla con "ya existe '…'", el archivo original queda intacto.
- [ ] `Delete` con el explorador enfocado y algo seleccionado: abre "Confirmar borrado" con el nombre y si es "el archivo" o "la carpeta" — nunca borra directo desde la tecla.
- [ ] Cualquier tecla que NO sea `y`/`Y` (incluido `Enter` y `Esc`) en la confirmación: cancela sin tocar el disco — a propósito no hay una "tecla por defecto" para una acción destructiva.
- [ ] `y` confirma: el archivo/carpeta desaparece del árbol y del disco de verdad (una carpeta se borra recursivamente, con todo lo que tuviera adentro).
- [ ] Las 3 acciones ("Explorador: Nuevo archivo"/"Nueva carpeta"/"Renombrar selección") aparecen en la paleta de comandos (`F1`) y funcionan igual que sus atajos.
- [ ] Limitación conocida: crear/renombrar/borrar algo refresca la carpeta contenedora leyéndola de nuevo del disco — si esa carpeta tenía OTRAS subcarpetas ya expandidas en el mismo nivel, quedan colapsadas de nuevo tras el refresco (se puede volver a expandirlas con `Enter`, no se pierde nada, solo el estado visual de "abierta").

## Scroll-follow en listas con selección (BACKLOG.md)

Fix transversal: ninguna de estas 5 listas (todas comparten el mismo
problema por dos causas distintas — 4 pasan por `tcode_ui::overlay::
dibujar`, el explorador arma su propia lista aparte) seguía la
selección con scroll. Antes, con más filas/resultados de los que
entraban en pantalla, bajar la selección con `↓` la dejaba resaltando
una fila que ya no se dibujaba — visible solo si volvías a subir. Ahora
`ratatui` recalcula el offset necesario en cada frame (vía `ListState`,
sin persistir nada de un frame al siguiente — mismo criterio que ya usa
`TableState` en la vista CSV).

- [ ] **Explorador** (`Ctrl+B`): con más archivos que los que entran en el panel, bajar la selección hasta el último — sigue visible en todo momento, nunca desaparece de pantalla. Subir de nuevo hasta el primero: mismo resultado. El resaltado de fila, los íconos de carpeta y las etiquetas del modo "salto rápido" (`Ctrl+K J`) se ven exactamente igual que antes.
- [ ] **Buscador de archivos** (`Ctrl+P`), en un proyecto con más de una pantalla de archivos: bajar/subir la selección hasta los extremos — siempre visible.
- [ ] **Paleta de comandos** (`F1`): mismo chequeo — hay más de 25 comandos, más que lo que entra en cualquier ventana chica.
- [ ] **Selector de temas** (`Ctrl+K Ctrl+T`): con los 13 temas (filtro "Todos"), bajar hasta el último (`Alto contraste`) y subir hasta el primero (`Dracula`) — el preview en vivo sigue aplicándose en cada fila visitada, igual que antes.
- [ ] **Visor de logs de LSP** (`Ctrl+K R`) con más de una pantalla de líneas: no tiene navegación con flechas (no hay ninguna fila "seleccionada" — el filtro de texto es la forma de acotar), así que el comportamiento correcto es simplemente seguir mostrando las líneas más recientes desde arriba, sin romper nada ni intentar scrollear a ningún lado raro.

## M2 — Paleta de comandos, buscador de archivos, splits, LSP

- [ ] `Ctrl+Shift+P` o `F1`: abre la paleta de comandos.
- [ ] Escribir en la paleta filtra por coincidencia difusa (no hace falta escribir el nombre completo ni en orden exacto de palabras).
- [ ] `↑`/`↓` navega los resultados, `Enter` ejecuta el comando seleccionado, `Esc` cierra sin ejecutar nada.
- [ ] `Ctrl+P`: abre el buscador de archivos (fuzzy finder) del proyecto; mismo comportamiento de filtro/navegación/`Enter`/`Esc`.
- [ ] `Ctrl+\` divide el panel activo verticalmente (lado a lado); `Ctrl+K Ctrl+\` lo divide horizontalmente (apilado).
- [ ] `Ctrl+1`/`Ctrl+2`/`Ctrl+3` cambian de panel; `Ctrl+K F` cierra el panel activo (nunca el último que queda).
- [ ] Abrir un archivo `.py` con `pyright` instalado (`npm install -g pyright`): aparecen diagnósticos (subrayado) al escribir código con errores, y desaparecen al corregirlos.
- [ ] La barra de estado muestra el resumen de errores/avisos cuando hay diagnósticos LSP activos.

### Cierre educado del protocolo LSP al salir

- [ ] Con un `.py` abierto y `pyright` "Conectado" (panel de administración → "Lenguajes / LSP"): salir de `tcode` (`Ctrl+Q` dos veces) y confirmar con `ps aux | grep pyright` (en otra terminal, justo antes y justo después de salir) que el proceso `pyright-langserver` ya no está — se cerró con el protocolo `shutdown`+`exit` en vez de matarlo en seco, y debería desaparecer casi al instante (no debería sentirse ninguna demora perceptible al cerrar `tcode`).
- [ ] Cambiar de un archivo `.py` (con LSP activo) a uno de un lenguaje distinto sin comando configurado (ej. `.rs`): el cambio de panel/archivo se siente **instantáneo**, sin ninguna pausa — el relanzado mata la sesión vieja directo (no espera el protocolo de cierre educado, que sí se usa solo al salir de `tcode`) para no introducir latencia al cambiar de archivo.
- [ ] (Extremo, opcional) Configurar a mano un comando LSP inválido/que cuelgue (ej. `cat` desde el panel de administración, sección "Lenguajes / LSP", tecla `c`) y luego salir de `tcode`: el cierre no debería tardar más de ~1 segundo — el servidor "no cooperativo" se mata igual una vez vencido ese margen, en vez de trabar el cierre para siempre.

### URI del LSP con caracteres especiales en la ruta

- [ ] Abrir un archivo `.py` cuya ruta tenga un espacio y un `#` en el nombre (ej. `mi archivo#1.py`) con `pyright` instalado: el LSP conecta igual ("Conectado" en el panel de administración) y los diagnósticos aparecen subrayados en el lugar correcto — antes de esta pieza, el `#` sin escapar rompía el URI (todo lo que sigue a un `#` se interpreta como fragmento, no como parte de la ruta) y los diagnósticos podían no llegar o llegar para la ruta equivocada.
- [ ] Lo mismo con tildes/eñes en el nombre del archivo o alguna carpeta del camino (ej. `código/año.py`).

## M3 — Búsqueda y reemplazo (`Ctrl+F` / `Ctrl+H`)

- [ ] `Ctrl+F`: abre la barra de búsqueda flotante en la esquina superior derecha (no tapa el código).
- [ ] Escribir una consulta: resalta todas las coincidencias en el texto y muestra el contador (`N/total`).
- [ ] `F3`/`Shift+F3`: salta a la siguiente/anterior coincidencia, con la barra abierta o cerrada (repite la última búsqueda).
- [ ] `Alt+R` (regex), `Alt+C` (sensible a mayúsculas), `Alt+W` (palabra completa): cada toggle cambia los resultados en vivo; un regex inválido muestra el error en la barra en vez de romper nada.
- [ ] `Ctrl+H`: abre en modo reemplazar (`Tab` alterna entre el campo de búsqueda y el de reemplazo).
- [ ] `Enter` con el campo de reemplazo activo: reemplaza solo la coincidencia actual y avanza a la siguiente.
- [ ] `Ctrl+Alt+Enter` o `Alt+Enter`: reemplaza TODAS las coincidencias de una vez.
- [ ] `Esc`: cierra la barra sin perder los cambios ya hechos.

## M3 — Vista Markdown (`Ctrl+K V` / `Ctrl+Shift+V`)

- [ ] Abrir un `.md`: por defecto se ve solo el código fuente (con resaltado de sintaxis Markdown), como cualquier otro archivo.
- [ ] `Ctrl+K V`: divide el panel 50/50 en fuente + preview renderizado en vivo (encabezados coloreados por nivel, negrita/cursiva, listas, citas, bloques de código resaltados, tablas alineadas, enlaces subrayados, imágenes como `[img: alt]`). Volver a presionar `Ctrl+K V` regresa a solo fuente.
- [ ] `Ctrl+Shift+V` (o el comando "Markdown: Ver solo preview" desde la paleta si esa combinación no llega en la terminal usada): muestra solo el preview a ancho completo.
- [ ] Editar el Markdown fuente mientras el preview está visible: el preview se actualiza en vivo.
- [ ] Probar en un archivo que NO sea Markdown: `Ctrl+K V`/`Ctrl+Shift+V` no hacen nada.

## M3 — Vista CSV/TSV (`Ctrl+K T`)

- [ ] Abrir un `.csv`: se muestra directamente como tabla (encabezado en negrita, columnas alineadas), no como texto plano.
- [ ] Abrir un `.tsv`: mismo comportamiento, usando tabulador como delimitador.
- [ ] Abrir un CSV con una celda citada que contiene el delimitador adentro (p. ej. `"Ciudad, de México"`): se muestra como una sola celda, no partida.
- [ ] Navegar con las flechas y con `Tab`/`Shift+Tab` (salta de fila al llegar al borde, como en una hoja de cálculo).
- [ ] `Enter` o `F2` sobre una celda: entra en modo edición con el valor actual precargado; escribir, `Enter` confirma y avanza a la fila de abajo, `Esc` cancela sin aplicar cambios.
- [ ] Guardar (`Ctrl+S`) tras editar una celda y volver a abrir el archivo (o revisarlo con otro editor/`cat`): el resto de las filas quedó intacto, byte a byte.
- [ ] `Ctrl+K T`: alterna a texto plano (para arreglar algo a mano) y de vuelta a tabla.

### Scroll horizontal con muchas columnas

Antes, un CSV con más columnas de las que entraban en el ancho de la
terminal hacía que `ratatui` encogiera TODAS las columnas
proporcionalmente hasta dejarlas de 1-2 caracteres, ilegibles (detectado
en capturas de Windows durante el diagnóstico del bug de renderizado,
`imagesWindows/image-4.png` en su momento). Ahora se muestra una ventana
de columnas completas que sí entran, siguiendo a la selección.

- [ ] Abrir un CSV/TSV con más columnas de las que entran en el ancho de la terminal (o angostar la ventana hasta lograrlo): se ven columnas completas y legibles desde la primera, no todas comprimidas a 1-2 caracteres.
- [ ] Mover la selección con `→` más allá de la última columna visible: la ventana se desplaza para mostrarla, sin saltos ni columnas a medias.
- [ ] Volver con `←` hasta la primera columna: la ventana vuelve a mostrar la columna 0 (no se queda scrolleada a la mitad).
- [ ] El encabezado (fila congelada) se desplaza junto con el cuerpo — nunca queda desalineado con las columnas que se ven abajo.
- [ ] Editar una celda de una columna fuera de la ventana original: el cursor de edición aparece en la posición correcta de pantalla tras el scroll (no en la posición "vieja" antes de desplazarse).

### Ordenar, filtrar, insertar/eliminar y ancho de columna (BACKLOG.md P2 #9)

Lo que había quedado fuera de M3. Probar con un CSV y un TSV que tengan
acentos y alguna celda citada con el delimitador adentro (p. ej.
`"Ciudad, de México"`), y revisar el archivo con `cat` después de
guardar.

- [ ] `Ctrl+K O` sobre una columna de texto: ordena ascendente sin distinguir mayúsculas ni tildes ("Ángel" antes que "beto"; "ñ" después de "n"); el encabezado no se mueve. Repetir `Ctrl+K O`: pasa a descendente, y de vuelta.
- [ ] `Ctrl+K O` sobre una columna numérica: ordena como número (`9` antes que `10`, negativos y decimales incluidos), no como texto. Las celdas vacías quedan al final en los dos sentidos.
- [ ] Tras ordenar, un solo `Ctrl+Z` devuelve el orden original completo. Guardar y `cat`: las celdas citadas siguen citadas igual que antes (ordenar no reescribe las filas, solo las mueve).
- [ ] `Ctrl+K /`, escribir un texto y `Enter`: quedan solo las filas cuya celda en la columna seleccionada lo contiene (sin distinguir mayúsculas ni tildes: "mexico" encuentra "México"); el encabezado sigue visible y una barra al pie muestra el filtro y "N de M filas".
- [ ] Con un filtro activo, editar una celda (`Enter`/`F2`) y guardar: el cambio quedó en la fila correcta del archivo (no en la que ocupa esa misma posición sin filtro).
- [ ] Quitar el filtro con `Esc` (con la tabla enfocada), con `Ctrl+K /` + `Enter` con el texto vacío, y con "CSV: Quitar filtro" desde la paleta: vuelven todas las filas y la selección sigue sobre la misma fila del archivo. `Esc` con el prompt abierto solo lo cierra, sin tocar el filtro vigente.
- [ ] `Ctrl+K ↓` / `Ctrl+K ↑`: inserta una fila vacía debajo/arriba de la seleccionada (y la selecciona al insertar debajo). Con un filtro activo, primero se quita el filtro. Un solo `Ctrl+Z` la quita.
- [ ] `Ctrl+K →` / `Ctrl+K ←`: inserta una columna vacía a la derecha/izquierda en todas las filas. Guardar y `cat`: las celdas con comas/comillas siguen correctamente citadas (y en el TSV, una coma no provoca comillas). Un solo `Ctrl+Z` lo revierte.
- [ ] `Ctrl+K E` elimina la fila seleccionada y `Ctrl+K Shift+E` la columna seleccionada; cada una se deshace con un solo `Ctrl+Z`. Con una sola columna, `Ctrl+K Shift+E` no hace nada.
- [ ] `Ctrl+K Shift+→` / `Ctrl+K Shift+←`: la columna seleccionada se ensancha/angosta de a 2 (más allá del máximo automático de 30, para leer una celda larga entera); `Ctrl+K W` la devuelve al ancho automático. Nada de esto marca el archivo como modificado.
- [ ] Probar un archivo cuya última línea NO termina en salto de línea: ordenar/insertar al final/eliminar la última fila no pegan dos filas en una línea, y el archivo sigue sin `\n` final.

## M3 — Multi-cursor (`Ctrl+D` / `Ctrl+Shift+L` / `Ctrl+Alt+↑↓`)

- [ ] Poner el cursor sobre una palabra y `Ctrl+D`: selecciona esa palabra (sin agregar un cursor nuevo todavía).
- [ ] `Ctrl+D` de nuevo (una o más veces): va agregando un cursor en cada siguiente ocurrencia de esa palabra, en orden, dando la vuelta al llegar al final del archivo.
- [ ] `Ctrl+Shift+L` (o `Ctrl+K L` si esa combinación no llega en la terminal usada): selecciona TODAS las ocurrencias de una sola vez.
- [ ] Con varios cursores activos, escribir **más de un carácter seguido** (p. ej. reemplazar una palabra completa): el texto queda correcto en TODAS las posiciones, no se corrompe ni se desordena — este es el caso que específicamente se rompía antes de corregirse, vale la pena probarlo con atención.
- [ ] Con varios cursores activos, `Backspace`: borra en todas las posiciones a la vez, de forma independiente.
- [ ] `Ctrl+Alt+↑` / `Ctrl+Alt+↓`: agrega un cursor una línea arriba/abajo de cada cursor existente, en la misma columna; no hace nada para los cursores que ya están en la primera/última línea.
- [ ] La barra de estado muestra "N cursores" cuando hay más de uno, y desaparece con uno solo.
- [ ] `Esc`: colapsa todo a un solo cursor (el principal), sin selección.
- [ ] Guardar con varios cursores activos y volver a abrir el archivo: el contenido quedó correcto.

## M4 — Selector de temas con preview en vivo (`Ctrl+K Ctrl+T`)

- [ ] `Ctrl+K Ctrl+T` (o "Tema: Seleccionar" desde la paleta de comandos, `Ctrl+Shift+P`/`F1`): abre el selector con los 13 temas embebidos, marcando con `*` (ASCII) el que está activo en ese momento.
- [ ] `↑`/`↓`: el editor de fondo cambia de tema en vivo con cada movimiento, sin tocar `config.toml` todavía (revisar el archivo mientras el selector sigue abierto: no debería haber cambiado).
- [ ] `Tab`: cicla el filtro `Todos` → `Oscuro` → `Claro` → `Alto contraste` → `Todos`; la lista se recorta a los temas de ese tipo (los `light` son solo Solarized Light, GitHub Light y Claro; "Alto contraste" es un único tema, `alto-contraste`).
- [ ] `Enter` sobre un tema: cierra el selector, el tema queda aplicado, y persiste en `config.toml` — reabrir `tcode` y confirmar que arranca con ese mismo tema.
- [ ] `Esc`: cierra el selector y vuelve exactamente al tema que estaba activo antes de abrirlo (no al primero de la lista ni al último visto en el preview), sin modificar `config.toml`.
- [ ] Revisar de pasada que los 10 temas nuevos (Monokai, One Dark, Nord, Gruvbox Dark, Tokyo Night, Catppuccin Mocha, Solarized Dark, Solarized Light, GitHub Light) se ven con colores razonables y texto legible, no solo Dracula/oscuro/claro.
- [ ] Como esta pieza agrega una ruta modal nueva al loop de dibujado (`crates/app/src/main.rs`): re-correr al menos la prueba básica de la sección de Windows más abajo, aunque no toque directamente el explorador.

### Tema "Alto contraste" (M5, tercer filtro del selector)

Negro puro + colores primarios saturados (amarillo/cian/verde/magenta/
naranja/rojo), sin tonos intermedios en ningún lado — pensado para
máxima diferencia perceptible, no para verse "lindo" (mismo criterio que
los temas "High Contrast" de VS Code/Windows).

- [ ] Filtrar por "Alto contraste" (`Tab` x3 desde "Todos"): muestra un único tema, `Alto contraste (oscuro, alto contraste)` — la etiqueta indica ambos ejes (oscuro/claro Y alto contraste, son independientes).
- [ ] Aplicarlo sobre un archivo con sintaxis resaltada (`.rs`/`.py`/etc.): palabras clave en amarillo negrita, strings en naranja, números y constantes en verde, funciones en cian, tipos en magenta — todo sobre fondo negro puro, sin ningún color apagado/pastel.
- [ ] La barra de estado se ve invertida (fondo blanco, texto negro) — a propósito, para marcar un límite visual inequívoco con el resto de la pantalla.
- [ ] Buscar algo con `Ctrl+F`: la coincidencia actual se ve en naranja bien visible, las demás en azul — ninguna se pierde contra el fondo negro.

### Importar temas de terceros en el selector (BACKLOG.md)

Hasta esta pieza, un tema `.toml` que alguien dejara en
`~/.config/tcode/themes/` (o el directorio portable de Windows) con un
nombre que no fuera ninguno de los 13 embebidos podía *usarse*
escribiendo `tema = "nombre-que-sea"` a mano en `config.toml` + `Ctrl+K
Ctrl+L`, pero nunca aparecía listado en `Ctrl+K Ctrl+T` — rompía el
flujo de "importar y elegir desde la lista" de `PLAN.md` §7. Ahora el
selector escanea esa carpeta cada vez que se abre.

- [ ] Copiar cualquier `.toml` de `runtime/themes/` a la carpeta de temas de usuario con OTRO nombre de archivo (por ejemplo `mi-tema-de-prueba.toml`, cambiándole también el campo `name` adentro para distinguirlo a simple vista) y abrir `Ctrl+K Ctrl+T`: aparece en la lista, después de los 13 embebidos, con su `name`/`type`/`alto_contraste` reales — no con el nombre del archivo.
- [ ] Navegar hasta esa fila con `↓`: el preview en vivo cambia el editor de fondo al tema copiado, igual que con cualquier tema embebido.
- [ ] `Enter` sobre esa fila: persiste en `config.toml` (`tema = "mi-tema-de-prueba"`, el nombre del ARCHIVO sin extensión, no el `name` de adentro) — reabrir `tcode` y confirmar que arranca con ese tema.
- [ ] Filtrar por "Oscuro"/"Claro"/"Alto contraste" (`Tab`): un tema de terceros que declare `type`/`alto_contraste` correctos en su TOML aparece en el filtro que corresponda, igual que uno embebido.
- [ ] Dejar un archivo `.toml` corrupto/inválido en la misma carpeta (por ejemplo texto que no sea TOML): el selector lo ignora en silencio — no rompe la lista ni el resto de los temas.
- [ ] Con la copia editable de un tema embebido ya creada (`Ctrl+K Ctrl+P`/"Duplicar tema activo" en algún momento deja `<tema>-mio.toml` en esa misma carpeta): esa copia NO aparece como una fila nueva separada en el selector — sigue sustituyendo transparentemente al original, como ya funcionaba antes de esta pieza.
- [ ] Sección "Temas" del panel de administración (`Ctrl+K A`): si el tema activo es uno de terceros (no embebido), la fila "Duplicar tema activo" sigue mostrando su nombre real entre paréntesis, no lo omite.

## M4 — Panel de administración (`Ctrl+,` / `Ctrl+K A`) y números de línea

- [ ] `Ctrl+,` para abrir el panel: en terminales sin protocolo Kitty puede llegar como una `,` suelta insertada en el texto en vez de abrir el panel (ambigüedad conocida, igual que otras de este proyecto) — si pasa, deshacer con `Ctrl+Z` y usar `Ctrl+K A` en su lugar.
- [ ] `Ctrl+K A` abre el panel a pantalla completa (no se ve el editor detrás): barra lateral a la izquierda con las 5 secciones (Atajos de teclado, Temas, Lenguajes/LSP, Editor, Interfaz), área central a la derecha, barra de contexto abajo.
- [ ] Barra lateral: `↑`/`↓` mueve la selección entre las 5 secciones — todas tienen contenido real hoy (ver las secciones dedicadas más abajo para cada una); "(próximamente)" ya no debería verse en ningún lado del panel.
- [ ] `Enter` o `→` sobre cualquier sección: entra al área central con las filas de esa sección.
- [ ] Dentro de "Editor": 5 filas (Tamaño de tabulación, Usar espacios en vez de tabs, Ajuste de línea, Números de línea, Modo VIM — ver la sección dedicada al modo VIM más abajo). `↑`/`↓` mueve la selección entre filas.
- [ ] Sobre una fila booleana (Usar espacios / Ajuste de línea / Números de línea / Modo VIM): `Enter`, `←` o `→` alternan Sí/No, y el cambio se persiste en `config.toml` al instante (revisar el archivo sin cerrar el panel).
- [ ] Sobre "Tamaño de tabulación": `←`/`→` decrementan/incrementan de 1 en 1, recortado entre 1 y 16 (no baja de 1 ni sube de 16 aunque se siga presionando).
- [ ] `Tab` alterna entre la barra lateral y el área central; `Esc` primero vuelve del área central a la barra, y un segundo `Esc` (ya en la barra) cierra el panel entero y devuelve el foco al editor.
- [ ] `Ctrl+F` dentro del panel (con foco en la barra o en el área central): abre la búsqueda global de opciones. Escribir una palabra sin tildes de un nombre de campo (p. ej. "tabula", "espacios", "ajuste") filtra la lista con las letras coincidentes en negrita; `Enter` salta directo a esa fila en el área central y cierra la búsqueda; `Esc` cancela sin saltar a ningún lado.
- [ ] `Ctrl+S` dentro de la sección Editor: no debería cambiar nada visible (los cambios ya se guardan solos al alternarlos) — solo confirma que no rompe nada.
- [ ] "Panel de administración: Abrir" y "Tema: Seleccionar" aparecen como resultados en la paleta de comandos (`Ctrl+Shift+P`/`F1`) y funcionan igual que sus atajos.
- [ ] Con "Números de línea" en "No": cerrar el panel y confirmar que el editor NO muestra el gutter de números a la izquierda del código. Con "Sí" (el valor por defecto): el gutter aparece, alineado a la derecha, con la línea del cursor en un color distinto al resto (según el tema activo — con Dracula puede no notarse por la misma coincidencia de colores que la selección, ver nota de multi-cursor más arriba; probar con el tema "oscuro" para verlo claramente).
- [ ] Probar en un archivo con más líneas que las que entran en la pantalla, y hacer scroll: el gutter se desplaza junto con el código y sigue mostrando el número real de cada línea (no un contador relativo al viewport).
- [ ] Achicar la ventana de la terminal a un ancho muy angosto con el gutter activo: no debería romper el render — el código sigue siendo legible aunque el gutter se termine ocultando si no entra.
- [ ] Como esta pieza toca el loop de dibujado y agrega una vista de pantalla completa nueva: re-correr al menos la prueba básica de la sección de Windows más abajo.

### Ajuste de línea: reflow real (word wrap)

- [ ] Con "Ajuste de línea" en "No" (el valor por defecto): abrir un archivo con una línea más ancha que la terminal — se recorta al ancho visible, sin partirse en varias filas (comportamiento de siempre, sin cambios).
- [ ] Activar "Ajuste de línea" (`Enter` sobre esa fila en la sección "Editor") y cerrar el panel: la(s) línea(s) más anchas que la terminal ahora se parten en varias filas de pantalla consecutivas, sin cortar ningún carácter (probar también con tildes/eñes: no debe partir un carácter UTF-8 a la mitad).
- [ ] Con el gutter de números activo: solo la PRIMERA fila de cada línea partida muestra su número — las filas de continuación van en blanco, igual que en VSCode.
- [ ] Mover el cursor con `↓`/`↑`/`Home`/`End` hacia y a través de una línea partida: la barra de estado muestra siempre la posición LÓGICA real (`Ln X, Col Y` de la línea completa, no reiniciada por fila) y el cursor visual de la terminal aparece en el lugar correcto de la fila que corresponde.
- [ ] El resaltado de "línea actual" (fondo distinto) cubre TODAS las filas de pantalla que ocupa la línea con el cursor, no solo la primera.
- [ ] Escribir/borrar texto cerca del punto donde una línea se parte: el ajuste se recalcula solo, sin romper nada ni perder texto.
- [ ] Con un archivo que tenga más líneas partidas que las que entran en pantalla: hacer scroll hasta el final y volver al principio — la vista se desplaza de a una FILA de pantalla (no de a una línea lógica completa) y el cursor se mantiene siempre visible, sin saltos raros ni quedar fuera de la ventana.
- [ ] Multi-cursor (`Ctrl+D`) con una selección que caiga en una línea partida: el marcador de cursor secundario (video invertido) aparece una sola vez, en la fila de pantalla correcta — no se duplica en las demás filas de esa misma línea.
- [ ] Un archivo `.py` con `pyright` activo y un diagnóstico en una línea partida: el subrayado del error/aviso cubre todas las filas de pantalla de esa línea.
- [ ] Vista Markdown dividida (`Ctrl+K V`): el ajuste de línea NO tiene efecto en la mitad "fuente" de esa vista en particular (queda como estaba, líneas recortadas) — es una limitación conocida y documentada (esa mitad comparte el scroll con la vista de preview de al lado, que no sabe de filas visuales). El ajuste sí funciona normal en "solo fuente" (sin dividir) del mismo archivo Markdown.
- [ ] Reiniciar `tcode`: el valor de "Ajuste de línea" persiste entre sesiones (queda guardado en `config.toml`).

### Regla vertical / guía de columna (BACKLOG.md P1 #5)

Marca una columna fija de la vista de código con un fondo distinto —
guía de ancho de línea (80/100/120...), como en cualquier otro editor.
Apagada por defecto. El color no es un campo nuevo de cada tema: se
deriva de `background`/`foreground` del tema activo (88% fondo + 12%
texto), así se adapta solo a temas oscuros y claros sin que ningún
archivo de tema haya tenido que tocarse.

- [ ] Sección "Editor" del panel de administración (`Ctrl+K A`): fila "Regla vertical (columna)" muestra "Apagada" por defecto.
- [ ] `→` o `Enter` sobre esa fila estando "Apagada": la prende en la columna 80 — confirmar en `config.toml` (`columna_regla = 80`) y que aparece la línea vertical en el código a esa columna.
- [ ] Con la regla prendida, `←`/`→` decrementan/incrementan la columna de a uno; subir más allá de 300 o bajar de 20 se recorta (no sigue subiendo/bajando).
- [ ] Bajar la columna repetidamente con `←` hasta cruzar 20: en vez de quedar clavada en 20, la fila vuelve a "Apagada" (un solo gesto para apagarla, sin necesitar otra tecla).
- [ ] En un archivo con líneas más cortas Y más largas que la columna configurada: la regla se ve en TODAS las filas (incluidas las líneas vacías/cortas, como una columna "fantasma" más allá del texto), no solo donde hay texto real.
- [ ] La regla se ve tanto en la línea con el cursor (fondo de "línea actual") como en el resto — no desaparece al posicionarse ahí.
- [ ] Con "Ajuste de línea" activado y una línea partida en varias filas de pantalla: la regla se ve en la MISMA columna de pantalla en todas las filas de esa línea (es relativa a la fila visual, no a la línea lógica completa).
- [ ] Con el explorador abierto (`Ctrl+B`): la columna de la regla sigue siendo relativa al área de código (que ahora es más angosta), no a toda la pantalla.
- [ ] Probar con un tema oscuro y uno claro (`Ctrl+K Ctrl+T`): en ambos la regla se distingue del fondo liso sin verse chillona ni invisible.
- [ ] Reiniciar `tcode`: el valor de la regla persiste entre sesiones.

## M4 — Sección "Temas" del panel de administración

- [ ] Dentro del panel (`Ctrl+K A`), la sección "Temas" ya NO dice "(próximamente)" y al entrar muestra 2 filas: "Elegir tema (con preview en vivo)" y "Duplicar tema activo para editar/exportar (<Nombre del tema activo>)".
- [ ] "Elegir tema" + `Enter`: cierra el panel de administración por completo y abre el selector de temas estándar (`Ctrl+K Ctrl+T`) — mismo comportamiento que invocarlo directo, con preview en vivo al navegar y todo.
- [ ] "Duplicar tema activo" + `Enter` (primera vez): aparece un mensaje debajo de la lista ("Copia creada en …") y se crea `~/.config/tcode/themes/<tema-activo>-mio.toml` (o el directorio portable en Windows) con el TOML completo del tema activo, listo para editar a mano.
- [ ] Repetir "Duplicar tema activo" con la copia ya creada: el mensaje cambia a "Ya existía: …" y el archivo NO se sobreescribe (confirmar que su contenido sigue igual si se lo edita a mano entre medio).
- [ ] El mensaje de la última acción se mantiene visible mientras se navega entre las 2 filas de "Temas", pero desaparece al volver a la barra lateral (`Esc`/`Tab`) o cambiar de sección.
- [ ] La búsqueda global del panel (`Ctrl+F`) también encuentra las filas de "Temas" (probar "duplicar" o "elegir") y salta bien a la sección/fila correcta.

## M4 — Sección "Atajos" del panel de administración

- [ ] Dentro del panel (`Ctrl+K A`), la sección "Atajos de teclado" ya NO dice "(próximamente)": al entrar se ven 3 filas especiales — "Restablecer TODOS los atajos por defecto", "Exportar atajos a archivo", "Importar atajos desde archivo" (sin íconos decorativos, a propósito — ver el bug de Windows más abajo) — seguidas de una fila por cada comando de la paleta, con su combinación actual a la derecha (o varias separadas por coma, como "Panel de administración: Abrir" que tiene `Ctrl+,` y `Ctrl+K A`).
- [ ] `Enter` sobre un comando: la fila muestra "‹ presioná la nueva combinación… ›" y la barra inferior cambia a "Presioná la nueva combinación · Esc cancela". Presionar cualquier tecla/combinación (probar una simple como `Ctrl+Alt+U`) la asigna de inmediato: la fila se actualiza, aparece el mensaje "Nuevo atajo: …", y **sin reiniciar el editor**, la tecla vieja deja de funcionar y la nueva sí.
- [ ] Repetir lo anterior pero presionando `Esc` en vez de una combinación: cancela sin cambiar nada (ni el mensaje ni el atajo).
- [ ] Intentar asignarle a un comando una combinación que ya usa OTRO comando distinto (p. ej. `Ctrl+S`, que ya es "Archivo: Guardar"): no se aplica el cambio, aparece "Ya usado por: Archivo: Guardar — no se cambió nada", y el atajo original de "Archivo: Guardar" sigue intacto.
- [ ] `Backspace` sobre un comando ya personalizado: lo devuelve a su atajo por defecto (uno o varios, como "Tema: Seleccionar" con `Ctrl+K Ctrl+T`) y muestra "Restablecido a su atajo por defecto".
- [ ] `Enter` sobre la fila 0 ("Restablecer TODOS"): todos los comandos vuelven a sus atajos por defecto de una vez, y `~/.config/tcode/keymap.toml` (o el directorio portable en Windows) se borra si existía.
- [ ] Personalizar un atajo hasta que quede como prefijo de un chord existente (p. ej. asignarle `Ctrl+K` solo, sin nada después, a cualquier comando): esa fila se pinta en rojo (detección de conflictos en tiempo real) — confirmar que sigue en rojo mientras el conflicto exista y que se restablece a un color normal al arreglarlo.
- [ ] La búsqueda global del panel (`Ctrl+F`) también encuentra comandos por su nombre en español (probar "guardar", "deshacer") y salta a la fila correcta de "Atajos" al confirmar.
- [ ] Cerrar el editor y volver a abrirlo tras personalizar algún atajo: el cambio persistió (`keymap.toml` sigue ahí con la personalización).
- [ ] Con `keymap.toml` de usuario editado A MANO (fuera del panel) mientras `tcode` está corriendo: `Ctrl+K Ctrl+L` (o el comando "Configuración: Recargar" desde la paleta) recarga también el keymap, no solo `config.toml`/tema — un atajo nuevo agregado a mano funciona sin reiniciar.
- [ ] `Backspace` sobre cualquiera de las 3 filas especiales: no hace nada (ese gesto es solo para comandos personalizados).

### Exportar/importar keymap desde archivo

- [ ] `Enter` sobre "Exportar atajos a archivo": crea `keymap-exportado.toml` en el mismo directorio que `keymap.toml` (o el portable en Windows) con el keymap completo activo, y muestra "Exportado a …" con la ruta exacta.
- [ ] `Enter` sobre "Importar atajos desde archivo" SIN haber dejado ningún archivo antes: muestra "No hay nada para importar — dejá el archivo en …", sin romper nada.
- [ ] Copiar el `keymap-exportado.toml` a `keymap-importar.toml` (mismo directorio), editar a mano un atajo dentro (por ejemplo, cambiar `"Ctrl+S" = "archivo.guardar"` a otra combinación), y volver a `Enter` sobre "Importar": muestra "Importado desde …" y **el cambio se aplica en caliente sin reiniciar** — probar que la combinación vieja deja de funcionar y la nueva del archivo importado sí.
- [ ] El keymap importado también queda persistido como el `keymap.toml` activo: cerrar y volver a abrir `tcode` mantiene los atajos importados.
- [ ] Un `keymap-importar.toml` con TOML inválido (por ejemplo, una línea rota a mano): el mensaje muestra el error de parseo en vez de romper el editor o dejarlo con un keymap a medio aplicar.

## M4 — Sección "Lenguajes / LSP" del panel de administración

- [ ] Dentro del panel (`Ctrl+K A`), la sección "Lenguajes / LSP" ya NO dice "(próximamente)": muestra una fila por cada uno de los 5 lenguajes de M1 (Rust, Python, JavaScript, Go, Markdown) con: si está habilitado, el comando LSP configurado (o "(sin LSP configurado)" para los que todavía no tienen uno — todos salvo Python), si ese binario está en el `PATH`, y el estado en vivo ("Conectado"/"Iniciando…"/"Inactivo").
- [ ] Con `pyright` instalado y un archivo `.py` abierto: la fila de "Python" muestra "pyright-langserver --stdio [en el PATH]" y el estado pasa de "Iniciando…" a "Conectado" solo, sin tocar nada — confirma que el panel refleja el estado real de la sesión LSP activa, no un valor fijo.
- [ ] `Enter` (o `←`/`→`) sobre una fila: alterna "Habilitado: Sí"/"No" y persiste en `config.toml` (sección `[lenguajes]`, `lsp_deshabilitado`) al instante.
- [ ] Deshabilitar Python mientras hay una sesión LSP activa para un `.py` abierto: la sesión se cierra sola (el estado pasa a "Inactivo") **sin reiniciar el editor** — confirmar también que los diagnósticos (subrayados) que hubiera desaparecen.
- [ ] Volver a habilitarlo: si el archivo `.py` sigue siendo el activo, el LSP se relanza solo (pasa a "Iniciando…" y después "Conectado").
- [ ] Cerrar el editor y volver a abrirlo con Python deshabilitado: el LSP no se lanza al abrir un `.py`, aunque `pyright` esté instalado.
- [ ] La búsqueda global del panel (`Ctrl+F`) encuentra los lenguajes por su nombre (probar "python", "rust") y salta a la fila correcta de "Lenguajes / LSP".
- [ ] Sin `pyright` instalado (o con el `PATH` alterado para que no se encuentre): la fila de Python muestra "[no encontrado en el PATH]" resaltado, y el comando LSP simplemente no se lanza (sin romper nada) al abrir un `.py`.
- [ ] **Solo en Windows**: con `pyright` instalado vía `npm install -g pyright` (que en Windows deja un `pyright-langserver.cmd`, no un `pyright-langserver` pelado), la fila de Python muestra "[en el PATH]" igual que en Linux/macOS — antes de esta pieza mostraba "[no encontrado en el PATH]" a pesar de estar instalado, porque la detección no probaba las extensiones de `PATHEXT` (`.exe`/`.cmd`/`.bat`/...).

### Comando LSP personalizado por lenguaje (`c` / `Backspace` en "Lenguajes / LSP")

- [ ] Sobre cualquier fila de "Lenguajes / LSP" (foco en el área central), `c` abre un campo de texto en el lugar del comando, con el cursor (`▏`) al final y el pie cambia a "Escribí el comando y sus argumentos · Enter guarda · Esc cancela". Si el lenguaje ya tiene un comando (propio o por defecto, como Python), el campo arranca precargado con ese valor completo, listo para ajustarlo en vez de reescribirlo entero.
- [ ] Escribir "rust-analyzer --stdio" sobre la fila de Rust (que no tiene LSP por defecto) y `Enter`: la fila pasa a mostrar "rust-analyzer --stdio (personalizado) [en el PATH]" (o "[no encontrado en el PATH]" si no está instalado) y el estado pasa a "Iniciando…"/"Conectado" si el archivo activo es de ese lenguaje — confirma en `config.toml` que quedó guardado en `[lenguajes.lsp_comando.rust]` con `comando`/`argumentos` separados.
- [ ] Sobre Python (que sí tiene comando por defecto), editar el precargado agregando un argumento (p. ej. `--verbose`) y `Enter`: la fila muestra "(personalizado)" junto al comando, y si hay un `.py` abierto el proceso se relanza usando el comando nuevo — confirmar con `ps aux | grep pyright` que el proceso real corre con el argumento agregado.
- [ ] `Esc` en vez de `Enter` mientras se edita: descarta el buffer, la fila vuelve a mostrar el valor de antes y no se persiste nada en `config.toml`.
- [ ] Escribir una línea vacía (o solo espacios) y `Enter`: no guarda nada (no tiene sentido un comando en blanco) — la fila queda igual que antes de entrar a editar.
- [ ] `Backspace` sobre una fila (sin estar editando) que tiene un comando personalizado: lo quita y vuelve a usar el de `tcode_lsp::comando_para` por defecto (o "(sin LSP configurado)" si no hay ninguno, como en Rust) — si había una sesión activa con ese lenguaje, se relanza con el comando por defecto (o se cierra, si no queda ninguno).
- [ ] Con un `.py` abierto y el LSP ya "Conectado" con el comando por defecto: editar el comando personalizado de Python en vivo (agregar/quitar un argumento) y confirmar con `Enter` — la sesión vieja se cierra y se relanza sola con el comando nuevo (pasa por "Iniciando…" y vuelve a "Conectado"), sin reiniciar el editor.
- [ ] Cerrar tcode y volver a abrirlo con un `.py` activo: el override de Python persiste (sigue mostrando "(personalizado)" y el LSP se conecta con el comando guardado, no con el de por defecto).

### Primera tanda de lenguajes nuevos: TypeScript, Java, C, C++

- [ ] Abrir un archivo `.ts` y otro `.tsx`: resaltado de sintaxis correcto (palabras clave como `interface`/`function`/`return`, tipos, strings — incluidos los template strings con `${...}` interpolado, comentarios); la statusbar muestra "TypeScript" para ambas extensiones.
- [ ] Abrir un archivo `.java`: resaltado correcto (palabras clave, tipos como `String`/`int`/`void`, nombres de método en verde, números, strings, comentarios); la statusbar muestra "Java".
- [ ] Abrir un archivo `.c`: resaltado correcto (`#include`, tipos, `return`, números, strings, comentarios); la statusbar muestra "C". Abrir un `.h`: también se detecta como C.
- [ ] Abrir un archivo `.cpp` (y opcionalmente `.hpp`): resaltado correcto, igual que C más lo propio de C++; la statusbar muestra "C++".
- [ ] Dentro del panel (`Ctrl+K A` → "Lenguajes / LSP"): ahora hay 9 filas en vez de 5 — las nuevas son TypeScript ("typescript-language-server --stdio"), Java ("(sin LSP configurado)" — sin comando por defecto, se configura a mano con `c` si se quiere), C y C++ (ambas con "clangd", compartido entre las dos).
- [ ] Con `clangd` instalado y un `.c` o `.cpp` abierto: el estado pasa de "Iniciando…" a "Conectado" solo.
- [ ] Un bloque de código Markdown con etiqueta ` ```typescript `, ` ```java `, ` ```c ` o ` ```cpp ` (alias `ts`/`c++`/`cxx` también) se resalta con la vista de preview (`Ctrl+K V`).

### Segunda tanda de lenguajes nuevos: Kotlin, C#, Ruby, PHP

- [ ] Abrir un archivo `.kt` (y opcionalmente `.kts`): resaltado correcto (`fun`/`val`/`return`, tipos como `String`, nombres de función en verde, strings con interpolación `$variable`, comentarios); la statusbar muestra "Kotlin".
- [ ] Abrir un archivo `.cs`: resaltado correcto (`class`/`static`/`void`, tipos, nombres de método, números, strings, comentarios); la statusbar muestra "C#".
- [ ] Abrir un archivo `.rb`: resaltado correcto (`def`/`end`, strings con interpolación `#{...}`, comentarios con `#`); la statusbar muestra "Ruby".
- [ ] Abrir un archivo `.php` que arranca con `<?php`: resaltado correcto (`echo`, variables `$x`, strings, números, comentarios `//`); la statusbar muestra "PHP".
- [ ] Dentro del panel (`Ctrl+K A` → "Lenguajes / LSP"): ahora hay 13 filas (9 de los 13 lenguajes objetivo de PLAN.md §6 — faltan HTML/CSS y SQL, ver la tanda siguiente) — las nuevas son Kotlin ("kotlin-language-server"), C# ("(sin LSP configurado)" — `omnisharp` necesita el directorio del proyecto, se configura a mano con `c` si se quiere), Ruby ("solargraph stdio") y PHP ("intelephense --stdio").
- [ ] Con `solargraph` o `intelephense` instalado y un `.rb`/`.php` abierto respectivamente: el estado pasa de "Iniciando…" a "Conectado" solo.
- [ ] Un bloque de código Markdown con etiqueta ` ```kotlin `, ` ```csharp `, ` ```ruby ` o ` ```php ` (alias `kt`/`cs`/`rb` también) se resalta con la vista de preview (`Ctrl+K V`).

### Tercera tanda de lenguajes nuevos: HTML, CSS, SQL — completa los 13 objetivo

- [ ] Abrir un archivo `.html` (o `.htm`): resaltado correcto (nombres de etiqueta en negrita como color de palabra clave, nombres de atributo de un color distinto, valores de atributo entre comillas como string, comentarios `<!-- -->`); la statusbar muestra "HTML". No hay resaltado de CSS/JS incrustado en `<style>`/`<script>` (mismo criterio que Markdown: solo la gramática de bloque, sin gramáticas inyectadas).
- [ ] Abrir un archivo `.css`: resaltado correcto (selectores de etiqueta/clase, nombres de propiedad como `color`/`background`, valores, comentarios `/* */`); la statusbar muestra "CSS". Con el tema Dracula activo, los nombres de propiedad se ven del mismo color que el texto normal — es a propósito, así resalta CSS el tema Dracula real (no es que falten resaltar).
- [ ] Abrir un archivo `.sql`: resaltado correcto (`SELECT`/`FROM`/`WHERE` en negrita, nombres de tabla, comentarios `-- `); la statusbar muestra "SQL". Los literales numéricos (`42`) se ven con el color de los strings en vez de uno propio — limitación conocida de la query de resaltado que trae la gramática (usa un patrón de Lua que el motor de regex de Rust no entiende), cosmética nomás.
- [ ] Dentro del panel (`Ctrl+K A` → "Lenguajes / LSP"): ahora hay 16 filas — los 13 lenguajes objetivo de PLAN.md §6 completos (HTML y CSS cuentan como 2 filas separadas aunque el plan las liste en una sola). Las nuevas son HTML ("vscode-html-language-server --stdio"), CSS ("vscode-css-language-server --stdio") y SQL ("sqls").
- [ ] Con `vscode-langservers-extracted` instalado (trae los binarios de HTML y CSS) y un `.html`/`.css` abierto: el estado pasa de "Iniciando…" a "Conectado" solo.
- [ ] Un bloque de código Markdown con etiqueta ` ```html `, ` ```css ` o ` ```sql ` se resalta con la vista de preview (`Ctrl+K V`).

### Ver logs de la sesión LSP activa (`Ctrl+K R`)

El stderr de la mayoría de los servidores LSP reales (no forma parte del
protocolo LSP en sí, que va todo por stdout) se descartaba antes por
completo (`Stdio::null()`) — no había forma de ver por qué un servidor no
conectaba o se comportaba raro más allá de "Conectado"/"Iniciando…"/
"Inactivo". Es un snapshot al abrir, no en vivo: cerrar y volver a abrir
muestra lo más nuevo. Sin scroll más allá de lo que entra en pantalla
(mismo overlay que la paleta de comandos) — el filtro es la forma de
encontrar algo que quedó afuera de esa primera pantalla.

- [ ] `Ctrl+K R` (o "LSP: Ver logs de la sesión activa" desde la paleta) sin ninguna sesión LSP activa: muestra "(sin logs — no hay ninguna sesión LSP activa, o no escribió nada en stderr)" en vez de una lista vacía sin explicación.
- [ ] Configurar un comando personalizado que escriba algo a stderr (por ejemplo un script `sh` de una línea con `echo ... >&2; cat`, vía `c` en "Lenguajes / LSP") y abrir un archivo de ese lenguaje: `Ctrl+K R` muestra esas líneas, la más reciente primero (arriba).
- [ ] Escribir texto en el campo de filtro: recorta la lista a las líneas que contienen ese texto (sin distinguir mayúsculas/minúsculas), con la parte que coincidió resaltada en negrita.
- [ ] `Backspace` en el filtro funciona como en cualquier otro campo de texto de la app.
- [ ] `Esc`: cierra el visor sin afectar la sesión LSP activa (no la reinicia ni la corta).
- [ ] Con un `.py` real y `pyright` conectado: la mayoría de los servidores reales se quedan en silencio mientras todo funciona bien — `Ctrl+K R` mostrando el mensaje de "sin logs" con una sesión "Conectado" activa es un resultado esperado, no un bug.

### Variables de entorno por comando LSP (BACKLOG.md, sintaxis `VAR=valor -- comando`)

El campo de edición del comando LSP (`c` en "Lenguajes / LSP") acepta,
antes del comando propiamente dicho, una lista de asignaciones
`VAR=valor` separadas de él por un token `--` suelto — ej.
`RUST_LOG=debug NODE_ENV=production -- rust-analyzer --stdio`. Se eligió
extender esta misma línea en vez de agregar un campo/modal separado
porque reutiliza el editor de texto que ya existe (precarga, `Enter`
guarda, `Esc` cancela) sin superficie de UI nueva, y porque
`ComandoLsp::como_linea()`/`fijar_comando_desde_linea()` son inversas
entre sí, así que lo que se ve al reabrir para editar es exactamente lo
que se guardó.

- [ ] Sobre una fila sin comando (p. ej. Rust) escribir `MI_VAR=hola -- /ruta/a/mi-script.sh` y `Enter`: la fila pasa a mostrar el comando (sin el `MI_VAR=hola --`, que no es parte del comando en sí) seguido de `[+1 var de entorno]`, y `config.toml` (`[lenguajes.lsp_comando.rust]`) queda con `comando`, `argumentos` y una tabla `env = { MI_VAR = "hola" }`.
- [ ] Con dos o más variables (`A=1 B=2 -- comando`): el sufijo dice `[+2 vars de entorno]` (plural correcto a partir de 2).
- [ ] Volver a editar esa fila con `c`: el campo se precarga con la línea completa incluyendo las variables y el separador `--`, lista para ajustar en vez de reescribir todo de cero.
- [ ] Un token antes del `--` que no tiene `=` (p. ej. `MI_VAR -- comando`, sin valor): se ignora en silencio — no rompe el parseo ni termina como parte del comando.
- [ ] Una línea que es solo `-- comando` (sin ninguna variable antes del separador): funciona igual que escribir `comando` directamente, sin sufijo de variables.
- [ ] Una línea que es solo variables y `--` sin comando después (p. ej. `A=1 --`): no guarda nada — igual que dejar el campo vacío, la fila queda como estaba antes de entrar a editar.
- [ ] Un comando sin ningún `--` en la línea (la sintaxis de siempre, sin variables): funciona exactamente igual que antes de esta pieza — comportamiento retrocompatible.
- [ ] Configurar una variable de entorno para un lenguaje con un script de prueba que la vuelque a stderr (p. ej. `echo "MI_VAR=$MI_VAR" >&2`) y abrir un archivo de ese lenguaje: `Ctrl+K R` muestra la línea con el valor real de la variable, confirmando que de verdad llegó al proceso hijo (no solo que se guardó en `config.toml`).
- [ ] Las variables configuradas se suman al entorno heredado del proceso de `tcode`, no lo reemplazan: una variable ya presente en el entorno del sistema (p. ej. `PATH`) sigue estando disponible para el servidor LSP aunque no se la mencione en el campo.

## M4 — Sección "Interfaz" del panel de administración

- [ ] Dentro del panel (`Ctrl+K A`), la sección "Interfaz" ya NO dice "(próximamente)": lista 7 filas — "Mostrar barra de estado" y 6 elementos de la statusbar (posición del cursor, codificación, fin de línea, lenguaje detectado, resumen de diagnósticos LSP, modo), todas en "Sí" por defecto.
- [ ] Desactivar (`Enter`/`←`/`→`) cualquiera de los 6 elementos de la statusbar: esa parte desaparece de la barra de estado real al cerrar el panel (probar "posición del cursor" — debe desaparecer "Ln X, Col Y" pero seguir viéndose el resto).
- [ ] Desactivar "Mostrar barra de estado": la barra entera desaparece y el editor/tabla ocupa esa fila también (no queda una franja vacía) — confirmar en un archivo con más líneas que la pantalla que el scroll sigue funcionando bien con el área más alta.
- [ ] Reactivar ambas cosas: la barra de estado vuelve a verse completa, con el elemento reactivado de nuevo presente.
- [ ] Todos los cambios persisten en `config.toml` (sección `[interfaz]`) al instante, sin reiniciar.
- [ ] Con un `.py` abierto y diagnósticos LSP activos (errores subrayados): desactivar "Statusbar: resumen de diagnósticos LSP" oculta el conteo de errores/avisos de la barra sin afectar el subrayado en el código.
- [ ] Probar con dos paneles divididos (`Ctrl+\`): "Mostrar barra de estado" afecta a los dos por igual (es una sola configuración global, no por panel).
- [ ] La búsqueda global del panel (`Ctrl+F`) encuentra estas filas por nombre (probar "modo", "barra de estado") y salta a la fila correcta.

## M4 — Editor visual de tema (`Ctrl+K Ctrl+P` / `Ctrl+K P`)

- [ ] `Ctrl+K Ctrl+P` (o `Ctrl+K P` si esa combinación no llega en la terminal usada): abre el editor visual a pantalla completa con la lista de ~29 colores del tema activo, cada uno con un "chip" de color (`██`) que se ve del color real, la etiqueta y el valor hex.
- [ ] Si el tema activo no tenía todavía una copia editable (`<tema>-mio.toml`): se crea sola al abrir el editor (mismo mecanismo que "Duplicar tema activo" en la sección Temas del panel admin) y pasa a ser el tema activo — confirmar en `config.toml` (`interfaz.tema`).
- [ ] Si ya existía una copia editable de una sesión anterior: el editor la reabre tal cual, sin volver a duplicar ni perder ediciones previas.
- [ ] `↑`/`↓` navega la lista completa (Editor: UI, Statusbar, Sintaxis, Diagnósticos, Git, Búsqueda).
- [ ] `Enter` sobre una fila: entra en edición con el valor actual precargado (sin el `#`); escribir un código hex nuevo y `Enter` lo aplica al instante — el chip de esa fila cambia de color, **el editor real detrás cambia de verdad** (probar con "UI: Fondo" — el fondo del código cambia sin cerrar el editor visual) y queda guardado en `<tema>-mio.toml`.
- [ ] Escribir caracteres que no sean hex (letras fuera de a-f, símbolos): se ignoran, no se insertan en el campo.
- [ ] Confirmar con un código de largo inválido (por ejemplo, menos de 6 caracteres): aparece "Color inválido: …" y NO se aplica ni se guarda nada — la edición queda abierta para corregir.
- [ ] `Esc` mientras se edita un color: cancela sin aplicar lo escrito, la fila conserva su valor anterior.
- [ ] `Esc` sobre la lista (sin estar editando ningún campo): cierra el editor completo y vuelve al editor de código, con todos los cambios ya guardados hasta ese momento.
- [ ] Cambiar un color de sintaxis (por ejemplo "Sintaxis: Palabra clave") que tenía negrita en el tema original: after el cambio, la palabra clave sigue en negrita en el código, solo cambió el color — confirmar abriendo `<tema>-mio.toml` y viendo que la fila sigue como `{ fg = "...", style = "bold" }`, no se convirtió en un string simple.
- [ ] Cerrar el editor y volver a abrir `tcode`: el tema `<original>-mio` sigue siendo el activo, con los colores editados.

### Paleta predefinida y ajuste HSL (los otros dos métodos de entrada)

- [ ] Con la lista de campos en foco (sin editar nada): la tecla `p` (sin `Ctrl`) abre "Elegir de la paleta predefinida" — 20 colores con nombre y chip de color real; `h` abre "Ajustar HSL con flechas" sobre el campo seleccionado.
- [ ] Paleta: `↑`/`↓` navega los 20 colores (se recorta en los extremos, no da la vuelta); `Enter` aplica el color elegido al campo, lo guarda en `<tema>-mio.toml` y **el editor real cambia de verdad** (mismo criterio que la edición por hex); `Esc` cancela sin aplicar nada.
- [ ] HSL: al entrar se ve un swatch grande con "Color resultante: #rrggbb" y tres filas (Matiz en grados, Saturación y Luminosidad en porcentaje) arrancando desde el color actual del campo, no desde cero.
- [ ] HSL: `←`/`→` cambia cuál de las tres filas está enfocada (resaltada); `↑`/`↓` sube/baja el valor de la fila enfocada — el matiz da la vuelta de 360 a 0 (y viceversa), saturación/luminosidad se recortan en 0 y 100.
- [ ] HSL: cada flecha de ajuste se ve reflejada **al instante** tanto en el swatch de esta pantalla como en el color real del campo en la lista (probar cerrando el ajuste sin confirmar — con `Esc` — para el siguiente punto).
- [ ] HSL: `Esc` a mitad de un ajuste revierte el campo exactamente al color que tenía antes de entrar a HSL (confirmar que `<tema>-mio.toml` no cambió). `Enter` en cambio persiste el color ya aplicado y muestra "Guardado".
- [ ] Un color de sintaxis con `bold`/`italic`: aplicar un color nuevo por paleta o por HSL también conserva el estilo (mismo comportamiento ya confirmado con hex).

## Guardar como (`Ctrl+Shift+S` / `Ctrl+K S`)

- [ ] Abrir `tcode` sin argumentos (buffer nuevo, "[Sin nombre]"), escribir algo y `Ctrl+S`: en vez de no hacer nada, se abre el prompt "Guardar como" — un recuadro centrado con un campo de ruta vacío y "Enter guarda · Esc cancela" debajo.
- [ ] Escribir una ruta (relativa o absoluta) y `Enter`: el archivo se crea en esa ruta con el contenido del buffer, el prompt se cierra, y la statusbar/pestaña pasa a mostrar esa ruta (ya no "[Sin nombre]").
- [ ] Con un archivo ya abierto (con ruta real): `Ctrl+Shift+S` (o `Ctrl+K S` si esa combinación no llega en la terminal usada) abre el mismo prompt, esta vez **precargado con la ruta actual** — cambiarla y `Enter` guarda una copia en la ruta nueva sin tocar ni borrar el archivo original, y la statusbar pasa a mostrar la ruta nueva.
- [ ] También aparece en la paleta de comandos (`Ctrl+Shift+P`/`F1`) como "Archivo: Guardar como...".
- [ ] Dejar el campo vacío y `Enter`: no guarda nada, muestra "la ruta no puede estar vacía" en el prompt (que sigue abierto) en vez de cerrarse o fallar en silencio.
- [ ] Escribir una ruta con un directorio inexistente (ej. `/carpeta-que-no-existe/archivo.txt`) y `Enter`: muestra el error real del sistema de archivos ("no se pudo crear...") sin perder lo escrito ni cerrar el prompt — se puede corregir la ruta ahí mismo.
- [ ] Escribir cualquier cosa después de un error (o `Backspace`): el mensaje de error desaparece del prompt.
- [ ] `Esc` en cualquier momento: cierra el prompt sin guardar nada y sin modificar el archivo/buffer.
- [ ] `Ctrl+S` normal (no `Shift`) sobre un archivo que **ya tiene ruta** sigue guardando directo, sin abrir ningún prompt — el cambio solo afecta al caso "buffer sin nombre" de antes.

## Distribución / instaladores

- [ ] `install/linux.sh` en una máquina Linux limpia (o `install/windows.ps1` en Windows): instala sin pedir contraseña/administrador, y `tcode` queda disponible en cualquier carpeta después de abrir una terminal nueva.
- [ ] La release en GitHub del tag correspondiente tiene los 5 binarios: `tcode-linux-x86_64.tar.gz`, `tcode-linux-arm64.tar.gz`, `tcode-macos-arm64.tar.gz`, `tcode-macos-x86_64.tar.gz`, `tcode-windows-x86_64.zip`.
- [ ] `install/linux.sh` en una VPS/máquina Linux ARM64 real (`uname -m` da `aarch64` — AWS Graviton, Oracle Ampere, Raspberry Pi de 64 bits): detecta la plataforma como `linux-arm64`, descarga ese binario (no el de x86_64) y funciona igual que en x86_64 — confirmar que el binario corre (`tcode --help` o abrir un archivo) sin error de "exec format error".
- [ ] `tcode --version` (o `-v`): imprime `tcode vX.Y.Z` con el tag real de la release instalada (no `-dev`) y termina sin abrir el editor — sirve para confirmar que `install/linux.sh`/`install/windows.ps1` dejaron el binario esperado. Con un binario compilado localmente (`cargo build`, sin pasar por el workflow de release), muestra en cambio `vX.Y.Z-dev` — confirma que no es "una release real" por accidente.
- [ ] Los binarios de Linux (`tcode-linux-x86_64.tar.gz` y `tcode-linux-arm64.tar.gz`) son ahora estáticos (target musl, no gnu) — `file tcode` en Linux debe decir "statically linked" (o no listar ningún intérprete/`.so` dinámico de libc); `ldd tcode` responde "not a dynamic executable". Correrlo en cualquier distro Linux, sin importar qué tan vieja sea su glibc (o directamente sin glibc, como Alpine), no debe dar ningún error `version 'GLIBC_2.XX' not found` — es justamente el problema que este cambio elimina de raíz (dos intentos previos fijando una versión de Ubuntu más vieja en el runner de CI no alcanzaron: siempre hay una VPS con una glibc todavía más vieja que la elegida).

## Modo VIM opcional (`config.editor.modo_vim`)

Apagado por defecto — no cambia el comportamiento de nadie que no lo
prenda a propósito (panel de administración, sección "Editor", o a mano
en `config.toml`). Desde esta entrega tiene gramática completa de
operador + conteo + movimiento/objeto de texto, modo Visual y línea de
comandos `:` (ver la tabla en MANUAL.md). El resto de atajos de tcode
(flechas, `Ctrl+S`, `Ctrl+B`, splits, etc.) siguen funcionando igual en
cualquier modo — el modo VIM solo cambia qué significa un carácter suelto
sin modificador, y solo con el foco en el editor (no en el explorador ni
en la vista de tabla CSV).

- [ ] Con el modo apagado (por defecto): abrir cualquier archivo, escribir texto normal con `hjkl` incluidos — se insertan como letras comunes, nada cambió.
- [ ] Prender "Modo VIM" en el panel de administración (sección Editor): la barra de estado del panel activo pasa a `NORMAL` de inmediato, sin tener que reabrir el archivo.
- [ ] En modo Normal: `h`/`j`/`k`/`l` mueven el cursor (sin pasar de línea con `h`/`l`); `0`/`^`/`$` van al inicio / primer no blanco / fin de línea; `gg`/`G` al inicio/fin del archivo y `5G` a la línea 5.
- [ ] Conteos: `3j`, `5x`, `3dd`, `2yy`, `4p` hacen lo mismo que repetir el comando esa cantidad de veces (y `3dd` se deshace con un solo `u`).
- [ ] `w`/`b`/`e` saltan por palabras (la puntuación es su propia palabra; una línea vacía también cuenta); `W`/`B`/`E` por palabras separadas solo por blancos.
- [ ] `f,`/`t,`/`F,`/`T,` buscan en la línea; `;` repite la búsqueda y `,` la repite hacia el otro lado. `%` salta al paréntesis/llave/corchete que corresponde; `{`/`}` saltan entre párrafos (líneas vacías).
- [ ] Operadores combinables: `dw`, `d$`, `d3w`, `2dw`, `dt,`, `df,`, `dG`, `dgg`, `d}`, `cw`, `c$`, `y3j`, `>j`. `dw` en la última palabra de una línea no se come el salto de línea; `cw` sobre una palabra no se come el espacio de después (como `ce`).
- [ ] Objetos de texto: `diw`/`daw`, `ci"`/`da"`, `di(`/`da(` (y `dib`), `di{`/`da{` — dentro de un bloque `{ ... }` multilínea, `di{` borra las líneas de adentro y deja las llaves en líneas propias.
- [ ] `D`, `C`, `Y`, `cc`, `S`, `s`, `x`, `X`, `r{c}` (con conteo: `3rx`), `J` (con conteo: `3J`), `~`, `>>`/`<<` (con la indentación de la config: espacios o tab).
- [ ] Inserción: `i`, `a`, `I`, `A`, `o`, `O` (estos dos copian la indentación de la línea actual). `Esc` vuelve a Normal con el cursor sobre el último carácter escrito.
- [ ] Un cambio con inserción (`cw` + texto + `Esc`, `o` + texto + `Esc`, `ihola` + `Esc`) se deshace con un solo `u`.
- [ ] `.` repite el último cambio (con su texto tipeado: `cwfoo<Esc>` y después `w.` cambia la siguiente palabra por `foo`); `3.` lo repite con otro conteo. Sin ningún cambio previo avisa "Nada para repetir".
- [ ] Registro: `dd`/`yy`/`Vd` guardan líneas enteras (`p` pega debajo, `P` arriba); `dw`/`x`/`y$`/`vd` guardan caracteres (`p` pega después del cursor, `P` antes). Yanquear/borrar en un archivo y pegar en otra pestaña o panel funciona (el registro es uno solo para toda la app).
- [ ] `v`: la barra de estado dice `VISUAL` y la selección se ve resaltada mientras se mueve el cursor (`hjkl`, `w`, `e`, `$`...); `iw`/`i(`/`a"` extienden la selección al objeto; `o` cambia de extremo; `d`/`x`, `y`, `c`/`s`, `>`/`<`, `J`, `~` operan sobre la selección; `Esc` sale sin hacer nada.
- [ ] `V`: la barra dice `VISUAL LÍNEA`; `d`/`y`/`c`/`>`/`<` operan sobre las líneas enteras. `v` y `V` alternan entre sí; repetir la misma tecla sale de Visual.
- [ ] `:` abre una línea al pie de la pantalla; `Esc` (o `Backspace` con la línea vacía) la cierra sin hacer nada; `↑`/`↓` recorren los comandos ya usados en la sesión.
- [ ] `:w` guarda por el mismo camino que `Ctrl+S` (formatea al guardar si está prendido; sobre un "[Sin nombre]" abre "Guardar como").
- [ ] `:q` sin cambios cierra la pestaña; si es la última pestaña del único panel, sale de tcode. Con cambios avisa "Cambios sin guardar: :q de nuevo para cerrar sin guardar" y no cierra; un segundo `:q` seguido cierra igual; `:q!` cierra sin preguntar. `:wq`/`:x` guardan y cierran. `:qa` sale (avisa si hay archivos modificados), `:qa!` sale igual.
- [ ] `:42` va a la línea 42; `:e otro.txt` abre ese archivo (relativo al directorio donde se lanzó tcode) en una pestaña nueva, o activa la que ya lo tenía; con una ruta que no existe avisa y no abre nada.
- [ ] `:s/a/b/` reemplaza la primera coincidencia en la línea del cursor, `:s/a/b/g` todas las de la línea, `:%s/a/b/g` en todo el archivo (con `i` sin distinguir mayúsculas); la barra de estado dice cuántos reemplazos hizo, y un `u` los deshace todos juntos. El patrón es un regex (sintaxis de Rust, la misma de `Ctrl+F` con regex).
- [ ] Un comando a medio escribir (`d`, `2d`, `ci`...) se ve en la barra de estado; una tecla que no lo completa lo cancela sin hacer nada; `Esc` también.
- [ ] Un panel nuevo por `Ctrl+\` arranca en Normal si el modo VIM está prendido.
- [ ] Con el foco en el explorador, las letras no ejecutan comandos VIM sobre el editor de atrás.
- [ ] Apagar "Modo VIM" desde el panel de administración: `i`/`Esc` siguen funcionando en el panel que ya estaba en Normal (queda ahí hasta salir de Insertar), pero un archivo nuevo que se abra después ya no entra en modo VIM.

### Limitaciones conocidas

- Sin registros con nombre (`"a`), marcas (`m`/`'`), macros (`q`), búsqueda con `/`/`?`/`n`/`*`, `Ctrl+R` como rehacer (sigue siendo el rehacer de tcode), `gJ`/`gu`/`gU`, ni `p` sobre una selección Visual.
- `.` no repite operaciones hechas en modo Visual; un conteo delante de `i`/`a`/`o` (`3ihola`) no repite la inserción.
- En `V` la selección resaltada va desde el borde de la línea del ancla hasta el cursor (no hasta el final de la línea del cursor), aunque la operación sí toma las líneas enteras.
- Moverse con las flechas (no con `hjkl`) en Visual colapsa la selección resaltada hasta la próxima tecla VIM.
- `:s` usa regex de Rust y reemplazo literal (sin `\1`/`&`); sin rangos `:{a},{b}s`. `:e` no crea archivos nuevos; `:w <ruta>` no está soportado (usar "Guardar como").

## ⚠️ Bug conocido en Windows — 5º intento (CRLF sin normalizar)

Ver detalle técnico completo (diagnóstico, intentos de fix ya probados y
descartados) en la memoria del proyecto / historial de PRs de
`fix(windows)`. El 4º intento (símbolos Unicode → ASCII, `BORDE_ASCII`)
se probó en Windows real: arregló los recuadros y diálogos internos
(explorador, selector de temas, paneles — confirmado con capturas, se ven
perfectos), pero el usuario reportó que el bug seguía apareciendo al
abrir archivos reales (`.py`, `.sql`, `.txt`, `.csv`) con texto
completamente descolocado/superpuesto, muy distinto a un simple
corrimiento de columnas.

**Causa real, confirmada por código** (no solo hipótesis): `Buffer::desde_archivo`
leía el archivo con `std::fs::read_to_string` y lo pasaba tal cual a
`ropey::Rope::from_str` sin normalizar el fin de línea. Un archivo con
CRLF (lo normal al editar en Windows) dejaba un `\r` colgando al final
de cada línea — el propio código ya lo admitía en un comentario de
`crates/core/src/csv.rs` ("el resto del editor no tiene ningún soporte
de CRLF"). Ese `\r`, al imprimirse en una terminal real, mueve el cursor
al inicio de la fila — pero el optimizador de `ratatui`/`crossterm` (que
evita un `MoveTo` explícito cuando asume que el cursor avanzó de forma
natural tras el `Print` anterior) no se entera de ese salto: el resto de
esa fila, y el diffing de los frames siguientes, quedan permanentemente
desalineados. Coincide exactamente con las capturas: se rompe con
contenido común y corriente (no solo con íconos), es mucho más grave
cuantas más líneas CRLF tenga el archivo, y "no se autocorrige sola".

Arreglado normalizando el `rope` interno a `\n` siempre al cargar
(recordando si el archivo era CRLF para reescribirlo igual al guardar,
`crates/core/src/buffer.rs`, `Eol`) — la barra de estado ahora también
muestra el fin de línea real (`LF`/`CRLF`) en vez de un `"LF"` fijo.

- [ ] Abrir en Windows Terminal un archivo `.py`/`.sql`/`.txt` real que se sepa que tiene CRLF (cualquier archivo editado antes con Notepad/VS Code en Windows sirve): el texto se ve completo y alineado, sin fragmentos superpuestos ni huecos — el bug de las capturas de este intento.
- [ ] La barra de estado muestra `CRLF` para ese archivo (antes siempre decía `LF`, sin importar el archivo).
- [ ] Editar una línea de ese archivo y guardar (`Ctrl+S`): reabrirlo (o `Get-Content -Raw archivo | Format-Hex` en PowerShell) confirma que sigue usando `\r\n`, no se convirtió a LF por accidente.
- [ ] Abrir un archivo con LF normal (por ejemplo cualquier `.rs` del propio repo clonado en Windows con `core.autocrlf=false`): la barra de estado sigue diciendo `LF`, sin cambios de comportamiento.
- [ ] Repetir la prueba del 4º intento (explorador `Ctrl+B`, panel de administración, paleta de comandos, editor de tema) para confirmar que los bordes ASCII se mantienen bien sin este cambio haber tocado nada ahí.
- [ ] Si el problema reaparece pese a esto: guardar el archivo exacto que falla (no solo una captura) para poder reproducirlo aquí directamente — hasta ahora el diagnóstico se hizo por captura de pantalla, sin poder correr el archivo real.

Nota: una de las capturas de este bug mostró de paso otro problema no
relacionado (tabla CSV con muchas columnas comprimida a 1-2 caracteres
por falta de scroll horizontal) — ya arreglado, ver la sección "Scroll
horizontal con muchas columnas" en M3 — Vista CSV/TSV más arriba.

## Rendimiento: pegar texto grande y mantener teclas apretadas

Antes pegar ~500 líneas tardaba más de un minuto y medio (se veía entrar
"línea por línea"), y mantener apretada una flecha congelaba la pantalla
para después saltar de golpe más allá de donde se quería ir. Medido en
tmux con un `.rs` de 500 líneas: pegar pasó de 104,6 s a 0,02 s, y 300
flechas seguidas de 1 s a 0,02 s.

- [ ] Copiar ~500 líneas de código de otro programa y pegarlas en un archivo (`Cmd+V`/`Ctrl+Shift+V` de la terminal): aparecen todas de una vez, sin demora visible.
- [ ] Lo pegado queda idéntico al original: la indentación NO se acumula línea tras línea (antes cada salto de línea pasaba por `Enter`).
- [ ] Un solo `Ctrl+Z` deshace el pegado completo (no línea por línea ni carácter por carácter).
- [ ] Pegar con una selección activa reemplaza la selección.
- [ ] Mantener apretada `↓` en un archivo largo y soltarla: el cursor se detiene donde se soltó, sin congelarse ni seguir de largo.
- [ ] Pegar con la paleta (`Ctrl+Shift+P`), el buscador (`Ctrl+P`) o "Guardar como" abiertos: el texto va al campo del prompt, sin saltos de línea (y sin confirmarlo solo).
- [ ] Pegar con la confirmación de borrado del explorador abierta NO confirma el borrado aunque lo pegado empiece con `y`.
- [ ] Con un LSP activo (p. ej. un `.py` con pyright), pegar código con un error: el diagnóstico aparece igual que al tipearlo.

### Archivos grandes (miles de líneas)

El resaltado de sintaxis ahora es incremental y solo se calcula para lo
visible (antes cada tecla re-parseaba el archivo entero: ~40 ms por tecla
con 10.000 líneas).

- [ ] Abrir un archivo de código de varios miles de líneas y tipear de corrido en el medio: cada tecla aparece al instante, sin retraso perceptible.
- [ ] Abrir un comentario de bloque (`/*` en Rust/JS/C) o un string sin cerrar en el medio del archivo: todo lo que sigue en pantalla cambia de color al momento; al cerrarlo (o borrarlo) vuelve a su color normal.
- [ ] Ir al final del archivo (`Ctrl+End`) y volver al principio (`Ctrl+Home`): los colores son correctos en ambos extremos (incluidos comentarios o strings de varias líneas que empiezan fuera de la pantalla).
- [ ] Con ajuste de línea activo (`Ctrl+,` → Editor), las líneas largas se siguen partiendo bien, con los números de línea solo en la primera fila de cada una.
- [ ] Dos paneles (`Ctrl+\`) con archivos distintos: cada uno se resalta bien al editar en cualquiera de los dos.

### Archivos grandes: LSP incremental, ajuste de línea y archivos con muchos errores

Segunda parte (BACKLOG.md P1 #14). Si el servidor lo anuncia (pyright
sí), `didChange` lleva solo el tramo editado en vez del archivo entero;
moverse sin editar ya no copia el archivo en cada frame (ni para el
resaltado ni para el LSP); con ajuste de línea solo se parten en filas
las líneas visibles. Medido en tmux con 10.000 líneas, mediana por
frame: `.py` con pyright tipeando 8,7 → 5,3 ms (con ajuste 14,6 → 5,5
ms); moviéndose con ajuste de línea `.rs` 5,3 → 1,4 ms y `.py` 7,6 → 1,9
ms. Un `.py` que en realidad es código Rust (lleno de errores de
sintaxis) pasó de ~1,3 s por tecla a ~7 ms: el re-parseo que tarda más
de 250 ms se cancela y se sigue con los colores anteriores, y se
reintenta cada 2 s.

- [ ] En un `.py` con pyright, tipear un nombre inexistente en una línea que tenga acentos y emoji ANTES de ese punto (p. ej. `x = "😀 ñ" + no_existe`): aparece el error subrayado en esa línea y la barra de estado lo cuenta.
- [ ] Borrarlo: el error desaparece. Hacer varias ediciones más (unir dos líneas con `Backspace` al inicio de una, `Enter`, pegar un bloque, `Ctrl+Z`) dejando el archivo válido: la barra de estado termina sin errores, igual que `pyright archivo.py` sobre lo guardado.
- [ ] Con ajuste de línea activo en un archivo largo con muchas líneas más anchas que la terminal: bajar con `↓` más allá de la pantalla, `Ctrl+End`, `Ctrl+Home`, y `End` en una línea partida — el cursor queda siempre visible, en la fila correcta, sin saltos raros de scroll respecto de la versión anterior.
- [ ] Con ajuste de línea activo, editar una línea partida (agregarle texto hasta que ocupe una fila más, y borrarlo): lo de abajo se corre bien y el cursor sigue en su lugar.
- [ ] Con ajuste de línea activo en un archivo grande, plegar un bloque (`Ctrl+K [`) que ocupa más de una pantalla, moverse por encima y por debajo, `Ctrl+End`/`Ctrl+Home`, y desplegar: las líneas plegadas no se ven ni ocupan filas, el cursor nunca queda fuera de la pantalla y al desplegar todo vuelve a su lugar.
- [ ] Con formatear al guardar activo y rust-analyzer: desordenar la indentación de un `.rs`, guardar (queda formateado), tipear algo más con acentos y volver a guardar: formatea de nuevo sin romper el archivo (el LSP recibe las ediciones incrementales también después de aplicar el formateo).
- [ ] Abrir un archivo grande de código Rust renombrado a `.py` (miles de líneas) y tipear en el medio: las teclas aparecen al instante (antes se trababa más de un segundo por tecla); los colores de lo recién editado pueden quedar aproximados hasta unos segundos después — en un archivo así ya eran incorrectos igual.

## Config por proyecto (`.tcode/config.toml`)

Una `.tcode/config.toml` en el proyecto pisa, clave por clave, la config
global del usuario solo para ese proyecto (BACKLOG.md P2 #8). Para
probar sin tocar tu config real, corré `tcode` con `HOME=<carpeta
temporal>` y armá un repo de prueba (`git init` o una carpeta `.git`
vacía) con, p. ej.:

```toml
[editor]
numeros_de_linea = false
typo_clave = 1

[interfaz]
tema = "claro"

[lenguajes.lsp_comando.python]
comando = "./malicioso.sh"
```

- [ ] Abrir un archivo de ese repo (también en una subcarpeta, p. ej. `src/main.rs`): sin números de línea y con el tema Claro, aunque la global diga lo contrario.
- [ ] Abrir un archivo FUERA del repo (o `tcode` sin argumentos desde otra carpeta): vuelven los valores de la global.
- [ ] Una `.tcode/config.toml` por encima de la raíz git del repo NO se aplica (la búsqueda se detiene en la carpeta que tiene `.git`).
- [ ] `Ctrl+,`: arriba del área central aparece "Config de proyecto activa: <ruta> — pisa: editor.numeros_de_linea, interfaz.tema", el aviso "Ignorado por seguridad: lenguajes.lsp_comando" y "Claves desconocidas: editor.typo_clave". Sin proyecto, esa cabecera no aparece.
- [ ] Sección "Editor": "Números de línea" muestra el valor de la GLOBAL con `[proyecto: No]` al lado. Cambiarlo (o cualquier otra fila) cambia solo la global.
- [ ] Después de cambiar algo desde el panel, abrir la `config.toml` global del `HOME` temporal: tiene el cambio hecho, pero NO `tema = "claro"` ni `numeros_de_linea = false` del proyecto (salvo que se hayan elegido a propósito ahí) ni ningún `lsp_comando` de python.
- [ ] "Lenguajes / LSP": Python muestra su comando por defecto (o el de la global), nunca `./malicioso.sh`. Con `lsp_deshabilitado = ["python"]` en el proyecto, la fila marca `[proyecto: No]`; con `lsp_deshabilitado = []` en el proyecto NO se vuelve a prender un LSP que la global tiene apagado.
- [ ] Romper el TOML del proyecto (p. ej. `[editor` sin cerrar, o `tamano_tabulacion = "cuatro"`): el editor arranca igual con la global, y la cabecera del panel dice "IGNORADA (TOML inválido: ...)" / "(valor inválido: ...)" en rojo.
- [ ] Arreglar el archivo con el editor abierto y `Ctrl+K Ctrl+L` (`config.recargar`): se aplica sin reiniciar. Lo mismo al crear o borrar la `.tcode/config.toml`.

### Limitaciones conocidas de la config por proyecto

- La búsqueda parte del archivo abierto al arrancar (o del cwd) y queda fija toda la sesión: abrir después un archivo de otro repo no cambia la config.
- Un proyecto no puede "apagar" algo que la global fija como opcional (p. ej. la regla vertical: TOML no tiene `null`), ni volver a habilitar un LSP apagado en la global.
- Con `interfaz.tema` pisado por el proyecto, elegir otro tema en el selector (`Ctrl+K Ctrl+T`) lo guarda en la global pero el que se ve sigue siendo el del proyecto.

## Guardado automático (`Ctrl+,` → Editor, BACKLOG.md P2 #4)

Tres modos: **Nunca** (por defecto), **Al perder foco** y **Cada N
segundos**. Solo toca archivos con nombre y con cambios sin guardar.

- [ ] Sin tocar la config (modo "Nunca"): editar un archivo, esperar un minuto, cambiar de panel, abrir el explorador (`Ctrl+B`): el archivo en disco NO cambia y la statusbar sigue mostrando `*`.
- [ ] `Ctrl+,` → Editor: aparecen "Guardado automático" (Nunca) y "Guardado automático: segundos" (30 s); `←`/`→`/`Enter` recorren los tres modos (dando la vuelta) y los segundos van de a 5, entre 5 y 600. Los cambios quedan en `config.toml`.
- [ ] Modo "Cada N segundos" con 5 s: editar y esperar — a los ~5 s el `*` desaparece y el archivo en disco tiene el cambio. Seguir tipeando sin pausa: se guarda cada ~5 s igual.
- [ ] Modo "Al perder foco": editar y esperar — no se guarda solo. Pasar al explorador (`Ctrl+B`), dividir (`Ctrl+\`) o cambiar de panel (`Ctrl+1`/`Ctrl+2`): se guarda en ese momento.
- [ ] Modo "Al perder foco": editar y abrir otro archivo en el mismo panel (`Ctrl+P` o `Enter` en el explorador): el archivo anterior queda guardado en disco antes de ser reemplazado. Abrir solo la paleta/buscador (sin confirmar) NO guarda.
- [ ] Modo "Al perder foco": editar y pasar a otra ventana/pestaña de la terminal (o a otro panel de tmux con `set -g focus-events on`): se guarda. Si la terminal no soporta eventos de foco, este caso no pasa pero los demás sí.
- [ ] Un "[Sin nombre]" (buffer nuevo, p. ej. el segundo panel tras `Ctrl+\`) con texto nunca se guarda solo ni abre "Guardar como" por su cuenta.
- [ ] Guardado que falla (p. ej. `chmod 444` al archivo antes de editar): la statusbar muestra `ERROR: no se pudo guardar: ...` junto a la ruta, el `*` sigue ahí y el editor responde normal. Al devolver el permiso, el siguiente guardado (automático o `Ctrl+S`) funciona y el error desaparece.
- [ ] Con "Al perder foco" y un guardado que falla, intentar abrir otro archivo en ese panel: NO se abre (no se pierden los cambios) y queda el error en la statusbar.
- [ ] En reposo con "Cada N segundos" prendido, `tcode` no consume CPU notable (`top`/`ps`), y pegar ~500 líneas sigue siendo instantáneo.

## Logs del LSP en vivo (`Ctrl+K R`, BACKLOG.md P1 #2)

Para forzar líneas nuevas, un comando LSP que envuelva al real y escriba
a stderr cada segundo (`Ctrl+,` → Lenguajes / LSP → Python → `c`), p. ej.
un script con `( while true; do echo "latido $i" >&2; i=$((i+1)); sleep 1; done ) &`
seguido de `pyright-langserver --stdio`.

- [ ] Abrir un `.py` y `Ctrl+K R`: las líneas nuevas aparecen arriba solas, sin cerrar y reabrir; el campo dice "en vivo".
- [ ] Escribir un filtro mientras llegan líneas: el filtro no se borra ni se pierden teclas; solo aparecen las líneas nuevas que coinciden.
- [ ] `↓`/`AvPág` resaltan una fila y recorren hacia lo más viejo; mientras tanto la fila resaltada se queda en la misma línea de log aunque lleguen nuevas (el texto dice "↑ para volver a lo nuevo").
- [ ] `↑` hasta pasar la primera fila vuelve a "en vivo" (sin fila resaltada, lo nuevo entra arriba).
- [ ] Con el servidor callado (un LSP normal sin la envoltura), el visor abierto no redibuja ni consume CPU en reposo.
- [ ] `Esc` cierra; reabrir con `Ctrl+K R` arranca otra vez en "en vivo" y sin filtro.

## Plegado de bloques (code folding)

Plegar oculta las líneas de un bloque (cuerpo de función, clase, `impl`,
`if`/`for`, objeto/array, comentario de bloque...) y deja visible su
primera línea con un marcador ` ... ` al final; la línea de cierre (`}`,
`end`, `</div>`) queda visible debajo, como en VSCode. Rangos por
tree-sitter para todos los lenguajes con gramática salvo Markdown (sin
plegado a propósito); por indentación para el resto (texto plano, YAML,
TOML...). En terminales sin protocolo Kitty `Ctrl+Shift+[`/`]` y `Ctrl+K
Ctrl+0` pueden no llegar: usar `Ctrl+K [`, `Ctrl+K ]` y `Ctrl+K 0`.

- [ ] En un `.rs` (p. ej. una copia de `crates/core/src/editor.rs`), cursor dentro de una función: `Ctrl+K [` pliega el bloque más interno que contiene al cursor; la cabecera muestra ` ... ` y los números de línea saltan (p. ej. de 10 a 25).
- [ ] Repetir `Ctrl+K [` pliega el bloque siguiente hacia afuera (p. ej. el `if` y después la función entera).
- [ ] `Ctrl+K ]` sobre la cabecera la despliega; los bloques de adentro que estaban plegados siguen plegados.
- [ ] `Ctrl+K 0` pliega todo el archivo; `Ctrl+K Ctrl+J` despliega todo. Con todo plegado, moverse y hacer scroll por un archivo de miles de líneas sigue siendo instantáneo.
- [ ] `↓` desde la cabecera plegada salta a la primera línea después del bloque; `↑` desde ahí vuelve a la cabecera. `→` al final de la cabecera va al principio de la línea siguiente al bloque; `←` al principio de esa línea vuelve al final de la cabecera.
- [ ] `Shift+↓` desde la cabecera selecciona el bloque plegado entero; `Backspace` lo borra y el pliegue desaparece (no queda plegando otras líneas).
- [ ] `Enter` en una línea de más arriba (o borrar una línea de más arriba): el bloque plegado se corre con su contenido y sigue plegado sobre las mismas líneas de código.
- [ ] Escribir en la cabecera plegada (p. ej. renombrar la función) no la despliega; `Enter` en el medio o al final de la cabecera sí.
- [ ] `Ctrl+Z` después de editar cerca de un pliegue: el pliegue vuelve a su lugar (o se despliega si se tocó), nunca oculta líneas equivocadas.
- [ ] `Ctrl+F` buscando un texto que está dentro de un bloque plegado: al saltar a esa coincidencia el bloque se despliega.
- [ ] En Python (`.py`): plegar un `def`/`class` oculta el cuerpo entero desde la línea del `def`; en JavaScript/TypeScript: funciones, objetos, arrays y comentarios `/* */`.
- [ ] Con ajuste de línea activo (`Ctrl+,` → Editor), plegar y desplegar sigue funcionando; el marcador va en la última fila de una cabecera partida en varias filas.
- [ ] En un `.yaml` o `.txt` indentado: `Ctrl+K [` pliega por indentación.
- [ ] En un `.md`, `Ctrl+K [` no hace nada (Markdown no tiene plegado).
- [ ] La paleta (`Ctrl+Shift+P`) lista "Plegado: Plegar el bloque del cursor", "Desplegar...", "Plegar todo" y "Desplegar todo", y funcionan igual que los atajos.
- [ ] Dos paneles (`Ctrl+\`) con archivos distintos: plegar en uno no afecta al otro.

## Indicadores de git en el gutter

BACKLOG.md P2 #6. Una columna entre los números de línea y el código con
`+` (agregada), `~` (modificada) y `-` (líneas borradas justo antes)
respecto de `HEAD`, con los colores de la sección `[git]` del tema. La
base se lee con `git cat-file` en segundo plano al abrir y al guardar; el
diff se recalcula en vivo al escribir. Preparar un repo de prueba:
`git init`, un archivo de ~10 líneas commiteado.

- [ ] Abrir el archivo commiteado sin tocarlo: aparece la columna de git (el código se corre 1 columna a la derecha) pero sin ninguna marca.
- [ ] Agregar una línea nueva en el medio: aparece `+` verde (según el tema) en esa línea, sin guardar.
- [ ] Cambiar una letra de una línea existente: aparece `~` amarilla; al deshacer (`Ctrl+Z`) la marca desaparece.
- [ ] Borrar una línea entera: aparece `-` roja en la línea que quedó justo después del hueco; borrar la última línea del archivo pone el `-` en la nueva última.
- [ ] Reemplazar una línea por dos distintas: las dos quedan con `~`.
- [ ] Guardar (`Ctrl+S`), commitear desde otra terminal y volver a `Ctrl+S` en tcode: las marcas desaparecen.
- [ ] Archivo fuera de cualquier repo: sin columna de git (el código arranca donde siempre), sin errores ni demoras al abrir.
- [ ] Archivo nuevo sin trackear dentro del repo: sin columna ni marcas (decisión: igual que VSCode/Helix, no se marca todo como agregado).
- [ ] `Ctrl+,` → Editor → "Indicadores de git en el gutter" en `No`: la columna desaparece al instante; en `Sí` vuelve con las marcas al día.
- [ ] Con "Números de línea" apagado y el archivo en un repo: se sigue viendo la columna de git sola (marca + espacio); con un archivo fuera de un repo no hay gutter en absoluto.
- [ ] Con ajuste de línea activo, una línea larga agregada muestra `+` en todas sus filas de pantalla.
- [ ] Cambiar de tema (`Ctrl+K Ctrl+T`): los colores de las marcas cambian con el tema.
- [ ] Un repo con archivos CRLF (`core.autocrlf` o commiteados así): abrir uno sin tocarlo no muestra marcas.
- [ ] Archivo de 10.000 líneas commiteado: tipear de corrido en el medio sigue siendo instantáneo.
- [ ] Sin `git` en el `PATH` (p. ej. `PATH=/nada "$(command -v tcode)" archivo`): abre normal, sin columna ni errores.

## Modo zen y panel maximizado (`Ctrl+K Z`, `F11`/`Ctrl+K G`)

BACKLOG.md P3 #12 y #11. El modo zen es de sesión (no se guarda en la
config): oculta explorador y statusbar sin tocar sus toggles. "Pantalla
completa" en una terminal = maximizar el panel activo de un split (como
el zoom de tmux); el tamaño de letra y la ventana los controla el
emulador de terminal, no tcode.

- [ ] Con el explorador visible y enfocado (`Ctrl+B`), `Ctrl+K Z`: se ven solo los paneles de código, sin explorador ni statusbar; las flechas mueven el cursor del editor, no la selección del árbol.
- [ ] Otra vez `Ctrl+K Z`: vuelven explorador y statusbar, y el foco vuelve al explorador (las flechas mueven la selección del árbol).
- [ ] Con la statusbar apagada en `Ctrl+,` → Interfaz, entrar y salir de zen: sigue apagada (zen no toca la config; `config.toml` no cambia).
- [ ] En zen: `F1`/`Ctrl+Shift+P` abre la paleta, `Ctrl+P` el buscador, `Ctrl+F` la búsqueda, `Ctrl+K Ctrl+T` el selector de temas — todos se ven y funcionan; al cerrarlos se sigue en zen.
- [ ] En zen, abrir un archivo desde `Ctrl+P`: se abre y se sigue en zen.
- [ ] En zen, `Ctrl+B`: sale del zen y muestra el explorador enfocado (aunque antes del zen estuviera oculto). `Ctrl+K J`, `Ctrl+K N`/`C`/`M` también salen del zen antes de actuar.
- [ ] Con 2 paneles, `F11` (o `Ctrl+K G` si la terminal/SO se come `F11`): el panel activo ocupa toda el área de edición, con `[MAX]` a la derecha de su statusbar. Otra vez: vuelven los dos paneles como estaban.
- [ ] Con 3 paneles (vertical + horizontal), maximizar el del medio y restaurar: el layout vuelve idéntico, con el mismo panel activo.
- [ ] Maximizado, `Ctrl+K 1`/`2`/`3` (o `Ctrl+1/2/3`): sale del maximizado y va a ese panel (como tmux). Dividir (`Ctrl+K \`) o cerrar (`Ctrl+K F`) también sale del maximizado primero.
- [ ] Con un solo panel, `F11` no hace nada (sin `[MAX]`).
- [ ] Zen + maximizado: solo el código del panel activo en toda la pantalla (sin `[MAX]`, porque no hay statusbar). Salir de zen: vuelve la statusbar con `[MAX]`; `F11`: vuelven todos los paneles.
- [ ] En la paleta, buscar "zen" y "maximizar": aparecen "Ver: Alternar modo zen (solo el código)" y "Ver: Maximizar/restaurar el panel activo" y funcionan.
- [ ] Salir de tcode en zen y volver a abrir: arranca normal (el zen no se recuerda).

## Pestañas de archivos abiertos (`Ctrl+PageDown`/`Ctrl+PageUp`, `Ctrl+W`, `Alt+N`)

BACKLOG.md P3 #10 (primera mitad). Cada panel tiene sus pestañas; abrir
un archivo agrega una en vez de reemplazar lo que se estaba viendo.

- [ ] `tcode` sin argumentos y abrir un archivo con `Ctrl+P`: queda una sola pestaña (el "[Sin nombre]" vacío se reemplaza, no queda al lado).
- [ ] Abrir 4 archivos con `Ctrl+P`/explorador: 4 pestañas, cada una nueva a la derecha de la activa, y la activa resaltada.
- [ ] Dos archivos con el mismo nombre en carpetas distintas (`a/mod.rs`, `b/mod.rs`): las pestañas muestran `a/mod.rs` y `b/mod.rs`; uno solo muestra `mod.rs`.
- [ ] `Ctrl+PageDown`/`Ctrl+PageUp` recorren las pestañas y dan la vuelta en los extremos. `Ctrl+K PageDown`/`Ctrl+K PageUp` hacen lo mismo.
- [ ] `Alt+1`..`Alt+9` van a esa pestaña (en macOS con Option como Meta); un número sin pestaña no hace nada. En la paleta, "Pestañas" lista siguiente/anterior/cerrar/ir a la N.
- [ ] Editar en una pestaña, mover el cursor y el scroll, plegar un bloque, cambiar de pestaña y volver: texto, `*`, cursor, scroll, pliegue y `Ctrl+Z` siguen ahí.
- [ ] Un `.md` con preview (`Ctrl+K V`) y un `.csv` en vista tabla con una celda seleccionada: al ir y volver conservan su vista.
- [ ] Volver a abrir con `Ctrl+P` un archivo ya abierto (y modificado) en el panel: solo se activa su pestaña, sin perder los cambios.
- [ ] `Ctrl+W` en una pestaña sin cambios: se cierra y queda activa la de su derecha (o la anterior, si era la última).
- [ ] `Ctrl+W` con cambios: no cierra y la statusbar dice "Cambios sin guardar: Ctrl+W de nuevo...". Otra tecla y después `Ctrl+W`: vuelve a avisar. `Ctrl+W` dos veces seguidas: cierra sin guardar (el archivo en disco no cambia).
- [ ] `Ctrl+W` en la última pestaña del único panel: queda "[Sin nombre]" vacío. Con split: se cierra ese panel.
- [ ] `Ctrl+K F` sobre un panel con alguna pestaña modificada (aunque no sea la visible): pide repetirlo; sin cambios, cierra directo.
- [ ] Con cambios en una pestaña que NO es la activa, `Ctrl+Q`: no sale y avisa "1 archivo con cambios sin guardar"; otro `Ctrl+Q` seguido sale.
- [ ] Split (`Ctrl+K \`) con pestañas distintas en cada panel: cada barra muestra las suyas; la activa del panel con foco va en negrita.
- [ ] 15 pestañas en un panel angosto: la activa siempre se ve, con `<`/`>` en los bordes cuando hay más de ese lado; `Alt+1` lleva la barra al principio.
- [ ] `Ctrl+,` → Interfaz → "Mostrar pestañas" en No: desaparece la barra y el código sube una fila; los atajos siguen andando. `config.toml` global tiene `mostrar_pestanas = false`.
- [ ] Modo zen (`Ctrl+K Z`): sin barra de pestañas; al salir vuelve. Maximizado (`F11`/`Ctrl+K G`): la barra del panel maximizado se ve.
- [ ] Guardado automático "al perder foco": editar una pestaña y cambiar a otra con `Ctrl+PageDown`: la primera se guarda (desaparece el `*`).
- [ ] LSP (pyright): `malo.py` con un error de tipos y `bueno.py` sin errores en dos pestañas: al alternar, la statusbar muestra "1 error" solo en `malo.py`; escribir un error en `bueno.py` lo marca ahí y no en `malo.py`.
- [ ] Archivo de 200.000 líneas en una pestaña: `Ctrl+PageDown`/`Ctrl+PageUp` mantenidos cambian de pestaña al instante, y al volver sigue en la misma línea.

## Temas en formato Helix

BACKLOG.md P3 #13. Un `.toml` de tema de Helix dejado en la carpeta de
temas del usuario (`~/.config/tcode/themes/`, en macOS `~/Library/
Application Support/tcode/themes/`) se convierte al vuelo al cargarlo;
el archivo nunca se reescribe. Hay temas de ejemplo en
`crates/config/tests/fixtures/helix/` (`onedark`, `onelight`, `gruvbox`
y `gruvbox_dark_hard`, que hereda del anterior); también sirve cualquiera
de `runtime/themes/` del repo de Helix.

- [ ] Copiar `onedark.toml` y `onelight.toml` a la carpeta de temas: sin reiniciar, `Ctrl+K Ctrl+T` los lista al final como "onedark (Helix)" y "onelight (Helix)".
- [ ] `Tab` hasta el filtro "Oscuro": aparece `onedark (Helix)` y no `onelight`; en "Claro", al revés.
- [ ] Moverse sobre `onedark (Helix)`: preview en vivo con fondo `#282c34`, palabras clave violetas, strings verdes, comentarios grises en cursiva; `Enter` lo deja activo y sobrevive a reiniciar `tcode`.
- [ ] Copiar `gruvbox.toml` y `gruvbox_dark_hard.toml`: el segundo (solo `inherits = "gruvbox"` + otro `bg0`) se ve como gruvbox pero con fondo más oscuro (`#1d2021`).
- [ ] Borrar `gruvbox.toml` y volver a elegir `gruvbox_dark_hard`: no falla — usa el tema base "Oscuro" de `tcode` para todo lo que no define (fondo `#1d2021` igual).
- [ ] Un tema de Helix con scopes raros, modificadores desconocidos o colores que no existen en `[palette]`: se lista y se aplica igual (lo que no entiende queda con el color de texto).
- [ ] Un `.toml` roto (TOML inválido) en la carpeta: no aparece en el selector y no rompe nada.
- [ ] Con un tema de Helix activo, `Ctrl+,` → Temas → "Duplicar tema activo": crea `<tema>-mio.toml` en formato `tcode` (con `name`, `[ui]`, `[statusbar]`...); el archivo de Helix original queda byte a byte igual.
- [ ] Con un tema de Helix activo, `Ctrl+K Ctrl+P`: el editor visual muestra sus colores convertidos; cambiar uno guarda en `<tema>-mio.toml` (formato `tcode`) y nunca toca el original.

## Breadcrumbs (ruta > símbolos arriba del código)

BACKLOG.md P3 #10 (segunda mitad). Una fila arriba del código de cada
panel: ruta del archivo relativa a la raíz del repo git (o al directorio
de trabajo si no hay repo) y los contenedores con nombre que encierran
al cursor. Símbolos para Rust, Python, JavaScript/TypeScript, Go, Java,
C/C++, C#, Kotlin, Ruby, PHP y encabezados de Markdown; el resto (HTML,
CSS, SQL, texto plano) muestra solo la ruta.

- [ ] Desde la raíz del repo, `tcode crates/core/src/editor.rs`: arriba del código se ve `crates > core > src > editor.rs`.
- [ ] Bajar hasta adentro de un método de `impl Editor`: `... > editor.rs > impl Editor > fn <método>`; moverse al método siguiente y el breadcrumb cambia; en la línea en blanco entre dos métodos queda solo `impl Editor`.
- [ ] Con el cursor en la indentación de la línea `fn ...` o justo después del `}` que la cierra: sigue mostrando esa `fn` (no el `impl` de afuera).
- [ ] Bajar hasta `mod tests`: `mod tests > fn <test>`.
- [ ] Un `.py` con `class A:` / `def b(self):` adentro: `class A > def b`; una función suelta: `def f`; una línea fuera de todo: solo la ruta.
- [ ] Un `.ts` con `namespace` > `class` > método: `namespace X > class Y > metodo()`; `const f = () => {...}`: `f()`.
- [ ] Un `.md`: los encabezados en los que está el cursor (`# Título > ## Sección`).
- [ ] Un `.txt` (sin lenguaje): solo la ruta. Un buffer nuevo (`tcode` sin argumentos, o el panel nuevo de un split): `[Sin nombre]`.
- [ ] Un CSV en vista de tabla: solo la ruta (sin símbolos).
- [ ] Achicar la ventana de a poco: primero desaparecen las carpetas del medio (`crates > .. > src > editor.rs > ...`), después todas (`.. > editor.rs > ...`), después los símbolos de afuera (`editor.rs > .. > fn interna`) y por último se acorta el símbolo más interno con `..`. El nombre del archivo y el símbolo más interno siempre quedan. Sin caracteres raros ni desalineados (separador ASCII ` > `).
- [ ] Tipear rápido en un archivo Rust de 10.000 líneas, con breadcrumbs prendidos y apagados: se siente igual (la consulta de símbolos usa el árbol que ya tiene el resaltado, cacheada por revisión + posición del cursor).
- [ ] `Ctrl+,` (o `Ctrl+K A`) → Interfaz → "Mostrar breadcrumbs (ruta > símbolos)": apagarlo saca la fila en todos los paneles y el código sube una fila; `config.toml` queda con `mostrar_breadcrumbs = false`. Prenderlo de nuevo: vuelve.
- [ ] Con 2 paneles, cada uno muestra su propio breadcrumb; maximizado (`F11`/`Ctrl+K G`) el panel activo lo sigue mostrando.
- [ ] Con la barra de pestañas prendida: el orden de arriba hacia abajo es pestañas, breadcrumbs, código. Abrir otro archivo en una pestaña nueva y alternar (`Ctrl+PageDown`/`Ctrl+PageUp`): el breadcrumb pasa a la ruta y los símbolos del cursor de la pestaña activa, sin demora.
- [ ] Apagar "Mostrar pestañas" y dejar los breadcrumbs: los breadcrumbs quedan en la primera fila del panel.
- [ ] Modo zen (`Ctrl+K Z`): los breadcrumbs se ocultan junto con la statusbar y las pestañas; al salir del zen vuelven (sin tocar la config).

---

Si algo de esta lista falla, abrir un PR contra `develop` con el fix (nunca
directo a `main`) y volver a correr la sección correspondiente antes de
cerrarlo — ver el flujo de ramas en el [README](./README.md#flujo-de-ramas).
