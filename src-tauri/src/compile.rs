use crate::log_parse::{parse_log, Issue};
use crate::{project, AppState};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};

const TECTONIC_VERSION: &str = "0.15.0";

/// SHA-256 de los assets del release 0.15.0 (pinneados el 2026-07-18): el
/// binario descargado se ejecuta, así que una descarga manipulada sería
/// ejecución de código arbitrario.
const TECTONIC_SHA256: &[(&str, &str)] = &[
    ("x86_64-unknown-linux-musl.tar.gz", "dfb82876f2986862996e564fa507a9e576e0c1e3bee63c2c1bd677c2543e6407"),
    ("aarch64-unknown-linux-musl.tar.gz", "1f59f9fb8eb65e8ba18658fc9016767e7d3e12488ded8b8fffa34254e51ce42c"),
    ("x86_64-apple-darwin.tar.gz", "dd42576eaa4c0df58c243dd78b7b864d9deb405ffdfcdadd1b79a31faceab747"),
    ("aarch64-apple-darwin.tar.gz", "24bd46566fa30d41101848405e9cbc4645edb92d8f857c9d21262174fb70cd33"),
    ("x86_64-pc-windows-msvc.zip", "1d6bb76f049c8a3774f6e9d66e4b04e1a8c3dcb37527b6b41b7e894328e7bf29"),
];

#[derive(Serialize, Clone)]
pub struct CompileStatus {
    /// "compiling" | "ok" | "error"
    pub state: String,
    pub message: Option<String>,
    /// PDF compilado, relativo a la raíz (dentro de .dottex/build/).
    pub pdf_path: Option<String>,
    /// Errores y avisos extraídos del log.
    pub issues: Vec<Issue>,
}

fn emit_status(app: &AppHandle, status: CompileStatus) {
    let _ = app.emit("compile:status", status);
}

fn emit_log(app: &AppHandle, line: impl Into<String>) {
    let _ = app.emit("compile:log", line.into());
}

/// El PDF nunca toca la carpeta del usuario: queda en .dottex/build/ y se
/// entrega con `export_pdf`.
#[tauri::command]
pub async fn compile(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state.compiling.swap(true, Ordering::SeqCst) {
        return Err("Ya hay una compilación en curso".into());
    }
    // RAII: el flag se limpia aunque el future se cancele (ventana cerrada a
    // media compilación) o run_compile haga panic; si no, quedaría atascado
    // en `true` y toda compilación posterior sería rechazada.
    struct ClearOnDrop<'a>(&'a std::sync::atomic::AtomicBool);
    impl Drop for ClearOnDrop<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _guard = ClearOnDrop(&state.compiling);
    let result = run_compile(&app, &state).await;
    if let Err(e) = &result {
        emit_status(
            &app,
            CompileStatus {
                state: "error".into(),
                message: Some(e.clone()),
                pdf_path: None,
                issues: Vec::new(),
            },
        );
    }
    result
}

/// Copia el PDF compilado a la ruta que el usuario eligió en el diálogo de
/// guardado (ruta absoluta de confianza: viene del diálogo nativo).
#[tauri::command]
pub fn export_pdf(state: State<'_, AppState>, dest: String) -> Result<(), String> {
    let src = build_pdf_path(&state)?;
    if !src.is_file() {
        return Err("No hay PDF compilado todavía".into());
    }
    std::fs::copy(&src, &dest).map_err(|e| format!("No se pudo exportar el PDF: {e}"))?;
    Ok(())
}

fn build_pdf_path(state: &AppState) -> Result<PathBuf, String> {
    Ok(state
        .build_dir()?
        .join(format!("{}.pdf", state.pdf_stem()?)))
}

