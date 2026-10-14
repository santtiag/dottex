<script lang="ts">
  import { onDestroy } from "svelte";
  import * as pdfjs from "pdfjs-dist";
  import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
  import { Minus, MoveHorizontal, Plus } from "@lucide/svelte";
  import { readFileBytes } from "./ipc";
  import { settings } from "./state.svelte";
  import { t } from "./i18n.svelte";

  // new URL(…, import.meta.url) y no un import `?url`: en `tauri dev`, WebKit
  // pide por adelantado los recursos de la carga anterior y Vite, si todavía no
  // ha transformado este archivo, responde a `…?url` con el worker crudo. Eso
  // queda en caché, el import se queda sin `default` y toda la página cae con
  // «500 Internal Error».
  pdfjs.GlobalWorkerOptions.workerSrc = new URL(
    "../../node_modules/pdfjs-dist/build/pdf.worker.min.mjs",
    import.meta.url,
  ).href;

  // `path` relativo al proyecto; `version` fuerza recarga del mismo archivo.
  let {
    source,
    target = null,
    dark = false,
    onSyncClick,
  }: {
    source: { path: string; version: number } | null;
    dark?: boolean;
    /** destino SyncTeX: página + y en bp desde arriba */
    target?: { page: number; y: number; seq: number } | null;
    /** Ctrl/Cmd+clic sobre una página (coords en bp desde arriba-izquierda) */
    onSyncClick?: (page: number, x: number, y: number) => void;
  } = $props();

  let container: HTMLDivElement;
  let doc: PDFDocumentProxy | null = null;
  let task: PDFDocumentLoadingTask | null = null;
  let observer: IntersectionObserver | null = null;
  let zoom = $state(1); // multiplicador sobre "ajustar al ancho"; 100% por defecto
  let numPages = $state(0);
  let loadSeq = 0;
  let renderSeq = 0; // invalida pinturas en vuelo cuando cambia doc/zoom/ancho
  let paintTasks = new Set<{ cancel: () => void }>();
  let renderScale = 1; // px CSS por bp del último render
  let targetSeen = 0;
  // Ctrl (o Cmd) pulsado: el clic salta al código y el cursor lo anuncia
  let armed = $state(false);
  const arm = (e: KeyboardEvent | PointerEvent) =>
    (armed = !!onSyncClick && (e.ctrlKey || e.metaKey));
  // Al soltar la propia tecla, GTK aún la da por pulsada en `ctrlKey`: sin
  // esto el cursor se quedaba «armado» después de soltar Ctrl.
  const disarm = (e: KeyboardEvent) =>
    e.key === "Control" || e.key === "Meta" ? (armed = false) : arm(e);

  // SyncTeX forward: scroll a la página/altura y marcador temporal.
  $effect(() => {
    if (!target || target.seq === targetSeen || !container) return;
    targetSeen = target.seq;
    const holder = container.querySelector<HTMLElement>(`[data-page="${target.page}"]`);
    if (!holder) return;
    const y = target.y * renderScale;
    container.scrollTo({
      top: holder.offsetTop + y - container.clientHeight / 3,
      behavior: "smooth",
    });
    const mark = document.createElement("div");
    mark.className = "sync-mark";
    mark.style.top = `${y}px`;
    holder.appendChild(mark);
    setTimeout(() => mark.remove(), 1600);
  });

  function onContainerClick(e: MouseEvent) {
    if (!(e.ctrlKey || e.metaKey) || !onSyncClick) return;
    const holder = (e.target as HTMLElement).closest<HTMLElement>(".page");
    if (!holder) return;
    const rect = holder.getBoundingClientRect();
    onSyncClick(
      Number(holder.dataset.page),
      (e.clientX - rect.left) / renderScale,
      (e.clientY - rect.top) / renderScale,
    );
  }

  $effect(() => {
    if (source) load(source.path, source.version);
  });

  onDestroy(() => {
    observer?.disconnect();
    task?.destroy();
  });

  async function load(path: string, _version: number) {
    const seq = ++loadSeq;
    try {
      const bytes = await readFileBytes(path);
      const newTask = pdfjs.getDocument({ data: new Uint8Array(bytes) });
      const newDoc = await newTask.promise;
      if (seq !== loadSeq) {
        newTask.destroy();
        return;
      }
      const oldTask = task;
      task = newTask;
      doc = newDoc;
      numPages = newDoc.numPages;
      const scroll = container?.scrollTop ?? 0;
      await render();
      container.scrollTop = scroll;
      oldTask?.destroy();
    } catch (e) {
      console.error("PDF load:", e);
    }
  }

  // Placeholders del tamaño de cada página; el canvas se pinta solo cuando
  // la página entra al viewport (IntersectionObserver).
  async function render() {
    if (!doc || !container) return;
    const seq = ++renderSeq;
    for (const t of paintTasks) t.cancel(); // pinturas del render anterior
    paintTasks.clear();
    observer?.disconnect();
    // conservar la posición como fracción: las alturas cambian con el zoom
    const ratio = container.scrollHeight > 0 ? container.scrollTop / container.scrollHeight : 0;
    container.replaceChildren();
    const first = await doc.getPage(1);
    if (seq !== renderSeq || !doc) return; // otro render/load empezó durante el await
    const base = first.getViewport({ scale: 1 });
    const fitWidth = Math.max(container.clientWidth - 32, 100) / base.width;
    const scale = fitWidth * zoom;
    renderScale = scale;

    observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            observer?.unobserve(e.target);
            paintPage(e.target as HTMLElement, scale, seq);
          }
        }
      },
      { root: container, rootMargin: "300px" },
    );

    for (let i = 1; i <= doc.numPages; i++) {
      const holder = document.createElement("div");
      holder.className = "page";
      holder.dataset.page = String(i);
      const box = deviceBox(base.width * scale, base.height * scale);
      holder.style.width = `${box.w}px`;
      holder.style.height = `${box.h}px`;
      container.appendChild(holder);
      observer.observe(holder);
    }
    container.scrollTop = ratio * container.scrollHeight;
  }

  async function paintPage(holder: HTMLElement, scale: number, seq: number) {
    if (!doc || seq !== renderSeq) return;
    try {
      const page = await doc.getPage(Number(holder.dataset.page));
      if (seq !== renderSeq) return;
      const vp = page.getViewport({ scale });
      // Un píxel de canvas por píxel físico, exacto. WebKitGTK reduce los
      // canvas sin filtrar: sobremuestrear (2×, 3×) no suaviza, se come los
      // trazos finos de Computer Modern. Comprobado en la app real.
      const box = deviceBox(vp.width, vp.height);
      const canvas = document.createElement("canvas");
      canvas.width = box.pw;
      canvas.height = box.ph;
      canvas.style.width = `${box.w}px`;
      canvas.style.height = `${box.h}px`;
      holder.style.width = canvas.style.width;
      holder.style.height = canvas.style.height;
      // Sin `canvasContext`: el contexto lo crea pdf.js, opaco y por software
      // (`willReadFrequently`), que es su valor por defecto. Pasarle uno propio
      // lo cambia por un canvas acelerado por GPU.
      const paint = page.render({
        canvas,
        viewport: vp,
        transform: [canvas.width / vp.width, 0, 0, canvas.height / vp.height, 0, 0],
      });
      paintTasks.add(paint);
      try {
        await paint.promise;
      } finally {
        paintTasks.delete(paint);
      }
      if (seq !== renderSeq) return;
      // Capa de texto de pdf.js: spans transparentes sobre el canvas, para
      // poder seleccionar y copiar. No interviene en el pintado.
      // ponytail: sin el `endOfContent` del visor de pdf.js; arrastrar por un
      // hueco entre párrafos puede alargar la selección. Añadirlo si molesta.
      const textDiv = document.createElement("div");
      textDiv.className = "textLayer";
      textDiv.style.setProperty("--total-scale-factor", String(scale));
      const text = new pdfjs.TextLayer({
        textContentSource: page.streamTextContent(),
        container: textDiv,
        viewport: vp,
      });
      paintTasks.add(text);
      text
        .render()
        .catch(() => {}) // cancelada por zoom/recarga
        .finally(() => paintTasks.delete(text));
      holder.replaceChildren(canvas, textDiv);
    } catch (e) {
      // la cancelación por zoom/recarga no es un error
      if ((e as Error)?.name !== "RenderingCancelledException") console.error("PDF page:", e);
    }
  }

  /** Tamaño de página en píxeles físicos enteros (`pw`×`ph`) y su
   *  equivalente CSS, fraccionario si el dpr lo es (1.25 en GTK a 120 ppp). */
  function deviceBox(cssW: number, cssH: number) {
    const dpr = window.devicePixelRatio || 1;
    const pw = Math.max(1, Math.floor(cssW * dpr));
    const ph = Math.max(1, Math.floor(cssH * dpr));
    return { pw, ph, w: pw / dpr, h: ph / dpr };
  }

  function setZoom(z: number) {
    zoom = Math.min(4, Math.max(0.25, z));
    render();
  }

  // Re-ajustar al ancho cuando cambia el tamaño del panel (arrastre del divisor).
  $effect(() => {
    if (!container) return;
    let lastW = container.clientWidth;
    let timer: ReturnType<typeof setTimeout>;
    const ro = new ResizeObserver(() => {
      if (container.clientWidth === lastW) return;
      lastW = container.clientWidth;
      clearTimeout(timer);
      timer = setTimeout(render, 150);
    });
    ro.observe(container);
    return () => {
      clearTimeout(timer);
      ro.disconnect();
    };
  });
