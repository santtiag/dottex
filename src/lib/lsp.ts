// Puente con texlab (LSP de LaTeX). El backend lo arranca y hace de tubería:
// `lsp_send` hacia el servidor, eventos `lsp:message` de vuelta. Si texlab no
// arranca (sin red en la primera descarga), el editor sigue con latexComplete.
import type { CompletionContext } from "@codemirror/autocomplete";
import { EditorState, type Extension } from "@codemirror/state";
import { keymap, type EditorView } from "@codemirror/view";
import {
  LSPClient,
  findReferencesKeymap,
  hoverTooltips,
  jumpToDefinitionKeymap,
  serverCompletionSource,
  serverDiagnostics,
  type Transport,
} from "@codemirror/lsp-client";
import { listen } from "@tauri-apps/api/event";
import { tick } from "svelte";
import { lspSend, lspStart, lspStop, type Issue } from "./ipc";
import { setLspActive } from "./latexComplete";
import { app, openFile, saveActive, toast } from "./state.svelte";
import { t } from "./i18n.svelte";

let client: LSPClient | null = null;
let rootUri = "";
let sessionId = 0; // sesión del backend cuyos eventos se aceptan
let startSeq = 0;
let handlers: ((message: string) => void)[] = [];
let listening: Promise<unknown> | undefined;
let mainView: EditorView | null = null;
let warned = false;

const transport: Transport = {
  send: (m) => void lspSend(m).catch((e) => console.error("lsp:", e)),
  subscribe: (h) => void handlers.push(h),
  unsubscribe: (h) => void (handlers = handlers.filter((x) => x !== h)),
};

// ponytail: codificación tipo encodeURI; un nombre con `[ ] ^ |` no coincide
// byte a byte con el URI que usa texlab (solo afectaría a saltar a ese archivo).
const uriOf = (path: string) =>
  `${rootUri}/${path
    .split("/")
    .map((s) => encodeURI(s).replace(/[?#]/g, encodeURIComponent))
    .join("/")}`;

function pathOf(uri: string): string | null {
  try {
    const u = decodeURIComponent(uri);
    const root = decodeURIComponent(rootUri) + "/";
    return u.startsWith(root) ? u.slice(root.length) : null;
  } catch {
    return null;
  }
}

interface LspDiagnostic {
  severity?: number; // 1 error · 2 aviso · 3 info · 4 pista
  message: string;
  range: { start: { line: number; character: number }; end: { line: number; character: number } };
}

/** texlab publica los diagnósticos de todo el proyecto, no solo del archivo abierto. */
function onDiagnostics(_: LSPClient, params: { uri: string; diagnostics: LspDiagnostic[] }) {
  const file = pathOf(params.uri);
  if (file === null) return true;
  const issues: Issue[] = params.diagnostics
    // las pistas (label sin usar, entrada .bib sin citar) son ruido en la lista
    .filter((d) => (d.severity ?? 1) <= 3)
    .map((d) => ({
      severity: (d.severity ?? 1) === 1 ? "error" : "warning",
      file,
      line: d.range.start.line + 1,
      message: d.message,
      source: "syntax",
      col: d.range.start.character,
      endLine: d.range.end.line + 1,
      endCol: d.range.end.character,
    }));
  app.lspIssues = [...app.lspIssues.filter((i) => i.file !== file), ...issues];
  return true; // la lista de problemas es la única dueña de los diagnósticos
}

function reset() {
  client?.disconnect();
  client = null;
  handlers = [];
  sessionId = 0;
  if (app.lspIssues.length) app.lspIssues = [];
  setLspActive(false);
  app.lspSession++;
}

/** Arranca texlab para el proyecto recién abierto (puede descargarlo: no bloquea). */
export async function startLsp() {
  reset();
  const seq = ++startSeq;
  try {
    const session = await lspStart();
    await (listening ??= listen<{ id: number; message: string | null }>(
      "lsp:message",
      ({ payload }) => {
        if (payload.id !== sessionId) return; // rezagado de un servidor anterior
        if (payload.message === null) reset(); // texlab terminó
        else for (const h of handlers) h(payload.message);
      },
    ));
    if (seq !== startSeq) return; // se abrió otro proyecto mientras tanto
    sessionId = session.id;
    rootUri = session.root_uri;
    client = new LSPClient({
      rootUri,
      extensions: [
        hoverTooltips(),
        serverDiagnostics(), // por su sincronización automática; el handler es el nuestro
        keymap.of([...jumpToDefinitionKeymap, ...findReferencesKeymap]),
      ],
      notificationHandlers: { "textDocument/publishDiagnostics": onDiagnostics },
    });
    // saltar a una definición o referencia de otro archivo: abrirlo en el editor
    client.workspace.displayFile = async (uri) => {
      const path = pathOf(uri);
      if (path === null || !mainView) return null;
      if (app.active?.path !== path) {
        // cambiar de archivo descarta el buffer: guardar antes lo no guardado
        if (app.dirty) await saveActive(mainView.state.doc.toString());
        await openFile(path);
        await tick();
      }
      return mainView;
    };
    client.connect(transport);
    setLspActive(true);
    app.lspSession++;
  } catch (e) {
    if (seq === startSeq && !warned) {
      warned = true;
      toast(t("lspUnavailable", String(e)));
    }
  }
}

export function stopLsp() {
  startSeq++;
  reset();
  lspStop().catch(() => {});
}

/** El editor editable se registra para poder llevarlo a otro archivo. */
export function setLspView(view: EditorView | null) {
  mainView = view;
}

// texlab lista las carpetas tal cual; la interna del proyecto no es una ruta útil
const completion = [
  {
    autocomplete: async (ctx: CompletionContext) => {
      const res = await serverCompletionSource(ctx);
      return res && { ...res, options: res.options.filter((o) => o.label !== ".dottex") };
    },
  },
];

/** Conecta un archivo del proyecto a texlab; vacía si no hay servidor. */
export function lspExtension(path: string): Extension {
  if (!client) return [];
  return [
    client.plugin(uriOf(path), path.endsWith(".bib") ? "bibtex" : "latex"),
    // la fuente, no serverCompletion(): ese activa el autocompletado por su
    // cuenta y se saltaría la preferencia del usuario
    EditorState.languageData.of(() => completion),
  ];
}
