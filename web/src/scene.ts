// Typy sceny z `gui.scene` (bc-host/src/scene.rs).

import type { Rect } from "./view";

export interface Layer {
  index: number;
  role: string;
  name: string;
  color: string;
  mode: string;
  speed: number;
  power_max: number;
  power_min: number;
  passes: number;
  output: boolean;
  show: boolean;
  air_assist: boolean;
}

export interface SceneObject {
  id: number;
  /** Skrót geometrii i wyglądu — scena przyrostowa (`gui.scene {have}`). */
  hash: string;
  /** Obiekt bez zmian względem `have`: tylko id i skrót. */
  same?: boolean;
  name: string | null;
  type: string;
  layer: number;
  visible: boolean;
  locked: boolean;
  helper: boolean;
  note: string | null;
  bbox: Rect | null;
  /** Płaskie tablice [x0, y0, x1, y1, …] w mm. */
  polylines: number[][];
  polyline_layers: number[];
  closed: boolean[];
  /** Węzły (końce segmentów) i środki segmentów do przyciągania, płaskie x,y. */
  nodes: number[];
  mids: number[];
  error: string | null;
  /** Parametry kształtu parametrycznego (okno Właściwości kształtu); `null` dla ścieżek i grup. */
  params?: Record<string, number | null> | null;
}

export interface Scene {
  revision: number;
  name: string;
  workspace: [number, number];
  layers: Layer[];
  objects: SceneObject[];
  selection: number[];
  notes: string;
  /** Obrazy notatek (treść: `document.note_image`). */
  note_images: { name: string; size: number }[];
  docs: { name: string; path: string | null; dirty: boolean }[];
  active: number;
  history: { undo: string[]; redo: string[] };
  /** Wybrane urządzenie: sterownik, płyta K40, regulacja mocy (REQ-13-045). */
  device?: DeviceInfo | null;
}

export interface DeviceInfo {
  name: string;
  driver: string;
  board: string | null;
  pwm: boolean;
}

/** Urządzenie bez regulacji mocy z programu (K40 M2 Nano, Moshi) — pole mocy nieaktywne. */
export function noPwm(s: Scene | null | undefined): DeviceInfo | null {
  return s?.device && !s.device.pwm ? s.device : null;
}

export function unionBox(boxes: (Rect | null)[]): Rect | null {
  let r: Rect | null = null;
  for (const b of boxes) {
    if (!b) continue;
    r = r ? [Math.min(r[0], b[0]), Math.min(r[1], b[1]), Math.max(r[2], b[2]), Math.max(r[3], b[3])] : [...b];
  }
  return r;
}

export function selectionBox(s: Scene): Rect | null {
  const sel = new Set(s.selection);
  return unionBox(s.objects.filter((o) => sel.has(o.id)).map((o) => o.bbox));
}

export function hexToRgb(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return [1, 1, 1];
  const n = parseInt(m[1], 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}
