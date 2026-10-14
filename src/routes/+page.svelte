<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    ChevronDown,
    Circle,
    CircleCheck,
    CircleX,
    Download,
    Eraser,
    LoaderCircle,
    Minus,
    PanelLeft,
    PanelRight,
    PencilLine,
    Play,
    ScrollText,
    Settings as SettingsIcon,
    Square,
    TriangleAlert,
    X,
    setLucideProps,
  } from "@lucide/svelte";
  import type { CompileStatus, RecentProject } from "$lib/ipc";
  import { cleanArtifacts, getRecentProjects, saveAsTemplate } from "$lib/ipc";
  import {
    app,
    autoCompile,
    closeProject,
    exportPdf,
    fileBlobUrl,
    onCompileStatus,
    openProject,
    pickAndOpenProject,
    problems,
    refreshActiveFromDisk,
    reloadTree,
    requestCompile,
    saveActive,
    setRootFile,
    settings,
    syncFromPdf,
    syncToPdf,
    toast,
  } from "$lib/state.svelte";
  import { invalidateCompletions, loadSnippets } from "$lib/latexComplete";
  import { clearImgCache } from "$lib/visualLatex";
  import { cursorVars } from "$lib/cursors";
  import { t } from "$lib/i18n.svelte";
  import Editor from "$lib/Editor.svelte";
  import FileTree from "$lib/FileTree.svelte";
  import IssueList from "$lib/IssueList.svelte";
  import Outline from "$lib/Outline.svelte";
  import PdfViewer from "$lib/PdfViewer.svelte";
  import SearchPanel from "$lib/SearchPanel.svelte";
  import Settings from "$lib/Settings.svelte";
  import Welcome from "$lib/Welcome.svelte";
  import "../app.css";

  // estilo único de iconos para toda la app
  setLucideProps({ size: 16, strokeWidth: 1.75 });

  const win = getCurrentWindow();
  // En macOS los botones nativos (traffic lights) se superponen a la barra.
  const isMac = navigator.userAgent.includes("Macintosh");

  let sysDark = $state(false);
  const dark = $derived(app.theme === "auto" ? sysDark : app.theme === "dark");
  let sidebarOpen = $state(true);
  let pdfOpen = $state(true);
  let sidebarTab = $state<"files" | "outline" | "search" | "issues">("files");
  let sidebarW = $state(244);
  // Parte del espacio editor+PDF que ocupa el panel derecho. Una fracción y no
  // píxeles: el reparto se conserva al pasar de media pantalla a completa.
  let pdfFrac = $state(0.5);
  // Anchos efectivos: en una ventana estrecha (media pantalla en un tiling
  // WM) los paneles ceden sitio para que el editor no quede aplastado.
  let winW = $state(1280);
  const sideW = $derived(Math.min(sidebarW, Math.round(winW * 0.28)));
  // lo que se reparten el editor y el panel derecho
  const splitW = $derived(winW - (sidebarOpen ? sideW + 4 : 0) - 4);
  const MIN_PANE = 160;
  const fitRight = (w: number) => Math.max(MIN_PANE, Math.min(splitW - MIN_PANE, w));
  const rightW = $derived(Math.round(fitRight(splitW * pdfFrac)));
  let logEl: HTMLDivElement | undefined = $state();
  let settingsOpen = $state(false);
  // layout de paneles .tex: código|visual a la izquierda, pdf|visual-lectura a la derecha
  let layout = $state<"code-pdf" | "visual-pdf" | "code-visual">("code-pdf");
  let liveTex = $state("");
  const isTex = $derived(app.active?.kind === "text" && app.active.path.endsWith(".tex"));
  $effect.pre(() => {
    const a = app.active;
    void a?.path;
    liveTex = a?.kind === "text" ? a.content : "";
  });
  function setLeft(v: "code" | "visual") {
    layout = v === "visual" ? "visual-pdf" : layout === "code-visual" ? "code-visual" : "code-pdf";
  }
  function setRight(v: "pdf" | "visual") {
    layout = v === "visual" ? "code-visual" : layout === "visual-pdf" ? "visual-pdf" : "code-pdf";
  }
  // buffer en vivo del editor: compilar siempre guarda lo que se ve en pantalla
  let editorRef = $state<{ getText: () => string | undefined }>();
  // menú del proyecto (abrir otra carpeta, recientes, cerrar)
  let projMenuOpen = $state(false);
  let recents = $state<RecentProject[]>([]);

  // «Guardar como plantilla…»: null = entrada de menú; texto = campo del nombre
  let tplName = $state<string | null>(null);

  async function saveTemplate(e: SubmitEvent) {
    e.preventDefault();
    const name = tplName?.trim();
    if (!name) return;
    try {
      // la plantilla se copia del disco: que incluya lo que hay en pantalla
      const text = editorRef?.getText();
      if (app.dirty && text !== undefined) await saveActive(text);
      await saveAsTemplate(name);
      toast(t("templateSaved", name));
    } catch (err) {
      toast(String(err));
    }
    projMenuOpen = false;
  }

  async function toggleProjMenu() {
    projMenuOpen = !projMenuOpen;
    tplName = null;
    if (projMenuOpen)
      recents = (await getRecentProjects()).filter((r) => r.path !== app.project?.path);
  }

  onMount(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    sysDark = mq.matches;
    mq.addEventListener("change", (e) => (sysDark = e.matches));
    loadSnippets();

    // la interfaz ya está montada debajo: fundir y retirar el skeleton de arranque
    const boot = document.getElementById("boot");
    boot?.classList.add("out");
    setTimeout(() => boot?.remove(), 200);

    // las líneas del log llegan en ráfagas: acumular y volcar una vez por frame
    let logBuf: string[] = [];
    let logFlush = false;
    const unlisten = Promise.all([
      listen<CompileStatus>("compile:status", (e) => onCompileStatus(e.payload)),
      listen<string>("compile:log", (e) => {
        logBuf.push(e.payload);
        if (logFlush) return;
        logFlush = true;
        requestAnimationFrame(() => {
          logFlush = false;
          app.log.push(...logBuf);
          logBuf = [];
          if (app.log.length > 2000) app.log.splice(0, app.log.length - 2000);
          logEl?.scrollTo(0, logEl.scrollHeight);
        });
      }),
      // el payload indica si el cambio fue estructural (crear/borrar/renombrar)
      listen<boolean>("fs:changed", (e) => {
        if (e.payload) reloadTree();
        refreshActiveFromDisk();
        invalidateCompletions();
        clearImgCache(); // imágenes editadas en disco se recargan al re-renderizar
      }),
    ]);
    return () => unlisten.then((fns) => fns.forEach((f) => f()));
  });

  // Cursores propios: variables en <html> para que alcancen también a los
  // tooltips que CodeMirror cuelga del body.
  let cursorNames: string[] = [];
  $effect(() => {
    const root = document.documentElement.style;
    if (!settings.customCursor) {
      for (const name of cursorNames) root.removeProperty(name);
      return;
    }
    let stale = false; // el tema cambió antes de que terminara de dibujarse
    cursorVars(dark).then((vars) => {
      if (stale) return;
      cursorNames = Object.keys(vars);
      for (const [name, value] of Object.entries(vars)) root.setProperty(name, value);
    });
    return () => {
      stale = true;
    };
  });

  let shellEl: HTMLDivElement;
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
  /** Anillo sutil en el punto del clic; puramente decorativo. */
  function clickRing(e: PointerEvent) {
    if (!settings.clickFx || e.button !== 0 || reduceMotion.matches) return;
    const ring = document.createElement("span");
    ring.className = "click-ring";
    ring.style.left = `${e.clientX}px`;
    ring.style.top = `${e.clientY}px`;
    ring.onanimationend = () => ring.remove();
    shellEl.append(ring); // dentro del shell: hereda el acento del tema
  }

  const focusSelect = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };

  /** Ctrl+Enter: guarda, fija el archivo activo como raíz si no hay una y compila. */
  async function compileActive(text?: string) {
    if (!app.project) return;
    if (text !== undefined) await saveActive(text);
    if (!app.project.root_file && app.active?.path.endsWith(".tex")) {
      await setRootFile(app.active.path); // setRootFile ya dispara la compilación
    } else {
      requestCompile();
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") projMenuOpen = false;
    // e.defaultPrevented: CodeMirror ya manejó Mod-Enter (guardó y compiló)
    if (!(e.ctrlKey || e.metaKey) || e.defaultPrevented) return;
    if (e.key === "Enter") {
      e.preventDefault();
      compileActive(editorRef?.getText());
    } else if (e.key === ",") {
      e.preventDefault();
      settingsOpen = !settingsOpen;
    }
  }

  // Redimensionado de paneles arrastrando los separadores.
  function dragResize(e: MouseEvent, side: "left" | "right") {
    e.preventDefault();
    const startX = e.clientX;
    // Se parte del ancho que se ve, no del guardado: si la ventana lo tenía
    // recortado, el divisor tardaba en responder o no se movía.
    const start = side === "left" ? sideW : rightW;
    const move = (ev: MouseEvent) => {
      const d = ev.clientX - startX;
      if (side === "left") sidebarW = Math.max(140, Math.min(500, start + d));
      else pdfFrac = fitRight(start - d) / splitW;
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  async function clean() {
    try {
      await cleanArtifacts();
      toast(t("artifactsCleaned"));
    } catch (e) {
      toast(String(e));
    }
  }

  const issueCount = $derived(problems().length);
  const errorCount = $derived(problems().filter((i) => i.severity === "error").length);
  const warnCount = $derived(issueCount - errorCount);
  const StatusIcon = $derived(
    app.typing
      ? PencilLine
      : { idle: Circle, compiling: LoaderCircle, ok: CircleCheck, error: CircleX }[app.compileState],
  );
  const statusText = $derived(
    app.typing
      ? t("statusTyping")
      : {
          idle: t("statusIdle"),
          compiling: t("statusCompiling"),
          ok: t("statusOk"),
          error: t("statusError"),
        }[app.compileState],
  );

  // migas de pan: ruta del archivo + cadena de secciones hasta el cursor
  // ponytail: usa el contenido guardado (no el buffer en vivo); se actualiza al guardar
  // Índice de secciones recalculado solo cuando cambia el contenido; el
  // derivado del cursor solo camina este array (mover el cursor era antes
  // un split("\n") del documento entero).
  const sectionIndex = $derived.by(() => {
    if (!settings.breadcrumbs || app.active?.kind !== "text") return [];
    const out: { line: number; level: number; title: string }[] = [];
    const lines = app.active.content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      const m = lines[i].match(/\\(subsubsection|subsection|section)\*?\{([^}]*)/);
      if (m) {
        out.push({
          line: i + 1,
          level: m[1] === "section" ? 1 : m[1] === "subsection" ? 2 : 3,
          title: m[2],
        });
      }
    }
    return out;
  });
  const crumbs = $derived.by(() => {
    if (!settings.breadcrumbs || app.active?.kind !== "text") return [];
    let sec = "";
    let sub = "";
    let subsub = "";
    for (const s of sectionIndex) {
      if (s.line > app.cursorLine) break;
      if (s.level === 1) [sec, sub, subsub] = [s.title, "", ""];
      else if (s.level === 2) [sub, subsub] = [s.title, ""];
      else subsub = s.title;
    }
    return [...app.active.path.split("/"), sec, sub, subsub].filter(Boolean);
  });
</script>

<svelte:window onkeydown={onKeydown} onpointerdown={clickRing} bind:innerWidth={winW} />
<svelte:head><title>{app.project ? `${app.project.name} — dottex` : "dottex"}</title></svelte:head>

<div class="shell" class:dark bind:this={shellEl}>
  <header class="titlebar" data-tauri-drag-region>
    {#if isMac}<span class="mac-pad"></span>{/if}
    {#if app.project}
      <button class="tb" title={t("toggleSidebar")} aria-label={t("toggleSidebar")} onclick={() => (sidebarOpen = !sidebarOpen)}><PanelLeft /></button>
      <div class="projwrap">
        <button class="tb proj" title={t("switchProject")} onclick={toggleProjMenu}>
          <span class="pname">{app.project.name}</span><ChevronDown size={13} />
        </button>
        {#if projMenuOpen}
          <button class="menu-backdrop" aria-label={t("close")} onclick={() => (projMenuOpen = false)}></button>
          <div class="menu">
            <button
              onclick={() => {
                projMenuOpen = false;
                pickAndOpenProject();
              }}
            >
              {t("openFolder")}
            </button>
            {#if recents.length}
              <div class="mhead">{t("recent")}</div>
              {#each recents as r (r.path)}
                <button
                  class="mrecent"
                  onclick={() => {
                    projMenuOpen = false;
                    openProject(r.path);
                  }}
                >
                  <strong>{r.name}</strong><span>{r.path}</span>
                </button>
              {/each}
            {/if}
            <div class="msep"></div>
            {#if tplName === null}
              <button onclick={() => (tplName = app.project?.name ?? "")}>{t("saveAsTemplate")}</button>
            {:else}
              <form class="mform" onsubmit={saveTemplate}>
                <input bind:value={tplName} placeholder={t("templateName")} use:focusSelect />
                <button type="submit">{t("save")}</button>
              </form>
            {/if}
            <button
              onclick={() => {
                projMenuOpen = false;
                closeProject();
              }}
            >
              {t("closeProject")}
            </button>
          </div>
        {/if}
      </div>
      <span class="file" data-tauri-drag-region>
        {app.active?.path ?? ""}{#if app.dirty}&nbsp;<span class="dot" title={t("unsaved")}></span>{/if}
      </span>
    {:else}
      <span class="appname" data-tauri-drag-region>dottex</span>
    {/if}
    <span class="spacer" data-tauri-drag-region></span>
    {#if app.project}
      <button
        class="compile"
        onclick={() => compileActive(editorRef?.getText())}
        disabled={app.compileState === "compiling"}
        title={t("compileTitle")}
      >
        {#if app.compileState === "compiling"}
          <span class="spin"><LoaderCircle size={14} /></span>{t("compiling")}
        {:else}
          <Play size={14} />{t("compile")}
        {/if}
      </button>
      <button class="tb" onclick={exportPdf} disabled={!app.pdf} title={t("exportPdfTitle")}><Download />PDF</button>
      <button class="tb" title={t("togglePdf")} aria-label={t("togglePdf")} onclick={() => (pdfOpen = !pdfOpen)}><PanelRight /></button>
    {/if}
    <button
      class="tb"
      title="{t('settings')} (Ctrl+,)"
      aria-label={t("settings")}
      onclick={() => (settingsOpen = true)}
    >
      <SettingsIcon />
    </button>
    {#if !isMac}
      <div class="winctl">
        <button title={t("minimize")} onclick={() => win.minimize()} aria-label={t("minimize")}>
          <Minus size={14} />
        </button>
        <button title={t("maximize")} onclick={() => win.toggleMaximize()} aria-label={t("maximize")}>
          <Square size={12} />
        </button>
        <button class="close" title={t("close")} onclick={() => win.close()} aria-label={t("close")}>
          <X size={14} />
        </button>
      </div>
    {/if}
  </header>

  {#if !app.project}
    <Welcome />
  {:else}
    <main
      class="panes"
      style:grid-template-columns="{sidebarOpen ? `${sideW}px 4px` : ""} 1fr {pdfOpen
        ? `4px ${rightW}px`
        : ""}"
    >
      {#if sidebarOpen}
        <aside class="sidebar">
          <nav class="tabs">
            <button class:active={sidebarTab === "files"} onclick={() => (sidebarTab = "files")}>
              {t("tabFiles")}
            </button>
            <button class:active={sidebarTab === "outline"} onclick={() => (sidebarTab = "outline")}>
              {t("tabOutline")}
            </button>
            <button class:active={sidebarTab === "search"} onclick={() => (sidebarTab = "search")}>
              {t("tabSearch")}
            </button>
            <button class:active={sidebarTab === "issues"} onclick={() => (sidebarTab = "issues")}>
              {t("tabIssues")}
              {#if issueCount}<span class="count" class:err={errorCount}>{issueCount}</span>{/if}
            </button>
          </nav>
          {#if sidebarTab === "files"}
            <FileTree />
          {:else if sidebarTab === "outline"}
            <Outline />
          {:else if sidebarTab === "search"}
            <SearchPanel />
          {:else}
            <IssueList />
          {/if}
        </aside>
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div class="resizer" role="separator" aria-orientation="vertical" onmousedown={(e) => dragResize(e, "left")}></div>
      {/if}

      <section class="center">
        {#if crumbs.length || isTex}
          <div class="editorbar">
            <span class="crumbs" title={crumbs.join(" › ")}>
              {#each crumbs as c, i}{#if i}<span class="sep">›</span>{/if}{c}{/each}
            </span>
            {#if isTex}
              <div class="viewmode">
                <button class:active={layout !== "visual-pdf"} onclick={() => setLeft("code")}>
                  {t("viewCode")}
                </button>
                <button class:active={layout === "visual-pdf"} onclick={() => setLeft("visual")}>
                  {t("viewVisual")}
                </button>
              </div>
            {/if}
          </div>
        {/if}
        {#if !app.active}
          <p class="empty">{t("selectFile")}</p>
        {:else if app.active.kind === "text"}
          <Editor
            bind:this={editorRef}
            path={app.active.path}
            content={app.active.content}
            {dark}
            auto={settings.autoCompile}
            visual={layout === "visual-pdf" && isTex}
            goto={app.goto}
            onSave={saveActive}
            onDirty={() => {
              app.dirty = true;
              if (settings.autoCompile) app.typing = true;
            }}
            onIdle={autoCompile}
            onCompile={compileActive}
            onCursor={(l) => (app.cursorLine = l)}
            onSyncClick={(l) => syncToPdf(l)}
            onDocChange={(text) => (liveTex = text)}
          />
        {:else if app.active.kind === "image"}
          {#await fileBlobUrl(app.active.path) then url}
            <div class="media"><img src={url} alt={app.active.path} /></div>
          {/await}
        {:else if app.active.kind === "pdf"}
          <PdfViewer source={{ path: app.active.path, version: 0 }} {dark} />
        {:else}
          <p class="empty">{t("unsupportedFormat", app.active.path)}</p>
        {/if}
      </section>

      {#if pdfOpen}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div class="resizer" role="separator" aria-orientation="vertical" onmousedown={(e) => dragResize(e, "right")}></div>
        <aside class="pdfpane">
          {#if isTex}
            <div class="editorbar">
              <div class="viewmode">
                <button class:active={layout !== "code-visual"} onclick={() => setRight("pdf")}>
                  {t("viewPdf")}
                </button>
                <button class:active={layout === "code-visual"} onclick={() => setRight("visual")}>
                  {t("viewVisual")}
                </button>
              </div>
            </div>
          {/if}
          <div class="pdfbody">
            {#if layout === "code-visual" && isTex && app.active?.kind === "text"}
              <Editor path={app.active.path} content={liveTex} {dark} visual readonly />
            {:else}
              <PdfViewer source={app.pdf} target={app.pdfTarget} {dark} onSyncClick={syncFromPdf} />
            {/if}
          </div>
        </aside>
      {/if}
    </main>

    {#if app.showLog}
      <!-- texto plano con pre-wrap: evita re-diferenciar hasta 2000 divs por frame -->
      <div class="log" bind:this={logEl}>{app.log.join("\n")}</div>
    {/if}

    <footer class="status">
      <span class="state {app.compileState}" class:typing={app.typing}>
        <span class:spin={app.compileState === "compiling" && !app.typing}><StatusIcon size={13} /></span>
        {statusText}
      </span>
      {#if issueCount}
        <button
          class="issues-btn"
          onclick={() => {
            sidebarOpen = true;
            sidebarTab = "issues";
          }}
        >
          {#if errorCount}<span class="e"><CircleX size={13} />{errorCount}</span>{/if}
          {#if warnCount}<span class="w"><TriangleAlert size={13} />{warnCount}</span>{/if}
        </button>
      {/if}
      {#if app.project.root_file}<span class="rootfile">{t("rootPrefix", app.project.root_file)}</span>{/if}
      <span class="spacer"></span>
      <button class="sbtn" onclick={clean} title={t("cleanArtifacts")} aria-label={t("cleanArtifacts")}>
        <Eraser size={14} />
      </button>
      <button
        class="sbtn"
        class:on={app.showLog}
        onclick={() => (app.showLog = !app.showLog)}
        title={app.showLog ? t("hideLog") : t("showLog")}
        aria-label={app.showLog ? t("hideLog") : t("showLog")}
      >
        <ScrollText size={14} />
      </button>
    </footer>
  {/if}

  {#if app.opening}
    <!-- silueta mientras se escanea la carpeta; va encima para no desmontar nada -->
    <div class="opening" aria-hidden="true">
      <div class="o-side">
        {#each [70, 55, 82, 48, 64, 40] as w (w)}<div class="sk" style:width="{w}%"></div>{/each}
      </div>
      <div class="o-center">
        {#each [42, 88, 76, 93, 61, 0, 84, 70, 90, 35, 0, 79, 66] as w, i (i)}
          <div class={w ? "sk" : ""} style:width="{w}%"></div>
        {/each}
      </div>
      <div class="o-pdf"><div class="sk"></div></div>
    </div>
  {/if}

  {#if settingsOpen}
    <Settings onClose={() => (settingsOpen = false)} />
  {/if}

  {#if app.toast}
    <div class="toast">{app.toast}</div>
  {/if}
</div>

<style>
  .shell {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border); /* contorno de ventana sin decoraciones */
    position: relative; /* ancla de .opening */
  }
  /* ---- silueta al abrir un proyecto ---- */
  .opening {
    position: absolute;
    inset: 40px 0 0; /* bajo la barra de título */
    z-index: 50;
    display: grid;
    grid-template-columns: 28% 1fr 40%;
    background: var(--bg);
    /* 120 ms de retardo: una carpeta pequeña abre antes y no llega a verse */
    animation: opening-in 0.15s ease 0.12s both;
  }
  @keyframes opening-in {
    from {
      opacity: 0;
    }
  }
  .opening > div {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .opening .sk,
  .opening .o-center > div {
    height: 12px;
    flex: none;
  }
  .o-side {
    background: var(--panel);
    border-right: 1px solid var(--border);
  }
  /* sobre el panel, el degradado normal de .sk (panel/hover) no se distingue */
  .o-side .sk {
    background-image: linear-gradient(90deg, var(--hover) 25%, var(--border) 50%, var(--hover) 75%);
  }
  .o-pdf {
    background: var(--pdf-bg);
    border-left: 1px solid var(--border);
    align-items: center;
  }
  .o-pdf .sk {
    width: 80%;
    height: auto;
    aspect-ratio: 1 / 1.414; /* A4 */
    border-radius: 2px;
  }
  /* ---- barra de título integrada ---- */
  .titlebar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 40px;
    padding-left: 8px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    flex: none;
    user-select: none;
  }
  .mac-pad {
    width: 70px; /* espacio para los traffic lights nativos */
    flex: none;
  }
  .appname {
    font-weight: 700;
    padding: 0 6px;
  }
  .tb {
    flex: none;
    display: flex;
    align-items: center;
    gap: 5px;
    min-height: 26px;
  }
  .proj {
    font-weight: 600;
    max-width: 220px;
  }
  .pname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* giro del indicador de compilación */
  .spin {
    display: grid;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  /* menú del proyecto */
  .projwrap {
    position: relative;
    flex: none;
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
    background: transparent;
    border: none;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 100;
    display: flex;
    flex-direction: column;
    min-width: 280px;
    max-width: 380px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.25);
    padding: 4px;
  }
  .menu > button {
    text-align: left;
    padding: 7px 10px;
    border-radius: 6px;
  }
  .menu > button:hover {
    background: var(--hover);
  }
  .mhead {
    font-size: 11px;
    text-transform: uppercase;
    color: var(--fg-dim);
    padding: 6px 10px 2px;
  }
  .mrecent {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
  }
  .mrecent span {
    font-size: 11.5px;
    color: var(--fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 350px;
  }
  .mform {
    display: flex;
    gap: 6px;
    padding: 4px 6px;
  }
  .mform input {
    flex: 1;
    min-width: 0;
    padding: 4px 8px;
  }
  .msep {
    height: 1px;
    background: var(--border);
    margin: 4px 0;
  }
  .file {
    font-size: 12px;
    color: var(--fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .compile {
    background: var(--accent);
    color: white;
    border: none;
    padding: 5px 14px;
    border-radius: 6px;
    font-weight: 600;
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .compile:hover {
    background: var(--accent);
    filter: brightness(1.08);
  }
  .compile:disabled {
    opacity: 0.6;
  }
  .winctl {
    display: flex;
    align-self: stretch;
    margin-left: 6px;
    flex: none;
  }
  .winctl button {
    width: 44px;
    height: 100%;
    border: none;
    border-radius: 0;
    display: grid;
    place-items: center;
    color: var(--fg-dim);
  }
  .winctl button:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .winctl .close:hover {
    background: #d33;
    color: white;
  }
  /* ---- paneles ---- */
  .panes {
    flex: 1;
    display: grid;
    min-height: 0;
  }
  .sidebar {
    background: var(--panel);
    border-right: 1px solid var(--border);
    min-width: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .tabs {
    display: flex;
    flex: none;
    border-bottom: 1px solid var(--border);
  }
  .tabs button {
    flex: 1 1 auto;
    white-space: nowrap;
    border-radius: 0;
    padding: 6px 2px;
    font-size: 12px;
    color: var(--fg-dim);
    border-bottom: 2px solid transparent;
  }
  .tabs button.active {
    color: var(--fg);
    border-bottom-color: var(--accent);
  }
  .count {
    background: #c90;
    color: white;
    border-radius: 8px;
    padding: 0 6px;
    font-size: 10px;
    margin-left: 4px;
  }
  .count.err {
    background: #d33;
  }
  .center {
    min-width: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  /* barra del editor: migas de pan */
  .editorbar {
    display: flex;
    align-items: center;
    padding: 3px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    flex: none;
    min-height: 24px;
  }
  .crumbs {
    font-size: 12px;
    color: var(--fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .crumbs .sep {
    margin: 0 5px;
    opacity: 0.6;
  }
  /* conmutador Código/Visual */
  .viewmode {
    display: flex;
    gap: 2px;
    margin-left: auto;
    padding: 1px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    flex: none;
  }
  .viewmode button {
    font-size: 13px;
    padding: 2px 14px;
    border-radius: 5px;
    color: var(--fg-dim);
  }
  .viewmode button.active {
    background: var(--accent);
    color: white;
  }
  .pdfpane {
    min-width: 0;
    overflow: hidden;
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
  }
  .pdfbody {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .resizer {
    cursor: col-resize;
    background: transparent;
  }
  .resizer:hover {
    background: var(--accent-dim);
  }
  .empty {
    margin: auto;
    color: var(--fg-dim);
    font-size: 13px;
  }
  .media {
    margin: auto;
    padding: 16px;
    max-width: 100%;
    max-height: 100%;
    overflow: auto;
  }
  .media img {
    max-width: 100%;
    background: white;
  }
  /* ---- log y barra de estado ---- */
  .log {
    flex: none;
    height: 180px;
    overflow: auto;
    font-family: ui-monospace, monospace;
    font-size: 11.5px;
    padding: 6px 10px;
    background: var(--log-bg);
    border-top: 1px solid var(--border);
    white-space: pre-wrap;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 3px 8px;
    background: var(--panel);
    border-top: 1px solid var(--border);
    font-size: 12px;
    flex: none;
  }
  .state.ok {
    color: #2a9d4a;
  }
  .state.error {
    color: #d33;
  }
  .state.compiling,
  .state.typing {
    color: var(--accent);
  }
  .issues-btn .e {
    color: #d33;
    font-weight: 600;
  }
  .issues-btn .w {
    color: #c90;
    font-weight: 600;
  }
  .rootfile {
    color: var(--fg-dim);
  }
  .state,
  .issues-btn,
  .issues-btn span {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .sbtn {
    display: grid;
    padding: 3px 6px;
    color: var(--fg-dim);
  }
  .sbtn:hover,
  .sbtn.on {
    color: var(--fg);
  }
  .sbtn.on {
    background: var(--hover);
  }
  .toast {
    position: fixed;
    bottom: 44px;
    left: 50%;
    transform: translateX(-50%);
    background: #333a;
    backdrop-filter: blur(8px);
    color: white;
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13px;
    z-index: 200;
    max-width: 80vw;
  }
</style>
