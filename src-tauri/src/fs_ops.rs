use crate::AppState;
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

/// Watcher recursivo sobre la raíz. Emite `fs:changed` con debounce; el
/// payload booleano indica si hubo un cambio estructural (crear/borrar/
/// renombrar) para que el frontend solo recargue el árbol cuando toca.
/// Ignora todo lo que pase dentro de .dottex/.
pub fn start_watcher(app: AppHandle, root: PathBuf) -> notify::Result<notify::RecommendedWatcher> {
    use notify::EventKind;

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&root, RecursiveMode::Recursive)?;
    let dottex = root.join(".dottex");
    // Tectonic genera ráfagas de eventos en .dottex/build durante cada
    // compilación; mejor no recibirlas. Si el backend no soporta desobservar
    // un subdirectorio, `classify` ya las filtra igualmente.
    let _ = watcher.unwatch(&dottex);
    std::thread::spawn(move || {
        // (relevante, estructural); lo desconocido cuenta como estructural
        let classify = |ev: &notify::Result<notify::Event>| -> (bool, bool) {
            let Ok(e) = ev else { return (false, false) };
            if !e.paths.iter().any(|p| !p.starts_with(&dottex)) {
                return (false, false);
            }
            let structural = !matches!(
                e.kind,
                EventKind::Modify(notify::event::ModifyKind::Data(_))
                    | EventKind::Access(_)
            );
            (true, structural)
        };
        // El hilo termina solo cuando el watcher se dropea y el canal se cierra.
        while let Ok(first) = rx.recv() {
            let (mut emit, mut structural) = classify(&first);
            // ponytail: debounce fijo de 200ms; configurable si algún día hace falta
            let deadline = Instant::now() + Duration::from_millis(200);
            while let Ok(ev) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                let (r, s) = classify(&ev);
                emit |= r;
                structural |= s;
            }
            if emit {
                let _ = app.emit("fs:changed", structural);
            }
        }
    });
    Ok(watcher)
}

#[derive(Serialize)]
pub struct FileContent {
    /// "text" | "image" | "pdf" | "unsupported"
    pub kind: String,
    pub content: Option<String>,
    /// Ruta absoluta, para servir binarios vía asset protocol.
    pub abs_path: String,
}

const TEXT_EXTS: &[&str] = &[
    "tex", "bib", "cls", "sty", "bbx", "cbx", "bst", "txt", "md", "json", "toml", "yaml", "yml",
    "csv", "log", "html", "css", "js", "ts", "py", "sh",
];
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif"];

// async: la lectura corre en el pool de tokio, no en el hilo principal del UI.
#[tauri::command]
pub async fn read_file(state: State<'_, AppState>, path: String) -> Result<FileContent, String> {
    let abs = state.resolve(&path)?;
    tokio::task::spawn_blocking(move || read_file_inner(abs))
        .await
        .map_err(|e| e.to_string())?
}

fn read_file_inner(abs: std::path::PathBuf) -> Result<FileContent, String> {
    let ext = abs
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let abs_path = abs.display().to_string();

    if TEXT_EXTS.contains(&ext.as_str()) {
        let bytes = fs::read(&abs).map_err(|e| format!("No se pudo leer el archivo: {e}"))?;
        Ok(FileContent {
            kind: "text".into(),
            content: Some(String::from_utf8_lossy(&bytes).into_owned()),
            abs_path,
        })
    } else if IMAGE_EXTS.contains(&ext.as_str()) || ext == "pdf" {
        Ok(FileContent {
            kind: if ext == "pdf" { "pdf" } else { "image" }.into(),
            content: None,
            abs_path,
        })
    } else {
        // Extensión desconocida: si es UTF-8 válido lo tratamos como texto.
        let bytes = fs::read(&abs).map_err(|e| format!("No se pudo leer el archivo: {e}"))?;
        match String::from_utf8(bytes) {
            Ok(s) if !s.contains('\0') => Ok(FileContent {
                kind: "text".into(),
                content: Some(s),
                abs_path,
            }),
            _ => Ok(FileContent {
                kind: "unsupported".into(),
                content: None,
                abs_path,
            }),
        }
    }
}

/// Contenido binario crudo (PDF, imágenes) como cuerpo de la respuesta IPC,
/// sin pasar por JSON.
#[tauri::command]
pub async fn read_file_bytes(
    state: State<'_, AppState>,
    path: String,
) -> Result<tauri::ipc::Response, String> {
    let abs = state.resolve(&path)?;
    let bytes = tokio::task::spawn_blocking(move || fs::read(&abs))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("No se pudo leer el archivo: {e}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub async fn write_file(state: State<'_, AppState>, path: String, content: String) -> Result<(), String> {
    let abs = state.resolve(&path)?;
    fs::write(&abs, content).map_err(|e| format!("No se pudo guardar: {e}"))
}

#[tauri::command]
pub fn create_file(state: State<AppState>, path: String) -> Result<(), String> {
    let abs = state.resolve(&path)?;
    if abs.exists() {
        return Err("Ya existe un archivo con ese nombre".into());
    }
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&abs, "").map_err(|e| format!("No se pudo crear: {e}"))
}

#[tauri::command]
pub fn create_dir(state: State<AppState>, path: String) -> Result<(), String> {
    let abs = state.resolve(&path)?;
    if abs.exists() {
        return Err("Ya existe una carpeta con ese nombre".into());
    }
    fs::create_dir_all(&abs).map_err(|e| format!("No se pudo crear: {e}"))
}

