/**
 * Modo Visual (WYSIWYG): decoraciones de CodeMirror que ocultan la sintaxis
 * LaTeX y renderizan en línea títulos, estilos de texto, matemáticas (KaTeX),
 * imágenes, listas y tablas sobre el MISMO documento editable. El elemento
 * que toca la selección se muestra como fuente cruda ("cursor reveal"):
 * entrar con el cursor revela el LaTeX, salir lo vuelve a renderizar.
 *
 * Parsing por regex + emparejado de llaves, como el resto del codebase
 * (breadcrumbs, outline, latexMath).
 */
import {
  type ChangeDesc,
  type EditorState,
  type Extension,
  type Range,
  StateEffect,
  StateField,
} from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from "@codemirror/view";
import katex from "katex";
import { readFileBytes } from "./ipc";
import { MATH_RE, mathFromMatch } from "./latexMath";

// ---------------------------------------------------------------------------
// Escáner puro (testeable en node, sin DOM)
// ---------------------------------------------------------------------------

export interface TitleField {
  cls: string;
  src: string;
  /** posición del contenido en el doc (−1 si el comando no existe) */
  pos: number;
}

export type WidgetSpec =
  | { type: "math"; src: string; display: boolean }
  | { type: "image"; path: string }
  | { type: "table"; body: string }
  | { type: "item"; label: string }
  | { type: "preamble"; lines: number }
  | { type: "title"; fields: TitleField[] }
  | { type: "bibhead" };

export type Prim =
  | { kind: "hide"; from: number; to: number }
  | { kind: "mark"; from: number; to: number; cls: string }
  | { kind: "widget"; from: number; to: number; spec: WidgetSpec; block?: boolean };

export interface VisualElement {
  from: number;
  to: number;
  prims: Prim[];
}

const lineStart = (t: string, i: number) => t.lastIndexOf("\n", i - 1) + 1;
const lineEnd = (t: string, i: number) => {
  const n = t.indexOf("\n", i);
  return n < 0 ? t.length : n;
};

/** Posición del `}` que cierra la llave abierta en `open` (o -1). */
function matchBrace(text: string, open: number): number {
  let depth = 1;
  for (let i = open + 1; i < text.length; i++) {
    const c = text[i];
    if (c === "\\") i++;
    else if (c === "{") depth++;
    else if (c === "}" && --depth === 0) return i;
  }
  return -1;
}

/** Si el token queda solo en su línea, traga la línea entera (con su salto)
 *  para no dejar líneas vacías fantasma al ocultarlo. */
function fullLine(text: string, from: number, to: number): [number, number] {
  let a = from;
  while (a > 0 && (text[a - 1] === " " || text[a - 1] === "\t")) a--;
  let b = to;
  while (b < text.length && (text[b] === " " || text[b] === "\t")) b++;
  if ((a === 0 || text[a - 1] === "\n") && (b === text.length || text[b] === "\n"))
    return [a, Math.min(b + 1, text.length)];
  return [from, to];
}

// comandos con argumento cuyo contenido queda editable con una clase de estilo
const STYLED_RE =
  /\\(subsubsection|subsection|section|textbf|textit|emph|underline|texttt|caption)\*?\s*\{/g;
const STYLE_CLASS: Record<string, string> = {
  section: "cm-vl-sec1",
  subsection: "cm-vl-sec2",
  subsubsection: "cm-vl-sec3",
  textbf: "cm-vl-bf",
  textit: "cm-vl-it",
  emph: "cm-vl-it",
  underline: "cm-vl-ul",
  texttt: "cm-vl-tt",
  caption: "cm-vl-caption",
};

// referencias y citas: se muestra solo la clave como "chip"
const REF_RE = /\\(ref|eqref|autoref|cref|Cref|cite|citep|citet)\*?\s*\{([^}]*)\}/g;

const IMG_RE = /\\includegraphics\s*(?:\[[^\]]*\])?\s*\{([^}]+)\}/g;

