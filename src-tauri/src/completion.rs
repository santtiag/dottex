use crate::{project, search::collect_by_ext, AppState};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use tauri::State;

#[derive(Serialize)]
pub struct Cite {
    pub key: String,
    pub detail: String,
}

#[derive(Serialize)]
pub struct CompletionData {
    pub labels: Vec<String>,
    pub cites: Vec<Cite>,
    pub commands: Vec<String>,
}

/// Labels, claves de bibliografía y comandos definidos en el proyecto.
// ponytail: re-escanea todo el proyecto en cada llamada (el frontend cachea
// 3s); indexar incrementalmente si algún día hay proyectos enormes.
#[tauri::command]
pub fn completion_data(state: State<AppState>) -> Result<CompletionData, String> {
    let root = state.root()?;

    let label_re = regex::Regex::new(r"\\label\s*\{([^}]+)\}").unwrap();
    let cmd_re =
        regex::Regex::new(r"\\(?:new|renew|provide)command\*?\s*\{?\\([a-zA-Z]+)").unwrap();
    let def_re = regex::Regex::new(r"\\def\s*\\([a-zA-Z]+)").unwrap();

    let mut labels = BTreeSet::new();
    let mut commands = BTreeSet::new();
    let mut cites = Vec::new();

    let mut tex_files = Vec::new();
    collect_by_ext(&root, &root, &["tex", "sty", "cls"], &mut tex_files);
    for file in &tex_files {
        let Ok(text) = fs::read_to_string(root.join(file)) else {
            continue;
        };
        for c in label_re.captures_iter(&text) {
            labels.insert(c[1].to_string());
        }
        for c in cmd_re.captures_iter(&text).chain(def_re.captures_iter(&text)) {
            commands.insert(c[1].to_string());
        }
    }

    let mut bib_files = Vec::new();
    collect_by_ext(&root, &root, &["bib"], &mut bib_files);
    let mut seen = BTreeSet::new();
    for file in &bib_files {
        let Ok(text) = fs::read_to_string(root.join(file)) else {
            continue;
        };
        for cite in parse_bib(&text) {
            if seen.insert(cite.key.clone()) {
                cites.push(cite);
            }
        }
    }

    Ok(CompletionData {
        labels: labels.into_iter().collect(),
        cites,
        commands: commands.into_iter().collect(),
    })
}

fn parse_bib(text: &str) -> Vec<Cite> {
    use std::sync::OnceLock;
    static KEY_RE: OnceLock<regex::Regex> = OnceLock::new();
    static TITLE_RE: OnceLock<regex::Regex> = OnceLock::new();
    let key_re =
        KEY_RE.get_or_init(|| regex::Regex::new(r"@\w+\s*\{\s*([^,\s{}]+)\s*,").unwrap());
    let title_re = TITLE_RE
        .get_or_init(|| regex::Regex::new(r#"(?i)title\s*=\s*[\{"]+([^"{}]+)"#).unwrap());
    key_re
        .captures_iter(text)
        .map(|c| {
            // el título suele venir en los ~500 bytes siguientes a la clave;
            // recortar en una frontera UTF-8 válida
            let start = c.get(0).unwrap().end();
            let mut stop = (start + 500).min(text.len());
            while !text.is_char_boundary(stop) {
                stop -= 1;
            }
            let detail = title_re
                .captures(&text[start..stop])
                .map(|t| t[1].trim().chars().take(80).collect())
                .unwrap_or_default();
            Cite {
                key: c[1].to_string(),
                detail,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bib_keys_and_titles() {
        let bib = r#"
@article{knuth84,
  author = {Donald Knuth},
  title  = {The {TeX}book},
  year   = {1984}
}
@book{acentos23, title = "Compilación rápida de ecuaciones", year = {2023}}
"#;
        let cites = parse_bib(bib);
        assert_eq!(cites.len(), 2);
        assert_eq!(cites[0].key, "knuth84");
        assert_eq!(cites[0].detail, "The");
        assert_eq!(cites[1].key, "acentos23");
        assert_eq!(cites[1].detail, "Compilación rápida de ecuaciones");
    }
}

const DEFAULT_SNIPPETS: &str = r#"{
  "fig": "\\begin{figure}[htbp]\n\t\\centering\n\t\\includegraphics[width=${1:0.8}\\textwidth]{${2:ruta}}\n\t\\caption{${3:descripción}}\\label{fig:${4:nombre}}\n\\end{figure}",
  "tab": "\\begin{table}[htbp]\n\t\\centering\n\t\\caption{${1:descripción}}\\label{tab:${2:nombre}}\n\t\\begin{tabular}{${3:lcr}}\n\t\t${4}\n\t\\end{tabular}\n\\end{table}",
  "eq": "\\begin{equation}\\label{eq:${1:nombre}}\n\t${2}\n\\end{equation}",
  "item": "\\begin{itemize}\n\t\\item ${1}\n\\end{itemize}",
  "enum": "\\begin{enumerate}\n\t\\item ${1}\n\\end{enumerate}",
  "sec": "\\section{${1:título}}\\label{sec:${2:nombre}}",
  "sub": "\\subsection{${1:título}}\\label{sec:${2:nombre}}",
  "frac": "\\frac{${1:num}}{${2:den}}",
  "env": "\\begin{${1:entorno}}\n\t${2}\n\\end{${1:entorno}}"
}"#;

/// Snippets del usuario: JSON `{trigger: cuerpo}` en el dir de datos de la
/// app; se crea con valores por defecto la primera vez. Editable a mano.
#[tauri::command]
pub fn get_snippets() -> Result<serde_json::Value, String> {
    let path = project::data_dir().join("snippets.json");
    if !path.is_file() {
        let _ = fs::create_dir_all(path.parent().unwrap());
        fs::write(&path, DEFAULT_SNIPPETS).map_err(|e| e.to_string())?;
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text)
        .map_err(|e| format!("snippets.json inválido ({}): {e}", path.display()))
}
