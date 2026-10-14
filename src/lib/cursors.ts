// Cursores propios: un dibujo de 32×32 como imagen de `cursor`. Los pinta el
// sistema (sin retraso, sin JS que siga al ratón). Las reglas de app.css usan
// estas variables con el cursor de siempre como respaldo, así que sin ellas
// (preferencia apagada) todo queda como estaba.

// Lucide «pencil» (sus dos trazos en un solo `d`), a escala en la esquina del
// cursor del modo Visual
const PENCIL =
  "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497zM15 5l4 4";

// Lucide «code-xml»: el </> del cursor de Ctrl+clic en el PDF (ir al código)
const CODE = "M18 16l4-4-4-4M6 8l-4 4 4 4M14.5 4l-5 16";

/** Los dibujos son SVG, pero se entregan como PNG: una imagen de mapa de bits
 *  es lo que todos los webviews (WebKitGTK incluido) aceptan como cursor. */
async function image(body: string): Promise<string> {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32" fill="none" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;
  const img = new Image();
  img.src = `data:image/svg+xml,${encodeURIComponent(svg)}`;
  await img.decode();
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 32;
  canvas.getContext("2d")!.drawImage(img, 0, 0, 32, 32);
  return `url("${canvas.toDataURL("image/png")}")`;
}

/** Variables CSS `--cur-*` para el tema dado. */
export async function cursorVars(dark: boolean): Promise<Record<string, string>> {
  // mismos valores que --accent y --bg en app.css
  const accent = dark ? "#8385f0" : "#5b5bd6";
  const edge = dark ? "#17171f" : "#ffffff";
  // cada trazo va dos veces: ancho en `edge` debajo, fino en `accent` encima,
  // para que se lea sobre cualquier fondo (el papel del PDF siempre es blanco)
  const line = (d: string, k = 1) =>
    `<path d="${d}" stroke="${edge}" stroke-width="${4.5 * k}"/><path d="${d}" stroke="${accent}" stroke-width="${2 * k}"/>`;

  const arrow = `<path d="M5 3v19l5-4.6 3.4 7.8 3-1.3-3.4-7.7H20z" fill="${accent}" stroke="${edge}" stroke-width="1.5"/>`;
  const beam = line("M12 6h8M16 6v20M12 26h8");
  // k compensa la escala del grupo: el trazo final queda algo más fino que la barra
  const pencil = `<g transform="translate(20 2) scale(.46)">${line(PENCIL, 1.6)}</g>`;

  const ring = line("M23 20a4 4 0 1 0 0 8a4 4 0 1 0 0-8");
  const dot = `<circle cx="23" cy="24" r="3.5" fill="${accent}" stroke="${edge}" stroke-width="1.5"/>`;
  const code = `<g transform="translate(16.5 17) scale(.62)">${line(CODE, 1.2)}</g>`;

  // [variable, dibujo, punto activo, cursor del sistema de respaldo]
  const designs = [
    ["--cur-arrow", arrow, "5 3", "default"],
    ["--cur-pointer", arrow + ring, "5 3", "pointer"],
    ["--cur-settings", arrow + dot, "5 3", "default"],
    ["--cur-text", beam, "16 16", "text"],
    ["--cur-visual", beam + pencil, "16 16", "text"],
    ["--cur-sync", arrow + code, "5 3", "pointer"],
  ];
  return Object.fromEntries(
    await Promise.all(
      designs.map(async ([name, body, hot, fallback]) => [name, `${await image(body)} ${hot}, ${fallback}`]),
    ),
  );
}