const LIST_RE = /\\begin\{(itemize|enumerate)\}|\\end\{(itemize|enumerate)\}|\\item(?![A-Za-z])(?:\[([^\]]*)\])?[ \t]?/g;
const BULLETS = ["•", "◦", "▪"];

const TAB_RE = /\\begin\{tabular\}\s*(?:\[[^\]]*\])?\s*\{/g;

// tokens sin representación visual: se ocultan por completo
const HIDE_RE =
  /\\(?:begin|end)\{(?:figure\*?|table\*?|center|abstract)\}(?:\[[^\]]*\])?|\\(?:centering|noindent|tableofcontents|newpage|clearpage|pagebreak|bigskip|medskip|smallskip|newline|par)(?![A-Za-z])|\\label\{[^}]*\}|\\bibliographystyle\s*\{[^}]*\}|\\(?:vspace|hspace)\*?\{[^}]*\}|\\\\\*?(?:\[[^\]]*\])?|(?<!\\)%[^\n]*/g;

// bibliografía: encabezado, \bibitem numerado y \bibliography (BibTeX)
const BIB_RE =
  /\\begin\{thebibliography\}\s*\{[^}]*\}|\\end\{thebibliography\}|\\bibitem\s*(?:\[([^\]]*)\])?\s*\{[^}]*\}[ \t]?|\\bibliography\s*\{[^}]*\}/g;