</script>

<svelte:window onkeydown={arm} onkeyup={disarm} onblur={() => (armed = false)} />

<div class="viewer">
  <div class="toolbar">
    <button onclick={() => setZoom(zoom - 0.15)} title={t("zoomOut")} aria-label={t("zoomOut")}><Minus /></button>
    <span class="zoom">{Math.round(zoom * 100)}%</span>
    <button onclick={() => setZoom(zoom + 0.15)} title={t("zoomIn")} aria-label={t("zoomIn")}><Plus /></button>
    <button onclick={() => setZoom(1)} title={t("fitWidth")} aria-label={t("fitWidth")}><MoveHorizontal /></button>
    {#if numPages}<span class="pages">{t("pageCount", numPages)}</span>{/if}
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_click_events_have_key_events -->
  <div
    class="pdf-scroll"
    class:invert={dark && settings.pdfInvert}
    class:sync={armed}
    bind:this={container}
    onclick={onContainerClick}
    onpointermove={arm}
    role="document"
    title={onSyncClick ? t("syncClickTitle") : undefined}
  >
    {#if !source}
      <p class="empty">{t("compileForPdf")}</p>
    {/if}
  </div>
</div>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--pdf-bg);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    flex: none;
  }
  .toolbar button {
    width: 26px;
    height: 24px;
    padding: 0;
    display: grid;
    place-items: center;
    color: var(--fg-dim);
  }
  .toolbar button:hover {
    color: var(--fg);
  }
  .zoom {
    font-size: 12px;
    min-width: 40px;
    text-align: center;
  }
  .pages {
    font-size: 12px;
    color: var(--fg-dim);
    margin-left: auto;
  }
  .pdf-scroll {
    position: relative; /* referencia del offsetTop de las páginas */
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 16px;
  }
  .pdf-scroll :global(.page) {
    background: white;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.25);
    flex: none;
    position: relative; /* ancla del marcador SyncTeX */
    line-height: 0; /* sin hueco de línea base bajo el canvas */
  }
  /* papel oscuro: invierte la luminosidad y conserva los tonos */
  .pdf-scroll.invert :global(.page) {
    background: #141414;
  }
  .pdf-scroll.invert :global(.page canvas) {
    filter: invert(0.92) hue-rotate(180deg);
  }
  /* Capa de texto: lo mínimo de pdfjs-dist/web/pdf_viewer.css (`.textLayer`).
     Las dimensiones y el tamaño de cada span los pone pdf.js con estas variables. */
  .pdf-scroll :global(.textLayer) {
    --scale-round-x: 1px;
    --scale-round-y: 1px;
    --min-font-size: 1;
    --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size));
    --min-font-size-inv: calc(1 / var(--min-font-size));
    position: absolute;
    inset: 0;
    overflow: clip;
    text-align: initial;
    line-height: 1;
    letter-spacing: normal;
    word-spacing: normal;
    -webkit-text-size-adjust: none;
    text-size-adjust: none;
    transform-origin: 0 0;
  }
  .pdf-scroll :global(.textLayer :is(span, br)) {
    color: transparent;
    position: absolute;
    white-space: pre;
    cursor: var(--cur-text, text);
    transform-origin: 0% 0%;
    -webkit-user-select: text;
    user-select: text;
  }
  .pdf-scroll :global(.textLayer > :not(.markedContent)),
  .pdf-scroll :global(.textLayer .markedContent span:not(.markedContent)) {
    --font-height: 0;
    --scale-x: 1;
    --rotate: 0deg;
    font-size: calc(var(--text-scale-factor) * var(--font-height));
    transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv));
  }
  .pdf-scroll :global(.textLayer .markedContent) {
    display: contents;
  }
  .pdf-scroll :global(.textLayer ::selection) {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  /* Ctrl pulsado: el clic va al código, no empieza una selección */
  .pdf-scroll.sync :global(.page),
  .pdf-scroll.sync :global(.textLayer :is(span, br)) {
    cursor: var(--cur-sync, pointer);
    -webkit-user-select: none;
    user-select: none;
  }
  .pdf-scroll :global(.sync-mark) {
    position: absolute;
    left: 0;
    right: 0;
    height: 1.2em;
    transform: translateY(-1em);
    background: var(--accent-dim);
    border-left: 3px solid var(--accent);
    pointer-events: none;
    animation: sync-fade 1.6s ease-out forwards;
  }
  @keyframes sync-fade {
    from {
      opacity: 1;
    }
    to {
      opacity: 0;
    }
  }
  .empty {
    color: var(--fg-dim);
    font-size: 13px;
    margin: auto;
  }
</style>
