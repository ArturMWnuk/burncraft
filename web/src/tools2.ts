// Narzędzia M1.4b2b: lasso (REQ-04-022), obrót i skala dwupunktowa (REQ-04-050, 051),
// gumka (REQ-04-021), przycinanie (REQ-04-034), wypaczanie 4-punktowe, siatką
// i obwiednią z tymczasowym cofaniem (REQ-04-045…048). Zmiany dokumentu idą
// wyłącznie przez polecenia rejestru.

import { h } from "./dom";
import { pick } from "./hit";
import { t } from "./i18n";
import { Scene, selectionBox } from "./scene";
import type { SnapOptions, SnapResult } from "./snap";
import type { Tool } from "./tools";
import type { View } from "./view";

type Pt = [number, number];

export interface Host2 {
  view: View;
  scene: Scene | null;
  run(id: string, params?: unknown): Promise<any>;
  snapWorld(x: number, y: number, opt?: SnapOptions): SnapResult;
  redraw(): void;
  refreshTools(): void;
  toolOpts: { eraserWidth: number; warpMode: string };
  setToolOpt(key: "eraserWidth" | "warpMode", v: number | string): void;
}

function worldOf(view: View, e: MouseEvent): Pt {
  const r = (e.target as HTMLElement).getBoundingClientRect();
  return view.toWorld(e.clientX - r.left, e.clientY - r.top);
}

abstract class Base implements Tool {
  abstract id: string;
  cursor = "crosshair";
  constructor(protected host: Host2) {}
  down(_p: SnapResult, _e: PointerEvent) {}
  move(_p: SnapResult, _e: PointerEvent, _pressed: boolean) {}
  up(_p: SnapResult, _e: PointerEvent) {}
  key(_e: KeyboardEvent): boolean {
    return false;
  }
  preview(): number[][] {
    return [];
  }
  extraNodes(): number[] {
    return [];
  }
  input(): string | null {
    return null;
  }
}

// ---------- lasso ----------

function inside(poly: Pt[], x: number, y: number): boolean {
  let r = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) r = !r;
  }
  return r;
}

export class LassoTool extends Base {
  id = "lasso";
  private pts: Pt[] = [];

  down(_p: SnapResult, e: PointerEvent) {
    this.pts = [worldOf(this.host.view, e)];
  }

  move(_p: SnapResult, e: PointerEvent, pressed: boolean) {
    if (!pressed || !this.pts.length) return;
    const q = worldOf(this.host.view, e);
    const last = this.pts[this.pts.length - 1];
    if (Math.hypot(q[0] - last[0], q[1] - last[1]) * this.host.view.scale >= 2) this.pts.push(q);
    this.host.redraw();
  }

  up(_p: SnapResult, e: PointerEvent) {
    const poly = this.pts;
    this.pts = [];
    this.host.redraw();
    const s = this.host.scene;
    if (!s || poly.length < 3) return;
    const ids = s.objects
      .filter((o) => o.visible && o.polylines.length && o.polylines.every((p) => {
        for (let i = 0; i + 1 < p.length; i += 2) if (!inside(poly, p[i], p[i + 1])) return false;
        return true;
      }))
      .map((o) => o.id);
    const all = e.shiftKey ? [...new Set([...s.selection, ...ids])] : ids;
    void this.host.run("selection.set", { ids: all });
  }

  key(e: KeyboardEvent) {
    if (e.key === "Escape" && this.pts.length) {
      this.pts = [];
      this.host.redraw();
      return true;
    }
    return false;
  }

  preview() {
    return this.pts.length > 1 ? [[...this.pts.flat(), ...this.pts[0]]] : [];
  }
}

// ---------- obrót i skala dwupunktowa ----------

/** Klik 1 — środek, klik 2 — punkt odniesienia, klik 3 — cel. */
export class TwoPointTool extends Base {
  private pts: Pt[] = [];
  private cursorPt: Pt | null = null;

  constructor(public id: "rotate2" | "scale2", host: Host2) {
    super(host);
  }

  down(p: SnapResult) {
    this.pts.push([p.x, p.y]);
    if (this.pts.length < 3) return this.host.redraw();
    const [c, a, b] = this.pts;
    this.pts = [];
    this.host.redraw();
    const sel = this.host.scene?.selection ?? [];
    if (!sel.length) return;
    if (this.id === "rotate2") {
      const ang = Math.atan2(b[1] - c[1], b[0] - c[0]) - Math.atan2(a[1] - c[1], a[0] - c[0]);
      const deg = Number(((ang * 180) / Math.PI).toFixed(9));
      if (Math.abs(deg) > 1e-9) void this.host.run("object.transform", { ids: sel, rotate: deg, anchor: c });
    } else {
      const ra = Math.hypot(a[0] - c[0], a[1] - c[1]);
      const rb = Math.hypot(b[0] - c[0], b[1] - c[1]);
      if (ra < 1e-9 || rb < 1e-9) return;
      const k = Number((rb / ra).toFixed(12));
      void this.host.run("object.transform", { ids: sel, scale: [k, k], anchor: c });
    }
  }