export function scanVisual(text: string): VisualElement[] {
  const els: VisualElement[] = [];
  const push = (from: number, to: number, prims: Prim[]) => {
    if (to > from) els.push({ from, to, prims });
  };

  // preámbulo (hasta \begin{document}) y cola tras \end{document}: el cuerpo
  // de las demás pasadas se limita a lo que hay entre ambos
  let bodyFrom = 0;
  let bodyTo = text.length;
  const begin = /\\begin\{document\}/.exec(text);
  if (begin) {
    const to = lineEnd(text, begin.index + begin[0].length);
    const lines = text.slice(0, to).split("\n").length;
    push(0, to, [{ kind: "widget", from: 0, to, spec: { type: "preamble", lines }, block: true }]);
    bodyFrom = to;
  }
  const end = text.indexOf("\\end{document}", bodyFrom);
  if (end >= 0) {
    const from = lineStart(text, end);
    push(from, text.length, [{ kind: "hide", from, to: text.length }]);
    bodyTo = from;
  }

  const off = bodyFrom;
  const body = text.slice(bodyFrom, bodyTo);

  // metadatos de portada (preámbulo o cuerpo): alimentan \maketitle; la
  // posición del contenido permite saltar a editarlos con un clic en la portada
  const meta: Record<string, { src: string; pos: number }> = {};
  for (const m of text.matchAll(/\\(title|author|date|institute)\s*\{/g)) {
    const close = matchBrace(text, m.index + m[0].length - 1);
    if (close >= 0)
      meta[m[1]] = { src: text.slice(m.index + m[0].length, close), pos: m.index + m[0].length };
  }

  // \maketitle → portada renderizada (título, autor, institución, fecha)
  const none = { src: "", pos: -1 };
  for (const m of body.matchAll(/\\maketitle(?![A-Za-z])/g)) {
    const from = off + m.index;
    const to = from + m[0].length;
    push(from, to, [
      {
        kind: "widget",
        from,
        to,
        spec: {
          type: "title",
          fields: [
            { cls: "cm-vl-title", ...(meta.title ?? none) },
            { cls: "cm-vl-author", ...(meta.author ?? none) },
            { cls: "cm-vl-inst", ...(meta.institute ?? none) },
            { cls: "cm-vl-date", ...(meta.date ?? { src: "\\today", pos: -1 }) },
          ],
        },
      },
    ]);
  }

  // metadatos de portada en el cuerpo: sin representación propia (los muestra
  // \maketitle); incluye los running heads de llncs y similares
  for (const m of body.matchAll(
    /\\(?:title|titlerunning|subtitle|author|authorrunning|date|institute)\s*\{/g,
  )) {
    const close = matchBrace(body, m.index + m[0].length - 1);
    if (close < 0) continue;
    const [f, t] = fullLine(body, m.index, close + 1);
    push(off + f, off + t, [{ kind: "hide", from: off + f, to: off + t }]);
  }

  // bibliografía: encabezado + entradas [n]
  let bibN = 0;
  for (const m of body.matchAll(BIB_RE)) {
    if (m[0].startsWith("\\bibitem")) {
      // fullLine: si el comando va solo en su línea, la etiqueta [n] se une al texto siguiente
      const [f, t] = fullLine(body, m.index, m.index + m[0].length);
      const label = `[${m[1] ?? ++bibN}]`;
      push(off + f, off + t, [
        { kind: "widget", from: off + f, to: off + t, spec: { type: "item", label } },
      ]);
    } else if (m[0].startsWith("\\end")) {
      const [f, t] = fullLine(body, m.index, m.index + m[0].length);
      push(off + f, off + t, [{ kind: "hide", from: off + f, to: off + t }]);
    } else {
      // \begin{thebibliography} o \bibliography{…} → encabezado "Referencias"
      const from = off + m.index;
      const to = from + m[0].length;
      push(from, to, [{ kind: "widget", from, to, spec: { type: "bibhead" } }]);
    }
  }

  // matemáticas → widget KaTeX
  for (const m of body.matchAll(MATH_RE)) {
    const { src, display } = mathFromMatch(m);
    const from = off + m.index;
    const to = from + m[0].length;
    push(from, to, [{ kind: "widget", from, to, spec: { type: "math", src, display } }]);
  }

  // tabular → widget de tabla (solo lectura; el cursor dentro revela la fuente)
  for (const m of body.matchAll(TAB_RE)) {
    const colsEnd = matchBrace(body, m.index + m[0].length - 1);
    if (colsEnd < 0) continue;
    const endTok = body.indexOf("\\end{tabular}", colsEnd);
    if (endTok < 0) continue;
    // sin fullLine: conservar el salto de línea final para no pegar el texto siguiente
    const from = off + m.index;
    const to = off + endTok + "\\end{tabular}".length;
    const inner = body.slice(colsEnd + 1, endTok);
    push(from, to, [{ kind: "widget", from, to, spec: { type: "table", body: inner } }]);
  }

  // imágenes
  for (const m of body.matchAll(IMG_RE)) {
    const from = off + m.index;
    const to = from + m[0].length;
    push(from, to, [{ kind: "widget", from, to, spec: { type: "image", path: m[1].trim() } }]);
  }

  // secciones, estilos de texto y captions: ocultar comando y llaves,
  // el contenido queda editable con su clase
  for (const m of body.matchAll(STYLED_RE)) {
    const open = m.index + m[0].length - 1;
    const close = matchBrace(body, open);
    if (close < 0) continue;
    push(off + m.index, off + close + 1, [
      { kind: "hide", from: off + m.index, to: off + open + 1 },
      { kind: "mark", from: off + open + 1, to: off + close, cls: STYLE_CLASS[m[1]] },
      { kind: "hide", from: off + close, to: off + close + 1 },
    ]);
  }

  // \ref/\cite → chip con la clave
  for (const m of body.matchAll(REF_RE)) {
    const from = off + m.index;
    const to = from + m[0].length;
    push(from, to, [
      { kind: "hide", from, to: to - m[2].length - 1 },
      { kind: "mark", from: to - m[2].length - 1, to: to - 1, cls: "cm-vl-chip" },
      { kind: "hide", from: to - 1, to },
    ]);
  }

  // listas: begin/end ocultos, \item → viñeta o número (pasada secuencial
  // para llevar contadores y profundidad)
  const stack: { type: string; n: number }[] = [];
  for (const m of body.matchAll(LIST_RE)) {
    const [f, t] = fullLine(body, m.index, m.index + m[0].length);
    if (m[1]) {
      stack.push({ type: m[1], n: 0 });
      push(off + f, off + t, [{ kind: "hide", from: off + f, to: off + t }]);
    } else if (m[2]) {
      stack.pop();
      push(off + f, off + t, [{ kind: "hide", from: off + f, to: off + t }]);
    } else {
      const top = stack[stack.length - 1];
      const label =
        m[3] ?? (top?.type === "enumerate" ? `${++top.n}.` : BULLETS[Math.max(0, stack.length - 1) % 3]);
      const from = off + m.index;
      const to = off + m.index + m[0].length;
      push(from, to, [{ kind: "widget", from, to, spec: { type: "item", label } }]);
    }
  }

  // tokens cosméticos, \label y comentarios: ocultos
  for (const m of body.matchAll(HIDE_RE)) {
    const [f, t] = fullLine(body, m.index, m.index + m[0].length);
    push(off + f, off + t, [{ kind: "hide", from: off + f, to: off + t }]);
  }

  // resolución de solapes: orden por posición; anidado se acepta, solape
  // parcial gana el que empieza antes (p. ej. comentario dentro de una ecuación)
  els.sort((a, b) => a.from - b.from || b.to - a.to);
  const accepted: VisualElement[] = [];
  const open: VisualElement[] = [];
  for (const e of els) {
    while (open.length && open[open.length - 1].to <= e.from) open.pop();
    const top = open[open.length - 1];
    if (top && e.to > top.to) continue;
    accepted.push(e);
    open.push(e);
  }
  return accepted;
}

/** Cuerpo de un tabular → matriz de celdas (filas por \\, columnas por &). */
export function parseTabular(body: string): string[][] {
  return body
    .split(/\\\\(?:\[[^\]]*\])?/)
    .map((r) => r.replace(/\\(hline|toprule|midrule|bottomrule)(?![A-Za-z])/g, "").trim())
    .filter((r) => r.length)
    .map((r) => r.split(/(?<!\\)&/).map((c) => c.trim()));
}

// ---------------------------------------------------------------------------
// Render (DOM): widgets y utilidades
// ---------------------------------------------------------------------------

const katexCache = new Map<string, string>();
function katexHtml(src: string, display: boolean): string {
  const key = (display ? "D" : "I") + src;
  let html = katexCache.get(key);
  if (html === undefined) {
    html = katex.renderToString(src, { displayMode: display, throwOnError: false });
    // ponytail: evicción FIFO (Map conserva orden de inserción); LRU si hiciera falta
    if (katexCache.size > 500) katexCache.delete(katexCache.keys().next().value!);
    katexCache.set(key, html);
  }
  return html;
}

const esc = (s: string) =>
  s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]!);

