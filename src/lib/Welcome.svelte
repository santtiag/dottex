<script lang="ts">
  import { onMount } from "svelte";
  import { ask, open } from "@tauri-apps/plugin-dialog";
  import type { RecentProject, Template } from "./ipc";
  import {
    createFromTemplate,
    deleteTemplate,
    getRecentProjects,
    listTemplates,
    newTemplate,
  } from "./ipc";
  import { openProject, pickAndOpenProject, settings, toast } from "./state.svelte";
  import { t } from "./i18n.svelte";
  import {
    BookOpen,
    FileText,
    Folder,
    FolderOpen,
    Pencil,
    Plus,
    Presentation,
    Trash2,
    X,
  } from "@lucide/svelte";

  // null = cargando: se pintan filas y tarjetas fantasma en su sitio
  let recents = $state<RecentProject[] | null>(null);
  let templates = $state<Template[] | null>(null);
  // formulario en línea: proyecto nuevo (desde plantilla o en blanco) o plantilla nueva
  let draft = $state<
    { kind: "project"; template: string | null; name: string } | { kind: "template"; name: string } | null
  >(null);
  let busy = $state(false);

  async function loadTemplates() {
    try {
      templates = await listTemplates();
    } catch (e) {
      templates ??= [];
      toast(String(e));
    }
  }

  onMount(async () => {
    loadTemplates();
    recents = await getRecentProjects().catch(() => []);
  });

  const iconFor = (cls: string | null) =>
    cls === "beamer" ? Presentation : cls === "report" || cls === "book" ? BookOpen : FileText;

  const focus = (el: HTMLInputElement) => el.focus();

  async function pickDir(): Promise<boolean> {
    const dir = await open({
      directory: true,
      title: t("locationDialog"),
      defaultPath: settings.projectsDir || undefined,
    });
    if (typeof dir === "string") settings.projectsDir = dir;
    return typeof dir === "string";
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const d = draft;
    const name = d?.name.trim();
    if (!d || !name || busy) return;
    if (d.kind === "project" && !settings.projectsDir && !(await pickDir())) return;
    busy = true;
    try {
      const path =
        d.kind === "project"
          ? await createFromTemplate(d.template, settings.projectsDir, name)
          : await newTemplate(name);
      draft = null;
      await openProject(path);
    } catch (err) {
      toast(String(err));
    } finally {
      busy = false;
    }
  }

  async function remove(tpl: Template) {
    if (!(await ask(t("deleteTemplateAsk", tpl.name), { kind: "warning" }))) return;
    try {
      await deleteTemplate(tpl.name);
    } catch (e) {
      toast(String(e));
    }
    loadTemplates();
  }
</script>

