<script lang="ts">
  import { app, gotoIssue } from "./state.svelte";
  import { t } from "./i18n.svelte";
</script>

<div class="issues">
  {#if !app.issues.length}
    <p class="empty">
      {app.compileState === "ok" ? t("noIssues") : t("compileForIssues")}
    </p>
  {:else}
    {#each app.issues as issue, i (i)}
      <button class="issue" onclick={() => gotoIssue(issue)} title={issue.message}>
        <span class="sev {issue.severity}">{issue.severity === "error" ? "✕" : "⚠"}</span>
        <span class="body">
          <span class="msg">{issue.message}</span>
          {#if issue.file}
            <span class="loc">{issue.file}{issue.line ? `:${issue.line}` : ""}</span>
          {/if}
        </span>
      </button>
    {/each}
  {/if}
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
  .issue {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    text-align: left;
    padding: 6px 10px;
    border: none;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    font-size: 12px;
  }
  .sev {
    flex: none;
    font-weight: 700;
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
    gap: 2px;
    min-width: 0;
  }
  .msg {
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .loc {
    color: var(--accent);
    font-family: ui-monospace, monospace;
    font-size: 11px;
  }
</style>
