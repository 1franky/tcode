# PLAN.md — `tcode`

**Editor TUI portable con atajos estilo VSCode**
Autor: Francisco · Lenguaje: Rust · Interfaz: 100% terminal, todo por atajos de teclado
Nombres de comandos y descripciones en español para memorización rápida.

---

## 1. Visión y objetivos

Editor de código en **terminal (TUI)** que combina:

- **Ergonomía de VSCode**: atajos no-modales (`Ctrl+S`, `Ctrl+P`, `Ctrl+Shift+P`, `Ctrl+/`, etc.), sin la barrera de entrada de vim/helix.
- **Potencia de Neovim**: LSP nativo, tree-sitter, buffers múltiples, splits, extensible.
- **Portabilidad real**:
  - **Linux**: instalación sin `sudo`, se añade al `PATH` del usuario.
  - **Windows**: ejecutable único, funciona desde cualquier carpeta sin permisos de administrador (verdaderamente portable, tipo "unzip and run").
- **Soporte de lenguajes de primera**: autocompletado, diagnósticos, go-to-definition y refactors para Java, Kotlin, Python, JS/TS, Go, Rust, C/C++, C#, Ruby, PHP, HTML/CSS y frameworks asociados (Spring, Django, React, Vue, etc.) a través de LSPs oficiales.
- **Vistas especializadas**:
  - Doble vista para Markdown (source + preview renderizado en tiempo real).
  - Vista tabular para CSV/TSV (navegación por celdas, ordenamiento, filtros).
- **Panel de administración interno** para editar atajos, temas, LSPs y configuración sin salir del editor. **Todos los textos, comandos y descripciones en español.**
- **Sin mouse**: el editor es 100% operable por teclado. Cada acción tiene un atajo. El mouse es opcional (solo scroll y selección).
- **Barra de estado (statusbar)** siempre visible en la parte inferior con:
  - **Número de línea y columna del cursor** (formato `Ln 42, Col 7`).
  - Total de líneas del archivo.
  - Codificación (UTF-8, Latin-1, etc.).
  - Fin de línea (LF, CRLF).
  - Lenguaje detectado.
  - Estado del LSP (conectado / cargando / error).
  - Rama de git activa (fase 2).
  - Modo (INSERTAR / SELECCIÓN / COMANDO).

### No-goals (explícitos)

- No es un IDE gráfico. Todo corre en la terminal.
- No pretende reemplazar debugger visual (aunque puede integrar DAP en fase posterior).
- No usa GUI toolkits (Qt, GTK). Solo terminal.
- No depende del mouse para ninguna operación.

---

## 2. Decisiones de arquitectura

| Decisión | Elección | Justificación |
|---|---|---|
| Lenguaje | **Rust** (edición 2021) | Ecosistema maduro para editores (`ropey`, `tree-sitter-rs`, `tower-lsp`, `ratatui`). Binarios estáticos sin GC. Cross-compilación estable. |
| Framework TUI | **`ratatui` + `crossterm`** | Backend cross-platform, sin dependencia de `ncurses`. Funciona en Windows Terminal, PowerShell, cmd, kitty, alacritty, WezTerm. |
| Buffer de texto | **`ropey`** | Rope data structure. Edición O(log n) en archivos grandes. Es lo que usa Helix. |
| Syntax highlighting | **`tree-sitter`** con gramáticas por lenguaje | Parsing incremental. Highlighting exacto, no basado en regex. Soporte para 200+ lenguajes. |
| LSP | **`tower-lsp` (cliente)** + procesos externos (`rust-analyzer`, `pyright`, `jdtls`, etc.) | Estándar de la industria. No reinventamos LSP; delegamos a los servers oficiales de cada lenguaje. |
| Formato de config | **TOML** con `serde` | Legible, ampliamente usado (Cargo, Helix). Soporta comentarios. |
| Async runtime | **`tokio`** | Necesario para LSPs, watchers de archivos, y renderizado no-bloqueante. |
| Renderizado Markdown | **`pulldown-cmark`** + renderer propio a `ratatui` widgets | Preview real dentro de la terminal. |
| CSV | **crate `csv`** + widget `Table` de ratatui | Tabla navegable con headers fijos. |

---

## 3. Arquitectura modular

Workspace de Cargo con crates independientes. Facilita testing y evolución.

