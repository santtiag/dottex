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
  import { latexComplete } from "./latexComplete";
  import { mathPreview } from "./mathPreview";
  import { settings } from "./state.svelte";
  import "katex/dist/katex.min.css";

  const latex = StreamLanguage.define(stex);

  let {
    path,
    content,
    dark,
    auto,
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

  /** Envuelve la selección en sintaxis LaTeX (Mod-b / Mod-i). */
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

  /** Extensiones que dependen de la configuración del usuario. */
  function optExts() {
    const edDark = settings.editorTheme === "auto" ? dark : settings.editorTheme === "dark";
    return [
      settings.vim ? vim() : [], // debe preceder al resto de keymaps
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

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    height: 100%;
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
