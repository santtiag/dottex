use crate::log_parse::{parse_log, Issue};
use crate::{project, AppState};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};

const TECTONIC_VERSION: &str = "0.15.0";

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
    let result = run_compile(&app, &state).await;
    state.compiling.store(false, Ordering::SeqCst);
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

    // el binario se resuelve/instala una sola vez por sesión
    let cached = state.tectonic.lock().unwrap().clone();
    let engine = match cached {
        Some(p) if p.is_file() => p,
        _ => {
            let app2 = app.clone();
            let p = tokio::task::spawn_blocking(move || find_or_install_tectonic(&app2))
                .await
                .map_err(|e| e.to_string())??;
            *state.tectonic.lock().unwrap() = Some(p.clone());
            p
        }
    };

    emit_status(
        app,
        CompileStatus {
            state: "compiling".into(),
            message: None,
            pdf_path: None,
            issues: Vec::new(),
        },
    );
    emit_log(app, format!("$ tectonic -X compile {root_file}"));

    let build_dir = state.build_dir()?;
    let mut child = tokio::process::Command::new(&engine)
        .args(["-X", "compile", &root_file, "--synctex", "--keep-logs", "--keep-intermediates"])
        .arg("--outdir")
        .arg(&build_dir)
        .current_dir(&root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("No se pudo ejecutar Tectonic ({}): {e}", engine.display()))?;

    // Tectonic escribe su log a stderr; retransmitimos ambos como eventos.
    let mut tasks = Vec::new();
    if let Some(out) = child.stdout.take() {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_log(&app, line);
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_log(&app, line);
            }
        }));
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    for t in tasks {
        let _ = t.await;
    }

    let stem = state.pdf_stem()?;
    let issues = std::fs::read_to_string(build_dir.join(format!("{stem}.log")))
        .map(|log| parse_log(&log))
        .unwrap_or_default();

    if !status.success() {
        emit_status(
            app,
            CompileStatus {
                state: "error".into(),
                message: Some("La compilación falló; revisa los problemas o el log".into()),
                pdf_path: None,
                issues,
            },
        );
        return Ok(());
    }

    emit_status(
        app,
        CompileStatus {
            state: "ok".into(),
            message: None,
            pdf_path: Some(format!(".dottex/build/{stem}.pdf")),
            issues,
        },
    );
    Ok(())
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
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(tectonic_binary_name());
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    let installed = project::data_dir().join("bin").join(tectonic_binary_name());
    if installed.is_file() {
        return Ok(installed);
    }
    download_tectonic(app, &installed)
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
