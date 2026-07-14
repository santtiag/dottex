use crate::{fs_ops, rel_path, search::collect_by_ext, AppState, Project};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, State};

#[derive(Serialize, Clone)]
pub struct TreeNode {
    pub name: String,
    /// Ruta relativa a la raíz del proyecto, con separador `/`.
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    pub root_file: Option<String>,
    pub tree: Vec<TreeNode>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RecentProject {
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, Default)]
struct ProjectConfig {
    root_file: Option<String>,
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("dottex")
}

#[tauri::command]
pub fn open_project(
    app: AppHandle,
    state: State<AppState>,
    path: String,
) -> Result<ProjectInfo, String> {
    let root = fs::canonicalize(&path).map_err(|e| format!("No se pudo abrir la carpeta: {e}"))?;
    if !root.is_dir() {
        return Err("La ruta no es una carpeta".into());
    }
    for sub in ["build", "cache", "trash"] {
        fs::create_dir_all(root.join(".dottex").join(sub))
            .map_err(|e| format!("No se pudo crear .dottex/ (¿permisos?): {e}"))?;
    }

    let root_file = resolve_root_file(&root);
    save_config(&root, &root_file);

    let watcher = fs_ops::start_watcher(app, root.clone())
        .map_err(|e| format!("No se pudo iniciar el watcher: {e}"))?;

    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    // Reemplazar el proyecto anterior dropea su watcher.
    *state.project.lock().unwrap() = Some(Project {
        root: root.clone(),
        root_file: root_file.clone(),
        watcher,
    });

    add_recent(&name, &root);

    Ok(ProjectInfo {
        name,
        path: root.display().to_string(),
        root_file,
        tree: scan_tree(&root, &root),
    })
}

#[tauri::command]
pub fn list_tree(state: State<AppState>) -> Result<Vec<TreeNode>, String> {
    let root = state.root()?;
    Ok(scan_tree(&root, &root))
}

/// Fija manualmente el .tex raíz del proyecto (persistido en .dottex/config.json).
#[tauri::command]
pub fn set_root_file(state: State<AppState>, path: String) -> Result<(), String> {
    let abs = state.resolve(&path)?;
    if !abs.is_file() {
        return Err("El archivo no existe".into());
    }
    let root = state.root()?;
    if let Some(p) = state.project.lock().unwrap().as_mut() {
        p.root_file = Some(path.clone());
    }
    save_config(&root, &Some(path));
    Ok(())
}

#[tauri::command]
pub fn get_recent_projects() -> Vec<RecentProject> {
    fs::read_to_string(data_dir().join("recents.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<RecentProject>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|r| Path::new(&r.path).is_dir())
        .collect()
}

fn add_recent(name: &str, root: &Path) {
    let mut recents = get_recent_projects();
    let path = root.display().to_string();
    recents.retain(|r| r.path != path);
    recents.insert(
        0,
        RecentProject {
            name: name.to_string(),
            path,
        },
    );
    recents.truncate(10);
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(&recents) {
        let _ = fs::write(dir.join("recents.json"), json);
    }
}

/// Árbol recursivo de la carpeta, omitiendo entradas ocultas (incluye .dottex).
fn scan_tree(root: &Path, dir: &Path) -> Vec<TreeNode> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut nodes: Vec<TreeNode> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let abs = entry.path();
            let is_dir = abs.is_dir();
            let path = rel_path(root, &abs);
            Some(TreeNode {
                children: if is_dir {
                    scan_tree(root, &abs)
                } else {
                    Vec::new()
                },
                name,
                path,
                is_dir,
            })
        })
        .collect();
    nodes.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    nodes
}

/// Root file: config existente > `% !TEX root` > heurística \documentclass.
fn resolve_root_file(root: &Path) -> Option<String> {
    let config_path = root.join(".dottex").join("config.json");
    if let Ok(s) = fs::read_to_string(&config_path) {
        if let Ok(cfg) = serde_json::from_str::<ProjectConfig>(&s) {
            if let Some(rf) = cfg.root_file {
                if root.join(&rf).is_file() {
                    return Some(rf);
                }
            }
        }
    }
    detect_root_file(root)
}

fn save_config(root: &Path, root_file: &Option<String>) {
    let cfg = ProjectConfig {
        root_file: root_file.clone(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&cfg) {
        let _ = fs::write(root.join(".dottex").join("config.json"), json);
    }
}

fn detect_root_file(root: &Path) -> Option<String> {
    let mut tex_files = Vec::new();
    collect_by_ext(root, root, &["tex"], &mut tex_files);
    tex_files.sort();

    let magic = regex::Regex::new(r"(?im)^%\s*!\s*TEX\s+root\s*=\s*(.+?)\s*$").unwrap();
    let inputs = regex::Regex::new(r"\\(?:input|include|subfile)\{([^}]+)\}").unwrap();

    let mut with_docclass = Vec::new();
    let mut referenced: HashSet<String> = HashSet::new();

    for rel in &tex_files {
        let Ok(content) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if let Some(cap) = magic.captures(&content) {
            let target = normalize_rel(&Path::new(rel).parent().unwrap_or(Path::new("")).join(&cap[1]));
            if root.join(&target).is_file() {
                return Some(target);
            }
        }
        if content.contains("\\documentclass") {
            with_docclass.push(rel.clone());
        }
        for cap in inputs.captures_iter(&content) {
            let mut target = cap[1].trim().to_string();
            if !target.ends_with(".tex") {
                target.push_str(".tex");
            }
            // \input se resuelve desde la raíz, pero aceptamos también rutas
            // relativas al archivo que lo contiene.
            referenced.insert(normalize_rel(Path::new(&target)));
            referenced.insert(normalize_rel(
                &Path::new(rel).parent().unwrap_or(Path::new("")).join(&target),
            ));
        }
    }

    let mut candidates: Vec<String> = with_docclass
        .into_iter()
        .filter(|f| !referenced.contains(f))
        .collect();
    candidates.sort_by_key(|f| {
        let is_main = Path::new(f)
            .file_name()
            .is_some_and(|n| n.to_ascii_lowercase() == "main.tex");
        (!is_main, Path::new(f).components().count(), f.clone())
    });
    candidates.into_iter().next()
}

/// Normaliza `./` y `..` sin tocar el disco; separador `/`.
fn normalize_rel(p: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    parts.join("/")
}
