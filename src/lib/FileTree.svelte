<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import type { TrashEntry, TreeNode } from "./ipc";
  import * as ipc from "./ipc";
  import { app, openFile, reloadTree, setRootFile, toast } from "./state.svelte";
  import { t } from "./i18n.svelte";

  const expanded = new SvelteSet<string>();

  let trashOpen = $state(false);
  let trash = $state<TrashEntry[]>([]);

  async function loadTrash() {
    try {
      trash = await ipc.listTrash();
    } catch {
      trash = [];
    }
  }

  async function trashOp(op: Promise<unknown>) {
    try {
      await op;
      await Promise.all([loadTrash(), reloadTree()]);
    } catch (e) {
      toast(String(e));
    }
  }

  let menu = $state<{ x: number; y: number; node: TreeNode } | null>(null);
  // Edición en línea: renombrar un nodo o crear archivo/carpeta dentro de `dir`.
  let editing = $state<{ mode: "rename" | "newfile" | "newdir"; target: string; initial: string } | null>(null);

  function parentDir(path: string): string {
    const i = path.lastIndexOf("/");
    return i === -1 ? "" : path.slice(0, i);
  }

  // iconos SVG por tipo de archivo (documento con esquina doblada + variantes)
  const svgDoc = (fill: string, extra = "") =>
    `<svg width="15" height="15" viewBox="0 0 16 16"><path fill="${fill}" d="M4.5 1.5h4.9c.27 0 .52.1.71.3l2.6 2.6c.19.19.29.44.29.7V13c0 .83-.67 1.5-1.5 1.5h-7A1.5 1.5 0 0 1 3 13V3c0-.83.67-1.5 1.5-1.5z"/><path fill="#fff" fill-opacity=".4" d="M9.6 1.7V4c0 .55.45 1 1 1h2.2z"/>${extra}</svg>`;
  const textLines =
    '<path stroke="#fff" stroke-opacity=".9" stroke-width="1.1" stroke-linecap="round" d="M5.3 8h5.4M5.3 10h5.4M5.3 12h3.4"/>';
  const ICONS = {
    dir: '<svg width="15" height="15" viewBox="0 0 16 16"><path fill="#dca738" d="M1.5 12.4V3.6c0-.6.5-1.1 1.1-1.1h3.1c.32 0 .62.14.83.38l1.16 1.32h5.7c.6 0 1.1.5 1.1 1.1v7.1c0 .6-.5 1.1-1.1 1.1H2.6c-.6 0-1.1-.5-1.1-1.1z"/><path fill="#fff" fill-opacity=".25" d="M1.5 5.1h13v1.1h-13z"/></svg>',
    dirOpen:
      '<svg width="15" height="15" viewBox="0 0 16 16"><path fill="#b98a2e" d="M1.5 12V3.6c0-.6.5-1.1 1.1-1.1h3.1c.32 0 .62.14.83.38L7.7 4.2h5.7c.6 0 1.1.5 1.1 1.1v1.2H4.3c-.7 0-1.3.45-1.5 1.1L1.5 12z"/><path fill="#dca738" d="M4.2 7.3h10.2c.68 0 1.17.66.97 1.31l-1.06 3.5c-.19.63-.77 1.06-1.43 1.06H2.4c-.7 0-1.19-.68-.96-1.34l1.34-3.5c.21-.62.79-1.03 1.42-1.03z"/></svg>',
    tex: svgDoc("#4d7fe3", textLines),
    bib: '<svg width="15" height="15" viewBox="0 0 16 16"><path fill="#9a6fe0" d="M3 3.2c0-.94.76-1.7 1.7-1.7H13v10.3H4.7c-.63 0-1.22.2-1.7.56z"/><path fill="#7c50c9" d="M3 13.3c0-.94.76-1.7 1.7-1.7H13v2.9H4.7A1.7 1.7 0 0 1 3 13.3z"/><path stroke="#fff" stroke-opacity=".8" stroke-width="1" stroke-linecap="round" d="M6 4.6h4.5M6 6.8h4.5"/></svg>',
    pdf: svgDoc(
      "#e05252",
      '<text x="8" y="12.2" text-anchor="middle" font-size="4.6" font-weight="700" fill="#fff" font-family="sans-serif">PDF</text>',
    ),
    img: '<svg width="15" height="15" viewBox="0 0 16 16"><rect x="1.5" y="2.5" width="13" height="11" rx="1.5" fill="#4ca96b"/><circle cx="5.6" cy="6.2" r="1.3" fill="#fff" fill-opacity=".9"/><path fill="#fff" fill-opacity=".85" d="M3.5 11.5l3-3.4 2.2 2.3 1.9-2 2 3.1z"/></svg>',
    file: svgDoc("#98a0aa"),
  };

  function icon(node: TreeNode): string {
    if (node.is_dir) return expanded.has(node.path) ? ICONS.dirOpen : ICONS.dir;
    const ext = node.name.split(".").pop()?.toLowerCase() ?? "";
    if (["tex", "sty", "cls"].includes(ext)) return ICONS.tex;
    if (ext === "bib") return ICONS.bib;
    if (ext === "pdf") return ICONS.pdf;
    if (["png", "jpg", "jpeg", "gif", "webp", "svg"].includes(ext)) return ICONS.img;
    return ICONS.file;
  }

  function onNodeClick(node: TreeNode) {
    if (node.is_dir) {
      expanded.has(node.path) ? expanded.delete(node.path) : expanded.add(node.path);
    } else {
      openFile(node.path);
    }
  }

  function onContext(e: MouseEvent, node: TreeNode) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, node };
  }

  function startEdit(mode: "rename" | "newfile" | "newdir", node?: TreeNode) {
    menu = null;
    if (mode === "rename" && node) {
      editing = { mode, target: node.path, initial: node.name };
    } else {
      // crear dentro del dir del menú, o en la raíz desde los botones de arriba
      const dir = node ? (node.is_dir ? node.path : parentDir(node.path)) : "";
      if (dir) expanded.add(dir);
      editing = { mode, target: dir, initial: "" };
    }
  }

  async function confirmEdit(value: string) {
    const ed = editing;
    editing = null;
    if (!ed || !value.trim()) return;
    const name = value.trim();
    try {
      if (ed.mode === "rename") {
        const dir = parentDir(ed.target);
        await ipc.renamePath(ed.target, dir ? `${dir}/${name}` : name);
      } else {
        const path = ed.target ? `${ed.target}/${name}` : name;
        if (ed.mode === "newfile") {
          await ipc.createFile(path);
          await openFile(path);
        } else {
          await ipc.createDir(path);
        }
      }
      await reloadTree();
    } catch (e) {
      toast(String(e));
    }
  }

  async function remove(node: TreeNode) {
    menu = null;
    try {
      await ipc.deletePath(node.path);
      if (app.active?.path.startsWith(node.path)) app.active = null;
      await reloadTree();
      if (trashOpen) await loadTrash();
    } catch (e) {
      toast(String(e));
    }
  }

  function focusAndSelect(el: HTMLInputElement) {
    el.focus();
    const dot = el.value.lastIndexOf(".");
    el.setSelectionRange(0, dot > 0 ? dot : el.value.length);
  }