async fn run_compile(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let root = state.root()?;
    let root_file = state.root_file()?;
    let mut engine_name = crate::project::normalize_engine(&state.engine()?)?;
    let build_dir = state.build_dir()?;

    let tex_src = std::fs::read_to_string(root.join(&root_file)).ok();
    if engine_name == "tectonic" {
        if tex_src.as_deref().is_some_and(documentclass_wants_pdflatex) {
            engine_name = "pdflatex".into();
            emit_log(
                app,
                "documentclass[pdftex]: Tectonic no es compatible; usando pdflatex",
            );
        }
    }
    if engine_name != "tectonic" {
        if let Some(tex) = &tex_src {
            preflight_classic(tex)?;
        }
    }

    let stem = state.pdf_stem()?;
    clear_stale_aux(&build_dir, &stem);

    emit_status(
        app,
        CompileStatus {
            state: "compiling".into(),
            message: None,
            pdf_path: None,
            issues: Vec::new(),
        },
    );

    let (bin, args) = if engine_name == "tectonic" {
        (resolve_tectonic(app, state).await?, tectonic_args(&root_file, &build_dir))
    } else {
        resolve_classic(&engine_name, &root_file, &build_dir)?
    };
    emit_log(app, format!("$ {} {}", bin.display(), args.join(" ")));

    let _status = spawn_logged(app, &bin, &args, &root).await?;

    let issues = std::fs::read_to_string(build_dir.join(format!("{stem}.log")))
        .map(|log| parse_log(&log))
        .unwrap_or_default();

    let pdf_rel = format!(".dottex/build/{stem}.pdf");
    let has_pdf = build_dir.join(format!("{stem}.pdf")).is_file();
    let has_err = issues.iter().any(|i| i.severity == "error");
    let (state, message, pdf_path) = if has_pdf {
        (
            if has_err { "error" } else { "ok" },
            None,
            Some(pdf_rel),
        )
    } else {
        ("error", Some(compile_fail_message(&issues)), None)
    };
    emit_status(
        app,
        CompileStatus {
            state: state.into(),
            message,
            pdf_path,
            issues,
        },
    );
    Ok(())
}

async fn resolve_tectonic(app: &AppHandle, state: &AppState) -> Result<PathBuf, String> {
    let cached = crate::lock_or_recover(&state.tectonic).clone();
    match cached {
        Some(p) if p.is_file() => Ok(p),
        _ => {
            let app2 = app.clone();
            let p = tokio::task::spawn_blocking(move || find_or_install_tectonic(&app2))
                .await
                .map_err(|e| e.to_string())??;
            *crate::lock_or_recover(&state.tectonic) = Some(p.clone());
            Ok(p)
        }
    }
}

fn tectonic_args(root_file: &str, build_dir: &Path) -> Vec<String> {
    vec![
        "-X".into(),
        "compile".into(),
        root_file.into(),
        "--synctex".into(),
        "--keep-logs".into(),
        "--keep-intermediates".into(),
        "--outdir".into(),
        build_dir.to_string_lossy().into_owned(),
    ]
}

fn documentclass_wants_pdflatex(tex: &str) -> bool {
    let Some(i) = tex.find("\\documentclass") else {
        return false;
    };
    let rest = &tex[i..];
    let Some(ob) = rest.find('[') else {
        return false;
    };
    let Some(cb) = rest.find(']') else {
        return false;
    };
    if cb < ob {
        return false;
    }
    rest[ob + 1..cb]
        .split(',')
        .map(str::trim)
        .any(|o| o.eq_ignore_ascii_case("pdftex"))
}

fn texlive_install_hint() -> &'static str {
    if cfg!(windows) {
        "instala MiKTeX o TeX Live completo"
    } else if cfg!(target_os = "macos") {
        "brew install --cask mactex-no-gui"
    } else if Path::new("/usr/bin/dnf").is_file() {
        "sudo dnf install texlive-scheme-medium texlive-collection-latexextra texlive-collection-fontsrecommended latexmk"
    } else {
        "sudo apt install texlive-latex-recommended texlive-fonts-recommended texlive-latex-extra latexmk"
    }
}