/** Texto plano con las matemáticas embebidas renderizadas (celdas de tabla). */
function inlineHtml(src: string): string {
  let out = "";
  let last = 0;
  for (const m of src.matchAll(MATH_RE)) {
    out += esc(src.slice(last, m.index));
    out += katexHtml(mathFromMatch(m).src, false);
    last = m.index + m[0].length;
  }
  return out + esc(src.slice(last));
}

/** Clic en un widget: coloca la selección dentro de su rango → cursor reveal. */
function revealOnClick(view: EditorView, el: HTMLElement) {
  el.onmousedown = (e) => {
    e.preventDefault();
    const pos = view.posAtDOM(el);
    view.dispatch({
      selection: { anchor: Math.min(pos + 1, view.state.doc.length) },
      scrollIntoView: true,
    });
    view.focus();
  };
}

// ponytail: rutas relativas a la raíz del proyecto; no se resuelve
// \graphicspath ni PDF/EPS — placeholder con el nombre si no carga.
const imgCache = new Map<string, Promise<string>>();

/** Revoca y vacía el caché de imágenes (al cambiar archivos en disco). */
export function clearImgCache() {
  for (const p of imgCache.values()) {
    p.then((url) => URL.revokeObjectURL(url)).catch(() => {});
  }
  imgCache.clear();
}
function imageUrl(path: string): Promise<string> {
  let p = imgCache.get(path);
  if (!p) {
    p = (async () => {
      for (const cand of [path, `${path}.png`, `${path}.jpg`, `${path}.jpeg`]) {
        try {
          return URL.createObjectURL(new Blob([await readFileBytes(cand)]));
        } catch {
          /* siguiente candidato */
        }
      }
      throw new Error(path);
    })();
    imgCache.set(path, p);
  }
  return p;
}

class MathWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly display: boolean,
  ) {
    super();
  }
  eq(o: MathWidget) {
    return o.src === this.src && o.display === this.display;
  }
  toDOM(view: EditorView) {
    const el = document.createElement("span");
    el.className = this.display ? "cm-vl-math cm-vl-display" : "cm-vl-math";
    el.innerHTML = katexHtml(this.src, this.display);
    revealOnClick(view, el);
    return el;
  }
}

class ImageWidget extends WidgetType {
  constructor(readonly path: string) {
    super();
  }
  eq(o: ImageWidget) {
    return o.path === this.path;
  }
  toDOM(view: EditorView) {
    const el = document.createElement("span");
    el.className = "cm-vl-img";
    el.textContent = `⌧ ${this.path}`;
    imageUrl(this.path)
      .then((url) => {
        const img = document.createElement("img");
        img.src = url;
        img.alt = this.path;
        img.onload = () => view.requestMeasure();
        el.textContent = "";
        el.appendChild(img);
      })
      .catch(() => el.classList.add("cm-vl-missing"));
    revealOnClick(view, el);
    return el;
  }
}

class TableWidget extends WidgetType {
  constructor(readonly body: string) {
    super();
  }
  eq(o: TableWidget) {
    return o.body === this.body;
  }
  toDOM(view: EditorView) {
    const el = document.createElement("span");
    el.className = "cm-vl-table";
    const table = document.createElement("table");
    for (const row of parseTabular(this.body)) {
      const tr = table.insertRow();
      for (const cell of row) tr.insertCell().innerHTML = inlineHtml(cell);
    }
    el.appendChild(table);
    revealOnClick(view, el);
    return el;
  }
}

class ItemWidget extends WidgetType {
  constructor(readonly label: string) {
    super();
  }
  eq(o: ItemWidget) {
    return o.label === this.label;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = "cm-vl-item";
    el.textContent = `${this.label} `;
    return el;
  }
}

/** Contenido de \title/\author/… → HTML (separadores \\ y \and; \thanks/\inst/\orcidID fuera). */
function metaHtml(src: string): string {
  return inlineHtml(
    src
      .replace(/\\(thanks|inst|orcidID)\s*\{[^}]*\}/g, "")
      .replace(/\\email\s*\{([^}]*)\}/g, "$1")
      .replace(/\\\\\*?|\\and(?![A-Za-z])/g, " · "),
  );
}

class TitleWidget extends WidgetType {
  constructor(readonly fields: TitleField[]) {
    super();
  }
  eq(o: TitleWidget) {
    return JSON.stringify(o.fields) === JSON.stringify(this.fields);
  }
  toDOM(view: EditorView) {
    const el = document.createElement("div");
    el.className = "cm-vl-titleblock";
    for (const f of this.fields) {
      const src =
        f.cls === "cm-vl-date"
          ? f.src.replace(/\\today(?![A-Za-z])/g, new Date().toLocaleDateString())
          : f.src;
      if (!src.trim()) continue;
      const row = document.createElement("div");
      row.className = f.cls;
      row.innerHTML = metaHtml(src);
      if (f.pos >= 0) {
        // clic en la fila → cursor dentro del \title/\author/… correspondiente
        row.onmousedown = (e) => {
          e.preventDefault();
          e.stopPropagation();
          view.dispatch({ selection: { anchor: f.pos }, scrollIntoView: true });
          view.focus();
        };
      }
      el.appendChild(row);
    }
    revealOnClick(view, el);
    return el;
  }
}

class BibHeadWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM(view: EditorView) {
    const el = document.createElement("div");
    el.className = "cm-vl-bibhead";
    el.textContent = referencesLabel();
    revealOnClick(view, el);
    return el;
  }
}

let preambleLabel = (n: number) => `Preámbulo — ${n} líneas`;
let referencesLabel = () => "Referencias";

