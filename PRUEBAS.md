# Plan de pruebas manuales — tcode

Checklist para probar `tcode` de punta a punta antes de liberar una nueva
versión. Cubre todo lo implementado hasta la fecha: M0, M1, M2 y M3
completos, y M4 en progreso (ver [PLAN.md](./PLAN.md) §11 para el detalle
de cada milestone). Las secciones de M4 se van agregando pieza por pieza,
a medida que cada una se mergea a `develop`.

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
- [ ] Barra de estado inferior: `Ln`/`Col` correctos, cuenta total de líneas, marca `●` cuando hay cambios sin guardar.

## M1 — Configuración, temas, atajos, sintaxis, explorador

- [ ] `~/.config/tcode/config.toml` (o el equivalente portable en Windows) se crea solo la primera vez, con valores razonables.
- [ ] Cambiar `tema` en `config.toml` a `"oscuro"` o `"claro"`, `Ctrl+K Ctrl+L` (recargar config): el tema cambia en caliente sin reiniciar.
- [ ] Editar `keymap.toml` de usuario (copiarlo del embebido), cambiar un atajo, `Ctrl+K Ctrl+L`: el nuevo atajo funciona sin reiniciar (recién a partir de M4 pieza "editor de atajos" esto quedó realmente implementado — antes de esa pieza `Ctrl+K Ctrl+L` solo recargaba `config.toml`/tema, no el keymap, aunque este checklist ya lo daba por hecho).
- [ ] Abrir un archivo `.rs`, `.py`, `.js`, `.go` y `.md`: resaltado de sintaxis correcto (palabras clave, strings, comentarios, números).
- [ ] Abrir un archivo de una extensión no soportada: se ve como texto plano sin colorear, sin romperse.
- [ ] `Ctrl+B`: abre/cierra el explorador de archivos lateral.
- [ ] Con el explorador enfocado: `↑`/`↓` mueve la selección, `Enter` sobre una carpeta la expande/colapsa, `Enter` sobre un archivo lo abre en el editor y devuelve el foco a este último.
- [ ] `Esc` con el explorador visible: devuelve el foco al editor sin cerrar el explorador.

## M2 — Paleta de comandos, buscador de archivos, splits, LSP

