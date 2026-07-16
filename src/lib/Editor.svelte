<script lang="ts">
  import { onDestroy, onMount } from "svelte";
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
  import { Compartment, EditorSelection, EditorState } from "@codemirror/state";
  import {
    StreamLanguage,
    bracketMatching,
    defaultHighlightStyle,
    foldGutter,
    foldKeymap,
    indentOnInput,
    syntaxHighlighting,
  } from "@codemirror/language";
  import { defaultKeymap, history, historyKeymap, redo, undo } from "@codemirror/commands";
  import {
    autocompletion,
    closeBrackets,
    closeBracketsKeymap,
    completionKeymap,
  } from "@codemirror/autocomplete";
  import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
  import { vim } from "@replit/codemirror-vim";
  import { stex } from "@codemirror/legacy-modes/mode/stex";
  import { resolveTheme } from "./editorThemes";
  import { latexComplete } from "./latexComplete";
  import { mathPreview } from "./mathPreview";
  import { visualLatex } from "./visualLatex";
  import { settings } from "./state.svelte";
  import { t, type MsgKey } from "./i18n.svelte";
  import "katex/dist/katex.min.css";

  const latex = StreamLanguage.define(stex);

  let {
    path,
    content,
    dark,
    auto,
    visual,
    goto,
    onSave,
    onDirty,
    onIdle,
    onCompile,
    onCursor,
    onSyncClick,
  }: {
    path: string;
    content: string;
    dark: boolean;
    auto: boolean;
    /** modo Visual: sintaxis LaTeX oculta/renderizada en línea */
    visual: boolean;
    goto: { line: number; seq: number } | null;
    onSave: (text: string) => void;
    onDirty: () => void;
    /** ~450ms sin teclear con vista previa automática activa */
    onIdle: (text: string) => void;
    /** Mod-Enter: guardar y compilar */
    onCompile: (text: string) => void;
    onCursor: (line: number) => void;
    /** Ctrl/Cmd+clic: sincronizar con el PDF (SyncTeX forward) */
    onSyncClick: (line: number) => void;
  } = $props();

  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let shownPath = "";
  let syncing = false; // reemplazos programáticos no marcan el archivo como sucio
  let idleTimer: ReturnType<typeof setTimeout>;

  const opts = new Compartment();
  const isTex = $derived(path.endsWith(".tex"));

  /** Texto actual del buffer (para compilar sin perder cambios sin guardar). */
  export function getText(): string | undefined {
    return view?.state.doc.toString();
  }

  /** Envuelve la selección en sintaxis LaTeX (Mod-b / Mod-i / toolbar). */
  function wrapSelection(v: EditorView, before: string, after: string) {
    v.dispatch(
      v.state.changeByRange((range) => ({
        changes: [
          { from: range.from, insert: before },
          { from: range.to, insert: after },
        ],
        range: EditorSelection.range(range.from + before.length, range.to + before.length),
      })),
    );
  }

  // ---- barra de herramientas: envolturas/plantillas LaTeX (Código y Visual) ----
  interface Wrap {
    key: MsgKey;
    before: string;
    after: string;
  }
  const HEADINGS: Wrap[] = [
    { key: "tbSection", before: "\\section{", after: "}" },
    { key: "tbSubsection", before: "\\subsection{", after: "}" },
    { key: "tbSubsubsection", before: "\\subsubsection{", after: "}" },
  ];
  const MATHS: Wrap[] = [
    { key: "tbMathInline", before: "$", after: "$" },
    { key: "tbMathDisplay", before: "\\[\n  ", after: "\n\\]\n" },
    { key: "tbMathEq", before: "\\begin{equation}\n  ", after: "\n\\end{equation}\n" },
    { key: "tbMathFrac", before: "\\frac{", after: "}{}" },
    { key: "tbMathSqrt", before: "\\sqrt{", after: "}" },
    { key: "tbMathPow", before: "^{", after: "}" },
    { key: "tbMathSub", before: "_{", after: "}" },
    { key: "tbMathSum", before: "\\sum_{i=1}^{n} ", after: "" },
    { key: "tbMathInt", before: "\\int_{a}^{b} ", after: "" },
    { key: "tbMathLim", before: "\\lim_{x \\to \\infty} ", after: "" },
    {
      key: "tbMathMatrix",
      before: "\\begin{pmatrix}\n  a & b \\\\\n  c & d\n\\end{pmatrix}",
      after: "",
    },
  ];
  const ITEMIZE: Wrap = { key: "tbBullets", before: "\\begin{itemize}\n  \\item ", after: "\n\\end{itemize}\n" };
  const ENUMERATE: Wrap = { key: "tbNumbered", before: "\\begin{enumerate}\n  \\item ", after: "\n\\end{enumerate}\n" };
  const TABLE: Wrap = {
    key: "tbTable",
    before: "\\begin{tabular}{|c|c|c|}\n  \\hline\n  ",
    after: " &  &  \\\\\n  \\hline\n   &  &  \\\\\n  \\hline\n\\end{tabular}\n",
  };
  const IMAGE: Wrap = { key: "tbImage", before: "\\includegraphics[width=0.8\\textwidth]{", after: "}" };

  function toolWrap(before: string, after: string) {
    if (!view) return;
    wrapSelection(view, before, after);
    view.focus();
  }
  function toolPick(e: Event, items: Wrap[]) {
    const sel = e.currentTarget as HTMLSelectElement;
    const it = items[+sel.value];
    sel.value = "";
    if (it) toolWrap(it.before, it.after);
  }
  /** \href{url}{texto}: la selección pasa a ser el texto; el cursor, a la URL. */
  function toolLink() {
    if (!view) return;
    const { from, to } = view.state.selection.main;
    const text = view.state.sliceDoc(from, to);
    view.dispatch({
      changes: { from, to, insert: `\\href{}{${text}}` },
      selection: { anchor: from + "\\href{".length },
    });
    view.focus();
  }
  function toolCmd(fn: (v: EditorView) => unknown) {
    if (!view) return;
    fn(view);
    view.focus();
  }

  /** Extensiones que dependen de la configuración del usuario. */
  function optExts() {
    return [
      settings.vim ? vim() : [], // debe preceder al resto de keymaps
      settings.autocomplete ? [autocompletion(), keymap.of(completionKeymap)] : [],
      settings.closeBrackets ? [closeBrackets(), keymap.of(closeBracketsKeymap)] : [],
      drawSelection({ cursorBlinkRate: settings.steadyCursor ? 0 : 1200 }),
      settings.mathPreview ? mathPreview : [],
      visual
        ? visualLatex({
            preamble: (n) => t("preambleLines", n),
            references: () => t("references"),
          })
        : [],
      EditorView.contentAttributes.of({
        spellcheck: String(settings.spellcheck),
        lang: settings.spellLang,
      }),
      resolveTheme(settings.editorTheme, dark).ext,
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
        opts.of(optExts()),
        keymap.of([
          {
            key: "Mod-s",
            run: (v) => {
              onSave(v.state.doc.toString());
              return true;
            },
          },
          {
            // antes que defaultKeymap: sin esto Enter inserta salto de línea
            key: "Mod-Enter",
            run: (v) => {
              onCompile(v.state.doc.toString());
              return true;
            },
          },
          {
            key: "Mod-b",
            run: (v) => {
              wrapSelection(v, "\\textbf{", "}");
              return true;
            },
          },
          {
            key: "Mod-i",
            run: (v) => {
              wrapSelection(v, "\\textit{", "}");
              return true;
            },
          },
        ]),
        // equivalente a basicSetup, desmontado para poder configurar sus piezas
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
        // autocompletado de \ref/\cite/comandos/entornos + snippets
        latex.data.of({ autocomplete: latexComplete }),
        EditorView.lineWrapping,
        EditorView.updateListener.of((u) => {
          if (u.docChanged && !syncing) {
            onDirty();
            if (auto) {
              clearTimeout(idleTimer);
              idleTimer = setTimeout(() => onIdle(u.view.state.doc.toString()), 450);
            }
          }
          if (u.selectionSet || u.docChanged) {
            onCursor(u.state.doc.lineAt(u.state.selection.main.head).number);
          }
        }),
        EditorView.domEventHandlers({
          mousedown(e, v) {
            if (!(e.ctrlKey || e.metaKey)) return false;
            const pos = v.posAtCoords({ x: e.clientX, y: e.clientY });
            if (pos == null) return false;
            onSyncClick(v.state.doc.lineAt(pos).number);
            return true;
          },
        }),
      ],
    });
  }

  onMount(() => {
    view = new EditorView({ state: makeState(content), parent: host });
    shownPath = path;
  });

  onDestroy(() => {
    clearTimeout(idleTimer);
    view?.destroy();
  });

  // Salto a línea (desde la tabla de problemas).
  let gotoSeen = 0;
  $effect(() => {
    if (!view || !goto || goto.seq === gotoSeen) return;
    gotoSeen = goto.seq;
    const line = Math.min(goto.line, view.state.doc.lines);
    const pos = view.state.doc.line(line).from;
    view.dispatch({
      selection: { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: "center" }),
    });
    view.focus();
  });

  // Cambio de archivo -> estado nuevo; cambio externo del mismo archivo -> reemplazo del doc.
  $effect(() => {
    if (!view) return;
    if (path !== shownPath) {
      shownPath = path;
      view.setState(makeState(content));
    } else if (content !== view.state.doc.toString()) {
      syncing = true;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: content },
      });
      syncing = false;
    }
  });

  // configuración del usuario + tema (lee `settings` y `dark`)
  $effect(() => {
    view?.dispatch({ effects: opts.reconfigure(optExts()) });
  });
