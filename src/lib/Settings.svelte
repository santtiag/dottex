<script lang="ts">
  import { Hammer, Languages, Palette, SquarePen, X } from "@lucide/svelte";
  import { app, setEngine, settings } from "./state.svelte";
  import { editorThemes } from "./editorThemes";
  import { t, LOCALES, type MsgKey } from "./i18n.svelte";

  const THEMES = Object.entries(editorThemes);

  let { onClose }: { onClose: () => void } = $props();

  let section = $state<"editor" | "appearance" | "language" | "compile">("editor");

  // Solo se ofrecen fuentes realmente instaladas: si una no existe, el CSS
  // cae al fallback y "cambiar de fuente" no cambia nada (bug clásico).
  // ponytail: detección por medición en canvas; queryLocalFonts no existe en WebKitGTK.
  const CANDIDATE_FONTS = [
    "JetBrains Mono",
    "Fira Code",
    "Source Code Pro",
    "Cascadia Code",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Adwaita Mono",
    "Iosevka",
    "Iosevka Nerd Font",
    "Hack",
    "Liberation Mono",
    "Noto Sans Mono",
    "Roboto Mono",
    "IBM Plex Mono",
    "Inconsolata",
    "Menlo",
    "Monaco",
    "SF Mono",
    "Consolas",
    "Courier New",
  ];
  const probeCtx = document.createElement("canvas").getContext("2d")!;
  function fontAvailable(f: string): boolean {
    const probe = "mmmMMMiiilll010./\\";
    return ["monospace", "serif"].some((g) => {
      probeCtx.font = `16px ${g}`;
      const base = probeCtx.measureText(probe).width;
      probeCtx.font = `16px "${f}", ${g}`;
      return probeCtx.measureText(probe).width !== base;
    });
  }
  const FONTS = CANDIDATE_FONTS.filter(fontAvailable);
  // la fuente guardada siempre se ofrece, aunque ya no se detecte
  if (!FONTS.includes(settings.fontFamily)) FONTS.unshift(settings.fontFamily);
  const LANGS: [string, string][] = [
    ["es", "Español"],
    ["en", "English"],
    ["ca", "Català"],
    ["de", "Deutsch"],
    ["fr", "Français"],
    ["it", "Italiano"],
    ["pt", "Português"],
  ];

  const CATS = [
    ["editor", "catEditor", SquarePen],
    ["appearance", "catAppearance", Palette],
    ["compile", "catCompile", Hammer],
    ["language", "catLanguage", Languages],
  ] as const;

  type BoolKey = {
    [K in keyof typeof settings]: (typeof settings)[K] extends boolean ? K : never;
  }[keyof typeof settings];
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

