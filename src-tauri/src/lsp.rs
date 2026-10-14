use crate::{compile, lock_or_recover, project, AppState};
use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};

const TEXLAB_VERSION: &str = "5.26.0";

/// SHA-256 de los assets del release 5.26.0 (pinneados el 2026-10-09): el
/// binario descargado se ejecuta, igual que Tectonic.
const TEXLAB_SHA256: &[(&str, &str)] = &[
    ("texlab-x86_64-linux.tar.gz", "8697bd5e479d4584b14b7eed5c320c80ec4e1d91ebefbb6801e6bf38e9971300"),
    ("texlab-aarch64-linux.tar.gz", "a85cdfcd22454b8d8550f4b0f0620c45ab51760f302fac7a12bc18a890f70f8c"),
    ("texlab-x86_64-macos.tar.gz", "6091611f756b28e1a57612b130c196df4b0bb6e22dde5cf5d890578513397daf"),
    ("texlab-aarch64-macos.tar.gz", "af7972ffd230711ba04ada9b69cc32ce9111d9196ba69538062872faefdbee56"),
    ("texlab-x86_64-windows.zip", "cb028d44c3d2b85d36a2ed52d41a0ff43a341b1f04c500c56c4524c4eb72b316"),
    ("texlab-aarch64-windows.zip", "99b215e9a44169eb8d786c33484b963055e1c3dd40f68e14fcc47ae1c84e92c1"),
];

/// Servidor texlab del proyecto abierto. Como el watcher, vive en `AppState`:
/// al reemplazarlo o cerrarlo se dropea y el proceso muere con él.
pub struct LspServer {
    id: u64,
    child: Child,
    stdin: ChildStdin,
    /// `file://…` de la raíz, sin barra final.
    root_uri: String,
}

impl Drop for LspServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Serialize)]
pub struct LspSession {
    /// Etiqueta los eventos: los mensajes rezagados de un servidor anterior
    /// (cambio de proyecto) se descartan en el frontend.
    pub id: u64,
    pub root_uri: String,
}

#[derive(Serialize, Clone)]
struct LspEvent {
    id: u64,
    /// Mensaje JSON-RPC; `None` = el servidor terminó.
    message: Option<String>,
}

/// Lee un mensaje LSP (cabeceras `Content-Length` + cuerpo). `None` = EOF o
/// encuadre roto.
fn read_message(r: &mut impl BufRead) -> Option<String> {
    let mut len: Option<usize> = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().ok();
            }
        }
    }
    let len = len.filter(|n| *n <= 64 << 20)?;
    let mut buf = vec![0; len];
    r.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// Lo único que el webview puede pedirle a texlab. Fuera quedan
/// `workspace/didChangeConfiguration`, `workspace/executeCommand` y
/// `textDocument/build`: son las vías por las que texlab ejecuta programas.
const ALLOWED_METHODS: &[&str] = &[
    "initialize",
    "initialized",
    "shutdown",
    "exit",
    "$/cancelRequest",
    "textDocument/didOpen",
    "textDocument/didChange",
    "textDocument/didClose",
    "textDocument/didSave",
    "textDocument/completion",
    "completionItem/resolve",
    "textDocument/hover",
    "textDocument/definition",
    "textDocument/references",
];

