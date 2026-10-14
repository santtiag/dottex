// Mensaje del problema al final de su línea, sin pasar el ratón por encima.
// Lee el estado de @codemirror/lint, así que sigue a los diagnósticos al editar.
import { forEachDiagnostic, setDiagnosticsEffect, type Diagnostic } from "@codemirror/lint";
import { RangeSetBuilder, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  WidgetType,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";

const RANK = { error: 0, warning: 1, info: 2, hint: 3 };

class Lens extends WidgetType {
  constructor(
    readonly severity: string,
    readonly message: string,
  ) {
    super();
  }
  eq(other: Lens) {
    return other.severity === this.severity && other.message === this.message;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = `cm-errorlens cm-errorlens-${this.severity}`;
    el.textContent = this.message;
    return el;
  }
}

function build(view: EditorView): DecorationSet {
  // por línea, solo el problema más grave
  const byLineEnd = new Map<number, Diagnostic>();
  forEachDiagnostic(view.state, (d, from) => {
    const at = view.state.doc.lineAt(from).to;
    const cur = byLineEnd.get(at);
    if (!cur || RANK[d.severity] < RANK[cur.severity]) byLineEnd.set(at, d);
  });
  const set = new RangeSetBuilder<Decoration>();
  for (const at of [...byLineEnd.keys()].sort((a, b) => a - b)) {
    const d = byLineEnd.get(at)!;
    const message = d.message.split("\n")[0].slice(0, 160);
    set.add(at, at, Decoration.widget({ widget: new Lens(d.severity, message), side: 1 }));
  }
  return set.finish();
}

export const errorLens: Extension = [
  ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = build(view);
      }
      update(u: ViewUpdate) {
        const changed = u.transactions.some((tr) =>
          tr.effects.some((e) => e.is(setDiagnosticsEffect)),
        );
        if (u.docChanged || changed) this.decorations = build(u.view);
      }
    },
    { decorations: (v) => v.decorations },
  ),
  EditorView.baseTheme({
    ".cm-errorlens": {
      marginLeft: "2ch",
      padding: "0 6px",
      borderRadius: "4px",
      fontSize: "0.88em",
      fontStyle: "italic",
      // con ajuste de línea el mensaje puede partirse: cada trozo conserva su caja
      boxDecorationBreak: "clone",
      WebkitBoxDecorationBreak: "clone",
    },
    ".cm-errorlens-error": { color: "#d33", background: "#dd333318" },
    ".cm-errorlens-warning": { color: "#b80", background: "#cc990018" },
    ".cm-errorlens-info, .cm-errorlens-hint": { color: "#789", background: "#77889918" },
  }),
];
