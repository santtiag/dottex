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

    // el binario se resuelve/instala una sola vez por sesión
    let cached = crate::lock_or_recover(&state.tectonic).clone();
    let engine = match cached {
        Some(p) if p.is_file() => p,
        _ => {
            let app2 = app.clone();
            let p = tokio::task::spawn_blocking(move || find_or_install_tectonic(&app2))
                .await
                .map_err(|e| e.to_string())??;
            *crate::lock_or_recover(&state.tectonic) = Some(p.clone());
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
        // si el future se dropea (cierre de la app), no dejar tectonic huérfano
        .kill_on_drop(true)
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
