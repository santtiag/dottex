<script lang="ts">
  import { onMount } from "svelte";
  import type { RecentProject } from "./ipc";
  import { getRecentProjects } from "./ipc";
  import { openProject, pickAndOpenProject } from "./state.svelte";
  import { t } from "./i18n.svelte";

  let recents = $state<RecentProject[]>([]);

  onMount(async () => {
    recents = await getRecentProjects();
  });
</script>

<div class="welcome">
  <h1>dottex</h1>
  <p>{t("appTagline")}</p>
  <button class="primary" onclick={pickAndOpenProject}>{t("openFolder")}</button>

  {#if recents.length}
    <h2>{t("recent")}</h2>
    <ul>
      {#each recents as r (r.path)}
        <li>
          <button onclick={() => openProject(r.path)}>
            <strong>{r.name}</strong>
            <span>{r.path}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .welcome {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 24px;
  }
  h1 {
    font-size: 42px;
    margin: 0;
    font-weight: 700;
    letter-spacing: -1px;
  }
  p {
    color: var(--fg-dim);
    margin: 0 0 8px;
  }
  .primary {
    font-size: 15px;
    padding: 10px 24px;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 8px;
    cursor: pointer;
  }
  h2 {
    font-size: 13px;
    text-transform: uppercase;
    color: var(--fg-dim);
    margin: 24px 0 0;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: min(480px, 90%);
  }
  li button {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 8px 12px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
  }
  li button:hover {
    background: var(--hover);
  }
  li span {
    font-size: 12px;
    color: var(--fg-dim);
  }
</style>
