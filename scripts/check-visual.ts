// Chequeo del modo Visual: escáner puro + StateField con cursor reveal.
// Ejecutar:
//   node_modules/.pnpm/esbuild@*/node_modules/esbuild/bin/esbuild \
//     scripts/check-visual.ts --bundle --format=esm --outfile=/tmp/cv.mjs && node /tmp/cv.mjs
import { EditorState } from "@codemirror/state";
import { parseTabular, rescanEffect, scanVisual, visualField, visualLatex } from "../src/lib/visualLatex";

const assert = (cond: unknown, msg: string) => {
  if (!cond) throw new Error(`FALLO: ${msg}`);
};

const doc = `\\documentclass{article}
\\usepackage{amsmath}
\\title{Mi artículo}
\\author{Ana \\and Luis}
\\begin{document}
\\titlerunning{Corto}
\\institute{Uni}
\\maketitle
\\section{Hola}
Texto con \\textbf{negrita \\textit{anidada}} y $x^2$. % comentario
\\[
  E = mc^2
\\]
\\begin{itemize}
  \\item uno
  \\item dos
\\end{itemize}
\\begin{enumerate}
  \\item a
  \\item b
\\end{enumerate}
\\begin{figure}[h]
  \\centering
  \\includegraphics[width=0.5\\textwidth]{img/foo.png}
  \\caption{Un pie}
  \\label{fig:foo}
\\end{figure}
Ver \\ref{fig:foo}.
\\begin{tabular}{cc}
  a & $b_1$ \\\\
  \\hline
  c & d
\\end{tabular}
\\begin{thebibliography}{9}
  \\bibitem{knuth} Knuth, TeXbook.
  \\bibitem{lamport} Lamport, LaTeX.
\\end{thebibliography}
\\end{document}
resto ignorado`;

const els = scanVisual(doc);
const specs = els.flatMap((e) => e.prims).filter((p) => p.kind === "widget");
const of = (t: string) => specs.filter((p) => p.kind === "widget" && p.spec.type === t);

// preámbulo: bloque desde 0 que cubre \begin{document}
const pre = of("preamble")[0];
assert(pre && pre.from === 0 && doc.slice(0, pre.to).includes("\\begin{document}"), "preámbulo");
assert(
  els.some((e) => e.to === doc.length && doc.slice(e.from, e.to).startsWith("\\end{document}")),
  "cola tras \\end{document} oculta",
);

// sección con su clase; negrita anidada aceptada (contención, no solape)
const marks = els.flatMap((e) => e.prims).filter((p) => p.kind === "mark");
assert(marks.some((m) => m.kind === "mark" && m.cls === "cm-vl-sec1"), "sección");
assert(marks.filter((m) => m.kind === "mark" && m.cls === "cm-vl-bf").length === 1, "negrita");
assert(marks.some((m) => m.kind === "mark" && m.cls === "cm-vl-it"), "itálica anidada");
assert(marks.some((m) => m.kind === "mark" && m.cls === "cm-vl-caption"), "caption");
assert(marks.some((m) => m.kind === "mark" && m.cls === "cm-vl-chip"), "chip de \\ref");

// matemáticas: $x^2$, \[E=mc^2\] y $b_1$ (esta última contenida en la tabla)
assert(of("math").length === 3, `3 matemáticas, hay ${of("math").length}`);

// listas: viñetas y numeración; bibitems numerados al final
const items = of("item").map((p) => (p.kind === "widget" && p.spec.type === "item" ? p.spec.label : ""));
assert(
  JSON.stringify(items) === JSON.stringify(["•", "•", "1.", "2.", "[1]", "[2]"]),
  `items: ${items}`,
);

// portada: \maketitle con metadatos (preámbulo o cuerpo, como llncs) y
// posiciones que apuntan al contenido para el clic-para-editar
const title = of("title")[0];
assert(title && title.kind === "widget" && title.spec.type === "title", "portada \\maketitle");
const fields = title.kind === "widget" && title.spec.type === "title" ? title.spec.fields : [];
const fld = (cls: string) => fields.find((f) => f.cls === cls);
assert(fld("cm-vl-title")?.src === "Mi artículo", "título de portada");
assert(fld("cm-vl-author")?.src === "Ana \\and Luis", "autor de portada");
assert(fld("cm-vl-inst")?.src === "Uni", "institute de portada");
assert(doc.slice(fld("cm-vl-title")!.pos, fld("cm-vl-title")!.pos + 2) === "Mi", "pos del título");
assert(doc.slice(fld("cm-vl-inst")!.pos, fld("cm-vl-inst")!.pos + 3) === "Uni", "pos de institute");
// los metadatos del cuerpo (llncs) quedan ocultos, no como LaTeX crudo
for (const cmd of ["\\titlerunning", "\\institute"])
  assert(
    els.some((e) => doc.slice(e.from, e.to).startsWith(cmd) && e.prims[0].kind === "hide"),
    `${cmd} oculto`,
  );
assert(of("bibhead").length === 1, "encabezado de bibliografía");

// imagen y tabla
assert(of("image").length === 1, "imagen");
const tab = of("table")[0];
assert(tab && tab.kind === "widget" && tab.spec.type === "table", "tabular");
const rows = parseTabular(tab.spec.type === "table" ? tab.spec.body : "");
assert(rows.length === 2 && rows[0].length === 2 && rows[1][1] === "d", "celdas de la tabla");

// invariante: sin solapes parciales entre elementos aceptados
for (let i = 0; i < els.length; i++)
  for (let j = i + 1; j < els.length; j++) {
    const a = els[i], b = els[j];
    const overlap = a.from < b.to && b.from < a.to;
    const nested = (a.from <= b.from && b.to <= a.to) || (b.from <= a.from && a.to <= b.to);
    assert(!overlap || nested, `solape parcial: [${a.from},${a.to}) vs [${b.from},${b.to})`);
  }

// StateField: crear estado no lanza; cursor dentro de una ecuación la revela
const mk = (anchor: number) =>
  EditorState.create({ doc, selection: { anchor }, extensions: [visualLatex()] });
const mathEl = els.find((e) => e.prims.some((p) => p.kind === "widget" && p.spec.type === "math"))!;
const outside = mk(0).field(visualField).deco.size;
const inside = mk(mathEl.from + 1).field(visualField).deco.size;
assert(outside > 0, "hay decoraciones");
assert(inside === outside - 1, `reveal: ${outside} → ${inside}`);

// editar solo mapea las decoraciones (barato); el re-escaneo llega después
// como rescanEffect (en la app lo despacha rescanPlugin con debounce)
const tr = mk(0).update({ changes: { from: doc.indexOf("Texto"), insert: "$y$ " } });
assert(tr.state.field(visualField).deco.size === outside, "edición mapea sin re-escanear");
const tr2 = tr.state.update({
  effects: rescanEffect.of(scanVisual(tr.state.doc.toString())),
});
assert(tr2.state.field(visualField).deco.size === outside + 1, "nueva ecuación decorada tras el rescan");

console.log(`OK: ${els.length} elementos, ${outside} decoraciones, reveal y edición correctos`);
