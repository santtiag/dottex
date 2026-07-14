/**
 * Vista previa de ecuaciones en modo código: mientras el cursor está dentro
 * de una expresión matemática, un tooltip la renderiza con KaTeX.
 */
import type { EditorState } from "@codemirror/state";
import { StateField } from "@codemirror/state";
import { showTooltip, type Tooltip } from "@codemirror/view";
import katex from "katex";
import { MATH_RE, mathFromMatch } from "./latexMath";

// ponytail: escanea ±2000 chars alrededor del cursor; ecuaciones que crucen
// ese borde no se detectan — ampliar la ventana si alguna vez molesta.
const WINDOW = 2000;

function tooltipAt(state: EditorState): Tooltip | null {
  const pos = state.selection.main.head;
  const from = Math.max(0, pos - WINDOW);
  const text = state.doc.sliceString(from, Math.min(state.doc.length, pos + WINDOW));
  for (const m of text.matchAll(MATH_RE)) {
    const start = from + m.index;
    const end = start + m[0].length;
    if (pos < start || pos > end) continue;
    const { src, display } = mathFromMatch(m);
    return {
      pos: start,
      above: true,
      arrow: true,
      create: () => {
        const dom = document.createElement("div");
        dom.className = "cm-math-preview";
        katex.render(src, dom, { displayMode: display, throwOnError: false });
        return { dom };
      },
    };
  }
  return null;
}

export const mathPreview = StateField.define<Tooltip | null>({
  create: tooltipAt,
  update: (v, tr) => (tr.docChanged || tr.selection ? tooltipAt(tr.state) : v),
  provide: (f) => showTooltip.from(f),
});