fn kpsewhich(name: &str) -> bool {
    let Some(bin) = find_on_path("kpsewhich") else {
        return true;
    };
    std::process::Command::new(bin)
        .arg(name)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(true)
}

/// Canarios de MDPI/pdfLaTeX. Si no hay kpsewhich, pdflatex fallará con su log.
fn preflight_classic(tex: &str) -> Result<(), String> {
    if !documentclass_wants_pdflatex(tex) {
        return Ok(());
    }
    let missing: Vec<&str> = ["fancyhdr.sty", "ecrm1000.tfm"]
        .into_iter()
        .filter(|n| !kpsewhich(n))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "TeX Live incompleto (falta {}). Las plantillas MDPI necesitan pdfLaTeX con paquetes extra: {}",
        missing.join(", "),
        texlive_install_hint()
    ))
}

fn missing_tex_file(msg: &str) -> Option<&str> {
    let start = msg.find('`')?;
    let rest = &msg[start + 1..];
    let end = rest.find('\'')?;
    let name = &rest[..end];
    (name.ends_with(".sty") || name.ends_with(".cls") || name.ends_with(".tfm")).then_some(name)
}

fn compile_fail_message(issues: &[Issue]) -> String {
    let missing: Vec<&str> = issues
        .iter()
        .filter_map(|i| missing_tex_file(&i.message))
        .collect();
    if missing.is_empty() {
        "La compilación falló; revisa los problemas o el log".into()
    } else {
        format!(
            "Falta {}: {}. Revisa los problemas o el log.",
            missing.join(", "),
            texlive_install_hint()
        )
    }
}

fn resolve_classic(
    engine: &str,
    root_file: &str,
    build_dir: &Path,
) -> Result<(PathBuf, Vec<String>), String> {
    if let Some(latexmk) = find_on_path("latexmk") {
        return Ok((latexmk, latexmk_args(engine, root_file, build_dir)?));
    }
    let bin = find_on_path(engine).ok_or_else(|| {
        format!(
            "No se encontró {engine} ni latexmk. Instala TeX Live ({}) o cambia el compilador a Tectonic en Configuración.",
            texlive_install_hint()
        )
    })?;
    Ok((bin, classic_args(root_file, build_dir)))
}

fn classic_args(root_file: &str, build_dir: &Path) -> Vec<String> {
    vec![
        "-interaction=nonstopmode".into(),
        "-synctex=1".into(),
        "-output-directory".into(),
        build_dir.to_string_lossy().into_owned(),
        root_file.into(),
    ]
}

/// Auxiliares que pdflatex relee al arrancar. Si un compile anterior murió
/// a mitad (SIGPIPE), quedan truncados y la siguiente pasada explota.
fn clear_stale_aux(build_dir: &Path, stem: &str) {
    for ext in ["aux", "out", "toc", "lof", "lot", "fls", "fdb_latexmk"] {
        let _ = std::fs::remove_file(build_dir.join(format!("{stem}.{ext}")));
    }
}

fn log_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\n', '\r'])
        .to_string()
}

async fn drain_logged(app: AppHandle, reader: impl tokio::io::AsyncRead + Unpin) {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) => break,
            Ok(_) => emit_log(&app, log_line(&buf)),
            Err(_) => break,
        }
    }
}

fn latexmk_args(engine: &str, root_file: &str, build_dir: &Path) -> Result<Vec<String>, String> {
    let flag = match engine {
        "pdflatex" => "-pdf",
        "xelatex" => "-xelatex",
        "lualatex" => "-lualatex",
        other => {
            return Err(format!("latexmk no aplica a {other}"));
        }
    };
    Ok(vec![
        flag.into(),
        "-f".into(),
        "-interaction=nonstopmode".into(),
        "-synctex=1".into(),
        format!("-outdir={}", build_dir.display()),
        root_file.into(),
    ])
}

