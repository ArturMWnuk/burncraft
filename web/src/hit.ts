// Trafianie obiektów kursorem: krawędź w tolerancji albo wnętrze zamkniętego konturu.

import type { Scene, SceneObject } from "./scene";
import type { Rect } from "./view";

function segDist2(px: number, py: number, ax: number, ay: number, bx: number, by: number): number {
  const dx = bx - ax;
  const dy = by - ay;
  const l2 = dx * dx + dy * dy;
  let t = l2 > 0 ? ((px - ax) * dx + (py - ay) * dy) / l2 : 0;
  t = Math.max(0, Math.min(1, t));
  const qx = ax + t * dx - px;
  const qy = ay + t * dy - py;
  return qx * qx + qy * qy;
}

function hitsObject(o: SceneObject, x: number, y: number, tol: number): boolean {
  const b = o.bbox;
  if (!b || x < b[0] - tol || x > b[2] + tol || y < b[1] - tol || y > b[3] + tol) return false;
  let inside = false;
  for (let k = 0; k < o.polylines.length; k++) {
    const p = o.polylines[k];
    for (let i = 0; i + 3 < p.length; i += 2) {
      if (segDist2(x, y, p[i], p[i + 1], p[i + 2], p[i + 3]) <= tol * tol) return true;
    }
    if (o.closed[k]) {
      // parzysto-nieparzysta reguła wypełnienia (jak w bc-geom)
      for (let i = 0, j = p.length - 2; i < p.length; j = i, i += 2) {
        const yi = p[i + 1];
        const yj = p[j + 1];
        if (yi > y !== yj > y && x < ((p[j] - p[i]) * (y - yi)) / (yj - yi) + p[i]) inside = !inside;
      }
    }
  }
  return inside;
}

/** Najwyższy (ostatnio rysowany) widoczny obiekt pod punktem. */
export function pick(scene: Scene, x: number, y: number, tol: number): SceneObject | null {
  const hidden = new Set(scene.layers.filter((l) => !l.show).map((l) => l.index));
  for (let i = scene.objects.length - 1; i >= 0; i--) {
    const o = scene.objects[i];
    if (!o.visible || hidden.has(o.layer)) continue;
    if (hitsObject(o, x, y, tol)) return o;
  }
  return null;
}

/** Obiekty całkowicie wewnątrz prostokąta (zaznaczanie ramką). */
export function inRect(scene: Scene, r: Rect): number[] {
  const [x0, y0, x1, y1] = [Math.min(r[0], r[2]), Math.min(r[1], r[3]), Math.max(r[0], r[2]), Math.max(r[1], r[3])];
  return scene.objects
    .filter((o) => o.visible && o.bbox && o.bbox[0] >= x0 && o.bbox[1] >= y0 && o.bbox[2] <= x1 && o.bbox[3] <= y1)
    .map((o) => o.id);
}
