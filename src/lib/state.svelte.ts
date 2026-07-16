import { open, save } from "@tauri-apps/plugin-dialog";
import * as ipc from "./ipc";
import type { CompileStatus, Issue, ProjectInfo, TreeNode } from "./ipc";
import { t } from "./i18n.svelte";

export interface ActiveFile {
  path: string;
  kind: string;
  absPath: string;
  content: string;
}

export const app = $state({
  project: null as ProjectInfo | null,
  tree: [] as TreeNode[],
  active: null as ActiveFile | null,
  dirty: false,
  compileState: "idle" as "idle" | "compiling" | "ok" | "error",
  issues: [] as Issue[],
  log: [] as string[],
  showLog: false,
  /// PDF de vista previa: ruta relativa dentro de .dottex/build + versión
  /// para forzar recarga tras cada compilación.
  pdf: null as { path: string; version: number } | null,
  auto: true, // vista previa automática al dejar de escribir
  typing: false, // hay una recompilación automática pendiente del debounce
  goto: null as { line: number; seq: number } | null,
  /// línea del cursor en el editor (para resaltar el outline)
  cursorLine: 1,
  /// destino de scroll en el visor de vista previa (SyncTeX forward)
  pdfTarget: null as { page: number; y: number; seq: number } | null,
  theme: (globalThis.localStorage?.getItem("dottex-theme") ?? "auto") as
    | "auto"
    | "light"
    | "dark",
  toast: "",
});

/// Preferencias del usuario (persistidas como JSON en localStorage).
export const settings = $state({
  autocomplete: true,
  closeBrackets: true,
  steadyCursor: false,
  vim: false,
  breadcrumbs: true,
  mathPreview: true,
  spellcheck: false,
  spellLang: "es",
  locale: "es" as "es" | "en",
  editorTheme: "auto" as string, // "auto" o un id de editorThemes
  pdfInvert: true,
  fontSize: 14,
  fontFamily: "JetBrains Mono",
  lineHeight: 1.5,
});

if (globalThis.localStorage) {
  try {
    Object.assign(settings, JSON.parse(localStorage.getItem("dottex-settings") ?? "{}"));
  } catch {
    /* JSON corrupto: se usan los valores por defecto */
  }
  $effect.root(() => {
    $effect(() => localStorage.setItem("dottex-settings", JSON.stringify(settings)));
  });
}

let toastTimer: ReturnType<typeof setTimeout>;
export function toast(msg: string) {
  app.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = ""), 4000);
}

/** Binarios del proyecto como blob URL (revoca el anterior). */
let lastBlobUrl = "";
export async function fileBlobUrl(path: string): Promise<string> {
  const bytes = await ipc.readFileBytes(path);
  if (lastBlobUrl) URL.revokeObjectURL(lastBlobUrl);
  lastBlobUrl = URL.createObjectURL(new Blob([bytes]));
  return lastBlobUrl;
}

/** Diálogo de sistema para abrir una carpeta como proyecto. */
export async function pickAndOpenProject() {
  const dir = await open({ directory: true, title: "Abrir carpeta como proyecto" });
  if (typeof dir === "string") await openProject(dir);
}

export async function openProject(path: string) {
  try {
    const info = await ipc.openProject(path);
    app.project = info;
    app.tree = info.tree;
    app.active = null;
    app.dirty = false;
    app.pdf = null;
    app.compileState = "idle";
    app.issues = [];
    app.log = [];
    if (info.root_file) await openFile(info.root_file);
  } catch (e) {
    toast(String(e));
  }
}

export async function openFile(path: string) {
  try {
    const f = await ipc.readFile(path);
    app.active = { path, kind: f.kind, absPath: f.abs_path, content: f.content ?? "" };
    app.dirty = false;
  } catch (e) {
    toast(String(e));
  }
}

export async function setRootFile(path: string) {
  try {
    await ipc.setRootFile(path);
    if (app.project) app.project.root_file = path;
    requestCompile();
  } catch (e) {
    toast(String(e));
  }
}

export async function saveActive(content: string) {
  if (!app.active) return;
  try {
    await ipc.writeFile(app.active.path, content);
    app.active.content = content;
    app.dirty = false;
  } catch (e) {
    toast(String(e));
  }
}

/** Guardado + recompilación disparados por el debounce del editor. */
export async function autoCompile(content: string) {
  app.typing = false;
  await saveActive(content);
  requestCompile();
}

let pending = false;

/** Compila; si ya hay una compilación en curso, encola exactamente una más. */
export function requestCompile() {
  if (!app.project) return;
  if (app.compileState === "compiling") {
    pending = true;
    return;
  }
  ipc.compile().catch((e) => toast(String(e)));
}

export function onCompileStatus(s: CompileStatus) {
  app.compileState = s.state;
  if (s.state === "compiling") return;
  app.issues = s.issues;
  if (s.state === "error" && s.message && !s.issues.length) {
    app.showLog = true;
    app.log.push(s.message);
  }
  if (s.pdf_path) app.pdf = { path: s.pdf_path, version: Date.now() };
  if (pending) {
    pending = false;
    requestCompile();
  }
}

export async function reloadTree() {
  try {
    app.tree = await ipc.listTree();
  } catch {
    /* proyecto cerrado */
  }
}

/** Recarga el archivo activo si cambió en disco y no hay ediciones sin guardar. */
export async function refreshActiveFromDisk() {
  if (!app.active || app.active.kind !== "text" || app.dirty) return;
  try {
    const f = await ipc.readFile(app.active.path);
    if (f.content !== null && f.content !== app.active.content) {
      app.active.content = f.content;
    }
  } catch {
    /* el archivo pudo haber sido eliminado */
  }
}

let gotoSeq = 0;
export function gotoLine(line: number) {
  app.goto = { line, seq: ++gotoSeq };
}

/** Salta al archivo/línea de un issue. */
export async function gotoIssue(issue: Issue) {
  if (!issue.file) return;
  if (app.active?.path !== issue.file) await openFile(issue.file);
  if (issue.line) gotoLine(issue.line);
}

let pdfTargetSeq = 0;
/** SyncTeX forward: editor -> posición en el PDF de vista previa. */
export async function syncToPdf(line?: number) {
  if (!app.active || !app.pdf) return;
  try {
    const hit = await ipc.synctexForward(app.active.path, line ?? app.cursorLine);
    if (hit) app.pdfTarget = { page: hit.page, y: hit.y, seq: ++pdfTargetSeq };
  } catch (e) {
    toast(String(e));
  }
}

/** SyncTeX inverse: clic en el PDF -> archivo y línea en el editor. */
export async function syncFromPdf(page: number, x: number, y: number) {
  try {
    const hit = await ipc.synctexInverse(page, x, y);
    if (!hit) return;
    if (app.active?.path !== hit.file) await openFile(hit.file);
    gotoLine(hit.line);
  } catch (e) {
    toast(String(e));
  }
}

/** Diálogo "guardar como" para entregar el PDF compilado. */
export async function exportPdf() {
  if (!app.pdf) return;
  const name = app.pdf.path.split("/").pop() ?? "documento.pdf";
  try {
    const dest = await save({
      title: t("exportPdfDialog"),
      defaultPath: name,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (dest) {
      await ipc.exportPdf(dest);
      toast(t("pdfExported", dest));
    }
  } catch (e) {
    toast(String(e));
  }
}
