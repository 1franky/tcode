# Manual de uso de `tcode`

Guía práctica para gente que ya instaló `tcode` y quiere usarlo — para el
diseño interno, arquitectura y roadmap del proyecto ver [PLAN.md](./PLAN.md).

## Índice

- [Primeros pasos](#primeros-pasos)
- [Atajos esenciales](#atajos-esenciales)
- [Multi-cursor y selección](#multi-cursor-y-selección)
- [Búsqueda y reemplazo](#búsqueda-y-reemplazo)
- [Paneles divididos (splits)](#paneles-divididos-splits)
- [Explorador de archivos](#explorador-de-archivos)
- [Paleta de comandos y buscador de archivos](#paleta-de-comandos-y-buscador-de-archivos)
- [Vistas especiales: Markdown y CSV](#vistas-especiales-markdown-y-csv)
- [Temas](#temas)
- [Panel de administración](#panel-de-administración)
- [LSP: autocompletado y diagnósticos](#lsp-autocompletado-y-diagnósticos)
- [Personalizar atajos](#personalizar-atajos)
- [Dónde vive la configuración](#dónde-vive-la-configuración)

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
| `Ctrl+S` | Guardar |
| `Ctrl+Z` / `Ctrl+Y` | Deshacer / Rehacer |
| `Ctrl+Q` | Salir |
| `Tab` / `Shift+Tab` | Indentar / desindentar |
| `Ctrl+B` | Mostrar/ocultar el explorador de archivos lateral |
| `Ctrl+P` | Buscar archivo por nombre (difuso) |
| `Ctrl+Shift+P` o `F1` | Paleta de comandos (buscar cualquier acción por nombre) |
| `Ctrl+F` / `Ctrl+H` | Buscar / Buscar y reemplazar en el archivo |
| `Ctrl+,` o `Ctrl+K A` | Panel de administración |
| `Ctrl+K Ctrl+T` | Selector de temas (con preview en vivo) |
| `Ctrl+K Ctrl+L` | Recargar `config.toml`/`keymap.toml` sin reiniciar |

Varios atajos con `Ctrl+Shift+<letra>` o símbolos de control (`Ctrl+\`,
`Ctrl+,`) no llegan igual en todos los emuladores de terminal — el keymap
por defecto siempre trae una alternativa con `Ctrl+K <letra>` que funciona
en cualquier terminal clásica (ver la tabla completa en
[`runtime/keymaps/default.toml`](./runtime/keymaps/default.toml), o la
sección "Atajos de teclado" del panel de administración, que muestra
todas las combinaciones activas).

## Multi-cursor y selección

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

## Temas

`tcode` trae 12 temas incluidos (Dracula, Monokai, One Dark, Nord,
Gruvbox Dark, Tokyo Night, Catppuccin Mocha, Solarized Dark/Light,
GitHub Light, más dos temas simples "Oscuro"/"Claro" de las primeras
versiones).

- **`Ctrl+K Ctrl+T`**: selector de temas con **preview en vivo** —
  navegá la lista con `↑`/`↓` y el editor se recolorea al instante;
  `Enter` confirma, `Esc` vuelve al tema anterior.
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
| **Editor** | Tamaño de tabulación, espacios vs. tabs, ajuste de línea, números de línea. |
| **Interfaz** | Mostrar/ocultar la barra de estado y cada uno de sus elementos (posición del cursor, codificación, fin de línea, lenguaje, diagnósticos, modo). |

Todos los cambios se aplican y persisten al instante en `config.toml`, sin
tocar ni reiniciar nada más.

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