async fn spawn_logged(
    app: &AppHandle,
    bin: &Path,
    args: &[String],
    cwd: &Path,
) -> Result<std::process::ExitStatus, String> {
    let mut child = tokio::process::Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("No se pudo ejecutar {}: {e}", bin.display()))?;

    let mut tasks = Vec::new();
    if let Some(out) = child.stdout.take() {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            drain_logged(app, out).await;
        }));
    }
    if let Some(err) = child.stderr.take() {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            drain_logged(app, err).await;
        }));
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    for t in tasks {
        let _ = t.await;
    }
    Ok(status)
}

// ponytail: herramienta de checksum del sistema (como curl/tar más abajo);
// migrar al crate sha2 si falla en alguna plataforma.
fn sha256_file(path: &Path) -> Result<String, String> {
    let out = if cfg!(windows) {
        std::process::Command::new("certutil")
            .arg("-hashfile")
            .arg(path)
            .arg("SHA256")
            .output()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("shasum").args(["-a", "256"]).arg(path).output()
    } else {
        std::process::Command::new("sha256sum").arg(path).output()
    }
    .map_err(|e| format!("No se pudo calcular el checksum: {e}"))?;
    if !out.status.success() {
        return Err(format!("Checksum falló: {}", String::from_utf8_lossy(&out.stderr)));
    }
    // sha256sum/shasum: "<hex>  archivo"; certutil: el hex va en su propia línea
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .find(|t| t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()))
        .map(|t| t.to_ascii_lowercase())
        .ok_or_else(|| "Salida de checksum irreconocible".into())
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let bin = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&bin))
        .find(|p| p.is_file())
}

fn tectonic_binary_name() -> &'static str {
    if cfg!(windows) {
        "tectonic.exe"
    } else {
        "tectonic"
    }
}

