# Plan de pruebas manuales — tcode

Checklist para probar `tcode` de punta a punta antes de liberar una nueva
versión. Cubre todo lo implementado hasta la fecha: M0, M1, M2 y M3
completos (ver [PLAN.md](./PLAN.md) §11 para el detalle de cada milestone).

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
- [ ] Editar `keymap.toml` de usuario (copiarlo del embebido), cambiar un atajo, `Ctrl+K Ctrl+L`: el nuevo atajo funciona sin reiniciar.
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

## Distribución / instaladores

- [ ] `install/linux.sh` en una máquina Linux limpia (o `install/windows.ps1` en Windows): instala sin pedir contraseña/administrador, y `tcode` queda disponible en cualquier carpeta después de abrir una terminal nueva.
- [ ] La release en GitHub del tag correspondiente tiene los 4 binarios: `tcode-linux-x86_64.tar.gz`, `tcode-macos-arm64.tar.gz`, `tcode-macos-x86_64.tar.gz`, `tcode-windows-x86_64.zip`.

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
