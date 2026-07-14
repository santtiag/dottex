import { settings } from "./state.svelte";

// Idioma de la interfaz. Diccionarios planos es/en; `t()` es reactiva porque
// lee `settings.locale` ($state) en cada render. Para añadir un idioma basta
// con otro objeto con las mismas claves.
const es = {
  // Bienvenida
  appTagline: "Editor LaTeX local. Abre una carpeta para empezar.",
  openFolder: "Abrir carpeta…",
  recent: "Recientes",

  // Barra de título
  toggleSidebar: "Mostrar/ocultar panel lateral",
  switchProject: "Cambiar de proyecto",
  unsaved: "Sin guardar",
  compileTitle: "Compilar (Ctrl+Enter)",
  compile: "Compilar",
  compiling: "Compilando…",
  exportPdfTitle: "Exportar PDF…",
  togglePdf: "Mostrar/ocultar visor PDF",
  settings: "Configuración",
  minimize: "Minimizar",
  maximize: "Maximizar",
  close: "Cerrar",

  // Pestañas del panel lateral
  tabFiles: "Archivos",
  tabOutline: "Esquema",
  tabSearch: "Buscar",
  tabIssues: "Problemas",

  // Centro
  selectFile: "Selecciona un archivo del explorador",
  unsupportedFormat: "Formato no soportado: {0}",

  // Barra de estado
  statusTyping: "escribiendo…",
  statusIdle: "listo",
  statusCompiling: "compilando…",
  statusOk: "listo ✓",
  statusError: "con errores",
  rootPrefix: "raíz: {0}",
  autoPreviewTitle: "Recompilar automáticamente al dejar de escribir",
  autoPreview: "Vista previa automática",
  cleanArtifacts: "Limpiar artefactos",
  hideLog: "Ocultar log",
  showLog: "Ver log",
  artifactsCleaned: "Artefactos limpiados (.dottex/build y cache)",

  // Problemas
  noIssues: "Sin problemas ✓",
  compileForIssues: "Compila para ver errores y avisos",

  // Esquema
  untitled: "(sin título)",
  figure: "Figura",
  table: "Tabla",
  noOutline: "Sin estructura todavía (\\section, figuras, labels…)",
  openTexForOutline: "Abre un archivo .tex para ver su esquema",
  lineTitle: "línea {0}",

  // Búsqueda
  searchPlaceholder: "Buscar en el proyecto…",
  replacePlaceholder: "Reemplazar por…",
  regexTitle: "Expresión regular",
  replaceConfirm: "¿Reemplazar {0}?",
  replaceAll: "Reemplazar todo",
  noResults: "Sin resultados",
  first500: "Mostrando los primeros 500 resultados",
  saveBeforeReplace: "Guarda el archivo activo (Ctrl+S) antes de reemplazar en el proyecto",
  replaceResult: "{0} reemplazos en {1} archivo(s)",

  // Explorador de archivos
  newFile: "Nuevo archivo",
  newFileShort: "＋ archivo",
  newFolder: "Nueva carpeta",
  newFolderShort: "＋ carpeta",
  trash: "Papelera",
  trashEmpty: "Vacía",
  restore: "Restaurar",
  deleteForever: "Borrar definitivamente",
  emptyTrash: "Vaciar papelera",
  setRoot: "Fijar",
  rename: "Renombrar",
  deleteToTrash: "Eliminar (a papelera)",
  rootBadge: "raíz",

  // Visor PDF
  zoomOut: "Alejar",
  zoomIn: "Acercar",
  fitWidth: "Ajustar al ancho",
  pageCount: "{0} pág.",
  syncClickTitle: "Ctrl+clic: ir a la línea en el editor",
  compileForPdf: "Compila el proyecto para ver el PDF",

  // Exportar (diálogo del sistema)
  exportPdfDialog: "Exportar PDF",
  pdfExported: "PDF exportado a {0}",

  // Configuración: categorías
  catEditor: "Editor",
  catAppearance: "Apariencia",
  catLanguage: "Idioma",

  // Configuración: editor
  autocomplete: "Autocompletado",
  autocompleteDesc: "Sugiere comandos, referencias y snippets al escribir",
  closeBrackets: "Autocerrar llaves",
  closeBracketsDesc: "Inserta } ) ] automáticamente al abrir uno",
  steadyCursor: "Cursor fijo",
  steadyCursorDesc: "El cursor no parpadea: menos distracción visual",
  vim: "Atajos de Vim",
  vimDesc: "Emulación de Vim en el editor",
  breadcrumbs: "Migas de pan",
  breadcrumbsDesc: "Muestra el archivo y la sección actual sobre el editor",
  mathPreview: "Vista previa de ecuaciones",
  mathPreviewDesc: "Renderiza la ecuación bajo el cursor mientras escribes",
  fontSize: "Tamaño de letra del editor",
  fontFamily: "Fuente del editor",
  lineHeight: "Interlineado del editor",
  editorTheme: "Tema del editor",
  editorThemeDesc: "Colores del código",
  editorThemeAuto: "Según el tema general",
  themeLight: "Claro",
  editorThemeOneDark: "One Dark",

  // Configuración: apariencia
  generalTheme: "Tema general",
  generalThemeDesc: "Colores de toda la aplicación",
  themeAuto: "Automático (sistema)",
  themeDark: "Oscuro",
  pdfInvert: "PDF en modo oscuro",
  pdfInvertDesc: "Invierte los colores de la vista previa con tema oscuro",

  // Configuración: idioma
  uiLanguage: "Idioma de la aplicación",
  uiLanguageDesc: "Idioma de la interfaz",
  spellcheck: "Revisión ortográfica",
  spellcheckDesc: "Subraya errores usando el corrector del sistema",
  spellLang: "Idioma del corrector",
  dictHint: "Diccionario propio: clic derecho sobre una palabra subrayada → «Añadir al diccionario».",
};

