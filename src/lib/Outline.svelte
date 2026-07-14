<script lang="ts">
  import { app, gotoLine, syncToPdf } from "./state.svelte";
  import { t } from "./i18n.svelte";

  interface Entry {
    line: number; // 1-indexado
    level: number; // sangría
    kind: "section" | "float" | "label" | "todo";
    text: string;
  }

  const SECTIONS: Record<string, number> = {
    part: 0,
    chapter: 0,
    section: 1,
    subsection: 2,
    subsubsection: 3,
    paragraph: 4,
  };

  const sec_re = /\\(part|chapter|section|subsection|subsubsection|paragraph)\*?\s*\{([^}]*)\}/;
  const env_re = /\\begin\{(figure|table)\*?\}/;
  const caption_re = /\\caption\s*\{([^}]*)\}/;
  const label_re = /\\label\s*\{([^}]*)\}/;
  const todo_re = /(?:%\s*TODO[:\s]?(.*)|\\todo\s*\{([^}]*)\})/i;

  // ponytail: outline del archivo activo, no del documento completo;
  // recorrer los \input cuando haga falta un outline global.
  function parse(text: string): Entry[] {
    const entries: Entry[] = [];
    let level = 1;
    let openFloat: Entry | null = null;
    text.split("\n").forEach((raw, i) => {
      const line = i + 1;
      // ignora lo comentado (menos los TODO, que se buscan aparte)
      const code = raw.replace(/(^|[^\\])%.*$/, "$1");
      let m = code.match(sec_re);
      if (m) {
        level = SECTIONS[m[1]];
        entries.push({ line, level, kind: "section", text: m[2] || t("untitled") });
        return;
      }
      m = code.match(env_re);
      if (m) {
        openFloat = { line, level: level + 1, kind: "float", text: m[1] === "figure" ? t("figure") : t("table") };
        entries.push(openFloat);
      }
      m = code.match(caption_re);
      if (m && openFloat) {
        openFloat.text += `: ${m[1]}`;
        openFloat = null;
      }
      m = code.match(label_re);
      if (m) entries.push({ line, level: level + 1, kind: "label", text: m[1] });
      m = raw.match(todo_re);
      if (m) entries.push({ line, level: level + 1, kind: "todo", text: (m[1] ?? m[2] ?? "").trim() || "TODO" });
    });
    return entries;
  }

  const entries = $derived(
    app.active?.kind === "text" && app.active.path.endsWith(".tex")
      ? parse(app.active.content)
      : [],
  );

  // sección que contiene el cursor
  const currentLine = $derived.by(() => {
    let cur = -1;
    for (const e of entries) {
      if (e.kind === "section" && e.line <= app.cursorLine) cur = e.line;
    }
    return cur;
  });

  const ICONS = { section: "§", float: "❑", label: "🏷", todo: "☐" };

  function go(e: Entry) {
    gotoLine(e.line);
    syncToPdf(e.line); // las 3 zonas quedan alineadas
  }
</script>

<div class="outline">
  {#if !entries.length}
    <p class="empty">
      {app.active?.path.endsWith(".tex") ? t("noOutline") : t("openTexForOutline")}
    </p>
  {:else}
    {#each entries as e (e.line + e.kind + e.text)}
      <button
        class="entry {e.kind}"
        class:current={e.kind === "section" && e.line === currentLine}
        style:padding-left="{e.level * 14 + 10}px"
        onclick={() => go(e)}
        title={t("lineTitle", e.line)}
      >
        <span class="icon">{ICONS[e.kind]}</span>
        <span class="text">{e.text}</span>
      </button>
    {/each}
  {/if}
</div>

<style>
  .outline {
    overflow-y: auto;
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 4px 0;
  }
  .empty {
    color: var(--fg-dim);
    font-size: 12px;
    text-align: center;
    margin-top: 24px;
    padding: 0 12px;
  }
  .entry {
    display: flex;
    align-items: baseline;
    gap: 6px;
    text-align: left;
    border: none;
    border-radius: 0;
    padding-top: 3px;
    padding-bottom: 3px;
    font-size: 12.5px;
    white-space: nowrap;
  }
  .entry.current {
    background: var(--accent-dim);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .entry.section {
    font-weight: 600;
  }
  .entry.label {
    color: var(--fg-dim);
    font-family: ui-monospace, monospace;
    font-size: 11.5px;
  }
  .entry.todo {
    color: #c90;
  }
  .icon {
    flex: none;
    color: var(--fg-dim);
    font-size: 11px;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