#[tauri::command]
pub fn rename_path(state: State<AppState>, from: String, to: String) -> Result<(), String> {
    let from_abs = state.resolve(&from)?;
    let to_abs = state.resolve(&to)?;
    if to_abs.exists() {
        return Err("Ya existe algo con ese nombre".into());
    }
    if let Some(parent) = to_abs.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(&from_abs, &to_abs).map_err(|e| format!("No se pudo renombrar/mover: {e}"))
}

#[derive(Serialize, Deserialize)]
struct TrashMeta {
    original: String, // ruta relativa donde vivía
    deleted_at: u64,  // epoch millis
}

#[derive(Serialize)]
pub struct TrashEntry {
    pub id: String, // nombre de la entrada dentro de trash/
    pub name: String,
    pub original: Option<String>,
    pub deleted_at: u64,
}

/// Eliminar = mover a la papelera interna .dottex/trash/, con un .meta.json
/// al lado que recuerda la ubicación original para poder restaurar.
#[tauri::command]
pub fn delete_path(state: State<AppState>, path: String) -> Result<(), String> {
    let abs = state.resolve(&path)?;
    let name = abs
        .file_name()
        .ok_or("Ruta inválida")?
        .to_string_lossy()
        .into_owned();
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let trash = state.trash_dir()?;
    let id = unique_trash_id(&trash, millis, &name);
    fs::rename(&abs, trash.join(&id)).map_err(|e| format!("No se pudo eliminar: {e}"))?;
    let meta = TrashMeta {
        original: path,
        deleted_at: millis,
    };
    let _ = fs::write(
        trash.join(format!("{id}.meta.json")),
        serde_json::to_string(&meta).unwrap_or_default(),
    );
    Ok(())
}

/// Dos borrados del mismo nombre en el mismo milisegundo no deben pisarse.
fn unique_trash_id(trash: &std::path::Path, millis: u64, name: &str) -> String {
    let mut id = format!("{millis}__{name}");
    let mut n = 0u32;
    while trash.join(&id).exists() {
        n += 1;
        id = format!("{millis}-{n}__{name}");
    }
    id
}

/// Un id de papelera es un nombre plano, nunca una ruta.
fn check_trash_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id == "." || id == ".." {
        return Err("Id de papelera inválido".into());
    }
    Ok(())
}

#[tauri::command]
pub fn list_trash(state: State<AppState>) -> Result<Vec<TrashEntry>, String> {
    let trash = state.trash_dir()?;
    let Ok(entries) = fs::read_dir(&trash) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<TrashEntry> = entries
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            if id.ends_with(".meta.json") {
                return None;
            }
            let meta: Option<TrashMeta> = fs::read_to_string(trash.join(format!("{id}.meta.json")))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
            let name = id.split_once("__").map(|(_, n)| n.to_string()).unwrap_or_else(|| id.clone());
            Some(TrashEntry {
                deleted_at: meta.as_ref().map(|m| m.deleted_at).unwrap_or(0),
                original: meta.map(|m| m.original),
                id,
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    Ok(out)
}

#[tauri::command]
pub fn restore_trash(state: State<AppState>, id: String) -> Result<(), String> {
    check_trash_id(&id)?;
    let trash = state.trash_dir()?;
    let item = trash.join(&id);
    if !item.exists() {
        return Err("La entrada ya no está en la papelera".into());
    }
    let meta_path = trash.join(format!("{id}.meta.json"));
    let original = fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| serde_json::from_str::<TrashMeta>(&s).ok())
        .map(|m| m.original)
        // entradas antiguas sin meta: restaurar a la raíz con su nombre
        .unwrap_or_else(|| id.split_once("__").map(|(_, n)| n).unwrap_or(&id).to_string());
    let dest = state.resolve(&original)?;
    if dest.exists() {
        return Err(format!("Ya existe algo en la ubicación original ({original})"));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(&item, &dest).map_err(|e| format!("No se pudo restaurar: {e}"))?;
    let _ = fs::remove_file(&meta_path);
    Ok(())
}

/// Borrado permanente de una entrada de la papelera.
#[tauri::command]
pub fn delete_trash_item(state: State<AppState>, id: String) -> Result<(), String> {
    check_trash_id(&id)?;
    let trash = state.trash_dir()?;
    let item = trash.join(&id);
    if item.is_dir() {
        fs::remove_dir_all(&item).map_err(|e| e.to_string())?;
    } else if item.is_file() {
        fs::remove_file(&item).map_err(|e| e.to_string())?;
    }
    let _ = fs::remove_file(trash.join(format!("{id}.meta.json")));
    Ok(())
}

#[tauri::command]
pub fn empty_trash(state: State<AppState>) -> Result<(), String> {
    let trash = state.trash_dir()?;
    let _ = fs::remove_dir_all(&trash);
    fs::create_dir_all(&trash).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_ids_no_colisionan_en_el_mismo_milisegundo() {
        let tmp = std::env::temp_dir().join(format!("dottex-trash-test-{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let a = unique_trash_id(&tmp, 123, "x.tex");
        fs::write(tmp.join(&a), "").unwrap();
        let b = unique_trash_id(&tmp, 123, "x.tex");
        assert_eq!(a, "123__x.tex");
        assert_eq!(b, "123-1__x.tex");
        fs::remove_dir_all(&tmp).unwrap();
    }
}

/// Vacía .dottex/build y .dottex/cache ("Limpiar artefactos").
#[tauri::command]
pub fn clean_artifacts(state: State<AppState>) -> Result<(), String> {
    let dottex = state.dottex_dir()?;
    for sub in ["build", "cache"] {
        let dir = dottex.join(sub);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}
