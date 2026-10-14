<script lang="ts">
  import type { Component } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import {
    ArchiveRestore,
    BookOpen,
    ChevronDown,
    ChevronRight,
    File,
    FileCode,
    FilePlus,
    FileText,
    Folder,
    FolderOpen,
    FolderPlus,
    Image,
    Trash,
    X,
  } from "@lucide/svelte";
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

  // icono + tinte por tipo de archivo (mismo trazo que el resto de la UI)
  function icon(node: TreeNode): [Component, string] {
    if (node.is_dir) return [expanded.has(node.path) ? FolderOpen : Folder, "dir"];
    const ext = node.name.split(".").pop()?.toLowerCase() ?? "";
    if (["tex", "sty", "cls"].includes(ext)) return [FileCode, "tex"];
    if (ext === "bib") return [BookOpen, "bib"];
    if (ext === "pdf") return [FileText, "pdf"];
    if (["png", "jpg", "jpeg", "gif", "webp", "svg"].includes(ext)) return [Image, "img"];
    return [File, "file"];
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
    <button title={t("newFile")} onclick={() => startEdit("newfile")}><FilePlus size={14} />{t("newFileShort")}</button>
    <button title={t("newFolder")} onclick={() => startEdit("newdir")}><FolderPlus size={14} />{t("newFolderShort")}</button>
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
      {@const [Icon, kind] = icon(node)}
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
          <span class="icon {kind}"><Icon size={15} /></span>
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
      {#if trashOpen}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
      <Trash size={14} />{t("trash")}
    </button>
    {#if trashOpen}
      {#if !trash.length}
        <p class="trash-empty">{t("trashEmpty")}</p>
      {:else}
        <div class="trash-list">
          {#each trash as entry (entry.id)}
            <div class="trash-item" title={entry.original ?? entry.name}>
              <span class="tname">{entry.name}</span>
              <button title={t("restore")} aria-label={t("restore")} onclick={() => trashOp(ipc.restoreTrash(entry.id))}><ArchiveRestore size={13} /></button>
              <button
                class="tdanger"
                title={t("deleteForever")}
                aria-label={t("deleteForever")}
                onclick={() => trashOp(ipc.deleteTrashItem(entry.id))}
              >
                <X size={13} />
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
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    padding: 3px 8px;
    color: var(--fg-dim);
  }
  .tree-actions button:hover {
    color: var(--fg);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    cursor: var(--cur-pointer, pointer);
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
    color: var(--fg-dim);
  }
  .icon.dir {
    color: #d3a13a;
  }
  .icon.tex {
    color: var(--accent);
  }
  .icon.bib {
    color: #a37be0;
  }
  .icon.pdf {
    color: #e06060;
  }
  .icon.img {
    color: #4ca96b;
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
    display: flex;
    align-items: center;
    gap: 6px;
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
    display: grid;
    padding: 3px 5px;
    color: var(--fg-dim);
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
