// Jednostki długości w polach (REQ-03-027, 028): dokument zawsze w mm,
// pola w mm albo calach; każde pole przyjmuje też wartość z jednostką
// („2in”, „25,4 mm”, „3 cm”, „1"”) i przelicza ją na bieżącą.

import { num } from "./dom";

export type Unit = "mm" | "in";

const MM_PER = { mm: 1, cm: 10, in: 25.4 } as const;

let unit: Unit = "mm";

export function setUnit(u: string) {
  unit = u === "in" ? "in" : "mm";
}

export function getUnit(): Unit {
  return unit;
}

/** mm → liczba w bieżącej jednostce. */
export function toUnit(mm: number): number {
  return mm / MM_PER[unit];
}

/** Tekst pola: wartość w bieżącej jednostce (do 3 miejsc po przecinku). */
export function fmtLen(mm: number): string {
  return num(toUnit(mm));
}

/** Tekst pola → mm; jednostka w tekście ma pierwszeństwo przed bieżącą. */
export function parseLen(s: string): number | null {
  const m = /^\s*([-+]?\d*[.,]?\d+)\s*(mm|cm|in|"|″)?\s*$/i.exec(s);
  if (!m) return null;
  const v = Number(m[1].replace(",", "."));
  if (!Number.isFinite(v)) return null;
  const u = (m[2] ?? "").toLowerCase();
  const k = u === "mm" ? 1 : u === "cm" ? 10 : u === "in" || u === '"' || u === "″" ? 25.4 : MM_PER[unit];
  return v * k;
}
