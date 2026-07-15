<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { CompileStatus } from "$lib/ipc";
  import { cleanArtifacts } from "$lib/ipc";
  import {
    app,
    autoCompile,
    exportPdf,
    fileBlobUrl,
    onCompileStatus,
    pickAndOpenProject,
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

  const win = getCurrentWindow();
  // En macOS los botones nativos (traffic lights) se superponen a la barra.
  const isMac = navigator.userAgent.includes("Macintosh");

  let sysDark = $state(false);
  const dark = $derived(app.theme === "auto" ? sysDark : app.theme === "dark");
  let sidebarOpen = $state(true);
  let pdfOpen = $state(true);
  let sidebarTab = $state<"files" | "outline" | "search" | "issues">("files");
  let sidebarW = $state(230);
  let pdfW = $state(480);
  let logEl: HTMLDivElement | undefined = $state();
  let settingsOpen = $state(false);

  onMount(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    sysDark = mq.matches;
    mq.addEventListener("change", (e) => (sysDark = e.matches));
    loadSnippets();

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
      }),
    ]);
    return () => unlisten.then((fns) => fns.forEach((f) => f()));
  });

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
    if (!(e.ctrlKey || e.metaKey)) return;
    if (e.key === "Enter") {
      e.preventDefault();
      compileActive();
    } else if (e.key === ",") {
      e.preventDefault();
      settingsOpen = !settingsOpen;
    }
  }

  // Redimensionado de paneles arrastrando los separadores.
  function dragResize(e: MouseEvent, side: "left" | "right") {
    e.preventDefault();
    const startX = e.clientX;
    const start = side === "left" ? sidebarW : pdfW;
    const move = (ev: MouseEvent) => {
      const d = ev.clientX - startX;
      if (side === "left") sidebarW = Math.max(140, Math.min(500, start + d));
      else pdfW = Math.max(240, Math.min(window.innerWidth - 400, start - d));
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

  const errorCount = $derived(app.issues.filter((i) => i.severity === "error").length);
  const warnCount = $derived(app.issues.length - errorCount);
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
  const crumbs = $derived.by(() => {
    if (!settings.breadcrumbs || app.active?.kind !== "text") return [];
    const lines = app.active.content.split("\n").slice(0, app.cursorLine);
    let sec = "";
    let sub = "";
    let subsub = "";
    for (const l of lines) {
      let m;
      if ((m = l.match(/\\section\*?\{([^}]*)/))) [sec, sub, subsub] = [m[1], "", ""];
      else if ((m = l.match(/\\subsection\*?\{([^}]*)/))) [sub, subsub] = [m[1], ""];
      else if ((m = l.match(/\\subsubsection\*?\{([^}]*)/))) subsub = m[1];
    }
    return [...app.active.path.split("/"), sec, sub, subsub].filter(Boolean);
  });
</script>

<svelte:window onkeydown={onKeydown} />
<svelte:head><title>{app.project ? `${app.project.name} — dottex` : "dottex"}</title></svelte:head>

<div class="shell" class:dark>
  <header class="titlebar" data-tauri-drag-region>
    {#if isMac}<span class="mac-pad"></span>{/if}
    {#if app.project}
      <button class="tb" title={t("toggleSidebar")} onclick={() => (sidebarOpen = !sidebarOpen)}>☰</button>
      <button class="tb proj" title={t("switchProject")} onclick={pickAndOpenProject}>
        {app.project.name}
      </button>
      <span class="file" data-tauri-drag-region>
        {app.active?.path ?? ""}{#if app.dirty}&nbsp;<span class="dot" title={t("unsaved")}>●</span>{/if}
      </span>
    {:else}
      <span class="appname" data-tauri-drag-region>dottex</span>
    {/if}
    <span class="spacer" data-tauri-drag-region></span>
    {#if app.project}
      <button
        class="compile"
        onclick={() => requestCompile()}
        disabled={app.compileState === "compiling"}
        title={t("compileTitle")}
      >
        {app.compileState === "compiling" ? t("compiling") : `▶ ${t("compile")}`}
      </button>
      <button class="tb" onclick={exportPdf} disabled={!app.pdf} title={t("exportPdfTitle")}>⬇ PDF</button>
      <button class="tb" title={t("togglePdf")} onclick={() => (pdfOpen = !pdfOpen)}>◫</button>
    {/if}
    <button
      class="tb"
      title="{t('settings')} (Ctrl+,)"
      aria-label={t("settings")}
      onclick={() => (settingsOpen = true)}
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
        <circle cx="12" cy="12" r="3" />
      </svg>
    </button>
    {#if !isMac}
      <div class="winctl">
        <button title={t("minimize")} onclick={() => win.minimize()} aria-label={t("minimize")}>
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" stroke-width="1.2" /></svg>
        </button>
        <button title={t("maximize")} onclick={() => win.toggleMaximize()} aria-label={t("maximize")}>
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1.2" rx="1" /></svg>
        </button>
        <button class="close" title={t("close")} onclick={() => win.close()} aria-label={t("close")}>
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" stroke-width="1.2" /></svg>
        </button>
      </div>
    {/if}
  </header>

  {#if !app.project}
    <Welcome />
  {:else}
    <main
      class="panes"
      style:grid-template-columns="{sidebarOpen ? `${sidebarW}px 4px` : ""} 1fr {pdfOpen
        ? `4px ${pdfW}px`
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
              {#if app.issues.length}<span class="count" class:err={errorCount}>{app.issues.length}</span>{/if}
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
        {#if crumbs.length}
          <div class="editorbar">
            <span class="crumbs" title={crumbs.join(" › ")}>
              {#each crumbs as c, i}{#if i}<span class="sep">›</span>{/if}{c}{/each}
            </span>
          </div>
        {/if}
        {#if !app.active}
          <p class="empty">{t("selectFile")}</p>
        {:else if app.active.kind === "text"}
          <Editor
            path={app.active.path}
            content={app.active.content}
            {dark}
            auto={app.auto}
            goto={app.goto}
            onSave={saveActive}
            onDirty={() => {
              app.dirty = true;
              if (app.auto) app.typing = true;
            }}
            onIdle={autoCompile}
            onCompile={compileActive}
            onCursor={(l) => (app.cursorLine = l)}
            onSyncClick={(l) => syncToPdf(l)}
          />
        {:else if app.active.kind === "image"}
          {#await fileBlobUrl(app.active.path) then url}
            <div class="media"><img src={url} alt={app.active.path} /></div>
          {/await}
        {:else if app.active.kind === "pdf"}
          <PdfViewer source={{ path: app.active.path, version: 0 }} />
        {:else}
          <p class="empty">{t("unsupportedFormat", app.active.path)}</p>
        {/if}
      </section>

      {#if pdfOpen}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div class="resizer" role="separator" aria-orientation="vertical" onmousedown={(e) => dragResize(e, "right")}></div>
        <aside class="pdfpane">
          <PdfViewer source={app.pdf} target={app.pdfTarget} onSyncClick={syncFromPdf} />
        </aside>
      {/if}
    </main>

    {#if app.showLog}
      <div class="log" bind:this={logEl}>
        {#each app.log as line}<div>{line}</div>{/each}
      </div>
    {/if}

    <footer class="status">
      <span class="state {app.compileState}" class:typing={app.typing}>● {statusText}</span>
      {#if app.issues.length}
        <button
          class="issues-btn"
          onclick={() => {
            sidebarOpen = true;
            sidebarTab = "issues";
          }}
        >
          {#if errorCount}<span class="e">✕ {errorCount}</span>{/if}
          {#if warnCount}<span class="w">⚠ {warnCount}</span>{/if}
        </button>
      {/if}
      {#if app.project.root_file}<span class="rootfile">{t("rootPrefix", app.project.root_file)}</span>{/if}
      <span class="spacer"></span>
      <label class="autolbl" title={t("autoPreviewTitle")}>
        <input type="checkbox" bind:checked={app.auto} /> {t("autoPreview")}
      </label>
      <button onclick={clean}>{t("cleanArtifacts")}</button>
      <button onclick={() => (app.showLog = !app.showLog)}>
        {app.showLog ? t("hideLog") : t("showLog")}
      </button>
    </footer>
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
  }
  .tb svg {
    display: block;
  }
  .proj {
    font-weight: 600;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file {
    font-size: 12px;
    color: var(--fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    color: var(--accent);
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
    flex: 1;
    border-radius: 0;
    padding: 6px 4px;
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
  .pdfpane {
    min-width: 0;
    overflow: hidden;
    border-left: 1px solid var(--border);
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
  .autolbl {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--fg-dim);
    cursor: pointer;
  }
  /* interruptor (toggle) a partir del checkbox nativo, como en Configuración */
  .autolbl input {
    appearance: none;
    -webkit-appearance: none;
    position: relative;
    width: 28px;
    height: 16px;
    border-radius: 999px;
    background: var(--border);
    cursor: pointer;
    flex: none;
    margin: 0;
    transition: background 0.15s ease;
  }
  .autolbl input::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
    transition: transform 0.15s ease;
  }
  .autolbl input:checked {
    background: var(--accent);
  }
  .autolbl input:checked::after {
    transform: translateX(12px);
  }
  .autolbl input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
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
