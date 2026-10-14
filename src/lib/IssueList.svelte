<script lang="ts">
  import type { Issue } from "./ipc";
  import { app, gotoIssue, problems } from "./state.svelte";
  import { hintFor } from "./issueHints";
  import { t } from "./i18n.svelte";
  import { CircleX, TriangleAlert } from "@lucide/svelte";

  // por archivo; dentro, errores antes que avisos y luego por línea
  const groups = $derived.by(() => {
    const byFile = new Map<string, Issue[]>();
    for (const i of problems()) {
      const list = byFile.get(i.file ?? "");
      list ? list.push(i) : byFile.set(i.file ?? "", [i]);
    }
    for (const list of byFile.values()) {
      list.sort(
        (a, b) =>
          +(b.severity === "error") - +(a.severity === "error") || (a.line ?? 0) - (b.line ?? 0),
      );
    }
    return [...byFile].sort(([a], [b]) => a.localeCompare(b));
  });
</script>

<div class="issues">
  {#each groups as [file, list] (file)}
    <div class="file">{file || t("srcCompile")}<span>{list.length}</span></div>
    {#each list as issue, i (i)}
      {@const hint = hintFor(issue.message)}
      <button class="issue" onclick={() => gotoIssue(issue)}>
        <span class="sev {issue.severity}">
          {#if issue.severity === "error"}<CircleX size={14} />{:else}<TriangleAlert size={14} />{/if}
        </span>
        <span class="body">
          <span class="msg">{issue.message}</span>
          {#if issue.context}<code>{issue.context}</code>{/if}
          {#if hint}<span class="hint">{t(hint)}</span>{/if}
          <span class="meta">
            {#if issue.line}<span class="loc">{t("issueLine", issue.line)}</span>{/if}
            <span class="src">{t(issue.source === "syntax" ? "srcSyntax" : "srcCompile")}</span>
          </span>
        </span>
      </button>
    {/each}
  {:else}
    <p class="empty">
      {app.compileState === "ok" ? t("noIssues") : t("compileForIssues")}
    </p>
  {/each}
</div>

<style>
  .issues {
    overflow-y: auto;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .empty {
    color: var(--fg-dim);
    font-size: 12px;
    text-align: center;
    margin-top: 24px;
    padding: 0 12px;
  }
  .file {
    position: sticky;
    top: 0;
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 5px 10px;
    font-size: 11.5px;
    font-family: ui-monospace, monospace;
    color: var(--fg-dim);
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    overflow-wrap: anywhere;
  }
  .issue {
    flex: none;
    display: flex;
    gap: 8px;
    align-items: flex-start;
    text-align: left;
    padding: 7px 10px;
    border: none;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    font-size: 12px;
  }
  .sev {
    flex: none;
    display: grid;
    margin-top: 1px;
  }
  .sev.error {
    color: #d33;
  }
  .sev.warning {
    color: #c90;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .msg {
    overflow-wrap: anywhere;
  }
  code {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    padding: 2px 5px;
    border-radius: 4px;
    background: var(--hover);
    overflow-wrap: anywhere;
  }
  .hint {
    color: var(--fg-dim);
  }
  .meta {
    display: flex;
    gap: 8px;
    font-size: 11px;
  }
  .loc {
    color: var(--accent);
    font-family: ui-monospace, monospace;
  }
  .src {
    color: var(--fg-dim);
  }
</style>