```
tcode/
├── Cargo.toml                    # workspace
├── crates/
│   ├── core/                     # buffer (ropey), historia (undo/redo), cursores
│   ├── ui/                       # ratatui layouts, paneles, statusbar, tabs
│   ├── syntax/                   # tree-sitter loader, highlighter, folding
│   ├── lsp/                      # cliente LSP, gestión de servers, completions
│   ├── config/                   # parser TOML, keybindings, temas
│   ├── keymap/                   # sistema de atajos, resolución de conflictos
│   ├── views/
│   │   ├── code/                 # vista principal de código
│   │   ├── markdown/             # split source + preview
│   │   ├── csv/                  # vista tabular
│   │   └── admin/                # panel de configuración interactivo
│   ├── commands/                 # command palette (Ctrl+Shift+P)
│   ├── fs/                       # file explorer, fuzzy finder, watchers
│   └── app/                      # binario principal, event loop, state global
├── runtime/                      # gramáticas tree-sitter, temas, keymaps por defecto
│   ├── grammars/
│   ├── themes/
│   └── keymaps/
├── docs/
└── install/
    ├── linux.sh
    └── windows.ps1
```

**Principio arquitectónico clave:** el `core` no conoce nada de UI. La UI observa cambios del `core` vía eventos. Esto permite tests unitarios rápidos y una eventual GUI en el futuro sin reescribir.

---

## 4. Sistema de atajos (keybindings)

### Filosofía

- **No-modal por defecto** (estilo VSCode). Se puede activar modo "vim-like" opcional en config.
- **Nombres de comandos en español**: `archivo.guardar`, `editor.copiar`, `lsp.renombrar`. Facilita memorización y aparecen así en el panel de administración y la paleta de comandos.
- Atajos definidos en TOML. Recargables en caliente sin reiniciar.
- Panel de administración permite editarlos visualmente, con detección de conflictos y búsqueda por nombre en español.
- **Atajos encadenados (chord)** estilo VSCode: `Ctrl+K` seguido de otra tecla para acciones secundarias, sin agotar combinaciones globales.

### Atajos por defecto

#### 📁 Archivos

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+N` | `archivo.nuevo` | Nuevo archivo |
| `Ctrl+O` | `archivo.abrir` | Abrir archivo |
| `Ctrl+K Ctrl+O` | `archivo.abrir_carpeta` | Abrir carpeta como proyecto |
| `Ctrl+S` | `archivo.guardar` | Guardar |
| `Ctrl+Shift+S` | `archivo.guardar_como` | Guardar como… |
| `Ctrl+K S` | `archivo.guardar_todo` | Guardar todos |
| `Ctrl+W` | `archivo.cerrar` | Cerrar pestaña |
| `Ctrl+K W` | `archivo.cerrar_todo` | Cerrar todas las pestañas |
| `Ctrl+Shift+T` | `archivo.reabrir` | Reabrir último cerrado |
| `Ctrl+Q` | `app.salir` | Salir del editor |

#### 🧭 Navegación

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+P` | `buscar.archivos` | Buscar archivo (fuzzy) |
| `Ctrl+Shift+P` | `paleta.comandos` | Paleta de comandos |
| `Ctrl+G` | `ir.a_linea` | Ir a línea… |
| `Ctrl+T` | `ir.a_simbolo_global` | Ir a símbolo en el proyecto |
| `Ctrl+Shift+O` | `ir.a_simbolo_archivo` | Ir a símbolo en el archivo |
| `F12` | `lsp.ir_a_definicion` | Ir a definición |
| `Alt+F12` | `lsp.ver_definicion` | Ver definición (peek) |
| `Shift+F12` | `lsp.ver_referencias` | Ver referencias |
| `Ctrl+Home` | `cursor.inicio_archivo` | Ir al inicio del archivo |
| `Ctrl+End` | `cursor.fin_archivo` | Ir al final del archivo |
| `Ctrl+←` / `Ctrl+→` | `cursor.palabra_anterior/siguiente` | Palabra anterior/siguiente |
| `Alt+←` / `Alt+→` | `cursor.retroceder/avanzar` | Retroceder/avanzar en historial |
| `Ctrl+U` | `cursor.deshacer_movimiento` | Deshacer último salto de cursor |