{#snippet toggle(key: BoolKey, label: MsgKey, desc: MsgKey)}
  <label class="row">
    <span class="lbl">{t(label)}<small>{t(desc)}</small></span>
    <input type="checkbox" bind:checked={settings[key]} />
  </label>
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={t("settings")}>
    <nav class="sidebar">
      <h2>{t("settings")}</h2>
      {#each CATS as [id, key, Icon] (id)}
        <button class="cat" class:active={section === id} onclick={() => (section = id)}>
          <Icon />{t(key)}
        </button>
      {/each}
    </nav>

    <div class="panel">
      <header>
        <h3>{t(CATS.find(([id]) => id === section)![1])}</h3>
        <button class="close" onclick={onClose} title={t("close")} aria-label={t("close")}><X /></button>
      </header>

      <div class="content">
        {#if section === "editor"}
          <h4>{t("grpWriting")}</h4>
          <div class="group">
            {@render toggle("autocomplete", "autocomplete", "autocompleteDesc")}
            {@render toggle("closeBrackets", "closeBrackets", "closeBracketsDesc")}
            {@render toggle("vim", "vim", "vimDesc")}
          </div>
          <h4>{t("grpAids")}</h4>
          <div class="group">
            {@render toggle("breadcrumbs", "breadcrumbs", "breadcrumbsDesc")}
            {@render toggle("mathPreview", "mathPreview", "mathPreviewDesc")}
            {@render toggle("inlineErrors", "inlineErrors", "inlineErrorsDesc")}
            {@render toggle("steadyCursor", "steadyCursor", "steadyCursorDesc")}
          </div>
        {:else if section === "appearance"}
          <h4>{t("grpTheme")}</h4>
          <div class="group">
            <label class="row">
              <span class="lbl">{t("generalTheme")}<small>{t("generalThemeDesc")}</small></span>
              <select
                bind:value={app.theme}
                onchange={() => localStorage.setItem("dottex-theme", app.theme)}
              >
                <option value="auto">{t("themeAuto")}</option>
                <option value="light">{t("themeLight")}</option>
                <option value="dark">{t("themeDark")}</option>
              </select>
            </label>
            <label class="row">
              <span class="lbl">{t("editorTheme")}<small>{t("editorThemeDesc")}</small></span>
              <select bind:value={settings.editorTheme}>
                <option value="auto">{t("editorThemeAuto")}</option>
                <!-- "light" sigue la paleta de la app en claro y en oscuro -->
                <option value="light">{t("editorThemeApp")}</option>
                <optgroup label={t("themeLight")}>
                  {#each THEMES.filter(([id, th]) => !th.dark && id !== "light") as [id, th] (id)}
                    <option value={id}>{th.label}</option>
                  {/each}
                </optgroup>
                <optgroup label={t("themeDark")}>
                  {#each THEMES.filter(([, th]) => th.dark) as [id, th] (id)}
                    <option value={id}>{th.label}</option>
                  {/each}
                </optgroup>
              </select>
            </label>
          </div>
          <h4>{t("grpType")}</h4>
          <div class="group">
            <label class="row">
              <span class="lbl">{t("fontFamily")}</span>
              <select bind:value={settings.fontFamily}>
                {#each FONTS as f (f)}<option value={f}>{f}</option>{/each}
              </select>
            </label>
            <label class="row">
              <span class="lbl">{t("fontSize")}</span>
              <input type="number" min="10" max="24" bind:value={settings.fontSize} />
            </label>
            <label class="row">
              <span class="lbl">{t("lineHeight")}</span>
              <input type="number" min="1" max="2.5" step="0.1" bind:value={settings.lineHeight} />
            </label>
          </div>
          <h4>{t("grpPdf")}</h4>
          <div class="group">
            {@render toggle("pdfInvert", "pdfInvert", "pdfInvertDesc")}
          </div>
          <h4>{t("grpEffects")}</h4>
          <div class="group">
            {@render toggle("customCursor", "customCursor", "customCursorDesc")}
            {@render toggle("clickFx", "clickFx", "clickFxDesc")}
          </div>
        {:else if section === "compile"}
          <h4>{t("grpEngine")}</h4>
          <div class="group">
            {#if app.project}
              <label class="row">
                <span class="lbl">{t("engine")}<small>{t("engineDesc")}</small></span>
                <select
                  bind:value={app.project.engine}
                  onchange={() => {
                    if (app.project) setEngine(app.project.engine);
                  }}
                >
                  <option value="tectonic">Tectonic</option>
                  <option value="pdflatex">pdfLaTeX</option>
                  <option value="xelatex">XeLaTeX</option>
                  <option value="lualatex">LuaLaTeX</option>
                </select>
              </label>
            {/if}
            <p class="hint">{t(app.project ? "engineHint" : "engineNoProject")}</p>
          </div>
          <h4>{t("grpPreview")}</h4>
          <div class="group">
            {@render toggle("autoCompile", "autoCompile", "autoCompileDesc")}
          </div>
        {:else}
          <h4>{t("grpInterface")}</h4>
          <div class="group">
            <label class="row">
              <span class="lbl">{t("uiLanguage")}<small>{t("uiLanguageDesc")}</small></span>
              <select bind:value={settings.locale}>
                {#each LOCALES as [code, name] (code)}<option value={code}>{name}</option>{/each}
              </select>
            </label>
          </div>
          <h4>{t("grpSpelling")}</h4>
          <div class="group">
            {@render toggle("spellcheck", "spellcheck", "spellcheckDesc")}
            <label class="row">
              <span class="lbl">{t("spellLang")}</span>
              <select bind:value={settings.spellLang}>
                {#each LANGS as [code, name] (code)}<option value={code}>{name}</option>{/each}
              </select>
            </label>
            <p class="hint">{t("dictHint")}</p>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    z-index: 150;
    display: grid;
    place-items: center;
  }
  .dialog {
    display: grid;
    grid-template-columns: 200px 1fr;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.3);
    width: min(760px, 94vw);
    height: min(560px, 86vh);
    overflow: hidden;
  }
  /* barra de categorías */
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 10px;
    background: var(--bg);
    border-right: 1px solid var(--border);
  }
  .sidebar h2 {
    margin: 2px 8px 12px;
    font-size: 15px;
    font-weight: 700;
  }
  .cat {
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    padding: 7px 10px;
    font-size: 13px;
    color: var(--fg-dim);
    border-radius: 7px;
  }
  .cat:hover {
    background: var(--hover);
  }
  .cat.active {
    background: var(--accent-dim);
    color: var(--accent);
    font-weight: 600;
  }
  /* panel de opciones */
  .panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 18px 10px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .close {
    color: var(--fg-dim);
    padding: 4px;
    display: grid;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 18px 18px;
  }
  /* grupo de opciones con su encabezado */
  h4 {
    margin: 16px 2px 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--fg-dim);
  }
  .group {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 9px;
    padding: 0 12px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 9px 0;
    cursor: var(--cur-pointer, pointer);
    border-bottom: 1px solid var(--border);
  }
  .row:last-child {
    border-bottom: none;
  }
  .lbl {
    font-size: 13px;
  }
  .lbl small {
    display: block;
    color: var(--fg-dim);
    font-size: 11.5px;
    margin-top: 2px;
  }
  /* interruptor (toggle) a partir del checkbox nativo */
  input[type="checkbox"] {
    appearance: none;
    -webkit-appearance: none;
    position: relative;
    width: 36px;
    height: 20px;
    border-radius: 999px;
    background: var(--border);
    cursor: var(--cur-pointer, pointer);
    flex: none;
    transition: background 0.15s ease;
  }
  input[type="checkbox"]::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
    transition: transform 0.15s ease;
  }
  input[type="checkbox"]:checked {
    background: var(--accent);
  }
  input[type="checkbox"]:checked::after {
    transform: translateX(16px);
  }
  input[type="checkbox"]:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  input[type="number"] {
    width: 70px;
  }
  select {
    font: inherit;
    font-size: 12.5px;
    color: inherit;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 6px;
    max-width: 200px;
  }
  .hint {
    font-size: 11.5px;
    color: var(--fg-dim);
    margin: 0;
    padding: 9px 0;
    line-height: 1.5;
  }
</style>
