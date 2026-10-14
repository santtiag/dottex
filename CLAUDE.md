# dottex

Editor LaTeX de escritorio, local y sin nube («un Overleaf en tu máquina»).
Tauri 2 (Rust) + SvelteKit estático (Svelte 5, runes) + CodeMirror 6 + pdf.js.
El usuario trabaja en español: interfaz, comentarios y documentación van en español
(el README está en inglés).

## Comandos

```bash
pnpm install
pnpm tauri dev          # app en desarrollo (Vite en :1420 + build debug)
pnpm check              # svelte-check; debe quedar en 0 errores y 0 avisos
pnpm build:arch         # solo el binario: src-tauri/target/release/dottex
cd src-tauri && cargo test
cd packaging/arch && makepkg -di   # paquete nativo de Arch (ver «Entorno»)
```

Chequeos sin DOM del editor (no hay runner de tests en el frontend):

```bash
ES=$(ls -d node_modules/.pnpm/esbuild@*/node_modules/esbuild/bin/esbuild | head -1)
$ES scripts/check-visual.ts --bundle --format=esm --outfile=/tmp/cv.mjs && node /tmp/cv.mjs
$ES scripts/check-editor.ts --bundle --format=esm --outfile=/tmp/ce.mjs && node /tmp/ce.mjs
```

`cargo clippy` no está instalado.

## Mapa

Frontend (`src/`):

- `routes/+page.svelte` — armazón de la ventana: barra de título propia (la ventana
  no tiene decoraciones), paneles, barra de estado, atajos.
- `lib/state.svelte.ts` — estado global. `app` es estado de sesión; `settings` son
  las preferencias y se guardan solas en `localStorage` (`dottex-settings`).
- `lib/ipc.ts` — única puerta al backend: tipos y un `invoke` por comando.
- `lib/Editor.svelte`, `lib/visualLatex.ts` — CodeMirror; el modo Visual son
  decoraciones sobre el mismo `.tex`, no una conversión.
- `lib/lsp.ts` — cliente de texlab (autocompletado, hover, ir a definición y
  errores de sintaxis en vivo). Sin texlab, `lib/latexComplete.ts` es el respaldo;
  con él, solo aporta los snippets.
- `lib/errorLens.ts`, `lib/issueHints.ts`, `lib/IssueList.svelte` — los problemas
  (sintaxis + compilación, unidos en `problems()`) dentro del editor y en la pestaña.
- `lib/Welcome.svelte` — inicio: recientes y plantillas.
- `lib/cursors.ts` — cursores propios por zona (variables `--cur-*`; las reglas están
  en `app.css`). Se entregan como PNG: WebKitGTK ignora un SVG como imagen de cursor.
- `app.html` — skeleton de arranque (`#boot`) y la clase `.sk` de bloque fantasma,
  en línea porque deben existir antes de que cargue el bundle.
- `lib/PdfViewer.svelte` — visor pdf.js con pintado perezoso por página.
- `lib/Settings.svelte` — configuración en cuatro secciones con subgrupos.
- `lib/i18n.svelte.ts` — diccionarios planos `es` y `en`.

Backend (`src-tauri/src/`): `lib.rs` (estado, `resolve()` de rutas y registro de
comandos), `compile.rs` (Tectonic y latexmk), `project.rs`, `fs_ops.rs` (archivos,
papelera, watcher), `synctex.rs`, `search.rs`, `completion.rs`, `log_parse.rs`,
`lsp.rs` (proceso texlab y puente JSON-RPC), `templates.rs` (plantillas: carpetas en
`~/.local/share/dottex/templates/`).

Empaquetado: `packaging/arch/` (PKGBUILD + `.desktop`), `.github/workflows/release.yml`
(instaladores de Linux, macOS y Windows al empujar una etiqueta `v*`).

## Reglas del proyecto

- **Frontera de confianza en el IPC**: los comandos reciben rutas relativas al
  proyecto y `AppState::resolve` rechaza absolutas, `..` y symlinks que escapen.
  Todo comando nuevo que toque disco pasa por ahí. El puente LSP tiene la suya en
  `lsp.rs::sanitize_outgoing`: lista blanca de métodos (nada que haga a texlab
  ejecutar programas) y documentos dentro de la raíz.
- **Nada generado en la carpeta del usuario**: build, caché, papelera y config del
  proyecto viven en `.dottex/`.
- **Textos**: toda cadena visible es una clave en `i18n.svelte.ts`, añadida a los
  dos diccionarios (`en` está tipado como `typeof es`, así que falta una y no compila).
- **Iconos**: solo Lucide (`@lucide/svelte`), con el tamaño y trazo fijados una vez
  en `+page.svelte` (`setLucideProps`). Sin emoji ni glifos Unicode como iconos.
  En los widgets de CodeMirror, que son DOM plano, se usa `lucideMask()` de
  `visualLatex.ts`.
- **Preferencias**: una opción nueva es una clave en `settings` más su fila en
  `Settings.svelte`; no crear otra vía de persistencia.
- **Nada de imports `?url` de `node_modules`**: en `tauri dev`, WebKit pide por
  adelantado los recursos de la carga anterior y Vite sirve `…?url` como archivo
  crudo si aún no transformó a quien lo importa; queda en caché y la página cae con
  «500 Internal Error». Usar `new URL("…", import.meta.url)` (ver `PdfViewer.svelte`).
  Si aparece ese 500, la caché está en `~/.local/share/dev.dottex.app/WebKitCache`.
