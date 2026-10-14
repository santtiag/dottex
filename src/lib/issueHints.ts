import type { MsgKey } from "./i18n.svelte";

// Explicación en claro de los errores típicos (mensajes de TeX y de texlab).
// ponytail: tabla de los ~10 más frecuentes; ampliar cuando falte alguno.
const HINTS: [RegExp, MsgKey][] = [
  [/Undefined control sequence/, "hintUndefinedCs"],
  [/Missing \\begin\{document\}/, "hintMissingBegin"],
  [/Missing \$ inserted|Display math should end/, "hintMissingDollar"],
  [/Too many \}'s|Extra \}|Unexpected "\}"/, "hintExtraBrace"],
  [
    /Runaway argument|Missing [{}] inserted|Expecting a curly bracket|File ended while scanning|Paragraph ended before/,
    "hintUnclosedBrace",
  ],
  [/Environment .+ undefined/, "hintEnvUndefined"],
  [/\\begin\{.+\}.* ended by \\end\{|Mismatched environment/, "hintEnvMismatch"],
  [/File [`'"].+['"] not found/, "hintFileNotFound"],
  [/(Reference|Citation) [`'].+' .*undefined|Undefined (reference|citation)/, "hintUndefinedRef"],
  [/(Extra|Misplaced) alignment tab/, "hintAlignTab"],
];

export const hintFor = (message: string): MsgKey | null =>
  HINTS.find(([re]) => re.test(message))?.[1] ?? null;
