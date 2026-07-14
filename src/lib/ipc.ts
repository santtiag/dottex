import { invoke } from "@tauri-apps/api/core";

export interface TreeNode {
  name: string;
  path: string; // relativa a la raíz del proyecto
  is_dir: boolean;
  children: TreeNode[];
}

export interface ProjectInfo {
  name: string;
  path: string;
  root_file: string | null;
  tree: TreeNode[];
}

export interface RecentProject {
  name: string;
  path: string;
}

export interface FileContent {
  kind: "text" | "image" | "pdf" | "unsupported";
  content: string | null;
  abs_path: string;
}

export interface Issue {
  severity: "error" | "warning";
  file: string | null;
  line: number | null;
  message: string;
}

export interface CompileStatus {
  state: "compiling" | "ok" | "error";
  message: string | null;
  pdf_path: string | null; // relativo a la raíz, dentro de .dottex/build/
  issues: Issue[];
}

export interface ForwardHit {
  page: number;
  x: number; // bp desde la esquina superior izquierda de la página
  y: number;
}

export interface InverseHit {
  file: string;
  line: number;
}

export interface SearchHit {
  file: string;
  line: number;
  col: number;
  preview: string;
}

export interface ReplaceResult {
  files: number;
  matches: number;
}

export interface Cite {
  key: string;
  detail: string;
}

export interface CompletionData {
  labels: string[];
  cites: Cite[];
  commands: string[];
}

export interface TrashEntry {
  id: string;
  name: string;
  original: string | null;
  deleted_at: number;
}

export const openProject = (path: string) => invoke<ProjectInfo>("open_project", { path });
export const setRootFile = (path: string) => invoke("set_root_file", { path });
export const searchProject = (query: string, isRegex: boolean, caseSensitive: boolean) =>
  invoke<SearchHit[]>("search_project", { query, isRegex, caseSensitive });
export const replaceInProject = (
  query: string,
  isRegex: boolean,
  caseSensitive: boolean,
  replacement: string,
) => invoke<ReplaceResult>("replace_in_project", { query, isRegex, caseSensitive, replacement });
export const completionData = () => invoke<CompletionData>("completion_data");
export const getSnippets = () => invoke<Record<string, string>>("get_snippets");
export const listTrash = () => invoke<TrashEntry[]>("list_trash");
export const restoreTrash = (id: string) => invoke("restore_trash", { id });
export const deleteTrashItem = (id: string) => invoke("delete_trash_item", { id });
export const emptyTrash = () => invoke("empty_trash");
export const synctexForward = (file: string, line: number) =>
  invoke<ForwardHit | null>("synctex_forward", { file, line });
export const synctexInverse = (page: number, x: number, y: number) =>
  invoke<InverseHit | null>("synctex_inverse", { page, x, y });
export const readFileBytes = (path: string) => invoke<ArrayBuffer>("read_file_bytes", { path });
export const exportPdf = (dest: string) => invoke("export_pdf", { dest });
export const getRecentProjects = () => invoke<RecentProject[]>("get_recent_projects");
export const listTree = () => invoke<TreeNode[]>("list_tree");
export const readFile = (path: string) => invoke<FileContent>("read_file", { path });
export const writeFile = (path: string, content: string) =>
  invoke("write_file", { path, content });
export const createFile = (path: string) => invoke("create_file", { path });
export const createDir = (path: string) => invoke("create_dir", { path });
export const renamePath = (from: string, to: string) => invoke("rename_path", { from, to });
export const deletePath = (path: string) => invoke("delete_path", { path });
export const cleanArtifacts = () => invoke("clean_artifacts");
export const compile = () => invoke("compile");
