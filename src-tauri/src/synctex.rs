use crate::AppState;
use serde::Serialize;
use std::io::Read;
use std::path::Path;
use tauri::State;

/// Posición en el PDF en puntos "big point" (bp = 1/72 in) medidos desde la
/// esquina superior izquierda de la página — lo que usa pdf.js al escalar.
#[derive(Serialize)]
pub struct ForwardHit {
    pub page: u32,
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize)]
pub struct InverseHit {
    pub file: String,
    pub line: u32,
}

struct Record {
    tag: u32,
    line: u32,
    x: i64, // unidades synctex (sp con Unit:1)
    y: i64,
    page: u32,
}

pub(crate) struct SyncData {
    records: Vec<Record>,
    /// tag -> ruta relativa al proyecto
    inputs: Vec<(u32, String)>,
    /// factor unidades synctex -> bp
    to_bp: f64,
}

/// Editor -> PDF: posición de una línea de un archivo.
#[tauri::command]
pub fn synctex_forward(
    state: State<AppState>,
    file: String,
    line: u32,
) -> Result<Option<ForwardHit>, String> {
    let data = load(&state)?;
    let Some(tag) = data.inputs.iter().find(|(_, p)| *p == file).map(|(t, _)| *t) else {
        return Ok(None);
    };
    let best = data
        .records
        .iter()
        .filter(|r| r.tag == tag)
        .min_by_key(|r| ((r.line as i64 - line as i64).abs(), r.y));
    Ok(best.map(|r| ForwardHit {
        page: r.page,
        x: r.x as f64 * data.to_bp,
        y: r.y as f64 * data.to_bp,
    }))
}

/// PDF -> editor: archivo y línea del punto clicado (bp desde arriba-izquierda).
#[tauri::command]
pub fn synctex_inverse(
    state: State<AppState>,
    page: u32,
    x: f64,
    y: f64,
) -> Result<Option<InverseHit>, String> {
    let data = load(&state)?;
    let xs = x / data.to_bp;
    let ys = y / data.to_bp;
    // lo vertical pesa más: interesa acertar la línea, no la palabra
    let best = data
        .records
        .iter()
        .filter(|r| r.page == page)
        .min_by_key(|r| (r.y as f64 - ys).abs() as i64 * 8 + (r.x as f64 - xs).abs() as i64);
    Ok(best.and_then(|r| {
        let file = data.inputs.iter().find(|(t, _)| *t == r.tag)?.1.clone();
        Some(InverseHit { file, line: r.line })
    }))
}

/// Descomprimir + parsear el .synctex es caro y esto corre en cada
/// interacción de sync: se cachea por (ruta, mtime), así cada compilación
/// (mtime nuevo) invalida sola.
fn load(state: &AppState) -> Result<std::sync::Arc<SyncData>, String> {
    let root = state.root()?;
    let stem = state.pdf_stem()?;
    let build = state.build_dir()?;
    let gz = build.join(format!("{stem}.synctex.gz"));
    let plain = build.join(format!("{stem}.synctex"));

    let path = if gz.is_file() {
        gz
    } else if plain.is_file() {
        plain
    } else {
        return Err("No hay datos de SyncTeX; compila primero".into());
    };
    let mtime = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .map_err(|e| e.to_string())?;

    let mut cache = crate::lock_or_recover(&state.synctex);
    if let Some((p, t, data)) = cache.as_ref() {
        if *p == path && *t == mtime {
            return Ok(data.clone());
        }
    }

    let text = if path.extension().is_some_and(|e| e == "gz") {
        let mut s = String::new();
        flate2::read::GzDecoder::new(
            std::fs::File::open(&path).map_err(|e| e.to_string())?,
        )
        .read_to_string(&mut s)
        .map_err(|e| format!("SyncTeX corrupto: {e}"))?;
        s
    } else {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    };
    let data = std::sync::Arc::new(parse(&text, &root));
    *cache = Some((path, mtime, data.clone()));
    Ok(data)
}

fn parse(text: &str, root: &Path) -> SyncData {
    let mut inputs = Vec::new();
    let mut records = Vec::new();
    let mut unit = 1.0f64;
    let mut mag = 1000.0f64;
    let mut page = 0u32;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Input:") {
            if let Some((tag, path)) = rest.split_once(':') {
                if let (Ok(tag), false) = (tag.parse::<u32>(), path.is_empty()) {
                    inputs.push((tag, crate::rel_path(root, Path::new(path))));
                }
            }
        } else if let Some(v) = line.strip_prefix("Unit:") {
            unit = v.parse().unwrap_or(1.0);
        } else if let Some(v) = line.strip_prefix("Magnification:") {
            mag = v.parse().unwrap_or(1000.0);
        } else if let Some(n) = line.strip_prefix('{') {
            page = n.parse().unwrap_or(page + 1);
        } else if let Some((tag, lnum, x, y)) = parse_record(line) {
            records.push(Record { tag, line: lnum, x, y, page });
        }
    }
    SyncData {
        records,
        inputs,
        // sp -> pt (÷65536) -> bp (×72/72.27), con unidad y magnificación
        to_bp: unit * (mag / 1000.0) / 65781.76,
    }
}

/// Registros de contenido: `h|v|x|k|g|$|(|[` + `tag,línea:x,y[...]`.
fn parse_record(line: &str) -> Option<(u32, u32, i64, i64)> {
    let mut chars = line.chars();
    if !"([hvxkg$".contains(chars.next()?) {
        return None;
    }
    let rest = chars.as_str();
    let mut parts = rest.split(':');
    let (tag, lnum) = parts.next()?.split_once(',')?;
    let (x, y) = parts.next()?.split_once(',')?;
    Some((tag.parse().ok()?, lnum.parse().ok()?, x.parse().ok()?, y.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records_and_pages() {
        let text = "\
SyncTeX Version:1
Input:1:/proj/main.tex
Input:13:/proj/sections/intro.tex
Output:pdf
Magnification:1000
Unit:1
X Offset:0
Y Offset:0
Content:
!267
{1
[1,11:4736287,46220575:26673152,41484288,0
h1,6:8799519,8865055:0,0,0
k13,2:14078081,11617567:5278562
g13,2:14078081,11617567
}1
";
        let data = parse(text, Path::new("/proj"));
        assert_eq!(data.inputs, vec![(1, "main.tex".into()), (13, "sections/intro.tex".into())]);
        assert_eq!(data.records.len(), 4);
        assert!(data.records.iter().all(|r| r.page == 1));
        // h1,6 a y=8865055 sp ≈ 134.8 bp desde arriba
        let h = data.records.iter().find(|r| r.line == 6).unwrap();
        let y_bp = h.y as f64 * data.to_bp;
        assert!((y_bp - 134.8).abs() < 1.0, "y_bp = {y_bp}");
        // registro de intro.tex conserva su tag
        assert!(data.records.iter().any(|r| r.tag == 13 && r.line == 2));
    }
}