- [ ] `Ctrl+Shift+P` o `F1`: abre la paleta de comandos.
- [ ] Escribir en la paleta filtra por coincidencia difusa (no hace falta escribir el nombre completo ni en orden exacto de palabras).
- [ ] `↑`/`↓` navega los resultados, `Enter` ejecuta el comando seleccionado, `Esc` cierra sin ejecutar nada.
- [ ] `Ctrl+P`: abre el buscador de archivos (fuzzy finder) del proyecto; mismo comportamiento de filtro/navegación/`Enter`/`Esc`.
- [ ] `Ctrl+\` divide el panel activo verticalmente (lado a lado); `Ctrl+K Ctrl+\` lo divide horizontalmente (apilado).
- [ ] `Ctrl+1`/`Ctrl+2`/`Ctrl+3` cambian de panel; `Ctrl+K F` cierra el panel activo (nunca el último que queda).
- [ ] Abrir un archivo `.py` con `pyright` instalado (`npm install -g pyright`): aparecen diagnósticos (subrayado) al escribir código con errores, y desaparecen al corregirlos.
- [ ] La barra de estado muestra el resumen de errores/avisos cuando hay diagnósticos LSP activos.

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

- [ ] `Ctrl+K Ctrl+T` (o "Tema: Seleccionar" desde la paleta de comandos, `Ctrl+Shift+P`/`F1`): abre el selector con los 12 temas embebidos, marcando con `●` el que está activo en ese momento.
- [ ] `↑`/`↓`: el editor de fondo cambia de tema en vivo con cada movimiento, sin tocar `config.toml` todavía (revisar el archivo mientras el selector sigue abierto: no debería haber cambiado).
- [ ] `Tab`: cicla el filtro `Todos` → `Oscuro` → `Claro` → `Todos`; la lista se recorta a los temas de ese tipo (los `light` son solo Solarized Light, GitHub Light y Claro).
- [ ] `Enter` sobre un tema: cierra el selector, el tema queda aplicado, y persiste en `config.toml` — reabrir `tcode` y confirmar que arranca con ese mismo tema.
- [ ] `Esc`: cierra el selector y vuelve exactamente al tema que estaba activo antes de abrirlo (no al primero de la lista ni al último visto en el preview), sin modificar `config.toml`.
- [ ] Revisar de pasada que los 10 temas nuevos (Monokai, One Dark, Nord, Gruvbox Dark, Tokyo Night, Catppuccin Mocha, Solarized Dark, Solarized Light, GitHub Light) se ven con colores razonables y texto legible, no solo Dracula/oscuro/claro.
- [ ] Como esta pieza agrega una ruta modal nueva al loop de dibujado (`crates/app/src/main.rs`): re-correr al menos la prueba básica de la sección de Windows más abajo, aunque no toque directamente el explorador.

## M4 — Panel de administración (`Ctrl+,` / `Ctrl+K A`) y números de línea

- [ ] `Ctrl+,` para abrir el panel: en terminales sin protocolo Kitty puede llegar como una `,` suelta insertada en el texto en vez de abrir el panel (ambigüedad conocida, igual que otras de este proyecto) — si pasa, deshacer con `Ctrl+Z` y usar `Ctrl+K A` en su lugar.
- [ ] `Ctrl+K A` abre el panel a pantalla completa (no se ve el editor detrás): barra lateral a la izquierda con las 5 secciones, área central a la derecha, barra de contexto abajo.
- [ ] Barra lateral: `↑`/`↓` mueve la selección entre las 5 secciones; las que todavía no tienen contenido real (Atajos, Temas, Lenguajes/LSP, Interfaz) se marcan "(próximamente)" y su área central muestra un resumen de qué van a traer, en vez de quedar vacía.
- [ ] `Enter` o `→` sobre "Editor" (la única sección implementada por ahora): entra al área central; sobre cualquier sección "(próximamente)" no hace nada.
- [ ] Dentro de "Editor": 4 filas (Tamaño de tabulación, Usar espacios en vez de tabs, Ajuste de línea, Números de línea). `↑`/`↓` mueve la selección entre filas.
- [ ] Sobre una fila booleana (Usar espacios / Ajuste de línea / Números de línea): `Enter`, `←` o `→` alternan Sí/No, y el cambio se persiste en `config.toml` al instante (revisar el archivo sin cerrar el panel).
- [ ] Sobre "Tamaño de tabulación": `←`/`→` decrementan/incrementan de 1 en 1, recortado entre 1 y 16 (no baja de 1 ni sube de 16 aunque se siga presionando).
- [ ] `Tab` alterna entre la barra lateral y el área central; `Esc` primero vuelve del área central a la barra, y un segundo `Esc` (ya en la barra) cierra el panel entero y devuelve el foco al editor.
- [ ] `Ctrl+F` dentro del panel (con foco en la barra o en el área central): abre la búsqueda global de opciones. Escribir una palabra sin tildes de un nombre de campo (p. ej. "tabula", "espacios", "ajuste") filtra la lista con las letras coincidentes en negrita; `Enter` salta directo a esa fila en el área central y cierra la búsqueda; `Esc` cancela sin saltar a ningún lado.
- [ ] `Ctrl+S` dentro de la sección Editor: no debería cambiar nada visible (los cambios ya se guardan solos al alternarlos) — solo confirma que no rompe nada.
- [ ] "Panel de administración: Abrir" y "Tema: Seleccionar" aparecen como resultados en la paleta de comandos (`Ctrl+Shift+P`/`F1`) y funcionan igual que sus atajos.
- [ ] Con "Números de línea" en "No": cerrar el panel y confirmar que el editor NO muestra el gutter de números a la izquierda del código. Con "Sí" (el valor por defecto): el gutter aparece, alineado a la derecha, con la línea del cursor en un color distinto al resto (según el tema activo — con Dracula puede no notarse por la misma coincidencia de colores que la selección, ver nota de multi-cursor más arriba; probar con el tema "oscuro" para verlo claramente).
- [ ] Probar en un archivo con más líneas que las que entran en la pantalla, y hacer scroll: el gutter se desplaza junto con el código y sigue mostrando el número real de cada línea (no un contador relativo al viewport).
- [ ] Achicar la ventana de la terminal a un ancho muy angosto con el gutter activo: no debería romper el render — el código sigue siendo legible aunque el gutter se termine ocultando si no entra.
- [ ] Como esta pieza toca el loop de dibujado y agrega una vista de pantalla completa nueva: re-correr al menos la prueba básica de la sección de Windows más abajo.

## M4 — Sección "Temas" del panel de administración

- [ ] Dentro del panel (`Ctrl+K A`), la sección "Temas" ya NO dice "(próximamente)" y al entrar muestra 2 filas: "Elegir tema (con preview en vivo)" y "Duplicar tema activo para editar/exportar (<Nombre del tema activo>)".
- [ ] "Elegir tema" + `Enter`: cierra el panel de administración por completo y abre el selector de temas estándar (`Ctrl+K Ctrl+T`) — mismo comportamiento que invocarlo directo, con preview en vivo al navegar y todo.
- [ ] "Duplicar tema activo" + `Enter` (primera vez): aparece un mensaje debajo de la lista ("Copia creada en …") y se crea `~/.config/tcode/themes/<tema-activo>-mio.toml` (o el directorio portable en Windows) con el TOML completo del tema activo, listo para editar a mano.
- [ ] Repetir "Duplicar tema activo" con la copia ya creada: el mensaje cambia a "Ya existía: …" y el archivo NO se sobreescribe (confirmar que su contenido sigue igual si se lo edita a mano entre medio).
- [ ] El mensaje de la última acción se mantiene visible mientras se navega entre las 2 filas de "Temas", pero desaparece al volver a la barra lateral (`Esc`/`Tab`) o cambiar de sección.
- [ ] La búsqueda global del panel (`Ctrl+F`) también encuentra las filas de "Temas" (probar "duplicar" o "elegir") y salta bien a la sección/fila correcta.

## M4 — Sección "Atajos" del panel de administración

- [ ] Dentro del panel (`Ctrl+K A`), la sección "Atajos de teclado" ya NO dice "(próximamente)": al entrar se ven 3 filas especiales — "↺ Restablecer TODOS los atajos por defecto", "⇩ Exportar atajos a archivo", "⇧ Importar atajos desde archivo" — seguidas de una fila por cada comando de la paleta, con su combinación actual a la derecha (o varias separadas por coma, como "Panel de administración: Abrir" que tiene `Ctrl+,` y `Ctrl+K A`).
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

- [ ] `Enter` sobre "⇩ Exportar atajos a archivo": crea `keymap-exportado.toml` en el mismo directorio que `keymap.toml` (o el portable en Windows) con el keymap completo activo, y muestra "Exportado a …" con la ruta exacta.
- [ ] `Enter` sobre "⇧ Importar atajos desde archivo" SIN haber dejado ningún archivo antes: muestra "No hay nada para importar — dejá el archivo en …", sin romper nada.
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

## Distribución / instaladores

- [ ] `install/linux.sh` en una máquina Linux limpia (o `install/windows.ps1` en Windows): instala sin pedir contraseña/administrador, y `tcode` queda disponible en cualquier carpeta después de abrir una terminal nueva.
- [ ] La release en GitHub del tag correspondiente tiene los 5 binarios: `tcode-linux-x86_64.tar.gz`, `tcode-linux-arm64.tar.gz`, `tcode-macos-arm64.tar.gz`, `tcode-macos-x86_64.tar.gz`, `tcode-windows-x86_64.zip`.
- [ ] `install/linux.sh` en una VPS/máquina Linux ARM64 real (`uname -m` da `aarch64` — AWS Graviton, Oracle Ampere, Raspberry Pi de 64 bits): detecta la plataforma como `linux-arm64`, descarga ese binario (no el de x86_64) y funciona igual que en x86_64 — confirmar que el binario corre (`tcode --help` o abrir un archivo) sin error de "exec format error".

## ⚠️ Bug conocido en Windows — todavía sin resolver

Ver detalle técnico completo (diagnóstico, intentos de fix ya probados y
descartados) en la memoria del proyecto / historial de PRs de
`fix(windows)`. Resumen para volver a probar tras cualquier cambio que
toque el render (`crates/app/src/main.rs`, `crates/ui/**`):

- [ ] En Windows (PowerShell o CMD, con y sin Windows Terminal), abrir el explorador con `Ctrl+B`, seleccionar "abrir archivo": verificar que el contenido del archivo y el árbol del explorador se dibujan completos, sin caracteres faltantes ni artefactos.
- [ ] Repetir la prueba anterior en una ventana angosta (~120x30) y en una ancha (~209x51) — el ancho de la ventana fue un factor real en versiones anteriores.
- [ ] Si el problema reaparece: anotar versión de Windows Terminal (`$env:WT_SESSION`, y su versión desde el menú "Acerca de"), versión de PowerShell (`$PSVersionTable`), tamaño exacto de ventana, y en qué momento exacto se ve mal (¿ya al abrir el archivo, o recién al desplazarse con las flechas?).

---

Si algo de esta lista falla, abrir un PR contra `develop` con el fix (nunca
directo a `main`) y volver a correr la sección correspondiente antes de
cerrarlo — ver el flujo de ramas en el [README](./README.md#flujo-de-ramas).
