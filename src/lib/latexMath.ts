/** Detección de expresiones matemáticas LaTeX (para la vista previa KaTeX). */

const LABEL_RE = /\\label\{[^}]*\}\s?/g;

// matemáticas: $$…$$, $…$, \[…\], \(…\) y entornos comunes
export const MATH_RE =
  /\$\$([\s\S]+?)\$\$|\$([^$\n]+?)\$|\\\[([\s\S]+?)\\\]|\\\(([\s\S]+?)\\\)|\\begin\{(equation|align|gather|multline|eqnarray)(\*?)\}([\s\S]+?)\\end\{\5\*?\}/g;

/** Cuerpo de un entorno matemático en la forma que KaTeX entiende. */
function envToKatex(env: string, body: string): string {
  const clean = body.replace(LABEL_RE, "").trim();
  if (env === "align" || env === "eqnarray") return `\\begin{aligned}${clean}\\end{aligned}`;
  if (env === "gather" || env === "multline") return `\\begin{gathered}${clean}\\end{gathered}`;
  return clean; // equation
}

/** Fuente KaTeX + modo display a partir de un match de MATH_RE. */
export function mathFromMatch(m: RegExpMatchArray): { src: string; display: boolean } {
  const display = m[1] !== undefined || m[3] !== undefined || m[5] !== undefined;
  const src =
    m[5] !== undefined ? envToKatex(m[5], m[7]) : (m[1] ?? m[2] ?? m[3] ?? m[4] ?? "");
  return { src, display };
}
