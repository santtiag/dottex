# dottex

Editor LaTeX de escritorio, open source y 100% local: un "Overleaf local" con la
experiencia de un editor nativo. Abres una carpeta de tu computador como
proyecto, editas `.tex` con resaltado, compilas a PDF con un clic y ves el
resultado en el visor integrado — sin nube, sin cuenta, sin ensuciar tu carpeta.

**Estado: Fase 4 (feature-complete del plan inicial).** Ver [Roadmap](#roadmap).

## Características

- **Abrir cualquier carpeta como proyecto**, con lista de proyectos recientes.
- **Explorador de archivos** con crear / renombrar / mover / eliminar
  (eliminar va a la papelera interna `.dottex/trash/`, nunca borra directo).
- **Editor** CodeMirror 6: resaltado LaTeX, números de línea, word-wrap,
  plegado, búsqueda/reemplazo, guardado con `Ctrl+S`, tema claro/oscuro
  siguiendo el sistema.
- **Tres modos**: *Código* (fuente LaTeX), *Visual* (edición tipo Word sobre
  el mismo `.tex`) y *Compilación* (el visor de PDF de la derecha). Un solo
  documento fuente, sin conversiones de ida y vuelta.
- **Modo visual completo**: negrita/cursiva/secciones/listas renderizadas en
  línea; **matemáticas dibujadas con KaTeX** (`$…$`, `$$…$$`, `\[…\]` y
  entornos `equation`/`align`/`gather`); **imágenes de `\includegraphics`
  mostradas en línea**; captions en cursiva. Los comandos de configuración
  (preámbulo, `\maketitle`, `\date`, `\label`, `\begin{document}`…) se
  ocultan tras una pastilla `⚙ n` clicable. Al poner el cursor dentro de
  cualquier elemento, su sintaxis reaparece para editarla. Barra de formato
  y `Ctrl+B`/`Ctrl+I` que insertan la sintaxis LaTeX por ti.
- **Compilación con Tectonic** (`Ctrl+Enter` o botón ▶): detección automática
  del root file (`\documentclass`, `% !TEX root`, proyectos multi-archivo con
  `\input`/`\include`) y opción "Fijar como raíz" en el menú contextual del
  árbol para elegirlo a mano.
- **Vista previa en vivo**: recompila sola ~450 ms después de dejar de
  escribir (desactivable en la barra de estado), con indicador
  escribiendo / compilando / listo / con errores.
- **Tabla de problemas**: errores y avisos extraídos del log, con archivo y
  línea; clic en un problema salta a la línea culpable. Log crudo disponible.
- **SyncTeX en ambas direcciones**: `Ctrl/Cmd+clic` en el editor salta a esa
  posición del PDF (con marcador), y `Ctrl/Cmd+clic` en el PDF abre el archivo
  y línea correspondientes.
- **Esquema del documento**: secciones, figuras/tablas (con su caption),
  labels y TODOs del archivo activo; resalta la sección donde está el cursor
  y al hacer clic alinea editor y PDF.
- **Autocompletado**: comandos LaTeX comunes y los `\newcommand` del
  proyecto, entornos en `\begin{}`, labels del proyecto (y del buffer sin
  guardar) en `\ref{}`, y claves de los `.bib` (con título) en `\cite{}`.
- **Snippets configurables**: `fig`, `tab`, `eq`, `sec`, `env`… con
  placeholders saltables (Tab). Se editan en
  `~/.local/share/dottex/snippets.json` (se crea con valores por defecto).
- **Búsqueda global** en el proyecto (texto o regex, con/sin mayúsculas) y
  reemplazo en todos los archivos, con resultados agrupados por archivo.
- **Papelera**: sección al pie del explorador para restaurar a la ubicación
  original, borrar definitivamente o vaciar.
- **Tema** automático / claro / oscuro desde la barra de estado.
- **Visor de PDF integrado** (pdf.js): zoom, render perezoso por página,
  conserva la posición al recompilar.
- **Visores de imagen y PDF** para los archivos del proyecto; texto plano en
  el editor; formato desconocido → mensaje claro.
- **Carpeta limpia**: todo lo generado (`.aux`, `.log`, `.synctex.gz` y el
  propio PDF) vive en `.dottex/` (oculta en el árbol); tu carpeta no se toca.
  El PDF se entrega con el botón "⬇ PDF" (diálogo guardar como). Acción
  "Limpiar artefactos" incluida.
- **Watcher**: cambios hechos fuera de la app refrescan el árbol y el editor.
- **Barra de título propia**, integrada con la barra de herramientas
  (en macOS se usan los botones nativos en overlay).

## Instalación del motor LaTeX

No necesitas instalar nada. Al compilar por primera vez, dottex busca
`tectonic` en el PATH y, si no está, **descarga el binario oficial**
(GitHub Releases, ~30 MB) a la carpeta de datos de la app
(`~/.local/share/dottex/bin` en Linux). Tectonic a su vez descarga los
paquetes LaTeX que use tu documento la primera vez y los cachea; después todo
funciona offline.

Si prefieres gestionarlo tú: instala Tectonic con tu gestor de paquetes
(`dnf install tectonic`, `brew install tectonic`, `cargo install tectonic`…)
y dottex usará ese.

## Build y ejecución

Requisitos comunes: [Rust](https://rustup.rs) (estable), Node.js ≥ 20 y pnpm.

### Linux

```bash
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev build-essential

pnpm install
pnpm tauri dev      # desarrollo
pnpm tauri build    # instaladores (.deb, .rpm, AppImage)
```

### macOS

```bash
xcode-select --install   # herramientas de línea de comandos
pnpm install
pnpm tauri dev
pnpm tauri build          # .app y .dmg
```

### Windows

Instala [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
y [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (incluido
en Windows 11). Luego:

```powershell
pnpm install
pnpm tauri dev
pnpm tauri build   # .msi y .exe (NSIS)
```

## Arquitectura

```
┌─────────────────────────── Tauri 2 ───────────────────────────┐
│  Frontend (WebView)              Backend (Rust)               │
│  SvelteKit + TS                                               │
│  ┌──────────────┐   invoke()    ┌──────────────────────────┐  │
│  │ FileTree     │──────────────▶│ project.rs  árbol, root  │  │
│  │ Editor (CM6) │               │             file, config │  │
│  │ IssueList    │               │ fs_ops.rs   CRUD, trash, │  │
│  │ PdfViewer    │◀──────────────│             watcher      │  │
│  │ (pdf.js)     │   events      │ compile.rs  tectonic     │  │
│  └──────────────┘               │ log_parse.rs issues      │  │
│      compile:status/log,        └──────────────────────────┘  │
│      fs:changed                             │ spawn           │
│  (binarios PDF/imagen por IPC               ▼                 │
│   crudo, sin JSON)                    tectonic -X compile     │
└───────────────────────────────────────────────────────────────┘
```

### Decisiones técnicas

- **Tectonic como binario externo, no como crate.** El crate arrastra
  dependencias C pesadas (harfbuzz/ICU) y builds frágiles; el binario oficial
  es autocontenido y se descarga una sola vez. Además mantiene abierta la
  puerta al motor del sistema (latexmk) como alternativa futura.
- **Todo lo generado va a `.dottex/`** (`build/`, `cache/`, `trash/`,
  `config.json`): la carpeta del usuario nunca se ensucia. La carpeta se
  oculta en el árbol (como todo dotfile).
- **Svelte 5 + CodeMirror 6 + pdf.js**: bundle pequeño y arranque rápido;
  CM6 y pdf.js son agnósticos al framework.
- **Frontera de confianza en el IPC**: los comandos reciben rutas relativas y
  el backend rechaza rutas absolutas o con `..`; el frontend solo puede tocar
  archivos dentro del proyecto abierto.
- **Resaltado LaTeX** con el modo `stex` de `@codemirror/legacy-modes` (no
  existe paquete CM6 oficial para LaTeX; es el camino estándar).

### Estructura de `.dottex/`

```
.dottex/
├── build/    # .aux .log .synctex.gz y PDF de la compilación
├── cache/    # reservado para el render en vivo (Fase 2)
├── trash/    # papelera interna: <timestamp>__<nombre>
└── config.json  # { "root_file": "main.tex" }
```

## Roadmap

- **Fase 1** ✓: proyecto, CRUD, editor, compilación manual, visor PDF,
  `.dottex/`.
- **Fase 2** ✓ (esta versión): vista previa en vivo con debounce, tabla de
  errores/avisos navegable, selector de root file, exportación del PDF,
  barra de título integrada.
- **Fase 3** ✓ (esta versión): esquema del documento, SyncTeX (doble
  sincronización editor ↔ PDF), sincronización de las 3 zonas.
- **Fase 4** ✓ (esta versión): autocompletado y autofill de `\ref`/`\cite`,
  búsqueda global con reemplazo, snippets, UI de papelera, selector de tema.
- **Pendiente / ideas**: motor del sistema (latexmk) como alternativa
  autodetectada, esquema global multi-archivo, instaladores firmados por SO.

## Licencia

[MIT](LICENSE)
