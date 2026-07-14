<script lang="ts">
  import { app, settings } from "./state.svelte";
  import { t, LOCALES, type MsgKey } from "./i18n.svelte";

  let { onClose }: { onClose: () => void } = $props();

  let section = $state<"editor" | "appearance" | "language">("editor");

  const FONTS = [
    "JetBrains Mono",
    "Fira Code",
    "Source Code Pro",
    "Cascadia Code",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
  ];
  const LANGS: [string, string][] = [
    ["es", "Español"],
    ["en", "English"],
    ["ca", "Català"],
    ["de", "Deutsch"],
    ["fr", "Français"],
    ["it", "Italiano"],
    ["pt", "Português"],
  ];

  const CATS: [typeof section, MsgKey][] = [
    ["editor", "catEditor"],
    ["appearance", "catAppearance"],
    ["language", "catLanguage"],
  ];
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={t("settings")}>
    <nav class="sidebar">
      <h2>{t("settings")}</h2>
      {#each CATS as [id, key] (id)}
        <button class="cat" class:active={section === id} onclick={() => (section = id)}>
          {t(key)}
        </button>
      {/each}
    </nav>

    <div class="panel">
      <header>
        <h3>{t(CATS.find(([id]) => id === section)![1])}</h3>
        <button class="close" onclick={onClose} title={t("close")} aria-label={t("close")}>✕</button>
      </header>

      <div class="content">
        {#if section === "editor"}
          <label class="row">
            <span class="lbl">{t("autocomplete")}<small>{t("autocompleteDesc")}</small></span>
            <input type="checkbox" bind:checked={settings.autocomplete} />
          </label>
          <label class="row">
            <span class="lbl">{t("closeBrackets")}<small>{t("closeBracketsDesc")}</small></span>
            <input type="checkbox" bind:checked={settings.closeBrackets} />
          </label>
          <label class="row">
            <span class="lbl">{t("steadyCursor")}<small>{t("steadyCursorDesc")}</small></span>
            <input type="checkbox" bind:checked={settings.steadyCursor} />
          </label>
          <label class="row">
            <span class="lbl">{t("vim")}<small>{t("vimDesc")}</small></span>
            <input type="checkbox" bind:checked={settings.vim} />
          </label>
          <label class="row">
            <span class="lbl">{t("breadcrumbs")}<small>{t("breadcrumbsDesc")}</small></span>
            <input type="checkbox" bind:checked={settings.breadcrumbs} />
          </label>
          <label class="row">
            <span class="lbl">{t("mathPreview")}<small>{t("mathPreviewDesc")}</small></span>
            <input type="checkbox" bind:checked={settings.mathPreview} />
          </label>
          <label class="row">
            <span class="lbl">{t("editorTheme")}<small>{t("editorThemeDesc")}</small></span>
            <select bind:value={settings.editorTheme}>
              <option value="auto">{t("editorThemeAuto")}</option>
              <option value="light">{t("themeLight")}</option>
              <option value="dark">{t("editorThemeOneDark")}</option>
            </select>
          </label>
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
        {:else if section === "appearance"}
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
            <span class="lbl">{t("pdfInvert")}<small>{t("pdfInvertDesc")}</small></span>
            <input type="checkbox" bind:checked={settings.pdfInvert} />
          </label>
        {:else}
          <label class="row">
            <span class="lbl">{t("uiLanguage")}<small>{t("uiLanguageDesc")}</small></span>
            <select bind:value={settings.locale}>
              {#each LOCALES as [code, name] (code)}<option value={code}>{name}</option>{/each}
            </select>
          </label>
          <label class="row">
            <span class="lbl">{t("spellcheck")}<small>{t("spellcheckDesc")}</small></span>
            <input type="checkbox" bind:checked={settings.spellcheck} />
          </label>
          <label class="row">
            <span class="lbl">{t("spellLang")}</span>
            <select bind:value={settings.spellLang}>
              {#each LANGS as [code, name] (code)}<option value={code}>{name}</option>{/each}
            </select>
          </label>
          <p class="hint">{t("dictHint")}</p>
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
    grid-template-columns: 176px 1fr;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.3);
    width: min(700px, 94vw);
    height: min(520px, 84vh);
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
    font-size: 14px;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 18px 18px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 9px 0;
    cursor: pointer;
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
    cursor: pointer;
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
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 6px;
    max-width: 200px;
  }
  .hint {
    font-size: 11.5px;
    color: var(--fg-dim);
    margin: 10px 0 0;
    line-height: 1.5;
  }
</style>
