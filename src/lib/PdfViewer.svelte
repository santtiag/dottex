<script lang="ts">
  import { onDestroy } from "svelte";
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
  import { readFileBytes } from "./ipc";
  import { settings } from "./state.svelte";
  import { t } from "./i18n.svelte";

  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

  // `path` relativo al proyecto; `version` fuerza recarga del mismo archivo.
  let {
    source,
    target = null,
    onSyncClick,
  }: {
    source: { path: string; version: number } | null;
    /** destino SyncTeX: página + y en bp desde arriba */
    target?: { page: number; y: number; seq: number } | null;
    /** Ctrl/Cmd+clic sobre una página (coords en bp desde arriba-izquierda) */
    onSyncClick?: (page: number, x: number, y: number) => void;
  } = $props();

  let container: HTMLDivElement;
  let doc: PDFDocumentProxy | null = null;
  let task: PDFDocumentLoadingTask | null = null;
  let observer: IntersectionObserver | null = null;
  let zoom = $state(1); // multiplicador sobre "ajustar al ancho"
  let numPages = $state(0);
  let loadSeq = 0;
  let renderScale = 1; // px CSS por bp del último render
  let targetSeen = 0;

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
    observer?.disconnect();
    container.replaceChildren();
    const first = await doc.getPage(1);
    const base = first.getViewport({ scale: 1 });
    const fitWidth = Math.max(container.clientWidth - 32, 100) / base.width;
    const scale = fitWidth * zoom;
    renderScale = scale;

    observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            observer?.unobserve(e.target);
            paintPage(e.target as HTMLElement, scale);
          }
        }
      },
      { root: container, rootMargin: "300px" },
    );

    for (let i = 1; i <= doc.numPages; i++) {
      const holder = document.createElement("div");
      holder.className = "page";
      holder.dataset.page = String(i);
      holder.style.width = `${Math.floor(base.width * scale)}px`;
      holder.style.height = `${Math.floor(base.height * scale)}px`;
      container.appendChild(holder);
      observer.observe(holder);
    }
  }

  async function paintPage(holder: HTMLElement, scale: number) {
    if (!doc) return;
    try {
      const page = await doc.getPage(Number(holder.dataset.page));
      const vp = page.getViewport({ scale });
      // ponytail: mínimo 2x — WebKitGTK reporta dpr=1 en HiDPI y el PDF sale borroso
      const dpr = Math.max(window.devicePixelRatio || 1, 2);
      const canvas = document.createElement("canvas");
      canvas.width = Math.floor(vp.width * dpr);
      canvas.height = Math.floor(vp.height * dpr);
      canvas.style.width = `${Math.floor(vp.width)}px`;
      canvas.style.height = `${Math.floor(vp.height)}px`;
      holder.style.width = canvas.style.width;
      holder.style.height = canvas.style.height;
      const ctx = canvas.getContext("2d")!;
      await page.render({ canvas, canvasContext: ctx, viewport: vp, transform: [dpr, 0, 0, dpr, 0, 0] }).promise;
      holder.replaceChildren(canvas);
    } catch (e) {
      console.error("PDF page:", e);
    }
  }

  function setZoom(z: number) {
    zoom = Math.min(4, Math.max(0.25, z));
    render();
  }
</script>

<div class="viewer" class:invert={settings.pdfInvert}>
  <div class="toolbar">
    <button onclick={() => setZoom(zoom - 0.15)} title={t("zoomOut")}>−</button>
    <span class="zoom">{Math.round(zoom * 100)}%</span>
    <button onclick={() => setZoom(zoom + 0.15)} title={t("zoomIn")}>+</button>
    <button onclick={() => setZoom(1)} title={t("fitWidth")}>⤢</button>
    {#if numPages}<span class="pages">{t("pageCount", numPages)}</span>{/if}
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_click_events_have_key_events -->
  <div
    class="pdf-scroll"
    bind:this={container}
    onclick={onContainerClick}
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
  }
  /* invierte los colores del PDF (opcional en Configuración), en cualquier tema */
  .viewer.invert .pdf-scroll :global(canvas) {
    filter: invert(0.93) hue-rotate(180deg);
  }
  .viewer.invert .pdf-scroll :global(.page) {
    background: #121316;
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
