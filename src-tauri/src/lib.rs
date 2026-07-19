mod compile;
mod completion;
mod fs_ops;
mod log_parse;
mod project;
mod search;
mod synctex;

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

/// Proyecto abierto. El watcher vive aquí: al cerrar/cambiar de proyecto se
/// dropea y su hilo de eventos termina solo.
pub struct Project {
    pub root: PathBuf,
    /// Ruta del .tex raíz, relativa a `root`.
    pub root_file: Option<String>,
    #[allow(dead_code)]
    watcher: notify::RecommendedWatcher,
}

#[derive(Default)]
pub struct AppState {
    pub project: Mutex<Option<Project>>,
    pub compiling: AtomicBool,
    /// Ruta del binario de tectonic, resuelta una vez por sesión.
    pub tectonic: Mutex<Option<PathBuf>>,
    /// SyncTeX parseado, cacheado por (ruta, mtime); se invalida al recompilar.
    pub(crate) synctex: Mutex<Option<(PathBuf, SystemTime, Arc<synctex::SyncData>)>>,
}

/// `lock()` que sobrevive a un mutex envenenado: un panic previo no debe
/// dejar todos los comandos muertos por el resto de la sesión.
pub fn lock_or_recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ruta relativa a la raíz del proyecto, con separador `/`.
pub fn rel_path(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .to_string_lossy()
        .trim_start_matches("./")
        .replace('\\', "/")
}

impl AppState {
    /// Une una ruta relativa a la raíz del proyecto, rechazando rutas
    /// absolutas o con `..` (frontera de confianza del IPC). Los symlinks
    /// tampoco pueden escapar: se canonicaliza el ancestro existente más
    /// profundo (el destino puede no existir todavía, p. ej. al crear).
    pub fn resolve(&self, rel: &str) -> Result<PathBuf, String> {
        let rel_path = Path::new(rel);
        if rel_path.is_absolute()
            || rel_path
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err("Ruta fuera del proyecto".into());
        }
        let root = self.root()?; // ya canonicalizada en open_project
        let joined = root.join(rel_path);
        let canon = match std::fs::canonicalize(&joined) {
            Ok(c) => c,
            // symlink roto: escribir lo crearía en su destino, fuera de control
            Err(_) if joined.is_symlink() => return Err("Ruta fuera del proyecto".into()),
            Err(_) => {
                let mut probe = joined.parent();
                loop {
                    let Some(p) = probe else {
                        return Err("Ruta fuera del proyecto".into());
                    };
                    match std::fs::canonicalize(p) {
                        Ok(c) => break c,
                        Err(_) => probe = p.parent(),
                    }
                }
            }
        };
        if !canon.starts_with(&root) {
            return Err("Ruta fuera del proyecto".into());
        }
        Ok(joined)
    }

    pub fn root(&self) -> Result<PathBuf, String> {
        let guard = lock_or_recover(&self.project);
        guard
            .as_ref()
            .map(|p| p.root.clone())
            .ok_or_else(|| "No hay proyecto abierto".into())
    }

    pub fn root_file(&self) -> Result<String, String> {
        let guard = lock_or_recover(&self.project);
        guard
            .as_ref()
            .ok_or("No hay proyecto abierto")?
            .root_file
            .clone()
            .ok_or_else(|| {
                "No se encontró el archivo raíz (.tex con \\documentclass). \
                 Puedes fijarlo en .dottex/config.json"
                    .into()
            })
    }

    // Layout interno de .dottex/: única fuente de verdad de las rutas.
    pub fn dottex_dir(&self) -> Result<PathBuf, String> {
        Ok(self.root()?.join(".dottex"))
    }

    pub fn build_dir(&self) -> Result<PathBuf, String> {
        Ok(self.dottex_dir()?.join("build"))
    }

    pub fn trash_dir(&self) -> Result<PathBuf, String> {
        Ok(self.dottex_dir()?.join("trash"))
    }

    /// Nombre base de los artefactos de compilación (stem del root file).
    pub fn pdf_stem(&self) -> Result<String, String> {
        Ok(Path::new(&self.root_file()?)
            .file_stem()
            .ok_or("Root file inválido")?
            .to_string_lossy()
            .into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_root(root: &Path) -> AppState {
        let watcher = notify::recommended_watcher(|_: notify::Result<notify::Event>| {}).unwrap();
        AppState {
            project: Mutex::new(Some(Project {
                root: root.to_path_buf(),
                root_file: None,
                watcher,
            })),
            ..Default::default()
        }
    }

    #[test]
    fn resolve_acepta_rutas_nuevas_y_bloquea_escapes() {
        let tmp = std::env::temp_dir().join(format!("dottex-resolve-test-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        let root = std::fs::canonicalize(&tmp).unwrap();
        let state = state_with_root(&root);

        // archivos por crear (guardar/renombrar/restaurar no deben romperse)
        assert!(state.resolve("sub/nuevo.tex").is_ok());
        assert!(state.resolve("dir/que/no/existe/archivo.tex").is_ok());
        // rutas absolutas y con ..
        assert!(state.resolve("/etc/passwd").is_err());
        assert!(state.resolve("../fuera.tex").is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/", root.join("escape")).unwrap();
            assert!(state.resolve("escape").is_err());
            assert!(state.resolve("escape/etc/passwd").is_err());
            std::os::unix::fs::symlink("/no/existe/fuera", root.join("roto")).unwrap();
            assert!(state.resolve("roto").is_err());
        }
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|_app| {
            // Señala al frontend que corre dentro de un AppImage (X11 forzado por
            // linuxdeploy) para que el visor de PDF suba la resolución del canvas.
            #[cfg(target_os = "linux")]
            {
                use tauri::Manager;
                if std::env::var("APPIMAGE").is_ok() {
                    if let Some(w) = _app.get_webview_window("main") {
                        let _ = w.eval("window.__DOTTEX_APPIMAGE__=true");
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project::open_project,
            project::get_recent_projects,
            project::list_tree,
            project::set_root_file,
            fs_ops::read_file,
            fs_ops::read_file_bytes,
            fs_ops::write_file,
            fs_ops::create_file,
            fs_ops::create_dir,
            fs_ops::rename_path,
            fs_ops::delete_path,
            fs_ops::clean_artifacts,
            compile::compile,
            compile::export_pdf,
            synctex::synctex_forward,
            synctex::synctex_inverse,
            search::search_project,
            search::replace_in_project,
            completion::completion_data,
            completion::get_snippets,
            fs_ops::list_trash,
            fs_ops::restore_trash,
            fs_ops::delete_trash_item,
            fs_ops::empty_trash,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