/// Busca tectonic en PATH, luego en el dir de datos de la app; si no está,
/// lo descarga de GitHub Releases (única vez que se necesita red, junto con
/// la primera descarga de paquetes LaTeX del propio Tectonic).
fn find_or_install_tectonic(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(p) = find_on_path("tectonic") {
        return Ok(p);
    }
    let installed = project::data_dir().join("bin").join(tectonic_binary_name());
    if installed.is_file() {
        return Ok(installed);
    }
    download_tectonic(app, &installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_de_archivo_vacio() {
        let p = std::env::temp_dir().join(format!("dottex-sha-test-{}", std::process::id()));
        std::fs::write(&p, b"").unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn latexmk_args_por_motor() {
        let d = Path::new("/tmp/build");
        let pdf = latexmk_args("pdflatex", "main.tex", d).unwrap();
        assert_eq!(
            pdf,
            [
                "-pdf",
                "-f",
                "-interaction=nonstopmode",
                "-synctex=1",
                "-outdir=/tmp/build",
                "main.tex"
            ]
        );
        assert_eq!(latexmk_args("xelatex", "a.tex", d).unwrap()[0], "-xelatex");
        assert_eq!(latexmk_args("lualatex", "a.tex", d).unwrap()[0], "-lualatex");
        assert!(latexmk_args("tectonic", "a.tex", d).is_err());
    }

    #[test]
    fn log_line_latin1_no_revienta() {
        assert_eq!(log_line(b"ok\n"), "ok");
        assert_eq!(log_line(b"ok\r\n"), "ok");
        // 0xE9 = é in Latin-1; UTF-8 would reject the line and close the pipe
        let s = log_line(&[b'c', b'a', 0xE9, b'\n']);
        assert!(s.starts_with("ca"));
        assert!(!s.ends_with('\n'));
    }

    #[test]
    fn clear_stale_aux_borra_aux_no_pdf() {
        let d = std::env::temp_dir().join(format!("dottex-aux-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for name in ["main.aux", "main.out", "main.pdf"] {
            std::fs::write(d.join(name), b"x").unwrap();
        }
        clear_stale_aux(&d, "main");
        assert!(!d.join("main.aux").exists());
        assert!(!d.join("main.out").exists());
        assert!(d.join("main.pdf").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn documentclass_pdftex_detecta_mdpi() {
        let mdpi = r"\documentclass[journal,article,submit,pdftex,moreauthors]{Definitions/mdpi}";
        assert!(documentclass_wants_pdflatex(mdpi));
        assert!(!documentclass_wants_pdflatex(r"\documentclass{article}"));
        assert!(!documentclass_wants_pdflatex(r"\documentclass[xelatex]{article}"));
    }

    #[test]
    fn preflight_omite_article_sin_pdftex() {
        assert!(preflight_classic(r"\documentclass{article}").is_ok());
    }

    #[test]
    fn compile_fail_menciona_sty_ausente() {
        let issues = vec![Issue {
            severity: "error".into(),
            file: Some("Definitions/mdpi.cls".into()),
            line: Some(36),
            message: "LaTeX Error: File `fancyhdr.sty' not found.".into(),
        }];
        let msg = compile_fail_message(&issues);
        assert!(msg.contains("fancyhdr.sty"));
        assert!(
            msg.contains("dnf")
                || msg.contains("apt")
                || msg.contains("MiKTeX")
                || msg.contains("mactex")
        );
    }
}

fn download_tectonic(app: &AppHandle, dest: &Path) -> Result<PathBuf, String> {
    let arch = std::env::consts::ARCH; // "x86_64" | "aarch64"
    let asset = match (std::env::consts::OS, arch) {
        ("linux", _) => format!("{arch}-unknown-linux-musl.tar.gz"),
        ("macos", _) => format!("{arch}-apple-darwin.tar.gz"),
        ("windows", _) => format!("{arch}-pc-windows-msvc.zip"),
        (os, _) => return Err(format!("Plataforma no soportada: {os}")),
    };
    let url = format!(
        "https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%40{v}/tectonic-{v}-{asset}",
        v = TECTONIC_VERSION
    );

    let bin_dir = dest.parent().unwrap();
    std::fs::create_dir_all(bin_dir).map_err(|e| e.to_string())?;
    let archive = bin_dir.join(&asset);

    emit_log(app, format!("Tectonic no encontrado; descargando {TECTONIC_VERSION}..."));
    emit_log(app, url.clone());

    // ponytail: curl + tar del sistema (vienen con Linux, macOS y Windows 10+)
    // en vez de reqwest + flate2/zip; si algún día falla en una plataforma,
    // migrar a reqwest.
    let run = |cmd: &str, args: &[&str]| -> Result<(), String> {
        let out = std::process::Command::new(cmd)
            .args(args)
            .output()
            .map_err(|e| format!("No se pudo ejecutar {cmd}: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("{cmd} falló: {}", String::from_utf8_lossy(&out.stderr)))
        }
    };
    run("curl", &["-fsSL", &url, "-o", &archive.to_string_lossy()]).map_err(|e| {
        format!("No se pudo descargar Tectonic (¿sin conexión?). Instálalo manualmente y reintenta. {e}")
    })?;
    let expected = TECTONIC_SHA256
        .iter()
        .find(|(a, _)| *a == asset)
        .map(|(_, h)| *h)
        .ok_or_else(|| format!("Sin checksum registrado para {asset}"))?;
    let actual = sha256_file(&archive)?;
    if actual != expected {
        let _ = std::fs::remove_file(&archive);
        return Err(format!(
            "El checksum de Tectonic no coincide (esperado {expected}, obtenido {actual}); \
             descarga corrupta o manipulada. Instálalo manualmente y reintenta."
        ));
    }
    run("tar", &["-xf", &archive.to_string_lossy(), "-C", &bin_dir.to_string_lossy()])?;
    let _ = std::fs::remove_file(&archive);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    if !dest.is_file() {
        return Err("La descarga de Tectonic no produjo el binario esperado".into());
    }
    emit_log(app, format!("Tectonic instalado en {}", dest.display()));
    Ok(dest.to_path_buf())
}
