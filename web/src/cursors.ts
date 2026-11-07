// Kursory zależne od narzędzia (REQ-03-055): krzyżyk albo strzałka z małym
// symbolem narzędzia, SVG 24×24 w data URL, z kursorem systemowym jako zapasowym.

/** Symbole narzędzi (ścieżki SVG w polu 12…23 px). */
const GLYPHS: Record<string, string> = {
  line: "M13 22 L22 13",
  lasso: "M17 13 C23 13 23 20 17 20 C12 20 12 15 15 14 M15 20 L14 23",
  trim: "M13 13 L22 22 M22 13 L13 22",
  eraser: "M13 19 L18 14 L22 18 L17 23 Z",
  rotate2: "M21 17 A4.5 4.5 0 1 1 17 12.5 M17 12.5 L15 11 M17 12.5 L15 14.5",
  scale2: "M13 22 L22 13 M17 13 L22 13 L22 18 M13 17 L13 22 L18 22",
  warp: "M13 15 Q15.5 12 18 15 T23 15 M13 21 Q15.5 18 18 21 T23 21",
  rect: "M13 14 H22 V21 H13 Z",
  ellipse: "M17.5 13 A4.5 4 0 1 0 17.6 13 Z",
  polygon: "M17.5 12.5 L22 15 L22 20 L17.5 22.5 L13 20 L13 15 Z",
  star: "M17.5 12 L19 16.2 L23 16.3 L19.8 18.8 L21 23 L17.5 20.5 L14 23 L15.2 18.8 L12 16.3 L16 16.2 Z",
  node: "M15 15 H20 V20 H15 Z",
};

/** Narzędzia ze strzałką (wskazywanie istniejących obiektów), reszta z krzyżykiem. */
const ARROW = new Set(["node", "warp"]);

const CROSS = "M7 0 V5 M7 9 V14 M0 7 H5 M9 7 H14";
const ARROW_PATH = "M1 1 L1 15 L5 11 L8 17 L10 16 L7 10 L12 10 Z";

function svg(tool: string): string {
  const glyph = GLYPHS[tool] ?? "";
  const base = ARROW.has(tool)
    ? `<path d="${ARROW_PATH}" fill="#000" stroke="#fff" stroke-width="1" stroke-linejoin="round"/>`
    : `<path d="${CROSS}" stroke="#fff" stroke-width="3"/><path d="${CROSS}" stroke="#000" stroke-width="1"/>`;
  const sym = glyph
    ? `<path d="${glyph}" fill="none" stroke="#fff" stroke-width="3" stroke-linejoin="round"/>` +
      `<path d="${glyph}" fill="none" stroke="#1f6fd1" stroke-width="1.4" stroke-linejoin="round"/>`
    : "";
  return `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">${base}${sym}</svg>`;
}

/**
 * Wartość CSS `cursor` dla narzędzia; `fallback` — kursor systemowy narzędzia
 * (gdy przeglądarka nie wyświetli SVG). Zaznaczanie zostaje przy strzałce systemowej.
 */
export function toolCursor(tool: string, fallback: string): string {
  if (tool === "select") return fallback;
  const [x, y] = ARROW.has(tool) ? [1, 1] : [7, 7];
  return `url("data:image/svg+xml,${encodeURIComponent(svg(tool))}") ${x} ${y}, ${fallback}`;
}
