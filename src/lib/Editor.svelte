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
    tooltips,
  } from "@codemirror/view";
  import { Compartment, EditorSelection, EditorState, type Text } from "@codemirror/state";
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
  import { forEachDiagnostic, lintGutter, setDiagnostics, type Diagnostic } from "@codemirror/lint";
  import { vim } from "@replit/codemirror-vim";
  import { stex } from "@codemirror/legacy-modes/mode/stex";
  import { resolveTheme } from "./editorThemes";
  import { latexComplete } from "./latexComplete";
  import { mathPreview } from "./mathPreview";
  import { visualLatex } from "./visualLatex";
  import { errorLens } from "./errorLens";
  import { hintFor } from "./issueHints";
  import { lspExtension, setLspView } from "./lsp";
  import type { Issue } from "./ipc";
  import { app, settings } from "./state.svelte";
  import { t, type MsgKey } from "./i18n.svelte";
  import {
    Bold,
    Image,
    Italic,
    Link,
    List,
    ListOrdered,
    Redo2,
    Table,
    Underline,
    Undo2,
  } from "@lucide/svelte";
  import "katex/dist/katex.min.css";

  const latex = StreamLanguage.define(stex);

  let {
    path,
    content,
    dark,
    auto = false,
    visual = false,
    readonly = false,
    goto = null,
    onSave,
    onDirty,
    onIdle,
    onCompile,
    onCursor,
    onSyncClick,
    onDocChange,
  }: {
    path: string;
    content: string;
    dark: boolean;
    auto?: boolean;
    /** modo Visual: sintaxis LaTeX oculta/renderizada en línea */
    visual?: boolean;
    /** preview: no edita, sin barra, sin cursor-reveal */
    readonly?: boolean;
    goto?: { line: number; col: number; seq: number } | null;
    onSave?: (text: string) => void;
    onDirty?: () => void;
    /** ~450ms sin teclear con vista previa automática activa */
    onIdle?: (text: string) => void;
    /** Mod-Enter: guardar y compilar */
    onCompile?: (text: string) => void;
    onCursor?: (line: number) => void;
    /** Ctrl/Cmd+clic: sincronizar con el PDF (SyncTeX forward) */
    onSyncClick?: (line: number) => void;
    onDocChange?: (text: string) => void;
  } = $props();

  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let shownPath = "";
  let syncing = false; // reemplazos programáticos no marcan el archivo como sucio
  let idleTimer: ReturnType<typeof setTimeout>;

  const opts = new Compartment();
  const isTex = $derived(path.endsWith(".tex"));

  // texlab: solo el editor editable de fuentes LaTeX se conecta al servidor
  const lspComp = new Compartment();
  let lspSeen = -1; // sesión de texlab con la que se construyó el estado
  function lspExt() {
    lspSeen = app.lspSession;
    return !readonly && /\.(tex|bib|sty|cls)$/.test(path) ? lspExtension(path) : [];
  }

  // ---- problemas dentro del editor (sintaxis en vivo + última compilación) ----
  type Diag = Diagnostic & { compile?: boolean };
  function toDiagnostic(doc: Text, i: Issue): Diag | null {
    if (!i.line) return null;
    const line = doc.line(Math.min(i.line, doc.lines));
    let from: number;
    let to: number;
    if (i.col != null) {
      // texlab da el rango exacto
      const end = doc.line(Math.min(i.endLine ?? i.line, doc.lines));
      from = Math.min(line.from + i.col, line.to);
      to = Math.max(from, Math.min(end.from + (i.endCol ?? i.col), end.to));
    } else {
      // el log solo da la línea; `context` es esa línea hasta donde TeX se
      // detuvo, así que su último símbolo es el culpable
      from = line.from + (line.text.length - line.text.trimStart().length);
      to = line.to;
      const tail = i.context?.trimEnd().slice(-24);
      const at = tail ? line.text.indexOf(tail) : -1;
      if (tail && at >= 0) {
        to = line.from + at + tail.length;
        const tok = /\\[a-zA-Z@]+$|\S$/.exec(line.text.slice(0, at + tail.length));
        from = line.from + (tok ? tok.index : at);
      }
    }
    const hint = hintFor(i.message);
    return {
      from,
      to,
      severity: i.severity,
      message: hint ? `${i.message}\n${t(hint)}` : i.message,
      source: t(i.source === "syntax" ? "srcSyntax" : "srcCompile"),
      compile: i.source !== "syntax",
    };
  }

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
      readonly ? [EditorState.readOnly.of(true), EditorView.editable.of(false)] : [],
      !readonly && settings.vim ? vim() : [], // debe preceder al resto de keymaps
      !readonly && settings.autocomplete ? [autocompletion(), keymap.of(completionKeymap)] : [],
      !readonly && settings.closeBrackets ? [closeBrackets(), keymap.of(closeBracketsKeymap)] : [],
      drawSelection({ cursorBlinkRate: settings.steadyCursor ? 0 : 1200 }),
      !readonly && settings.mathPreview ? mathPreview : [],
      !readonly && settings.inlineErrors ? errorLens : [],
      visual
        ? visualLatex({
            preamble: (n) => t("preambleLines", n),
            references: () => t("references"),
            reveal: !readonly,
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
        lspComp.of(lspExt()),
        // en el body: dentro del panel, el autocompletado y los tooltips de
        // error se cortaban contra el visor PDF
        tooltips({ parent: document.body }),
        keymap.of([
          {
            key: "Mod-s",
            run: (v) => {
              onSave?.(v.state.doc.toString());
              return true;
            },
          },
          {
            // antes que defaultKeymap: sin esto Enter inserta salto de línea
            key: "Mod-Enter",
            run: (v) => {
              onCompile?.(v.state.doc.toString());
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
        readonly ? [] : lintGutter(),
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
            onDocChange?.(u.view.state.doc.toString());
            if (!readonly) {
              onDirty?.();
              if (auto && onIdle) {
                clearTimeout(idleTimer);
                idleTimer = setTimeout(() => onIdle(u.view.state.doc.toString()), 450);
              }
            }
          }
          if (!readonly && (u.selectionSet || u.docChanged)) {
            onCursor?.(u.state.doc.lineAt(u.state.selection.main.head).number);
          }
        }),
        EditorView.domEventHandlers({
          mousedown(e, v) {
            if (readonly || !(e.ctrlKey || e.metaKey) || !onSyncClick) return false;
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
    if (!readonly) setLspView(view);
  });

  onDestroy(() => {
    if (!readonly) setLspView(null);
    clearTimeout(idleTimer);
    view?.destroy();
  });

  // Salto a línea (desde la tabla de problemas).
  let gotoSeen = 0;
  $effect(() => {
    if (!view || !goto || goto.seq === gotoSeen) return;
    gotoSeen = goto.seq;
    const line = Math.min(goto.line, view.state.doc.lines);
    const l = view.state.doc.line(line);
    const pos = Math.min(l.from + goto.col, l.to);
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

  // texlab arrancó o cayó: reconectar el archivo abierto
  $effect(() => {
    if (!view || app.lspSession === lspSeen) return;
    view.dispatch({ effects: lspComp.reconfigure(lspExt()) });
  });

  // Vuelca los problemas del archivo al estado de lint (subrayado, margen,
  // tooltip y mensaje en línea). Va después del efecto de cambio de archivo:
  // un estado recién creado empieza sin diagnósticos.
  let diagPath = "";
  let diagCompile: Issue[] | undefined;
  $effect(() => {
    if (!view || readonly) return;
    const doc = view.state.doc;
    const diags = app.lspIssues
      .filter((i) => i.file === path)
      .map((i) => toDiagnostic(doc, i))
      .filter((d) => d !== null);
    if (path !== diagPath || app.issues !== diagCompile) {
      diagPath = path;
      diagCompile = app.issues;
      for (const i of app.issues) {
        const d = i.file === path ? toDiagnostic(doc, i) : null;
        if (d) diags.push(d);
      }
    } else {
      // misma compilación: sus números de línea ya son viejos, pero lint los
      // ha ido moviendo con cada edición; se conservan donde están
      forEachDiagnostic(view.state, (d: Diag, from, to) => {
        if (d.compile) diags.push({ ...d, from, to });
      });
    }
    view.dispatch(setDiagnostics(view.state, diags));
  });

  // configuración del usuario + tema (lee `settings` y `dark`)
  $effect(() => {
    view?.dispatch({ effects: opts.reconfigure(optExts()) });
  });
</script>

<div class="wrap" class:visual>
  {#if isTex && !readonly}
    <div class="latexbar">
      <button title={t("tbUndo")} aria-label={t("tbUndo")} onclick={() => toolCmd(undo)}><Undo2 /></button>
      <button title={t("tbRedo")} aria-label={t("tbRedo")} onclick={() => toolCmd(redo)}><Redo2 /></button>
      <span class="tsep"></span>
      <button title={t("tbBold")} aria-label={t("tbBold")} onclick={() => toolWrap("\\textbf{", "}")}><Bold /></button>
      <button title={t("tbItalic")} aria-label={t("tbItalic")} onclick={() => toolWrap("\\textit{", "}")}><Italic /></button>
      <button title={t("tbUnderline")} aria-label={t("tbUnderline")} onclick={() => toolWrap("\\underline{", "}")}><Underline /></button>
      <span class="tsep"></span>
      <select title={t("tbHeadings")} onchange={(e) => toolPick(e, HEADINGS)}>
        <option value="" disabled selected>{t("tbHeadings")}</option>
        {#each HEADINGS as h, i (h.key)}<option value={i}>{t(h.key)}</option>{/each}
      </select>
      <button title={t(ITEMIZE.key)} aria-label={t(ITEMIZE.key)} onclick={() => toolWrap(ITEMIZE.before, ITEMIZE.after)}><List /></button>
      <button title={t(ENUMERATE.key)} aria-label={t(ENUMERATE.key)} onclick={() => toolWrap(ENUMERATE.before, ENUMERATE.after)}><ListOrdered /></button>
      <span class="tsep"></span>
      <select title={t("tbMath")} onchange={(e) => toolPick(e, MATHS)}>
        <option value="" disabled selected>{t("tbMathShort")}</option>
        {#each MATHS as m, i (m.key)}<option value={i}>{t(m.key)}</option>{/each}
      </select>
      <button title={t("tbLink")} aria-label={t("tbLink")} onclick={toolLink}><Link /></button>
      <button title={t(TABLE.key)} aria-label={t(TABLE.key)} onclick={() => toolWrap(TABLE.before, TABLE.after)}><Table /></button>
      <button title={t(IMAGE.key)} aria-label={t(IMAGE.key)} onclick={() => toolWrap(IMAGE.before, IMAGE.after)}><Image /></button>
    </div>
  {/if}
  <div class="editor" bind:this={host}></div>
</div>

<style>
  .wrap {
    height: 100%;
    /* bajo la barra de migas: sin esto WebKit no lo encoge y el final del
       editor queda recortado, fuera del alcance del scroll */
    flex: 1;
    min-height: 0;
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
    width: 26px;
    height: 24px;
    padding: 0;
    display: grid;
    place-items: center;
    color: var(--fg-dim);
  }
  .latexbar button:hover {
    color: var(--fg);
    background: var(--hover);
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
