import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { syntaxHighlighting } from "@codemirror/language";
import { oneDark, oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
import {
  amy,
  ayuLight,
  bespin,
  birdsOfParadise,
  clouds,
  cobalt,
  coolGlow,
  dracula,
  espresso,
  noctisLilac,
  rosePineDawn,
  smoothy,
  solarizedLight,
  tomorrow,
} from "thememirror";

export interface EditorTheme {
  label: string;
  dark: boolean;
  ext: Extension;
}

// Tema "light": sin fondo propio, el editor toma los colores de la app. El resto
// también tiene que seguirla (canalón, selección, cursor, línea activa, sintaxis):
// si no, CodeMirror los deja con su versión clara y en modo oscuro el canalón de
// números queda en blanco.
const appGutters = EditorView.theme({
  ".cm-gutters": {
    backgroundColor: "var(--panel)",
    color: "var(--fg-dim)",
    borderRightColor: "var(--border)",
  },
});
const appColors = (dark: boolean): Extension =>
  dark
    ? [appGutters, EditorView.darkTheme.of(true), syntaxHighlighting(oneDarkHighlightStyle)]
    : appGutters;

/// Los ids "light" y "dark" se conservan por compatibilidad con
/// configuraciones ya guardadas en localStorage.
export const editorThemes: Record<string, EditorTheme> = {
  light: { label: "App", dark: false, ext: appColors(false) },
  ayuLight: { label: "Ayu Light", dark: false, ext: ayuLight },
  clouds: { label: "Clouds", dark: false, ext: clouds },
  espresso: { label: "Espresso", dark: false, ext: espresso },
  noctisLilac: { label: "Noctis Lilac", dark: false, ext: noctisLilac },
  rosePineDawn: { label: "Rosé Pine Dawn", dark: false, ext: rosePineDawn },
  smoothy: { label: "Smoothy", dark: false, ext: smoothy },
  solarizedLight: { label: "Solarized Light", dark: false, ext: solarizedLight },
  tomorrow: { label: "Tomorrow", dark: false, ext: tomorrow },
  dark: { label: "One Dark", dark: true, ext: oneDark },
  amy: { label: "Amy", dark: true, ext: amy },
  bespin: { label: "Bespin", dark: true, ext: bespin },
  birdsOfParadise: { label: "Birds of Paradise", dark: true, ext: birdsOfParadise },
  cobalt: { label: "Cobalt", dark: true, ext: cobalt },
  coolGlow: { label: "Cool Glow", dark: true, ext: coolGlow },
  dracula: { label: "Dracula", dark: true, ext: dracula },
};

/** Theme efectivo: "auto" sigue el tema general de la app. */
export function resolveTheme(id: string, appDark: boolean): EditorTheme {
  const th = id !== "auto" && editorThemes[id] ? editorThemes[id] : editorThemes[appDark ? "dark" : "light"];
  return th === editorThemes.light ? { ...th, dark: appDark, ext: appColors(appDark) } : th;
}
