use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct Issue {
    /// "error" | "warning"
    pub severity: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

/// Parser pragmático del log de (La)TeX: errores `! ...` con su `l.N`,
/// warnings de LaTeX/paquetes, y atribución de archivo siguiendo la pila de
/// paréntesis del log.
// ponytail: el log envuelve a 79 columnas y puede partir nombres de archivo;
// si la atribución falla mucho, portar el parser de texlab.
pub fn parse_log(log: &str) -> Vec<Issue> {
    use std::sync::OnceLock;
    static LINE_RE: OnceLock<regex::Regex> = OnceLock::new();
    static WARN_RE: OnceLock<regex::Regex> = OnceLock::new();
    static INPUT_LINE_RE: OnceLock<regex::Regex> = OnceLock::new();
    let line_re = LINE_RE.get_or_init(|| regex::Regex::new(r"^l\.(\d+)").unwrap());
    let warn_re = WARN_RE
        .get_or_init(|| regex::Regex::new(r"(?:LaTeX|Package\s+\w+)\s+Warning:\s*(.+)").unwrap());
    let input_line_re =
        INPUT_LINE_RE.get_or_init(|| regex::Regex::new(r"on input line (\d+)\.?\s*$").unwrap());

    let lines: Vec<&str> = log.lines().collect();

    let mut stack: Vec<String> = Vec::new();
    let mut issues = Vec::new();
    let current_file =
        |stack: &Vec<String>| stack.iter().rev().find(|s| !s.is_empty()).cloned();

    for (i, line) in lines.iter().enumerate() {
        if let Some(msg) = line.strip_prefix("! ") {
            let lnum = lines[i + 1..(i + 12).min(lines.len())]
                .iter()
                .find_map(|l| line_re.captures(l))
                .and_then(|c| c[1].parse().ok());
            issues.push(Issue {
                severity: "error".into(),
                file: current_file(&stack),
                line: lnum,
                message: msg.trim().to_string(),
            });
        } else if let Some(c) = warn_re.captures(line) {
            let mut msg = c[1].trim().to_string();
            // el "on input line N" puede caer en la línea siguiente por el wrap
            let mut lnum = input_line_re.captures(&msg).and_then(|c| c[1].parse().ok());
            if lnum.is_none() {
                if let Some(next) = lines.get(i + 1) {
                    if let Some(c2) = input_line_re.captures(next) {
                        lnum = c2[1].parse().ok();
                        msg.push(' ');
                        msg.push_str(next.trim());
                    }
                }
            }
            issues.push(Issue {
                severity: "warning".into(),
                file: current_file(&stack),
                line: lnum,
                message: msg,
            });
        } else {
            track_files(line, &mut stack);
        }
    }
    issues
}

/// Pila de archivos: `(ruta` abre, `)` cierra. Paréntesis que no son de
/// archivo entran como marcador vacío para mantener el balance.
fn track_files(line: &str, stack: &mut Vec<String>) {
    let mut rest = line;
    while let Some(pos) = rest.find(['(', ')']) {
        if rest.as_bytes()[pos] == b')' {
            stack.pop();
            rest = &rest[pos + 1..];
        } else {
            let after = &rest[pos + 1..];
            let end = after
                .find(|c: char| c == '(' || c == ')' || c.is_whitespace())
                .unwrap_or(after.len());
            let token = after[..end].trim_start_matches("./");
            const KNOWN_EXTS: &[&str] = &[".tex", ".sty", ".cls", ".bib", ".bbx", ".cbx", ".def", ".cfg"];
            let has_known_ext = KNOWN_EXTS.iter().any(|ext| token.ends_with(ext));
            // TeX omite la extensión si \input no la lleva ("(sections/intro");
            // .tex es el default del propio TeX.
            let entry = if has_known_ext {
                token.to_string()
            } else if token.contains('/') {
                if token.rsplit('/').next().is_some_and(|seg| seg.contains('.')) {
                    token.to_string()
                } else {
                    format!("{token}.tex")
                }
            } else {
                String::new() // paréntesis que no es de archivo
            };
            stack.push(entry);
            rest = &after[end..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_error_warning_and_file_attribution() {
        // formato real de tectonic/pdftex: archivos sin ./ y sin extensión
        let log = "\
(main.tex
LaTeX Warning: Citation `knuth' on page 1 undefined on input line 5.
 (sections/intro
LaTeX Font Info:    External font `cmex10' loaded for size
(Font)              <6> on input line 7.
! Undefined control sequence.
l.2 \\badmacro
)
LaTeX Warning: Reference `fig:x' on page 1 undefined
on input line 9.
)";
        let issues = parse_log(log);
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].severity, "warning");
        assert_eq!(issues[0].line, Some(5));
        assert_eq!(issues[0].file.as_deref(), Some("main.tex"));
        assert_eq!(issues[1].severity, "error");
        assert_eq!(issues[1].line, Some(2));
        assert_eq!(issues[1].file.as_deref(), Some("sections/intro.tex"));
        assert_eq!(issues[2].line, Some(9), "línea en el renglón siguiente por wrap");
        assert_eq!(issues[2].file.as_deref(), Some("main.tex"));
    }

}