class PreambleWidget extends WidgetType {
  constructor(readonly lines: number) {
    super();
  }
  eq(o: PreambleWidget) {
    return o.lines === this.lines;
  }
  toDOM(view: EditorView) {
    const el = document.createElement("div");
    el.className = "cm-vl-preamble";
    el.textContent = `⚙ ${preambleLabel(this.lines)}`;
    revealOnClick(view, el);
    return el;
  }
}

function widgetOf(spec: WidgetSpec): WidgetType {
  switch (spec.type) {
    case "math":
      return new MathWidget(spec.src, spec.display);
    case "image":
      return new ImageWidget(spec.path);
    case "table":
      return new TableWidget(spec.body);
    case "item":
      return new ItemWidget(spec.label);
    case "preamble":
      return new PreambleWidget(spec.lines);
    case "title":
      return new TitleWidget(spec.fields);
    case "bibhead":
      return new BibHeadWidget();
  }
}

// ---------------------------------------------------------------------------
// StateField: decoraciones + cursor reveal
// ---------------------------------------------------------------------------

interface VState {
  els: VisualElement[];
  deco: DecorationSet;
  /** posiciones `from` de los elementos revelados por la selección */
  revealKey: string;
}

/** Elementos cuyo interior toca la selección → se muestran como fuente cruda. */
function revealKeyOf(state: EditorState, els: VisualElement[]): string {
  const sel = state.selection.ranges;
  let key = "";
  for (const el of els) {
    if (sel.some((r) => r.from < el.to && r.to > el.from)) key += el.from + ",";
  }
  return key;
}

function makeDeco(state: EditorState, els: VisualElement[]): VState {
  const sel = state.selection.ranges;
  const ranges: Range<Decoration>[] = [];
  let revealKey = "";
  for (const el of els) {
    // la selección toca el interior del elemento → se muestra la fuente cruda
    if (sel.some((r) => r.from < el.to && r.to > el.from)) {
      revealKey += el.from + ",";
      continue;
    }
    for (const p of el.prims) {
      if (p.to <= p.from) continue;
      if (p.kind === "hide") ranges.push(Decoration.replace({}).range(p.from, p.to));
      else if (p.kind === "mark") ranges.push(Decoration.mark({ class: p.cls }).range(p.from, p.to));
      else
        ranges.push(
          Decoration.replace({ widget: widgetOf(p.spec), block: p.block }).range(p.from, p.to),
        );
    }
  }
  return { els, deco: Decoration.set(ranges, true), revealKey };
}

/** Desplaza elementos y prims a través de una edición; descarta los borrados. */
function mapEls(els: VisualElement[], changes: ChangeDesc): VisualElement[] {
  const out: VisualElement[] = [];
  for (const el of els) {
    const from = changes.mapPos(el.from, 1);
    const to = changes.mapPos(el.to, -1);
    if (to <= from) continue;
    const prims: Prim[] = [];
    for (const p of el.prims) {
      const pf = changes.mapPos(p.from, 1);
      const pt = changes.mapPos(p.to, -1);
      if (pt > pf) prims.push({ ...p, from: pf, to: pt });
    }
    if (prims.length) out.push({ from, to, prims });
  }
  return out;
}

/** Resultado del re-escaneo diferido (lo despacha rescanPlugin). */
export const rescanEffect = StateEffect.define<VisualElement[]>();

// En cada edición solo se MAPEAN las decoraciones existentes (barato); el
// escaneo completo del documento corre con debounce en rescanPlugin. El
// elemento bajo el cursor ya está revelado mientras se escribe, así que el
// retraso no se nota.
export const visualField = StateField.define<VState>({
  create: (s) => makeDeco(s, scanVisual(s.doc.toString())),
  update(v, tr) {
    for (const e of tr.effects) {
      if (e.is(rescanEffect)) return makeDeco(tr.state, e.value);
    }
    if (tr.docChanged) {
      return { els: mapEls(v.els, tr.changes), deco: v.deco.map(tr.changes), revealKey: v.revealKey };
    }
    if (tr.selection) {
      // la mayoría de movimientos de cursor no cambian qué está revelado:
      // evitar reconstruir todas las decoraciones
      if (revealKeyOf(tr.state, v.els) === v.revealKey) return v;
      return makeDeco(tr.state, v.els);
    }
    return v;
  },
  provide: (f) => EditorView.decorations.from(f, (v) => v.deco),
});