- **Icono de la app**: la fuente es `src-tauri/app-icon.svg`. Tras editarlo,
  `pnpm tauri icon src-tauri/app-icon.svg` y copiarlo a la web (ver abajo).

## Render del PDF: no sobremuestrear

`PdfViewer.svelte` pinta cada página con un píxel de canvas por píxel físico,
exacto (`deviceBox`). WebKitGTK reduce los canvas sin filtrar, así que un canvas a
2× o 3× no se ve más nítido: pierde los trazos finos de Computer Modern. Esto se
comprobó en la app real; en Chromium el mismo código se ve bien, por eso un
navegador no sirve para juzgar el render.

El texto seleccionable es la `TextLayer` de pdf.js: spans transparentes encima del
canvas, que no intervienen en el pintado. Con Ctrl pulsado la capa deja de ser
seleccionable para que el Ctrl+clic (SyncTeX) no empiece una selección.

En esta máquina el webview reporta `devicePixelRatio` 1.25 aunque los monitores
estén a escala 1.0: es el escalado de texto de GNOME (`text-scaling-factor` 1.25),
que WebKitGTK aplica como zoom de página y no como escala de dispositivo. El layout
y el scroll van por tanto en píxeles físicos enteros, y los tamaños CSS de página
son fraccionarios a propósito (`pw / 1.25`) para caer en píxeles exactos.

El contexto 2D lo crea pdf.js (`page.render({ canvas, … })`, sin `canvasContext`):
opaco y por software, su valor por defecto. Pasarle uno propio lo convierte en un
canvas acelerado por GPU; el bitmap es idéntico píxel a píxel, así que no aporta nada.

Para comprobar un cambio de render: capturar la ventana y compararla fila a fila
con `canvas.toDataURL()` de la página visible. Debe coincidir 1:1 en varias
páginas, zooms y posiciones de scroll (salvo donde se superpone la barra de scroll).

## Probar en la app real

Los cambios de interfaz se verifican en la ventana real, no solo con `pnpm check`:

1. `WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9333 pnpm tauri dev` (solo la build debug
   expone el inspector).
2. WebSocket a `ws://127.0.0.1:9333/socket/1/1/WebPage`; esperar
   `Target.targetCreated` y mandar `Runtime.evaluate` envuelto en
   `Target.sendMessageToTarget`. `awaitPromise` no funciona: guardar el resultado en
   `window` y leerlo en otra llamada. Se anuncia más de un target para la misma
   página: enviar solo al primero de `type: "page"`, o la expresión corre dos veces.
3. Desde ahí, clics sobre el DOM o `window.__TAURI_INTERNALS__.invoke(...)` para
   llamar comandos del backend.
4. Captura solo de la ventana: `grim -T <foreign_toplevel_id>`, con el id que da
   `mmsg get all-clients` (appid `dottex`). Funciona con la ventana oculta en otra
   etiqueta y no recoge paneles ni notificaciones del escritorio.
5. El `EditorView` se alcanza con `document.querySelector(".cm-content").cmView.view`;
   una transacción con `userEvent: "input.type"` abre el autocompletado como al teclear.

Con la app abierta, `pnpm check` la recarga (su `svelte-kit sync` regenera archivos
que Vite vigila) y se pierde el proyecto abierto y lo no guardado; mientras se
prueba, usar `pnpm exec svelte-check --tsconfig ./tsconfig.json`.

Los diálogos nativos (abrir carpeta, exportar PDF) no se pueden manejar así; para
abrir un proyecto, invocar `open_project`, recargar y pulsar la entrada de Recientes.

## Entorno (CachyOS, Wayland, compositor mango)

- Rust viene de rustup (`rustup default stable`).
- `pnpm` viene de corepack, no de pacman: `makepkg` no lo ve como dependencia
  instalada, de ahí `makepkg -di` en lugar de `-si`.
- La ventana suele ocupar media pantalla (~755 px CSS de ancho). `+page.svelte`
  reparte los paneles con `sideW` y `rightW` para que el editor no quede aplastado;
  cualquier cambio de layout se prueba a ese ancho. El panel derecho se guarda como
  fracción (`pdfFrac`) y los divisores arrastran desde el ancho efectivo, no desde
  el guardado: si no, un tope del layout deja el divisor sin responder.
- GPU híbrida Intel + NVIDIA: la app abre bien sin `WEBKIT_DISABLE_DMABUF_RENDERER`.
- Tectonic: se usa el del `PATH` si existe; si no, la app descarga la 0.15.0 a
  `~/.local/share/dottex/bin/` (SHA-256 fijados en `compile.rs`).
- texlab: misma política (5.26.0, SHA-256 en `lsp.rs`); arranca al abrir un proyecto.

## Web

La landing es otro repositorio: `/home/santtiag/projects/dottex-website` (SvelteKit
estático en Vercel, GSAP + Lenis). Reproduce la ventana del IDE en
`src/lib/IdeWindow.svelte`, así que un cambio visible de la interfaz (barra de
título, pestañas, iconos, paleta) conviene reflejarlo allí. Comparte icono:
`src/lib/assets/favicon.svg` es copia de `src-tauri/app-icon.svg`.

## CodeGraph

El repo está indexado (`.codegraph/`). Para localizar o entender código, usar
`codegraph_explore` antes que grep. Tras cambios grandes, `codegraph sync`.