</script>

<svelte:window onclick={() => (menu = null)} />

<div class="wrap">
  <div class="tree-actions">
    <button title={t("newFile")} onclick={() => startEdit("newfile")}>{t("newFileShort")}</button>
    <button title={t("newFolder")} onclick={() => startEdit("newdir")}>{t("newFolderShort")}</button>
  </div>
  <div class="tree">

  {#snippet editRow(depth: number)}
    <div class="row edit" style:padding-left="{depth * 14 + 8}px">
      <input
        value={editing?.initial ?? ""}
        use:focusAndSelect
        onkeydown={(e) => {
          if (e.key === "Enter") confirmEdit(e.currentTarget.value);
          if (e.key === "Escape") editing = null;
        }}
        onblur={() => (editing = null)}
      />
    </div>
  {/snippet}

  {#snippet nodes(list: TreeNode[], depth: number)}
    {#each list as node (node.path)}
      {#if editing?.mode === "rename" && editing.target === node.path}
        {@render editRow(depth)}
      {:else}
        <div
          class="row"
          class:active={app.active?.path === node.path}
          class:root={app.project?.root_file === node.path}
          style:padding-left="{depth * 14 + 8}px"
          role="button"
          tabindex="0"
          onclick={() => onNodeClick(node)}
          onkeydown={(e) => e.key === "Enter" && onNodeClick(node)}
          oncontextmenu={(e) => onContext(e, node)}
        >
          <span class="icon">{@html icon(node)}</span>
          <span class="name">{node.name}</span>
          {#if app.project?.root_file === node.path}<span class="badge">{t("rootBadge")}</span>{/if}
        </div>
      {/if}
      {#if node.is_dir && expanded.has(node.path)}
        {#if editing && editing.mode !== "rename" && editing.target === node.path}
          {@render editRow(depth + 1)}
        {/if}
        {@render nodes(node.children, depth + 1)}
      {/if}
    {/each}
  {/snippet}

    {#if editing && editing.mode !== "rename" && editing.target === ""}
      {@render editRow(0)}
    {/if}
    {@render nodes(app.tree, 0)}
  </div>

  <div class="trash-sec">
    <button
      class="trash-head"
      onclick={() => {
        trashOpen = !trashOpen;
        if (trashOpen) loadTrash();
      }}
    >
      {trashOpen ? "▾" : "▸"} 🗑 {t("trash")}
    </button>
    {#if trashOpen}
      {#if !trash.length}
        <p class="trash-empty">{t("trashEmpty")}</p>
      {:else}
        <div class="trash-list">
          {#each trash as entry (entry.id)}
            <div class="trash-item" title={entry.original ?? entry.name}>
              <span class="tname">{entry.name}</span>
              <button title={t("restore")} onclick={() => trashOp(ipc.restoreTrash(entry.id))}>↩</button>
              <button
                class="tdanger"
                title={t("deleteForever")}
                onclick={() => trashOp(ipc.deleteTrashItem(entry.id))}
              >
                ✕
              </button>
            </div>
          {/each}
        </div>
        <button class="trash-clear" onclick={() => trashOp(ipc.emptyTrash())}>
          {t("emptyTrash")}
        </button>
      {/if}
    {/if}
  </div>
</div>

{#if menu}
  <div class="menu" style:left="{menu.x}px" style:top="{menu.y}px">
    {#if menu.node.is_dir}
      <button onclick={() => startEdit("newfile", menu!.node)}>{t("newFile")}</button>
      <button onclick={() => startEdit("newdir", menu!.node)}>{t("newFolder")}</button>
      <hr />
    {/if}
    {#if menu.node.path.endsWith(".tex") && app.project?.root_file !== menu.node.path}
      <button
        onclick={() => {
          setRootFile(menu!.node.path);
          menu = null;
        }}
      >
        {t("setRoot")}
      </button>
      <hr />
    {/if}
    <button onclick={() => startEdit("rename", menu!.node)}>{t("rename")}</button>
    <button class="danger" onclick={() => remove(menu!.node)}>{t("deleteToTrash")}</button>
  </div>
{/if}

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-size: 13px;
    user-select: none;
  }
  .tree {
    overflow-y: auto;
    flex: 1;
  }
  .tree-actions {
    display: flex;
    gap: 4px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .tree-actions button {
    font-size: 11px;
    padding: 2px 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.active {
    background: var(--accent-dim);
  }
  .icon {
    display: flex;
    align-items: center;
    flex: none;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .badge {
    font-size: 10px;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 6px;
    padding: 0 5px;
    flex: none;
  }
  .row.edit input {
    width: 100%;
    font-size: 13px;
  }
  .menu {
    position: fixed;
    z-index: 100;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.2);
    display: flex;
    flex-direction: column;
    min-width: 180px;
    padding: 4px;
  }
  .menu button {
    text-align: left;
    padding: 6px 10px;
    border: none;
    background: none;
    border-radius: 4px;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .menu .danger {
    color: #d33;
  }
  .menu hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
  /* papelera */
  .trash-sec {
    flex: none;
    border-top: 1px solid var(--border);
    max-height: 40%;
    display: flex;
    flex-direction: column;
  }
  .trash-head {
    text-align: left;
    padding: 6px 10px;
    font-size: 12px;
    color: var(--fg-dim);
    border-radius: 0;
  }
  .trash-empty {
    color: var(--fg-dim);
    font-size: 11.5px;
    margin: 2px 0 8px;
    text-align: center;
  }
  .trash-list {
    overflow-y: auto;
  }
  .trash-item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px 2px 24px;
    font-size: 12px;
  }
  .tname {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .trash-item button {
    padding: 1px 6px;
    font-size: 11px;
  }
  .tdanger:hover {
    color: #d33;
  }
  .trash-clear {
    margin: 4px 8px 8px;
    font-size: 11px;
    border: 1px solid var(--border);
  }
  .trash-clear:hover {
    border-color: #d33;
    color: #d33;
  }
</style>