#### ✏️ Edición

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+X` | `editor.cortar` | Cortar línea (o selección) |
| `Ctrl+C` | `editor.copiar` | Copiar línea (o selección) |
| `Ctrl+V` | `editor.pegar` | Pegar |
| `Ctrl+Z` | `editor.deshacer` | Deshacer |
| `Ctrl+Y` | `editor.rehacer` | Rehacer |
| `Ctrl+A` | `editor.seleccionar_todo` | Seleccionar todo |
| `Ctrl+L` | `editor.seleccionar_linea` | Seleccionar línea actual |
| `Ctrl+/` | `editor.comentar_linea` | Comentar/descomentar línea |
| `Ctrl+Shift+A` | `editor.comentar_bloque` | Comentar/descomentar bloque |
| `Alt+↑` / `Alt+↓` | `editor.mover_linea_arriba/abajo` | Mover línea arriba/abajo |
| `Alt+Shift+↑` / `Alt+Shift+↓` | `editor.duplicar_linea_arriba/abajo` | Duplicar línea |
| `Ctrl+Shift+K` | `editor.eliminar_linea` | Eliminar línea |
| `Ctrl+Enter` | `editor.linea_debajo` | Insertar línea debajo |
| `Ctrl+Shift+Enter` | `editor.linea_arriba` | Insertar línea arriba |
| `Tab` | `editor.indentar_o_autocompletar` | Indentar o aceptar sugerencia |
| `Shift+Tab` | `editor.desindentar` | Desindentar |
| `Ctrl+]` / `Ctrl+[` | `editor.indentar/desindentar` | Indentar/desindentar selección |
| `Ctrl+Backspace` | `editor.borrar_palabra_atras` | Borrar palabra hacia atrás |
| `Ctrl+Delete` | `editor.borrar_palabra_adelante` | Borrar palabra hacia adelante |

#### 🔍 Búsqueda y reemplazo

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+F` | `buscar.en_archivo` | Buscar en el archivo actual |
| `Ctrl+H` | `buscar.reemplazar` | Reemplazar en el archivo actual |
| `Ctrl+Shift+F` | `buscar.en_proyecto` | Buscar en todo el proyecto |
| `Ctrl+Shift+H` | `buscar.reemplazar_proyecto` | Reemplazar en todo el proyecto |
| `F3` | `buscar.siguiente` | Siguiente coincidencia |
| `Shift+F3` | `buscar.anterior` | Coincidencia anterior |
| `Alt+R` | `buscar.alternar_regex` | Alternar regex |
| `Alt+C` | `buscar.alternar_mayusculas` | Alternar sensibilidad a mayúsculas |
| `Alt+W` | `buscar.alternar_palabra` | Alternar palabra completa |

#### 🖱️ Multi-cursor y selección

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+D` | `cursor.seleccionar_siguiente_ocurrencia` | Añadir siguiente ocurrencia a la selección |
| `Ctrl+Shift+L` | `cursor.seleccionar_todas_ocurrencias` | Seleccionar todas las ocurrencias |
| `Ctrl+Alt+↑` / `Ctrl+Alt+↓` | `cursor.agregar_arriba/abajo` | Cursor adicional arriba/abajo |
| `Esc` | `cursor.una_seleccion` | Volver a un solo cursor |
| `Ctrl+Shift+→` / `Ctrl+Shift+←` | `editor.seleccionar_palabra` | Seleccionar palabra a derecha/izquierda |

#### 🪟 Paneles, pestañas y ventanas

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+B` | `panel.alternar_lateral` | Mostrar/ocultar barra lateral |
| `Ctrl+J` | `panel.alternar_inferior` | Mostrar/ocultar panel inferior |
| `Ctrl+\` | `panel.dividir_vertical` | Dividir editor verticalmente |
| `Ctrl+K Ctrl+\` | `panel.dividir_horizontal` | Dividir editor horizontalmente |
| `Ctrl+1` / `Ctrl+2` / `Ctrl+3` | `panel.ir_a_1/2/3` | Ir al panel 1/2/3 |
| `Ctrl+K F` | `panel.cerrar` | Cerrar panel actual |
| `Ctrl+Tab` | `pestaña.siguiente` | Siguiente pestaña |
| `Ctrl+Shift+Tab` | `pestaña.anterior` | Pestaña anterior |
| `Ctrl+PageDown` / `Ctrl+PageUp` | `pestaña.mover_derecha/izquierda` | Mover pestaña |

#### 🧠 LSP (autocompletado, refactor)

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+Space` | `lsp.autocompletar` | Sugerencias de autocompletado |
| `Ctrl+.` | `lsp.acciones_codigo` | Acciones de código (quick fix) |
| `F2` | `lsp.renombrar` | Renombrar símbolo |
| `Ctrl+K Ctrl+I` | `lsp.mostrar_info` | Mostrar información (hover) |
| `Ctrl+Shift+M` | `lsp.diagnosticos` | Mostrar panel de errores/avisos |
| `F8` / `Shift+F8` | `lsp.siguiente_error/anterior_error` | Ir al siguiente/anterior diagnóstico |

#### 💅 Formato

| Atajo | Comando | Descripción |
|---|---|---|
| `Shift+Alt+F` | `formato.documento` | Formatear documento completo |
| `Ctrl+K Ctrl+F` | `formato.seleccion` | Formatear selección |

#### 📄 Vistas especiales

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+K V` | `markdown.alternar_preview` | Alternar preview de Markdown |
| `Ctrl+K T` | `csv.alternar_vista_tabla` | Alternar vista tabular / texto plano en CSV |
| `Ctrl+Shift+V` | `markdown.preview_solo` | Ver solo preview de Markdown |

#### ⚙️ Configuración y admin

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+,` | `config.abrir_panel` | Abrir panel de configuración |
| `Ctrl+K Ctrl+S` | `config.editor_atajos` | Editor de atajos de teclado |
| `Ctrl+K Ctrl+T` | `config.selector_tema` | Cambiar tema |
| `Ctrl+K Ctrl+P` | `config.editor_tema` | Personalizar tema actual |
| `Ctrl+K Ctrl+L` | `config.recargar` | Recargar configuración |

#### 🔍 Zoom y vista

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl++` | `vista.aumentar_zoom` | Aumentar tamaño de letra |
| `Ctrl+-` | `vista.reducir_zoom` | Reducir tamaño de letra |
| `Ctrl+0` | `vista.zoom_reset` | Restablecer zoom |
| `F11` | `vista.pantalla_completa` | Pantalla completa |
| `Ctrl+K Z` | `vista.modo_zen` | Modo zen (sin distracciones) |
| `Ctrl+K L` | `vista.alternar_numeros` | Mostrar/ocultar números de línea |

#### 📚 Plegado (folding)

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+Shift+[` | `plegar.actual` | Plegar bloque actual |
| `Ctrl+Shift+]` | `plegar.desplegar` | Desplegar bloque actual |
| `Ctrl+K Ctrl+0` | `plegar.todo` | Plegar todo |
| `Ctrl+K Ctrl+J` | `plegar.desplegar_todo` | Desplegar todo |

#### 🖥️ Terminal integrada (fase 2)

| Atajo | Comando | Descripción |
|---|---|---|
| `` Ctrl+` `` | `terminal.alternar` | Mostrar/ocultar terminal |
| `` Ctrl+Shift+` `` | `terminal.nueva` | Nueva terminal |

#### 🌿 Git (fase 2)

| Atajo | Comando | Descripción |
|---|---|---|
| `Ctrl+Shift+G` | `git.abrir_panel` | Abrir panel de git |
| `Ctrl+K Ctrl+C` | `git.commit` | Commit rápido |

### Ejemplo de `keymap.toml`

```toml
# Los atajos se definen en TOML y se recargan en caliente.
# Los nombres de comando están en español para memorización rápida.

[global]
"Ctrl+S"         = "archivo.guardar"
"Ctrl+Shift+S"   = "archivo.guardar_como"
"Ctrl+P"         = "buscar.archivos"
"Ctrl+Shift+P"   = "paleta.comandos"
"Ctrl+B"         = "panel.alternar_lateral"
"Ctrl+,"         = "config.abrir_panel"
"F12"            = "lsp.ir_a_definicion"
"Ctrl+Space"     = "lsp.autocompletar"

[editor]
"Ctrl+/"         = "editor.comentar_linea"
"Alt+Up"         = "editor.mover_linea_arriba"
"Alt+Down"       = "editor.mover_linea_abajo"
"Tab"            = "editor.indentar_o_autocompletar"

[markdown]
"Ctrl+K V"       = "markdown.alternar_preview"

[csv]
"Ctrl+Right"     = "csv.columna_siguiente"
"Ctrl+Shift+S"   = "csv.ordenar_por_columna"
```

---

## 5. Panel de administración

Vista interna accesible con `Ctrl+,`. Se implementa como una vista más del sistema (no una GUI aparte), navegable 100% por teclado, con textos en español.

### Navegación del panel

- Barra lateral izquierda con secciones (`↑` `↓` para moverse, `Enter` para entrar).
- Área central editable.
- Barra inferior con contexto: `Tab` cambia foco, `Esc` vuelve atrás, `Ctrl+S` guarda cambios.
- Búsqueda global de opciones con `Ctrl+F` dentro del panel (buscas por texto en español, ej. "número de línea", "guardado automático").

### Secciones

1. **Atajos de teclado** (`Ctrl+K Ctrl+S`)
   - Lista buscable por comando en español o por combinación de teclas.
   - Editable en línea: seleccionas una fila, pulsas `Enter`, presionas la nueva combinación y se guarda.
   - Detección de conflictos en tiempo real (resaltados en rojo).
   - Botón "Restablecer valor por defecto" por atajo y global.
   - Exportar/importar keymap desde archivo `.toml`.

2. **Temas** (`Ctrl+K Ctrl+T` para elegir, `Ctrl+K Ctrl+P` para personalizar)
   - Selector con **preview en vivo** (el editor de fondo se colorea al desplazarte por la lista).
   - Filtro claro/oscuro/alto contraste.
   - Editor visual de tema (ver §7).
   - Duplicar tema y editarlo como propio.
   - Importar tema desde archivo, exportar el propio.

3. **Lenguajes / LSP**
   - Habilitar/deshabilitar servers por lenguaje.
   - Configurar comando, argumentos y variables de entorno.
   - Ver estado (conectado/error) y logs en tiempo real.
   - Indicador de si el LSP está en el `PATH` o falta instalarlo.

4. **Editor**
   - Tamaño de tabulación, espacios vs. tabs.
   - Ajuste de línea (wrap) on/off.
   - Guardado automático (nunca / al perder foco / cada N segundos).
   - Formateo al guardar on/off por lenguaje.
   - Mostrar/ocultar números de línea, minimapa, indicadores de git.
   - Regla vertical en columna N.

5. **Interfaz**
   - Densidad de UI (compacta / cómoda).
   - Mostrar/ocultar statusbar, tabs, breadcrumbs.
   - Elementos visibles en la statusbar (marcar/desmarcar): posición cursor, codificación, EOL, lenguaje, rama git, LSP, modo.

6. **Extensiones** (fase 2) — activar/desactivar plugins WASM.

Todo se persiste en `config.toml` en la ruta correspondiente (portable o de usuario). Los cambios se aplican sin reiniciar.

---

## 6. Soporte de lenguajes

### Estrategia

**Highlighting** vía tree-sitter (embebido en el binario o cargado desde `runtime/grammars/`).
**Autocompletado, diagnósticos, go-to-definition** delegado a LSPs externos que el usuario tenga instalados.

### Lenguajes objetivo (M0)

| Lenguaje | Tree-sitter | LSP recomendado | Frameworks soportados vía LSP |
|---|---|---|---|
| Rust | ✓ | `rust-analyzer` | — |
| Python | ✓ | `pyright` o `pylsp` | Django, Flask, FastAPI |
| Java | ✓ | `jdtls` | Spring Boot, Jakarta EE |
| Kotlin | ✓ | `kotlin-language-server` | Ktor, Spring |
| JavaScript/TypeScript | ✓ | `typescript-language-server` | React, Vue, Svelte, Next |
| Go | ✓ | `gopls` | — |
| C/C++ | ✓ | `clangd` | — |
| C# | ✓ | `omnisharp` | .NET |
| Ruby | ✓ | `solargraph` | Rails |
| PHP | ✓ | `intelephense` | Laravel, Symfony |
| HTML/CSS | ✓ | `vscode-langservers-extracted` | Tailwind (vía `tailwindcss-language-server`) |
| Markdown | ✓ | `marksman` | — |
| SQL | ✓ | `sqls` | — |

### Instalación de LSPs

- El editor **no instala LSPs automáticamente en M0** (evita descargas silenciosas).
- Panel de admin muestra: LSP requerido, si está en `PATH`, comando para instalarlo.
- En fase 2: comando `tcode lsp install <lang>` que descarga y coloca binarios en `~/.local/share/tcode/lsp/` (Linux) o junto al ejecutable (Windows portable).

---

## 7. Temas y personalización visual

### Temas incluidos por defecto

`tcode` viene con **10 temas preinstalados** que cubren los estilos más usados en la comunidad de desarrolladores. Se cargan desde `runtime/themes/` y funcionan sin conexión.

| # | Nombre | Estilo | Descripción breve |
|---|---|---|---|
| 1 | **Dracula** | Oscuro | Morado y rosa vibrantes. Muy popular y de alto contraste. |
| 2 | **Monokai** | Oscuro | Clásico de Sublime/TextMate. Verdes, rosas y amarillos saturados. |
| 3 | **One Dark** | Oscuro | Tema oficial de Atom. Azules y morados suaves, sobrio y legible. |
| 4 | **Nord** | Oscuro | Paleta fría inspirada en el ártico. Elegante, poco cansador. |
| 5 | **Gruvbox Dark** | Oscuro | Colores retro cálidos, muy popular en la comunidad Vim/Neovim. |
| 6 | **Tokyo Night** | Oscuro | Azules profundos con acentos violetas. Moderno y minimalista. |
| 7 | **Catppuccin Mocha** | Oscuro | Pastel suave, muy popular actualmente. Cuatro variantes incluidas. |
| 8 | **Solarized Dark** | Oscuro | Paleta científicamente balanceada por Ethan Schoonover. |
| 9 | **Solarized Light** | Claro | Versión clara de Solarized. Ideal para entornos muy iluminados. |
| 10 | **GitHub Light** | Claro | Réplica del tema oficial de GitHub. Familiar y limpio. |

Adicionalmente incluidos como *bonus* (activables desde el selector):
- **Gruvbox Light**
- **GitHub Dark**
- **Ayu Dark** / **Ayu Mirage** / **Ayu Light**

### Selector de tema

Accesible por `Ctrl+K Ctrl+T` o desde el panel de configuración → Temas.

- Lista buscable con filtros: `Claro`, `Oscuro`, `Alto contraste`.
- **Preview en vivo**: mientras te desplazas por la lista con `↑` `↓`, el editor de fondo va cambiando de tema en tiempo real. `Enter` confirma, `Esc` cancela y vuelve al tema anterior.
- Indicador visual del tema activo.

### Personalización de tema (`Ctrl+K Ctrl+P`)

Editor visual de temas dentro del panel de administración. Todo por teclado, con navegación en árbol.

**Grupos editables:**

1. **Sintaxis** (tokens de código)
   - Palabras clave, cadenas, números, comentarios, funciones, tipos, variables, operadores, constantes, decoradores.
2. **UI del editor**
   - Fondo, texto normal, línea actual resaltada, cursor, selección, márgenes.
   - Números de línea (normal y activo).
   - Guías de indentación.
3. **Paneles y statusbar**
   - Fondo/texto de la barra lateral, tabs (activa, inactiva, modificada).
   - Colores de la statusbar según modo.
4. **Diagnósticos y git**
   - Error, aviso, información, sugerencia.
   - Añadido, modificado, eliminado (git gutter).
5. **Búsqueda y multi-cursor**
   - Coincidencia actual, otras coincidencias, cursor secundario.

**Flujo de edición:**

- Cada campo abre un mini-selector de color:
  - Introducir código hex (`#a3b1c6`) o RGB.
  - Elegir de una paleta predefinida.
  - Ajustar HSL con flechas.
- **Preview en vivo** en un panel de ejemplo con código real (una función Rust, un bloque Markdown, una tabla CSV).
- **Duplicar tema base**: no editas el tema original directamente; `tcode` crea una copia con nombre `<original>-mio.toml` que puedes editar libremente.
- Guardar (`Ctrl+S`), restablecer valor por defecto por token, o restaurar el tema completo.

### Formato de tema TOML

```toml
# runtime/themes/mi-tema.toml
name = "Mi Tema"
extends = "one-dark"     # opcional: hereda todo lo no definido
type = "dark"

[ui]
background       = "#1e1e2e"
foreground       = "#cdd6f4"
cursor           = "#f5e0dc"
selection        = "#585b70"
line_number      = "#6c7086"
line_number_active = "#cdd6f4"
current_line     = "#313244"

[statusbar]
background       = "#181825"
foreground       = "#cdd6f4"
modo_insertar    = "#a6e3a1"
modo_seleccion   = "#f9e2af"

[syntax]
keyword          = { fg = "#cba6f7", style = "bold" }
string           = "#a6e3a1"
number           = "#fab387"
comment          = { fg = "#6c7086", style = "italic" }
function         = "#89b4fa"
type             = "#f9e2af"
variable         = "#cdd6f4"
constant         = "#fab387"
operator         = "#94e2d5"

[diagnostics]
error            = "#f38ba8"
warning          = "#f9e2af"
info             = "#89dceb"
hint             = "#94e2d5"

[git]
added            = "#a6e3a1"
modified         = "#f9e2af"
deleted          = "#f38ba8"
```

### Compartir temas

- Exportar el tema activo como archivo `.toml` desde el panel.
- Importar temas desde archivo (drop en `~/.config/tcode/themes/` en Linux, o carpeta `themes/` junto al ejecutable en Windows portable).
- **Compatibilidad**: parser tolerante que acepta temas en formato Helix (con avisos si algún campo no aplica), facilitando reutilizar temas de esa comunidad.

---

## 8. Vista Markdown doble

**Layout:** split vertical 50/50 configurable.

- **Panel izquierdo:** editor normal con highlighting Markdown.
- **Panel derecho:** preview renderizado usando `pulldown-cmark` traducido a widgets de `ratatui`:
  - Headings → texto con estilo bold + color por nivel.
  - Listas → bullets/números con indentación.
  - `code blocks` → resaltado con tree-sitter del lenguaje declarado.
  - Tablas → widget `Table`.
  - Links → estilo underline + color.
  - Imágenes → placeholder `[img: alt]` (o kitty graphics protocol en fase 2 si el terminal lo soporta).
- **Sincronización de scroll** opcional (activada por defecto).
- Toggle con `Ctrl+K V`.

---

## 9. Vista CSV como tabla

- Detección automática por extensión `.csv`, `.tsv`.
- **Widget:** `ratatui::widgets::Table` con:
  - Header row congelada.
  - Anchos de columna auto-ajustables + redimensionables manualmente.
  - Navegación por celda con flechas / `Tab`.
  - Modo edición de celda con `Enter` o `F2`.
- **Funciones:**
  - Ordenar por columna (`Ctrl+Shift+S`).
  - Filtrar por columna (`Ctrl+Shift+F`).
  - Insertar/eliminar filas y columnas.
  - Toggle a "modo raw" para editar como texto plano.
- Guardado respeta el delimitador y quoting original.

---

## 10. Portabilidad

### Linux

- **Build:** `cargo build --release --target x86_64-unknown-linux-musl` → binario estático sin dependencias de glibc.
- **Instalación** (`install/linux.sh`):
  1. Detecta si `~/.local/bin` existe (lo crea si no).
  2. Copia el binario ahí.
  3. Verifica si `~/.local/bin` está en `PATH`. Si no, añade la línea correspondiente a `~/.bashrc`, `~/.zshrc`, o `~/.config/fish/config.fish` según el shell activo.
  4. Coloca la config por defecto en `~/.config/tcode/`.
  5. Recursos (temas, gramáticas) en `~/.local/share/tcode/`.
- **Sin sudo en ningún momento.**
- Empaquetado adicional (fase 2): `.deb`, `.rpm`, AUR, Flatpak, Snap.

### Windows

- **Build:** `cargo build --release --target x86_64-pc-windows-gnu` (cross-compile desde Linux) o `--target x86_64-pc-windows-msvc` (nativo).
- **Modo portable (por defecto):**
  1. Se distribuye como `.zip` con el `.exe` y una carpeta `runtime/` al lado.
  2. Al arrancar, el editor busca `config.toml` **en el mismo directorio del `.exe`**.
  3. Si lo encuentra → modo portable: todo (config, cache, historial) vive en subcarpetas junto al ejecutable.
  4. Si no lo encuentra → modo estándar: usa `%APPDATA%\tcode\`.
- **Sin admin:** no toca el registro, no escribe en `Program Files`, no requiere instalador.
- **Añadir al PATH (opcional):** script `install/windows.ps1` que modifica la variable `PATH` **del usuario** (no del sistema), sin necesidad de elevación.

### macOS (bonus, casi gratis)

- `cargo build --release --target aarch64-apple-darwin` y `x86_64-apple-darwin`.
- Instalación tipo Linux: `~/.local/bin` o Homebrew formula (fase 2).

---

## 11. Milestones y roadmap

### M0 — Fundamentos (semanas 1–3)

- Setup workspace Cargo.
- `core`: buffer con `ropey`, cursores, undo/redo básico.
- `ui`: shell mínimo con ratatui, un panel de texto, statusbar.
- Abrir, editar, guardar archivos.
- Atajos hardcodeados básicos (`Ctrl+S`, `Ctrl+Q`, flechas, `Ctrl+Z`).
- **Criterio de aceptación:** puedes editar y guardar un `.txt`.

### M1 — Highlighting y configuración (semanas 4–6)

- Integrar tree-sitter con 5 lenguajes iniciales (Rust, Python, JS, Go, Markdown).
- Sistema de config TOML con hot-reload.
- Sistema de atajos configurables.
- Temas básicos (dark, light, dracula).
- File explorer lateral (`Ctrl+B`).

### M2 — LSP y command palette (semanas 7–10)

- Cliente LSP con `tower-lsp`.
- Autocompletado, diagnósticos inline, hover, go-to-definition.
- Command palette (`Ctrl+Shift+P`).
- Fuzzy file picker (`Ctrl+P`).
- Splits horizontales/verticales.
- **Criterio de aceptación:** puedes editar un proyecto Python con `pyright` y ver errores en tiempo real.

### M3 — Vistas especializadas (semanas 11–13)

- Vista Markdown doble con preview.
- Vista CSV tabular.
- Búsqueda y reemplazo (`Ctrl+F`, `Ctrl+H`) con regex opcional.
- Multi-cursor.

### M4 — Panel admin, temas y pulido (semanas 14–16)

- Panel de administración interno navegable por teclado, textos en español.
- Editor de atajos con detección de conflictos y búsqueda por nombre en español.
- **10 temas por defecto** incluidos (Dracula, Monokai, One Dark, Nord, Gruvbox Dark, Tokyo Night, Catppuccin Mocha, Solarized Dark/Light, GitHub Light).
- Selector de temas con **preview en vivo** al navegar la lista.
- **Editor visual de tema** (`Ctrl+K Ctrl+P`) con selector de color por token y panel de ejemplo en vivo.
- Import/export de temas TOML.
- Configuración de LSPs desde la UI.
- Integrar los 13 lenguajes objetivo.
- Statusbar completa con posición del cursor (`Ln X, Col Y`), configurable desde el panel.

### M5 — Distribución y portabilidad (semanas 17–18)

- Scripts de instalación Linux y Windows.
- Verificación de modo portable Windows.
- Cross-compilation en CI (GitHub Actions).
- Releases con binarios firmados.
- Documentación de usuario.

### Fase 2 (post-1.0)

- Sistema de plugins WASM.
- Instalador de LSPs integrado.
- Integración DAP (debugger).
- Terminal integrada (`Ctrl+\``).
- Git gutter y comandos git básicos.
- Kitty graphics protocol para imágenes en Markdown.