/// Frontera de confianza del puente LSP: método en la lista blanca, documento
/// dentro del proyecto y un `initialize` que no pueda inyectar configuración.
fn sanitize_outgoing(message: &str, root_uri: &str) -> Result<String, String> {
    use serde_json::Value;
    let mut v: Value =
        serde_json::from_str(message).map_err(|e| format!("Mensaje LSP inválido: {e}"))?;
    let Some(obj) = v.as_object_mut() else {
        return Err("Mensaje LSP inválido".into());
    };
    let method = obj.get("method").and_then(|m| m.as_str()).map(str::to_owned);
    match method.as_deref() {
        // respuesta a una petición del servidor: nunca lleva configuración,
        // porque `initialize` declara que no se la servimos
        None => {
            if let Some(r) = obj.get_mut("result") {
                *r = Value::Null;
            }
        }
        Some(m) if !ALLOWED_METHODS.contains(&m) => {
            return Err(format!("Método LSP no permitido: {m}"));
        }
        Some("initialize") => {
            let Some(p) = obj.get_mut("params").and_then(Value::as_object_mut) else {
                return Err("Mensaje LSP inválido".into());
            };
            p.insert("rootUri".into(), root_uri.into());
            for k in ["rootPath", "workspaceFolders", "initializationOptions"] {
                p.remove(k);
            }
            if let Some(ws) = p
                .get_mut("capabilities")
                .and_then(|c| c.get_mut("workspace"))
                .and_then(Value::as_object_mut)
            {
                ws.remove("configuration");
                ws.remove("didChangeConfiguration");
            }
        }
        Some(_) => {}
    }
    if let Some(uri) = v.pointer("/params/textDocument/uri").and_then(|u| u.as_str()) {
        let inside = uri
            .strip_prefix(root_uri)
            .and_then(|rest| rest.strip_prefix('/'))
            .is_some_and(|rest| {
                // `..` en claro o codificado, y nada de volver a subir con `\`
                let low = rest.to_ascii_lowercase();
                !low.split('/').any(|seg| seg == ".." || seg.contains("%2e%2e") || seg == ".%2e" || seg == "%2e.")
                    && !low.contains('\\')
                    && !low.contains("%5c")
                    && !low.contains("%2f")
            });
        if !inside {
            return Err("Documento fuera del proyecto".into());
        }
    }
    Ok(v.to_string())
}

fn texlab_binary_name() -> &'static str {
    if cfg!(windows) {
        "texlab.exe"
    } else {
        "texlab"
    }
}

/// texlab del PATH; si no, el del dir de datos de la app; si no, se descarga
/// de GitHub Releases (misma política que Tectonic).
fn find_or_install_texlab(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(p) = compile::find_on_path("texlab") {
        return Ok(p);
    }
    let dest = project::data_dir().join("bin").join(texlab_binary_name());
    if dest.is_file() {
        return Ok(dest);
    }
    let arch = std::env::consts::ARCH;
    let asset = match std::env::consts::OS {
        "linux" => format!("texlab-{arch}-linux.tar.gz"),
        "macos" => format!("texlab-{arch}-macos.tar.gz"),
        "windows" => format!("texlab-{arch}-windows.zip"),
        os => return Err(format!("Plataforma no soportada: {os}")),
    };
    let url = format!(
        "https://github.com/latex-lsp/texlab/releases/download/v{TEXLAB_VERSION}/{asset}"
    );
    compile::emit_log(app, format!("texlab no encontrado; descargando {TEXLAB_VERSION}..."));
    compile::emit_log(app, url.clone());
    compile::fetch_binary("texlab", &url, &asset, TEXLAB_SHA256, &dest)?;
    compile::emit_log(app, format!("texlab instalado en {}", dest.display()));
    Ok(dest)
}

/// Arranca texlab para el proyecto abierto. Sus mensajes salen como eventos
/// `lsp:message`; los del webview entran por `lsp_send`.
#[tauri::command]
pub async fn lsp_start(app: AppHandle, state: State<'_, AppState>) -> Result<LspSession, String> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    let root = state.root()?;
    let bin = {
        let app = app.clone();
        tokio::task::spawn_blocking(move || find_or_install_texlab(&app))
            .await
            .map_err(|e| e.to_string())??
    };
    // la descarga puede tardar: el proyecto pudo cambiar entre tanto
    if state.root()? != root {
        return Err("El proyecto cambió mientras arrancaba texlab".into());
    }
    let root_uri = tauri::Url::from_file_path(&root)
        .map_err(|_| "Ruta de proyecto inválida".to_string())?
        .to_string()
        .trim_end_matches('/')
        .to_string();

    let mut child = Command::new(&bin)
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("No se pudo ejecutar {}: {e}", bin.display()))?;
    let stdin = child.stdin.take().ok_or("texlab sin stdin")?;
    let stdout = child.stdout.take().ok_or("texlab sin stdout")?;

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    // El hilo termina solo: al matar el proceso su stdout se cierra.
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Some(message) = read_message(&mut reader) {
            let _ = app.emit("lsp:message", LspEvent { id, message: Some(message) });
        }
        // texlab murió por su cuenta: soltarlo aquí lo recoge (sin zombi)
        let state = app.state::<AppState>();
        let mut guard = lock_or_recover(&state.lsp);
        if guard.as_ref().is_some_and(|s| s.id == id) {
            *guard = None;
        }
        drop(guard);
        let _ = app.emit("lsp:message", LspEvent { id, message: None });
    });

    // Reemplazar el servidor anterior lo dropea (y lo mata).
    *lock_or_recover(&state.lsp) = Some(LspServer {
        id,
        child,
        stdin,
        root_uri: root_uri.clone(),
    });
    Ok(LspSession { id, root_uri })
}