const en: typeof es = {
  appTagline: "Local LaTeX editor. Open a folder to get started.",
  openFolder: "Open folder…",
  recent: "Recent",

  toggleSidebar: "Toggle sidebar",
  switchProject: "Switch project",
  unsaved: "Unsaved",
  compileTitle: "Compile (Ctrl+Enter)",
  compile: "Compile",
  compiling: "Compiling…",
  exportPdfTitle: "Export PDF…",
  togglePdf: "Toggle PDF viewer",
  settings: "Settings",
  minimize: "Minimize",
  maximize: "Maximize",
  close: "Close",

  tabFiles: "Files",
  tabOutline: "Outline",
  tabSearch: "Search",
  tabIssues: "Issues",

  selectFile: "Select a file from the explorer",
  unsupportedFormat: "Unsupported format: {0}",

  statusTyping: "typing…",
  statusIdle: "ready",
  statusCompiling: "compiling…",
  statusOk: "ready ✓",
  statusError: "with errors",
  rootPrefix: "root: {0}",
  autoPreviewTitle: "Recompile automatically when you stop typing",
  autoPreview: "Automatic preview",
  cleanArtifacts: "Clean artifacts",
  hideLog: "Hide log",
  showLog: "Show log",
  artifactsCleaned: "Artifacts cleaned (.dottex/build and cache)",

  noIssues: "No issues ✓",
  compileForIssues: "Compile to see errors and warnings",

  untitled: "(untitled)",
  figure: "Figure",
  table: "Table",
  noOutline: "No structure yet (\\section, figures, labels…)",
  openTexForOutline: "Open a .tex file to see its outline",
  lineTitle: "line {0}",

  searchPlaceholder: "Search in project…",
  replacePlaceholder: "Replace with…",
  regexTitle: "Regular expression",
  replaceConfirm: "Replace {0}?",
  replaceAll: "Replace all",
  noResults: "No results",
  first500: "Showing the first 500 results",
  saveBeforeReplace: "Save the active file (Ctrl+S) before replacing across the project",
  replaceResult: "{0} replacements in {1} file(s)",

  newFile: "New file",
  newFileShort: "＋ file",
  newFolder: "New folder",
  newFolderShort: "＋ folder",
  trash: "Trash",
  trashEmpty: "Empty",
  restore: "Restore",
  deleteForever: "Delete permanently",
  emptyTrash: "Empty trash",
  setRoot: "Set as root",
  rename: "Rename",
  deleteToTrash: "Delete (to trash)",
  rootBadge: "root",

  zoomOut: "Zoom out",
  zoomIn: "Zoom in",
  fitWidth: "Fit width",
  pageCount: "{0} pp.",
  syncClickTitle: "Ctrl+click: go to the line in the editor",
  compileForPdf: "Compile the project to see the PDF",

  exportPdfDialog: "Export PDF",
  pdfExported: "PDF exported to {0}",

  catEditor: "Editor",
  catAppearance: "Appearance",
  catLanguage: "Language",

  autocomplete: "Autocomplete",
  autocompleteDesc: "Suggests commands, references and snippets as you type",
  closeBrackets: "Auto-close brackets",
  closeBracketsDesc: "Auto-inserts } ) ] when you open one",
  steadyCursor: "Steady cursor",
  steadyCursorDesc: "The cursor doesn't blink: less visual distraction",
  vim: "Vim keybindings",
  vimDesc: "Vim emulation in the editor",
  breadcrumbs: "Breadcrumbs",
  breadcrumbsDesc: "Shows the file and current section above the editor",
  mathPreview: "Equation preview",
  mathPreviewDesc: "Renders the equation under the cursor as you type",
  fontSize: "Editor font size",
  fontFamily: "Editor font",
  lineHeight: "Editor line spacing",
  editorTheme: "Editor theme",
  editorThemeDesc: "Code colors",
  editorThemeAuto: "Follow general theme",
  themeLight: "Light",
  editorThemeOneDark: "One Dark",

  generalTheme: "General theme",
  generalThemeDesc: "Colors of the entire application",
  themeAuto: "Automatic (system)",
  themeDark: "Dark",
  pdfInvert: "PDF in dark mode",
  pdfInvertDesc: "Inverts the preview colors with the dark theme",

  uiLanguage: "Application language",
  uiLanguageDesc: "Interface language",
  spellcheck: "Spell check",
  spellcheckDesc: "Underlines errors using the system checker",
  spellLang: "Spell-check language",
  dictHint: "Personal dictionary: right-click an underlined word → “Add to dictionary”.",
};

const dicts = { es, en };

export type MsgKey = keyof typeof es;

/** Traduce una clave; `{0}`, `{1}`… se reemplazan por los argumentos. */
export function t(key: MsgKey, ...args: (string | number)[]): string {
  const s = dicts[settings.locale][key] ?? es[key];
  return args.length ? s.replace(/\{(\d+)\}/g, (_, i) => String(args[+i] ?? "")) : s;
}

export const LOCALES: [string, string][] = [
  ["es", "Español"],
  ["en", "English"],
];