</script>

<div class="wrap">
  {#if isTex}
    <div class="latexbar">
      <button title={t("tbUndo")} onclick={() => toolCmd(undo)}>↶</button>
      <button title={t("tbRedo")} onclick={() => toolCmd(redo)}>↷</button>
      <span class="tsep"></span>
      <button class="bf" title={t("tbBold")} onclick={() => toolWrap("\\textbf{", "}")}>B</button>
      <button class="it" title={t("tbItalic")} onclick={() => toolWrap("\\textit{", "}")}>I</button>
      <button class="ul" title={t("tbUnderline")} onclick={() => toolWrap("\\underline{", "}")}>U</button>
      <span class="tsep"></span>
      <select title={t("tbHeadings")} onchange={(e) => toolPick(e, HEADINGS)}>
        <option value="" disabled selected>{t("tbHeadings")}</option>
        {#each HEADINGS as h, i (h.key)}<option value={i}>{t(h.key)}</option>{/each}
      </select>
      <button title={t(ITEMIZE.key)} onclick={() => toolWrap(ITEMIZE.before, ITEMIZE.after)}>•≡</button>
      <button title={t(ENUMERATE.key)} onclick={() => toolWrap(ENUMERATE.before, ENUMERATE.after)}>1.</button>
      <span class="tsep"></span>
      <select title={t("tbMath")} onchange={(e) => toolPick(e, MATHS)}>
        <option value="" disabled selected>ƒ(x)</option>
        {#each MATHS as m, i (m.key)}<option value={i}>{t(m.key)}</option>{/each}
      </select>
      <button title={t("tbLink")} onclick={toolLink}>🔗</button>
      <button title={t(TABLE.key)} onclick={() => toolWrap(TABLE.before, TABLE.after)}>⊞</button>
      <button title={t(IMAGE.key)} onclick={() => toolWrap(IMAGE.before, IMAGE.after)}>🖼</button>
    </div>
  {/if}
  <div class="editor" bind:this={host}></div>
</div>

<style>
  .wrap {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .latexbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px;
    flex: none;
    padding: 3px 8px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .latexbar button {
    min-width: 26px;
    height: 24px;
    font-size: 13px;
    color: var(--fg-dim);
  }
  .latexbar button:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .latexbar .bf {
    font-weight: 700;
  }
  .latexbar .it {
    font-style: italic;
  }
  .latexbar .ul {
    text-decoration: underline;
  }
  .latexbar select {
    font: inherit;
    font-size: 12px;
    height: 24px;
    color: var(--fg-dim);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0 4px;
    max-width: 110px;
  }
  .tsep {
    width: 1px;
    height: 16px;
    background: var(--border);
    margin: 0 4px;
    flex: none;
  }
  .editor {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .editor :global(.cm-editor) {
    height: 100%;
  }
  .editor :global(.cm-math-preview) {
    padding: 4px 10px;
    max-width: 480px;
    overflow-x: auto;
  }
</style>
