use crate::{check_plain_name, project, AppState};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

const BLANK: &str = include_str!("../templates/blank.tex");

/// Plantillas de serie: se siembran la primera vez (como snippets.json) y a
/// partir de ahí son carpetas normales que el usuario edita o borra.
const BUILTIN: &[(&str, &str)] = &[
    ("En blanco", BLANK),
    ("Artículo", include_str!("../templates/article.tex")),
    ("Informe", include_str!("../templates/report.tex")),
    ("Presentación", include_str!("../templates/beamer.tex")),
];

#[derive(Serialize)]
pub struct Template {
    pub name: String,
    /// Ruta absoluta: editar una plantilla es abrirla como proyecto.
    pub path: String,
    /// Clase del documento raíz (article, beamer…), si se detecta.
    pub class: Option<String>,
    pub files: usize,
}

/// Una plantilla es una carpeta dentro del dir de datos de la app.
pub fn templates_dir() -> PathBuf {
    project::data_dir().join("templates")
}

/// ¿`root` (canónica) es una plantilla abierta para editarla?
pub fn is_template(root: &Path) -> bool {
    fs::canonicalize(templates_dir()).is_ok_and(|t| root.starts_with(t))
}

/// Un nombre de plantilla o de proyecto es un nombre plano y visible.
fn check_name(name: &str) -> Result<(), String> {
    check_plain_name(name)?;
    if name.starts_with('.') || name.trim() != name {
        return Err("Nombre inválido".into());
    }
    Ok(())
}

/// Copia recursiva saltando entradas ocultas (.dottex, .git…) y symlinks.
fn copy_visible(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_visible(&entry.path(), &dst.join(&name))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), dst.join(&name))?;
        }
    }
    Ok(())
}

fn count_visible(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| match e.file_type() {
            Ok(k) if k.is_dir() => count_visible(&e.path()),
            Ok(k) if k.is_file() => 1,
            _ => 0,
        })
        .sum()
}

fn document_class(dir: &Path) -> Option<String> {
    use std::sync::OnceLock;
    static CLASS_RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = CLASS_RE.get_or_init(|| {
        regex::Regex::new(r"\\documentclass\s*(?:\[[^\]]*\])?\s*\{([^}]+)\}").unwrap()
    });
    let text = fs::read_to_string(dir.join(project::detect_root_file(dir)?)).ok()?;
    re.captures(&text).map(|c| c[1].trim().to_string())
}

fn list_in(dir: &Path) -> Vec<Template> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Template> = entries
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| Template {
            name: e.file_name().to_string_lossy().into_owned(),
            path: e.path().display().to_string(),
            class: document_class(&e.path()),
            files: count_visible(&e.path()),
        })
        .collect();
    out.sort_by_key(|t| t.name.to_lowercase());
    out
}

/// Crea `parent/name` desde la carpeta `src`, o con un `main.tex` en blanco.
fn create_at(src: Option<&Path>, parent: &Path, name: &str) -> Result<PathBuf, String> {
    check_name(name)?;
    if !parent.is_absolute() || !parent.is_dir() {
        return Err("La ubicación no es una carpeta".into());
    }
    let dest = parent.join(name);
    if dest.exists() {
        return Err(format!("Ya existe: {}", dest.display()));
    }
    let res = match src {
        Some(src) if !src.is_dir() => return Err("La plantilla no existe".into()),
        // copiar una carpeta dentro de sí misma no termina nunca
        Some(src) if dest.starts_with(src) => {
            return Err("El destino está dentro de la plantilla".into())
        }
        Some(src) => copy_visible(src, &dest),
        None => fs::create_dir(&dest).and_then(|_| fs::write(dest.join("main.tex"), BLANK)),
    };
    res.map_err(|e| format!("No se pudo crear {}: {e}", dest.display()))?;
    Ok(dest)
}

// async + spawn_blocking: detectar la clase lee los .tex de cada plantilla.
#[tauri::command]
pub async fn list_templates() -> Result<Vec<Template>, String> {
    tokio::task::spawn_blocking(|| {
        let dir = templates_dir();
        if !dir.is_dir() {
            for (name, body) in BUILTIN {
                let t = dir.join(name);
                fs::create_dir_all(&t).map_err(|e| e.to_string())?;
                fs::write(t.join("main.tex"), body).map_err(|e| e.to_string())?;
            }
        }
        Ok(list_in(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Proyecto nuevo en `dest_parent/name` (ruta absoluta de confianza: viene
/// del diálogo nativo). Sin `template`, un `main.tex` en blanco.
#[tauri::command]
pub async fn create_from_template(
    template: Option<String>,
    dest_parent: String,
    name: String,
) -> Result<String, String> {
    let src = match &template {
        Some(t) => {
            check_name(t)?;
            Some(templates_dir().join(t))
        }
        None => None,
    };
    tokio::task::spawn_blocking(move || {
        create_at(src.as_deref(), Path::new(&dest_parent), &name)
            .map(|p| p.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Plantilla nueva con un `main.tex` en blanco; devuelve su ruta para abrirla.
#[tauri::command]
pub fn new_template(name: String) -> Result<String, String> {
    let dir = templates_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    create_at(None, &dir, &name).map(|p| p.display().to_string())
}

/// Guarda el proyecto abierto como plantilla (sin .dottex ni ocultos).
#[tauri::command]
pub async fn save_as_template(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let root = state.root()?;
    tokio::task::spawn_blocking(move || {
        let dir = templates_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        create_at(Some(&root), &dir, &name).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn delete_template(name: String) -> Result<(), String> {
    check_name(&name)?;
    fs::remove_dir_all(templates_dir().join(&name))
        .map_err(|e| format!("No se pudo eliminar la plantilla: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crear_desde_plantilla_salta_ocultos_y_valida_nombres() {
        let tmp = std::env::temp_dir().join(format!("dottex-tpl-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let src = tmp.join("plantilla");
        fs::create_dir_all(src.join(".dottex/build")).unwrap();
        fs::create_dir_all(src.join("figuras")).unwrap();
        fs::write(src.join("main.tex"), "\\documentclass[11pt]{beamer}\n\\begin{document}\\end{document}").unwrap();
        fs::write(src.join("figuras/a.png"), b"x").unwrap();
        fs::write(src.join(".dottex/build/main.pdf"), b"x").unwrap();
        fs::write(src.join(".gitignore"), b"x").unwrap();

        let dest = create_at(Some(&src), &tmp, "nuevo").unwrap();
        assert!(dest.join("main.tex").is_file());
        assert!(dest.join("figuras/a.png").is_file());
        assert!(!dest.join(".dottex").exists());
        assert!(!dest.join(".gitignore").exists());

        // el destino ya existe; nombres que no son planos; destino dentro del origen
        assert!(create_at(Some(&src), &tmp, "nuevo").is_err());
        for bad in ["", "..", "a/b", "a\\b", ".oculto", " x"] {
            assert!(create_at(Some(&src), &tmp, bad).is_err(), "{bad:?}");
        }
        assert!(create_at(Some(&src), &src, "dentro").is_err());
        assert!(create_at(Some(&tmp.join("no-existe")), &tmp, "otro").is_err());

        // sin plantilla: main.tex en blanco
        let blank = create_at(None, &tmp, "vacio").unwrap();
        assert_eq!(fs::read_to_string(blank.join("main.tex")).unwrap(), BLANK);

        let list = list_in(&tmp);
        let p = list.iter().find(|t| t.name == "plantilla").unwrap();
        assert_eq!(p.class.as_deref(), Some("beamer"));
        assert_eq!(p.files, 2);

        fs::remove_dir_all(&tmp).unwrap();
    }
}