// (async): la escritura en la tubería no debe correr en el hilo principal.
#[tauri::command(async)]
pub fn lsp_send(state: State<AppState>, message: String) -> Result<(), String> {
    let mut guard = lock_or_recover(&state.lsp);
    let srv = guard.as_mut().ok_or("texlab no está en marcha")?;
    let msg = sanitize_outgoing(&message, &srv.root_uri)?;
    write!(srv.stdin, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)
        .and_then(|_| srv.stdin.flush())
        .map_err(|e| format!("texlab no responde: {e}"))
}

#[tauri::command]
pub fn lsp_stop(state: State<AppState>) {
    *lock_or_recover(&state.lsp) = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encuadre_con_mensajes_pegados_y_lecturas_partidas() {
        let raw = "Content-Length: 7\r\n\r\n{\"a\":1}content-length: 10\r\nContent-Type: x\r\n\r\n{\"b\":\"ñ\"}";
        // búfer de 3 bytes: cada mensaje llega en trozos
        let mut r = BufReader::with_capacity(3, raw.as_bytes());
        assert_eq!(read_message(&mut r).as_deref(), Some("{\"a\":1}"));
        assert_eq!(read_message(&mut r).as_deref(), Some("{\"b\":\"ñ\"}"));
        assert_eq!(read_message(&mut r), None);
        // cuerpo truncado
        let mut r = BufReader::new("Content-Length: 50\r\n\r\n{}".as_bytes());
        assert_eq!(read_message(&mut r), None);
    }

    #[test]
    fn frontera_de_confianza_del_puente() {
        let root = "file:///home/u/proyecto";
        let open = |uri: &str| {
            format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}"}}}}}}"#)
        };
        assert!(sanitize_outgoing(&open("file:///home/u/proyecto/cap/intro.tex"), root).is_ok());
        for bad in [
            "file:///etc/passwd",
            "file:///home/u/proyecto-otro/a.tex",
            "file:///home/u/proyecto/../secreto.tex",
            "file:///home/u/proyecto/%2e%2e/secreto.tex",
            "file:///home/u/proyecto/a%2F..%2Fb.tex",
            "file:///home/u/proyecto",
        ] {
            assert!(sanitize_outgoing(&open(bad), root).is_err(), "{bad}");
        }
        // métodos que ejecutan programas o cambian la configuración
        for m in ["workspace/executeCommand", "workspace/didChangeConfiguration", "textDocument/build"] {
            let msg = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"{m}","params":{{}}}}"#);
            assert!(sanitize_outgoing(&msg, root).is_err(), "{m}");
        }
        assert!(sanitize_outgoing("no es json", root).is_err());

        // initialize: raíz forzada y sin vías de configuración
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///","workspaceFolders":[{"uri":"file:///"}],"initializationOptions":{"build":{"executable":"sh"}},"capabilities":{"workspace":{"configuration":true,"applyEdit":true}}}}"#;
        let v: serde_json::Value = serde_json::from_str(&sanitize_outgoing(init, root).unwrap()).unwrap();
        assert_eq!(v["params"]["rootUri"], root);
        assert!(v["params"].get("workspaceFolders").is_none());
        assert!(v["params"].get("initializationOptions").is_none());
        assert!(v["params"]["capabilities"]["workspace"].get("configuration").is_none());
        assert_eq!(v["params"]["capabilities"]["workspace"]["applyEdit"], true);

        // una respuesta al servidor no puede colar configuración
        let resp = r#"{"jsonrpc":"2.0","id":7,"result":[{"build":{"executable":"sh"}}]}"#;
        let v: serde_json::Value = serde_json::from_str(&sanitize_outgoing(resp, root).unwrap()).unwrap();
        assert!(v["result"].is_null());
    }
}