---

## 12. Riesgos y decisiones abiertas

| Riesgo | Mitigación |
|---|---|
| Curva de aprendizaje de Rust | Empezar por M0 minimalista. Usar `anyhow` para errores durante prototipado. Refactorizar tipos exactos en M1. |
| Complejidad de LSP | Empezar con **un solo LSP** (`rust-analyzer`) en M2. Añadir resto incrementalmente. |
| Rendering de Markdown fiel en TUI | Aceptar limitaciones (sin imágenes en M0). Priorizar legibilidad sobre fidelidad pixel-perfect. |
| Compilación en Windows para quien no tenga toolchain | CI genera binarios y publica en GitHub Releases. El usuario final descarga, no compila. |
| Tamaño del binario con gramáticas embebidas | Empezar embebidas para simplicidad. Migrar a carga dinámica si supera 30 MB. |
| Conflictos de atajos con terminales | Documentar teclas problemáticas (`Ctrl+/`, `Ctrl+Shift+P`) por emulador. Permitir remapeo. |

### Decisiones abiertas para resolver antes de M0

1. **Nombre del proyecto.**
2. **Licencia** (sugerido: MIT o Apache-2.0 dual).
3. **Estrategia de temas**: ¿compatibles con formato Helix, o formato propio?
4. **Detección de proyecto**: `.git` root, o marcador propio (`.tcode/`).
5. **Config global vs. per-project**: sí a ambas, con override.

---

## 13. Referencias técnicas

- Helix editor — <https://github.com/helix-editor/helix> (referencia de arquitectura Rust + tree-sitter).
- Micro editor — <https://github.com/zyedidia/micro> (referencia de UX no-modal y portabilidad).
- ratatui docs — <https://ratatui.rs/>.
- tree-sitter — <https://tree-sitter.github.io/>.
- LSP spec — <https://microsoft.github.io/language-server-protocol/>.
- ropey — <https://docs.rs/ropey/>.
