<script lang="ts">
  import type { SearchHit } from "./ipc";
  import { replaceInProject, searchProject } from "./ipc";
  import { app, gotoLine, openFile, refreshActiveFromDisk, reloadTree, toast } from "./state.svelte";
  import { t } from "./i18n.svelte";

  let query = $state("");
  let replacement = $state("");
  let isRegex = $state(false);
  let caseSensitive = $state(false);
  let hits = $state<SearchHit[]>([]);
  let searched = $state(false);
  let confirmReplace = $state(false);
  let timer: ReturnType<typeof setTimeout>;

  function schedule() {
    clearTimeout(timer);
    confirmReplace = false;
    timer = setTimeout(run, 300);
  }

  async function run() {
    if (!query) {
      hits = [];
      searched = false;
      return;
    }
    try {
      hits = await searchProject(query, isRegex, caseSensitive);
      searched = true;
    } catch (e) {
      toast(String(e));
    }
  }

  const grouped = $derived.by(() => {
    const map = new Map<string, SearchHit[]>();
    for (const h of hits) {
      (map.get(h.file) ?? map.set(h.file, []).get(h.file)!).push(h);
    }
    return [...map.entries()];
  });

  async function go(hit: SearchHit) {
    if (app.active?.path !== hit.file) await openFile(hit.file);
    gotoLine(hit.line);
  }

  async function replaceAll() {
    if (!confirmReplace) {
      confirmReplace = true; // segundo clic confirma
      return;
    }
    confirmReplace = false;
    if (app.dirty) {
      toast(t("saveBeforeReplace"));
      return;
    }
    try {
      const res = await replaceInProject(query, isRegex, caseSensitive, replacement);
      toast(t("replaceResult", res.matches, res.files));
      await Promise.all([run(), reloadTree(), refreshActiveFromDisk()]);
    } catch (e) {
      toast(String(e));
    }
  }
</script>

<div class="search">
  <div class="controls">
    <input
      type="search"
      placeholder={t("searchPlaceholder")}
      bind:value={query}
      oninput={schedule}
      onkeydown={(e) => e.key === "Enter" && run()}
    />
    <input type="search" placeholder={t("replacePlaceholder")} bind:value={replacement} />
    <div class="opts">
      <label><input type="checkbox" bind:checked={caseSensitive} onchange={run} /> Aa</label>
      <label title={t("regexTitle")}><input type="checkbox" bind:checked={isRegex} onchange={run} /> .*</label>
      <span class="spacer"></span>
      <button class="replace" disabled={!hits.length || !searched} onclick={replaceAll}>
        {confirmReplace ? t("replaceConfirm", hits.length) : t("replaceAll")}
      </button>
    </div>
  </div>

  <div class="results">
    {#if searched && !hits.length}
      <p class="empty">{t("noResults")}</p>
    {/if}
    {#each grouped as [file, items] (file)}
      <div class="file">{file} <span class="n">({items.length})</span></div>
      {#each items as hit (hit.line + ":" + hit.col)}
        <button class="hit" onclick={() => go(hit)} title="{file}:{hit.line}">
          <span class="ln">{hit.line}</span>
          <span class="prev">{hit.preview}</span>
        </button>
      {/each}
    {/each}
    {#if hits.length >= 500}
      <p class="empty">{t("first500")}</p>
    {/if}
  </div>
</div>

<style>
  .search {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .controls {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .controls input[type="search"] {
    width: 100%;
    border-color: var(--border);
  }
  .controls input[type="search"]:focus {
    border-color: var(--accent);
  }
  .opts {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .opts label {
    display: flex;
    align-items: center;
    gap: 3px;
    color: var(--fg-dim);
    cursor: pointer;
    font-family: ui-monospace, monospace;
  }
  .spacer {
    flex: 1;
  }
  .replace {
    font-size: 11.5px;
    border: 1px solid var(--border);
  }
  .replace:not(:disabled):hover {
    border-color: #d33;
    color: #d33;
  }
  .results {
    overflow-y: auto;
    flex: 1;
    font-size: 12px;
  }
  .file {
    padding: 6px 10px 2px;
    font-weight: 600;
    font-size: 11.5px;
    color: var(--accent);
    font-family: ui-monospace, monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .n {
    color: var(--fg-dim);
    font-weight: 400;
  }
  .hit {
    display: flex;
    gap: 8px;
    width: 100%;
    text-align: left;
    border: none;
    border-radius: 0;
    padding: 2px 10px;
  }
  .ln {
    color: var(--fg-dim);
    font-family: ui-monospace, monospace;
    min-width: 3ch;
    text-align: right;
    flex: none;
  }
  .prev {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .empty {
    color: var(--fg-dim);
    text-align: center;
    font-size: 12px;
    margin-top: 16px;
  }
</style>
