import {
  snippetCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { completionData, getSnippets, type CompletionData } from "./ipc";

// Comandos LaTeX frecuentes; los definidos en el proyecto se añaden aparte.
const STATIC_COMMANDS = `documentclass usepackage begin end input include includegraphics
section subsection subsubsection paragraph chapter part title author date maketitle
label ref eqref autoref pageref cite citep citet footnote caption centering
textbf textit texttt textsc textrm textsf underline emph tiny small normalsize large Large huge Huge
frac sqrt sum prod int lim infty partial nabla cdot times pm mp leq geq neq approx equiv
alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi pi rho sigma tau phi chi psi omega
Gamma Delta Theta Lambda Xi Pi Sigma Phi Psi Omega
mathbb mathcal mathrm mathbf mathit hat bar vec dot ddot tilde overline underbrace overbrace
left right big Big bigg Bigg quad qquad hspace vspace newline newpage clearpage tableofcontents
listoffigures listoftables bibliography bibliographystyle printbibliography addbibresource
item itemize enumerate description tabular hline toprule midrule bottomrule multicolumn multirow
newcommand renewcommand newenvironment def verb url href texorpdfstring today
noindent indent par bigskip medskip smallskip appendix abstract thanks and`.split(/\s+/);

const ENVS = `document figure table equation equation* align align* gather itemize enumerate
description tabular tabularx array matrix pmatrix bmatrix cases center flushleft flushright
quote quotation verbatim minipage abstract theorem lemma proof definition example remark
titlepage thebibliography frame block columns column`.split(/\s+/);

let snippets: Completion[] = [];

/** Carga los snippets del usuario (una vez por sesión). */
export async function loadSnippets() {
  try {
    const map = await getSnippets();
    snippets = Object.entries(map).map(([trigger, body]) =>
      snippetCompletion(body, { label: trigger, type: "keyword", detail: "snippet" }),
    );
  } catch (e) {
    console.error("snippets:", e);
  }
}

let cache: CompletionData | null = null;
let cacheAt = 0;

/** Invalida la caché (llamado cuando cambia algo en disco). */
export function invalidateCompletions() {
  cache = null;
}

async function data(): Promise<CompletionData> {
  if (!cache || Date.now() - cacheAt > 3000) {
    try {
      cache = await completionData();
      cacheAt = Date.now();
    } catch {
      cache = cache ?? { labels: [], cites: [], commands: [] };
    }
  }
  return cache;
}

export async function latexComplete(ctx: CompletionContext): Promise<CompletionResult | null> {
  // \ref{...} y variantes -> labels del proyecto + los del buffer sin guardar
  let m = ctx.matchBefore(/\\(?:ref|eqref|autoref|pageref|vref|cref|Cref|nameref)\{[^{}]*/);
  if (m) {
    const d = await data();
    const buffer = [...ctx.state.doc.toString().matchAll(/\\label\s*\{([^}]+)\}/g)].map(
      (x) => x[1],
    );
    const labels = [...new Set([...d.labels, ...buffer])];
    return {
      from: m.from + m.text.lastIndexOf("{") + 1,
      options: labels.map((l) => ({ label: l, type: "constant" })),
      validFor: /^[^{}]*$/,
    };
  }

  // \cite{...} y variantes (con listas separadas por coma) -> claves .bib
  m = ctx.matchBefore(/\\[a-zA-Z]*[cC]ite[a-zA-Z]*\*?(?:\[[^\]]*\])*\{[^{}]*/);
  if (m) {
    const d = await data();
    return {
      from: m.from + Math.max(m.text.lastIndexOf("{"), m.text.lastIndexOf(",")) + 1,
      options: d.cites.map((c) => ({ label: c.key, detail: c.detail, type: "constant" })),
      validFor: /^[\w:.-]*$/,
    };
  }

  // \begin{...} / \end{...} -> entornos
  m = ctx.matchBefore(/\\(?:begin|end)\{[a-zA-Z*]*/);
  if (m) {
    return {
      from: m.from + m.text.indexOf("{") + 1,
      options: ENVS.map((e) => ({ label: e, type: "type" })),
      validFor: /^[a-zA-Z*]*$/,
    };
  }

  // \comando -> comandos comunes + \newcommand del proyecto
  m = ctx.matchBefore(/\\[a-zA-Z]*/);
  if (m) {
    const d = await data();
    const cmds = [...new Set([...STATIC_COMMANDS, ...d.commands])];
    return {
      from: m.from,
      options: cmds.map((c) => ({ label: "\\" + c, type: "function" })),
      validFor: /^\\[a-zA-Z]*$/,
    };
  }

  // palabra normal -> snippets (fig, eq, tab…)
  m = ctx.matchBefore(/\w{2,}/);
  if (m && snippets.length) {
    return { from: m.from, options: snippets, validFor: /^\w*$/ };
  }
  return null;
}
