// Przyciąganie (REQ-03-006…010): węzły > środki odcinków > środki obiektów >
// krawędzie > prowadnice automatyczne (REQ-03-011) > siatka. Tolerancja
// w pikselach ekranu, wynik w mm.

import type { Scene } from "./scene";
import { gridStep, View } from "./view";

export type SnapKind = "node" | "mid" | "center" | "edge" | "align" | "grid" | "none";

export interface SnapResult {
  x: number;
  y: number;
  kind: SnapKind;
  /** Prowadnice automatyczne do narysowania (odcinki [x0, y0, x1, y1] w mm). */
  guides?: number[][];
}

export interface SnapSettings {
  grid: boolean;
  nodes: boolean;
  mids: boolean;
  centers: boolean;
  edges: boolean;
  align: boolean;
}

export const DEFAULT_SNAP: SnapSettings = { grid: true, nodes: true, mids: true, centers: true, edges: true, align: true };

/** Promień przyciągania w pikselach CSS. */
export const SNAP_PX = 8;

export interface SnapOptions {
  /** Obiekty pominięte (np. przesuwane). */
  exclude?: Set<number>;
  /** Dodatkowe węzły (np. rysowana ścieżka), płaska tablica x,y. */
  extraNodes?: number[];
  /** false — bez siatki (przesuwanie chwyconym punktem). */
  grid?: boolean;
}

function nearestInFlat(flat: number[], x: number, y: number, best: { d: number; x: number; y: number }) {
  for (let i = 0; i + 1 < flat.length; i += 2) {
    const dx = flat[i] - x;
    const dy = flat[i + 1] - y;
    const d = dx * dx + dy * dy;
    if (d < best.d) {
      best.d = d;
      best.x = flat[i];
      best.y = flat[i + 1];
    }
  }
}

export function snap(scene: Scene | null, view: View, x: number, y: number, s: SnapSettings, opt: SnapOptions = {}): SnapResult {
  const tol = SNAP_PX / view.scale;
  const tol2 = tol * tol;
  if (scene) {
    const hidden = new Set(scene.layers.filter((l) => !l.show).map((l) => l.index));
    const objs = scene.objects.filter(
      (o) =>
        o.visible && !hidden.has(o.layer) && !opt.exclude?.has(o.id) && o.bbox &&
        x >= o.bbox[0] - tol && x <= o.bbox[2] + tol && y >= o.bbox[1] - tol && y <= o.bbox[3] + tol,
    );
    const pick = (enabled: boolean, kind: SnapKind, fill: (best: { d: number; x: number; y: number }) => void): SnapResult | null => {
      if (!enabled) return null;
      const best = { d: tol2, x: 0, y: 0 };
      fill(best);
      return best.d < tol2 ? { x: best.x, y: best.y, kind } : null;
    };
    const hit =
      pick(s.nodes, "node", (b) => {
        for (const o of objs) nearestInFlat(o.nodes ?? [], x, y, b);
        if (opt.extraNodes) nearestInFlat(opt.extraNodes, x, y, b);
      }) ??
      pick(s.mids, "mid", (b) => {
        for (const o of objs) nearestInFlat(o.mids ?? [], x, y, b);
      }) ??
      pick(s.centers, "center", (b) => {
        for (const o of objs) {
          const bb = o.bbox!;
          nearestInFlat([(bb[0] + bb[2]) / 2, (bb[1] + bb[3]) / 2], x, y, b);
        }
      }) ??
      pick(s.edges, "edge", (b) => {
        for (const o of objs) for (const p of o.polylines) nearestOnPolyline(p, x, y, b);
      });
    if (hit) return hit;
    if (s.align) {
      const a = alignTo(scene, x, y, tol, opt.exclude, hidden);
      if (a) return a;
    }
  }
  if (s.grid && opt.grid !== false) {
    const step = gridStep(view.scale);
    return { x: roundTo(x, step), y: roundTo(y, step), kind: "grid" };
  }
  return { x, y, kind: "none" };
}

/**
 * Prowadnice automatyczne: współrzędna X lub Y kursora zgodna z węzłem albo
 * środkiem innego obiektu (dowolnie daleko) — przyciąga tę współrzędną
 * i zwraca linię od punktu wzorcowego do kursora.
 */
function alignTo(scene: Scene, x: number, y: number, tol: number, exclude: Set<number> | undefined, hidden: Set<number>): SnapResult | null {
  let bx: { d: number; v: number; from: number } | null = null;
  let by: { d: number; v: number; from: number } | null = null;
  const consider = (px: number, py: number) => {
    const dx = Math.abs(px - x);
    if (dx <= tol && (!bx || dx < bx.d || (dx === bx.d && Math.abs(py - y) < Math.abs(bx.from - y)))) bx = { d: dx, v: px, from: py };
    const dy = Math.abs(py - y);
    if (dy <= tol && (!by || dy < by.d || (dy === by.d && Math.abs(px - x) < Math.abs(by.from - x)))) by = { d: dy, v: py, from: px };
  };
  for (const o of scene.objects) {
    if (!o.visible || hidden.has(o.layer) || exclude?.has(o.id) || !o.bbox) continue;
    const n = o.nodes ?? [];
    for (let i = 0; i + 1 < n.length; i += 2) consider(n[i], n[i + 1]);
    consider((o.bbox[0] + o.bbox[2]) / 2, (o.bbox[1] + o.bbox[3]) / 2);
  }
  const ax = bx as { d: number; v: number; from: number } | null;
  const ay = by as { d: number; v: number; from: number } | null;
  if (!ax && !ay) return null;
  const rx = ax ? ax.v : x;
  const ry = ay ? ay.v : y;
  const guides: number[][] = [];
  if (ax) guides.push([ax.v, ax.from, ax.v, ry]);
  if (ay) guides.push([ay.from, ay.v, rx, ay.v]);
  return { x: rx, y: ry, kind: "align", guides };
}

function roundTo(v: number, step: number): number {
  const r = Math.round(v / step) * step;
  // usuń szum zmiennoprzecinkowy (0.1 × 3 = 0.30000000000000004)
  return Number(r.toFixed(10)) + 0;
}

function nearestOnPolyline(p: number[], x: number, y: number, best: { d: number; x: number; y: number }) {
  for (let i = 0; i + 3 < p.length; i += 2) {
    const ax = p[i], ay = p[i + 1], bx = p[i + 2], by = p[i + 3];
    const dx = bx - ax, dy = by - ay;
    const l2 = dx * dx + dy * dy;
    const t = l2 > 0 ? Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / l2)) : 0;
    const qx = ax + t * dx, qy = ay + t * dy;
    const d = (qx - x) ** 2 + (qy - y) ** 2;
    if (d < best.d) {
      best.d = d;
      best.x = qx;
      best.y = qy;
    }
  }
}

const KEY = "bc.snap";

export function loadSnap(): SnapSettings {
  try {
    return { ...DEFAULT_SNAP, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { ...DEFAULT_SNAP };
  }
}

export function saveSnap(s: SnapSettings) {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // brak dostępu do localStorage — ustawienia tylko w tej sesji
  }
}
