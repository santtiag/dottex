// Reproduce makeState de Editor.svelte en node: si una extensión es inválida,
// EditorState.create lanza aquí.
// Ejecutar:
//   node_modules/.pnpm/esbuild@*/node_modules/esbuild/bin/esbuild \
//     scripts/check-editor.ts --bundle --format=esm --outfile=/tmp/ce.mjs && node /tmp/ce.mjs
import {
  EditorView,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { Compartment, EditorState } from "@codemirror/state";
import {
  StreamLanguage,
  bracketMatching,
  defaultHighlightStyle,
  foldGutter,
  foldKeymap,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from "@codemirror/autocomplete";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { vim } from "@replit/codemirror-vim";
import { stex } from "@codemirror/legacy-modes/mode/stex";
import { oneDark } from "@codemirror/theme-one-dark";
import { latexComplete } from "../src/lib/latexComplete";
import { mathPreview } from "../src/lib/mathPreview";

const latex = StreamLanguage.define(stex);
const opts = new Compartment();

const settings = {
  autocomplete: true,
  closeBrackets: true,
  steadyCursor: false,
  vim: false,
  mathPreview: true,
  spellcheck: false,
  spellLang: "es",
  editorTheme: "auto",
  fontSize: 14,
  fontFamily: "JetBrains Mono",
  lineHeight: 1.5,
};

function optExts(dark: boolean) {
  const edDark = settings.editorTheme === "auto" ? dark : settings.editorTheme === "dark";
  return [
    settings.vim ? vim() : [],
    settings.autocomplete ? [autocompletion(), keymap.of(completionKeymap)] : [],
    settings.closeBrackets ? [closeBrackets(), keymap.of(closeBracketsKeymap)] : [],
    drawSelection({ cursorBlinkRate: settings.steadyCursor ? 0 : 1200 }),
    settings.mathPreview ? mathPreview : [],
    EditorView.contentAttributes.of({
      spellcheck: String(settings.spellcheck),
      lang: settings.spellLang,
    }),
    edDark ? oneDark : [],
    EditorView.theme({
      "&": { fontSize: `${settings.fontSize}px` },
      ".cm-content": { lineHeight: String(settings.lineHeight) },
      ".cm-scroller": { fontFamily: `"${settings.fontFamily}", ui-monospace, monospace` },
    }),
  ];
}

function makeState(text: string) {
  return EditorState.create({
    doc: text,
    extensions: [
      opts.of(optExts(false)),
      keymap.of([{ key: "Mod-s", run: () => true }]),
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      foldGutter(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      bracketMatching(),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      keymap.of([...defaultKeymap, ...historyKeymap, ...foldKeymap, ...searchKeymap]),
      latex,
      latex.data.of({ autocomplete: latexComplete }),
      EditorView.lineWrapping,
    ],
  });
}

const doc = "\\section{Hola}\nTexto $x^2$ y \\textbf{negrita}.\n";
const st = makeState(doc);
// reconfiguraciones como las del $effect
let tr = st.update({ effects: opts.reconfigure(optExts(true)) });
settings.vim = true;
tr = tr.state.update({ effects: opts.reconfigure(optExts(true)) });
settings.vim = false;
// edición + movimiento de cursor (dispara mathPreview.update)
tr = tr.state.update({ changes: { from: doc.length, insert: "$a+b$" }, selection: { anchor: doc.length + 3 } });
console.log(`OK: estado creado y reconfigurado sin excepción (doc len ${tr.state.doc.length})`);