<div class="home">
  <header class="hero">
    <img src="/favicon.png" alt="" width="44" height="44" />
    <div class="brand">
      <h1>dottex</h1>
      <p>{t("appTagline")}</p>
    </div>
    <button class="ghost" onclick={pickAndOpenProject}><FolderOpen />{t("openFolder")}</button>
    <button class="primary" onclick={() => (draft = { kind: "project", template: null, name: "" })}>
      <Plus />{t("newProject")}
    </button>
  </header>

  {#if draft}
    <form class="newform" onsubmit={create}>
      <strong>
        {draft.kind === "template"
          ? t("newTemplate")
          : draft.template
            ? t("newProjectFrom", draft.template)
            : t("newProjectBlank")}
      </strong>
      <!-- {#key}: al elegir otra tarjeta el campo vuelve a tomar el foco -->
      {#key draft.kind === "project" ? draft.template : "\0"}
        <input
          bind:value={draft.name}
          placeholder={t(draft.kind === "template" ? "templateName" : "projectName")}
          use:focus
        />
      {/key}
      {#if draft.kind === "project"}
        <button type="button" class="loc" onclick={pickDir} title={settings.projectsDir}>
          <Folder size={14} /><span>{settings.projectsDir ? `\u200e${settings.projectsDir}` : t("chooseLocation")}</span>
        </button>
      {/if}
      <button type="submit" class="primary" disabled={busy || !draft.name.trim()}>{t("create")}</button>
      <button type="button" class="icon" onclick={() => (draft = null)} title={t("cancel")} aria-label={t("cancel")}>
        <X />
      </button>
    </form>
  {/if}

  <div class="cols">
    <section>
      <h2>{t("recent")}</h2>
      {#if !recents}
        {#each [0, 1, 2] as i (i)}<div class="sk ghost-row"></div>{/each}
      {:else if recents.length}
        <ul class="recents">
          {#each recents as r (r.path)}
            <li>
              <button onclick={() => openProject(r.path)}>
                <strong>{r.name}</strong>
                <span title={r.path}>{r.path}</span>
              </button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="empty">{t("noRecent")}</p>
      {/if}
    </section>

    <section>
      <h2>{t("templates")}</h2>
      {#if !templates}
        <div class="grid">
          {#each [0, 1, 2, 3] as i (i)}<div class="sk ghost-card"></div>{/each}
        </div>
      {:else if templates.length}
        <div class="grid">
          {#each templates as tpl (tpl.name)}
            {@const Icon = iconFor(tpl.class)}
            <div class="card" class:active={draft?.kind === "project" && draft.template === tpl.name}>
              <button
                class="main"
                onclick={() => (draft = { kind: "project", template: tpl.name, name: "" })}
              >
                <Icon size={20} />
                <strong>{tpl.name}</strong>
                <span>{tpl.class ? `${tpl.class} · ` : ""}{t(tpl.files === 1 ? "templateFile" : "templateFiles", tpl.files)}</span>
              </button>
              <div class="acts">
                <button class="icon" onclick={() => openProject(tpl.path)} title={t("editTemplate")} aria-label={t("editTemplate")}>
                  <Pencil size={13} />
                </button>
                <button class="icon" onclick={() => remove(tpl)} title={t("deleteTemplate")} aria-label={t("deleteTemplate")}>
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <p class="empty">{t("noTemplates")}</p>
      {/if}
      <button class="ghost add" onclick={() => (draft = { kind: "template", name: "" })}>
        <Plus />{t("newTemplate")}
      </button>
    </section>
  </div>
</div>

<style>
  .home {
    height: 100%;
    overflow-y: auto;
    padding: 28px 32px;
  }
  .home > * {
    max-width: 920px;
    margin-inline: auto;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .brand {
    flex: 1;
    min-width: 0;
  }
  h1 {
    font-size: 26px;
    margin: 0;
    font-weight: 700;
    letter-spacing: -0.5px;
    line-height: 1.1;
  }
  .brand p {
    color: var(--fg-dim);
    margin: 2px 0 0;
    font-size: 12.5px;
  }
  .primary,
  .ghost {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 8px;
  }
  .primary {
    background: var(--accent);
    color: white;
  }
  .primary:hover {
    background: var(--accent);
    filter: brightness(1.1);
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
    filter: none;
  }
  .ghost {
    border-color: var(--border);
  }

  .newform {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 18px;
    padding: 10px 12px;
    background: var(--panel);
    border: 1px solid var(--accent);
    border-radius: 10px;
  }
  .newform strong {
    flex: none;
    font-size: 13px;
  }
  .newform input {
    flex: 1;
    min-width: 80px;
    padding: 6px 8px;
    border-radius: 6px;
  }
  .loc {
    flex: 0 1 auto;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 8px;
    border-color: var(--border);
    color: var(--fg-dim);
    font-size: 12px;
  }
  .loc span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 180px;
    /* de una ruta larga interesa el final */
    direction: rtl;
  }
  .icon {
    flex: none;
    display: grid;
    padding: 4px;
  }

  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.15fr);
    gap: 28px;
    margin-top: 24px;
  }
  h2 {
    font-size: 11.5px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--fg-dim);
    margin: 0 0 8px;
  }
  .empty {
    color: var(--fg-dim);
    font-size: 12.5px;
    margin: 0 0 10px;
  }

  .recents {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .recents button {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 7px 10px;
    border-radius: 8px;
  }
  .recents span {
    max-width: 100%;
    font-size: 12px;
    color: var(--fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* fantasmas: misma geometría que las filas y tarjetas reales (y que #boot en app.html) */
  .ghost-row {
    height: 47px;
    margin-bottom: 2px;
  }
  .ghost-card {
    height: 86px;
    border-radius: 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 8px;
  }
  .card {
    position: relative;
  }
  .card .main {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    padding: 12px;
    text-align: left;
    background: var(--panel);
    border-color: var(--border);
    border-radius: 10px;
    color: var(--fg);
  }
  .card .main :global(svg) {
    color: var(--accent);
    margin-bottom: 5px;
  }
  .card .main:hover,
  .card.active .main {
    border-color: var(--accent);
    background: var(--panel);
  }
  .card strong {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card .main span {
    font-size: 11.5px;
    color: var(--fg-dim);
  }
  .acts {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    gap: 2px;
    opacity: 0;
  }
  .card:hover .acts,
  .acts:focus-within {
    opacity: 1;
  }
  .add {
    margin-top: 10px;
    color: var(--fg-dim);
    border-style: dashed;
  }

  @media (max-width: 560px) {
    .home {
      padding: 20px 16px;
    }
    .cols {
      grid-template-columns: 1fr;
    }
    .newform {
      flex-wrap: wrap;
    }
  }
</style>