  move(p: SnapResult) {
    this.cursorPt = [p.x, p.y];
    if (this.pts.length) this.host.redraw();
  }

  key(e: KeyboardEvent) {
    if (e.key === "Escape" && this.pts.length) {
      this.pts = [];
      this.host.redraw();
      return true;
    }
    return false;
  }

  preview() {
    const [c, a] = this.pts;
    const out: number[][] = [];
    if (c && a) out.push([c[0], c[1], a[0], a[1]]);
    if (c && this.cursorPt) out.push([c[0], c[1], this.cursorPt[0], this.cursorPt[1]]);
    return out;
  }
}

// ---------- gumka ----------

export class EraserTool extends Base {
  id = "eraser";
  private pts: Pt[] = [];

  down(_p: SnapResult, e: PointerEvent) {
    this.pts = [worldOf(this.host.view, e)];
  }

  move(_p: SnapResult, e: PointerEvent, pressed: boolean) {
    if (!pressed || !this.pts.length) return;
    const q = worldOf(this.host.view, e);
    const last = this.pts[this.pts.length - 1];
    if (Math.hypot(q[0] - last[0], q[1] - last[1]) * this.host.view.scale >= 1) this.pts.push(q);
    this.host.redraw();
  }

  up() {
    const pts = this.pts;
    this.pts = [];
    this.host.redraw();
    if (pts.length) void this.host.run("path.erase", { points: pts, width: this.host.toolOpts.eraserWidth });
  }

  preview() {
    if (this.pts.length < 2) return [];
    // obrys pociągnięcia: linia środkowa i obie krawędzie (szerokość gumki)
    const w = this.host.toolOpts.eraserWidth / 2;
    const left: number[] = [];
    const right: number[] = [];
    for (let i = 0; i < this.pts.length; i++) {
      const a = this.pts[Math.max(0, i - 1)];
      const b = this.pts[Math.min(this.pts.length - 1, i + 1)];
      const d = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
      const nx = (-(b[1] - a[1]) / d) * w;
      const ny = ((b[0] - a[0]) / d) * w;
      left.push(this.pts[i][0] + nx, this.pts[i][1] + ny);
      right.push(this.pts[i][0] - nx, this.pts[i][1] - ny);
    }
    return [this.pts.flat(), left, right];
  }

  options() {
    return [numOpt(t("opt-eraser-width"), "opt-eraser-width", this.host.toolOpts.eraserWidth, (v) => {
      if (v > 0 && v <= 1000) this.host.setToolOpt("eraserWidth", v);
    })];
  }
}

function numOpt(label: string, testid: string, value: number, commit: (v: number) => void) {
  const input = h("input", { type: "text", inputmode: "decimal", class: "cell", value: String(value), "data-testid": testid, "data-command": "tool.set_option", "aria-label": label, title: label });
  const fire = () => {
    const v = Number(input.value.replace(",", "."));
    if (Number.isFinite(v)) commit(v);
  };
  input.addEventListener("keydown", (e: KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === "Enter") fire();
  });
  input.addEventListener("change", fire);
  return h("label", { class: "opt" }, label, input);
}

// ---------- przycinanie ----------

export class TrimTool extends Base {
  id = "trim";

  down(_p: SnapResult, e: PointerEvent) {
    const s = this.host.scene;
    if (!s) return;
    const [x, y] = worldOf(this.host.view, e);
    const o = pick(s, x, y, 6 / this.host.view.scale);
    if (o && !o.helper) void this.host.run("path.trim", { id: o.id, at: [x, y] });
  }
}

// ---------- wypaczanie ----------

const MODES: Record<string, { cols: number; rows: number; label: string }> = {
  "4": { cols: 1, rows: 1, label: "warp-mode-4" },
  mesh: { cols: 2, rows: 2, label: "warp-mode-mesh" },
  envelope: { cols: 3, rows: 1, label: "warp-mode-envelope" },
};

/** Odwzorowanie biliniowe jak w `bc_geom::cut::mesh_warp`. */
export function meshMap(b: number[], cols: number, rows: number, g: Pt[], x: number, y: number): Pt {
  const u = ((x - b[0]) / (b[2] - b[0])) * cols;
  const v = ((y - b[1]) / (b[3] - b[1])) * rows;
  const i = Math.min(Math.max(Math.floor(u), 0), cols - 1);
  const j = Math.min(Math.max(Math.floor(v), 0), rows - 1);
  const fu = u - i;
  const fv = v - j;
  const at = (ii: number, jj: number) => g[jj * (cols + 1) + ii];
  const [a, bb, c, d] = [at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)];
  return [
    (1 - fu) * (1 - fv) * a[0] + fu * (1 - fv) * bb[0] + (1 - fu) * fv * c[0] + fu * fv * d[0],
    (1 - fu) * (1 - fv) * a[1] + fu * (1 - fv) * bb[1] + (1 - fu) * fv * c[1] + fu * fv * d[1],
  ];
}

