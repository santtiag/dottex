use crate::{rel_path, AppState};
use serde::Serialize;
use std::fs;
use std::path::Path;
use tauri::State;

const SEARCH_EXTS: &[&str] = &["tex", "bib", "cls", "sty", "bbx", "cbx", "bst", "txt", "md"];
const MAX_HITS: usize = 500;

#[derive(Serialize)]
pub struct SearchHit {
    pub file: String,
    pub line: u32, // 1-indexado
    pub col: u32,
    pub preview: String,
}

#[derive(Serialize)]
pub struct ReplaceResult {
    pub files: u32,
    pub matches: u32,
}

fn build_regex(query: &str, is_regex: bool, case_sensitive: bool) -> Result<regex::Regex, String> {
    let pat = if is_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    regex::RegexBuilder::new(&pat)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("Expresión regular inválida: {e}"))
}

pub fn collect_by_ext(root: &Path, dir: &Path, exts: &[&str], out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let abs = entry.path();
        if abs.is_dir() {
            collect_by_ext(root, &abs, exts, out);
        } else if abs
            .extension()
            .is_some_and(|e| exts.contains(&e.to_string_lossy().as_ref()))
        {
            out.push(rel_path(root, &abs));
        }
    }
}

// async + spawn_blocking: el recorrido y la lectura de todo el proyecto no
// deben congelar el hilo principal del UI.
#[tauri::command]
pub async fn search_project(
    state: State<'_, AppState>,
    query: String,
    is_regex: bool,
    case_sensitive: bool,
) -> Result<Vec<SearchHit>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = build_regex(&query, is_regex, case_sensitive)?;
    let root = state.root()?;
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        collect_by_ext(&root, &root, SEARCH_EXTS, &mut files);
        files.sort();

        let mut hits = Vec::new();
        'outer: for file in files {
            let Ok(text) = fs::read_to_string(root.join(&file)) else {
                continue;
            };
            for (i, line) in text.lines().enumerate() {
                for m in re.find_iter(line) {
                    hits.push(SearchHit {
                        file: file.clone(),
                        line: i as u32 + 1,
                        col: m.start() as u32 + 1,
                        preview: line.trim().chars().take(200).collect(),
                    });
                    if hits.len() >= MAX_HITS {
                        break 'outer;
                    }
                }
            }
        }
        Ok(hits)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reemplazo en todo el proyecto. En modo texto plano el reemplazo es
/// literal; en modo regex admite grupos ($1, $2…).
#[tauri::command]
pub async fn replace_in_project(
    state: State<'_, AppState>,
    query: String,
    is_regex: bool,
    case_sensitive: bool,
    replacement: String,
) -> Result<ReplaceResult, String> {
    if query.is_empty() {
        return Ok(ReplaceResult { files: 0, matches: 0 });
    }
    let re = build_regex(&query, is_regex, case_sensitive)?;
    let root = state.root()?;
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        collect_by_ext(&root, &root, SEARCH_EXTS, &mut files);

        // dos pasadas: preparar todo en memoria y escribir al final, para que
        // un error a medias no deje el proyecto medio reemplazado
        let mut staged: Vec<(String, String)> = Vec::new();
        let mut result = ReplaceResult { files: 0, matches: 0 };
        for file in files {
            let Ok(text) = fs::read_to_string(root.join(&file)) else {
                continue;
            };
            let mut n = 0u32;
            let new_text = re.replace_all(&text, |caps: &regex::Captures| {
                n += 1;
                if is_regex {
                    let mut dst = String::new();
                    caps.expand(&replacement, &mut dst);
                    dst
                } else {
                    replacement.clone()
                }
            });
            if n == 0 {
                continue;
            }
            staged.push((file, new_text.into_owned()));
            result.matches += n;
        }
        for (file, new_text) in &staged {
            fs::write(root.join(file), new_text.as_bytes())
                .map_err(|e| format!("No se pudo escribir {file}: {e}"))?;
            result.files += 1;
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
