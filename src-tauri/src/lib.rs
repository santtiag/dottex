mod compile;
mod completion;
mod fs_ops;
mod log_parse;
mod project;
mod search;
mod synctex;

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

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
    /// absolutas o con `..` (frontera de confianza del IPC).
    pub fn resolve(&self, rel: &str) -> Result<PathBuf, String> {
        let rel_path = Path::new(rel);
        if rel_path.is_absolute()
            || rel_path
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err("Ruta fuera del proyecto".into());
        }
        Ok(self.root()?.join(rel_path))
    }

    pub fn root(&self) -> Result<PathBuf, String> {
        let guard = self.project.lock().unwrap();
        guard
            .as_ref()
            .map(|p| p.root.clone())
            .ok_or_else(|| "No hay proyecto abierto".into())
    }

    pub fn root_file(&self) -> Result<String, String> {
        let guard = self.project.lock().unwrap();
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
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