export class WarpTool extends Base {
  id = "warp";
  cursor = "default";
  bounds: number[] | null = null;
  grid: Pt[] = [];
  /** Tymczasowa historia (REQ-04-048): poprzednie siatki, bez zmian w dokumencie. */
  private undo: Pt[][] = [];
  private dragging: number | null = null;
  private selKey = "";

  get mode() {
    return MODES[this.host.toolOpts.warpMode] ?? MODES["4"];
  }

  /** Siatka równomierna na ramce zaznaczenia. */
  reset() {
    const s = this.host.scene;
    const b = s ? selectionBox(s) : null;
    this.undo = [];
    this.selKey = s ? s.selection.join(",") : "";
    if (!b || b[2] - b[0] < 1e-9 || b[3] - b[1] < 1e-9) {
      this.bounds = null;
      this.grid = [];
    } else {
      const { cols, rows } = this.mode;
      this.bounds = [...b];
      this.grid = [];
      for (let j = 0; j <= rows; j++) {
        for (let i = 0; i <= cols; i++) {
          this.grid.push([b[0] + ((b[2] - b[0]) * i) / cols, b[1] + ((b[3] - b[1]) * j) / rows]);
        }
      }
    }
    this.host.refreshTools();
    this.host.redraw();
  }

  onScene() {
    const key = this.host.scene?.selection.join(",") ?? "";
    // po zatwierdzeniu albo zmianie zaznaczenia — nowa siatka
    if (!this.undo.length || key !== this.selKey) this.reset();
  }

  down(_p: SnapResult, e: PointerEvent) {
    const [x, y] = worldOf(this.host.view, e);
    const tol = 8 / this.host.view.scale;
    let best = -1;
    let bd = tol;
    this.grid.forEach((g, k) => {
      const d = Math.hypot(g[0] - x, g[1] - y);
      if (d <= bd) {
        bd = d;
        best = k;
      }
    });
    if (best < 0) return;
    this.undo.push(this.grid.map((g) => [g[0], g[1]] as Pt));
    this.dragging = best;
  }

  move(_p: SnapResult, e: PointerEvent, pressed: boolean) {
    if (!pressed || this.dragging === null) return;
    const [x, y] = worldOf(this.host.view, e);
    const s = this.host.snapWorld(x, y, { exclude: new Set(this.host.scene?.selection ?? []) });
    this.grid[this.dragging] = [s.x, s.y];
    this.host.redraw();
  }

  up() {
    this.dragging = null;
  }

  key(e: KeyboardEvent): boolean {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && this.undo.length) {
      this.grid = this.undo.pop()!;
      this.host.redraw();
      return true;
    }
    if (e.key === "Enter" && this.bounds && this.undo.length) {
      const { cols, rows } = this.mode;
      const ids = this.host.scene?.selection ?? [];
      const points = this.grid.map((g) => [g[0], g[1]]);
      const bounds = this.bounds;
      this.undo = [];
      void this.host.run("path.warp", { ids, bounds, cols, rows, points });
      return true;
    }
    if (e.key === "Escape" && this.undo.length) {
      this.reset();
      return true;
    }
    return false;
  }

  preview(): number[][] {
    if (!this.bounds || !this.grid.length) return [];
    const { cols, rows } = this.mode;
    const lines: number[][] = [];
    // linie siatki
    for (let j = 0; j <= rows; j++) lines.push(this.grid.slice(j * (cols + 1), (j + 1) * (cols + 1)).flat());
    for (let i = 0; i <= cols; i++) {
      const col: number[] = [];
      for (let j = 0; j <= rows; j++) col.push(...this.grid[j * (cols + 1) + i]);
      lines.push(col);
    }
    // wypaczone kształty (podgląd na spłaszczonych poliliniach)
    if (this.undo.length) {
      const sel = new Set(this.host.scene?.selection ?? []);
      for (const o of this.host.scene?.objects ?? []) {
        if (!sel.has(o.id)) continue;
        for (const p of o.polylines) {
          const out: number[] = [];
          for (let k = 0; k + 1 < p.length; k += 2) out.push(...meshMap(this.bounds, cols, rows, this.grid, p[k], p[k + 1]));
          lines.push(out);
        }
      }
    }
    return lines;
  }

  markers() {
    return this.grid.map(([x, y]) => ({ x, y, kind: "handle" as const }));
  }

  options() {
    const sel = h(
      "select",
      { "data-testid": "opt-warp-mode", "data-command": "tool.set_option", "aria-label": t("warp-label"), title: t("warp-label"), onchange: (e: Event) => {
        this.host.setToolOpt("warpMode", (e.target as HTMLSelectElement).value);
        this.reset();
      } },
      ...Object.entries(MODES).map(([k, m]) => h("option", { value: k, selected: k === this.host.toolOpts.warpMode }, t(m.label))),
    );
    const hint = t(this.bounds ? "warp-hint-active" : "warp-hint-select");
    return [h("label", { class: "opt" }, t("warp-label"), sel), h("span", { class: "muted" }, hint)];
  }
}