/** Re-escaneo completo ~250ms después de la última edición. */
const rescanPlugin = ViewPlugin.define((view) => {
  let timer: ReturnType<typeof setTimeout> | null = null;
  return {
    update(u: ViewUpdate) {
      if (!u.docChanged) return;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = null;
        view.dispatch({ effects: rescanEffect.of(scanVisual(view.state.doc.toString())) });
      }, 250);
    },
    destroy() {
      if (timer) clearTimeout(timer);
    },
  };
});

const theme = EditorView.theme({
  ".cm-gutters": { display: "none" },
  ".cm-activeLine": { backgroundColor: "transparent" },
  // sin fontFamily aquí: manda la fuente del editor elegida en Configuración
  ".cm-content": {
    maxWidth: "74ch",
    margin: "0 auto",
    padding: "28px 16px",
  },
  ".cm-vl-sec1": { fontSize: "1.7em", fontWeight: "700" },
  ".cm-vl-sec2": { fontSize: "1.4em", fontWeight: "700" },
  ".cm-vl-sec3": { fontSize: "1.15em", fontWeight: "700" },
  ".cm-vl-bf": { fontWeight: "700" },
  ".cm-vl-it": { fontStyle: "italic" },
  ".cm-vl-ul": { textDecoration: "underline" },
  ".cm-vl-tt": { fontFamily: "ui-monospace, monospace", fontSize: "0.92em" },
  ".cm-vl-caption": { fontStyle: "italic", color: "var(--fg-dim)", fontSize: "0.92em" },
  ".cm-vl-chip": {
    background: "var(--hover)",
    border: "1px solid var(--border)",
    borderRadius: "4px",
    padding: "0 4px",
    fontSize: "0.85em",
  },
  ".cm-vl-math": { cursor: "pointer" },
  ".cm-vl-display": { display: "inline-block", width: "100%", textAlign: "center" },
  ".cm-vl-img": { cursor: "pointer" },
  ".cm-vl-img img": { maxWidth: "100%", background: "white", borderRadius: "4px" },
  ".cm-vl-missing": {
    color: "var(--fg-dim)",
    border: "1px dashed var(--border)",
    borderRadius: "4px",
    padding: "0 6px",
  },
  ".cm-vl-table": { cursor: "pointer", display: "inline-block" },
  ".cm-vl-table table": { borderCollapse: "collapse", margin: "4px 0" },
  ".cm-vl-table td": { border: "1px solid var(--border)", padding: "3px 10px" },
  ".cm-vl-item": { color: "var(--accent)", fontWeight: "700" },
  ".cm-vl-titleblock": { textAlign: "center", margin: "10px 0 18px", cursor: "pointer" },
  ".cm-vl-title": { fontSize: "1.8em", fontWeight: "700" },
  ".cm-vl-author": { fontSize: "1.1em", marginTop: "6px" },
  ".cm-vl-inst": { color: "var(--fg-dim)", marginTop: "4px" },
  ".cm-vl-date": { color: "var(--fg-dim)", marginTop: "4px" },
  ".cm-vl-bibhead": {
    fontSize: "1.4em",
    fontWeight: "700",
    margin: "14px 0 6px",
    cursor: "pointer",
  },
  ".cm-vl-preamble": {
    display: "inline-block",
    color: "var(--fg-dim)",
    background: "var(--panel)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
    padding: "4px 12px",
    margin: "0 0 10px",
    fontSize: "12px",
    cursor: "pointer",
  },
});

/** Extensión del modo Visual. `labels` traduce los textos generados. */
export function visualLatex(labels?: {
  preamble?: (lines: number) => string;
  references?: () => string;
}): Extension {
  if (labels?.preamble) preambleLabel = labels.preamble;
  if (labels?.references) referencesLabel = labels.references;
  return [visualField, rescanPlugin, theme];
}
