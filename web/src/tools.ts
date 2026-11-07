// Narzędzia rysowania (REQ-04-001, 002, 005…008). Każde kończy się poleceniem
// z rejestru (shape.rect / ellipse / polygon / star / path) — frontend nie
// zmienia dokumentu sam (REQ-01-036).

import type { SnapResult } from "./snap";

export type Pt = [number, number];

export interface ToolHost {
  /** Aktywna warstwa dla nowych kształtów. */
  layer: number;
  /** Tolerancja „trafienia” punktu w mm (np. zamknięcie konturu). */
  pickTolerance(): number;
  create(id: string, params: Record<string, unknown>): Promise<unknown>;
  redraw(): void;
}

export interface Tool {
  id: string;
  cursor: string;
  down(p: SnapResult, e: PointerEvent): void;
  move(p: SnapResult, e: PointerEvent, pressed: boolean): void;
  up(p: SnapResult, e: PointerEvent): void;
  /** true = klawisz obsłużony przez narzędzie. */
  key(e: KeyboardEvent): boolean;
  /** Linie podglądu w mm. */
  preview(): number[][];
  /** Węzły rysowanej ścieżki — kandydaci przyciągania. */
  extraNodes(): number[];
  /** Tekst wpisywany z klawiatury (null = brak pola). */
  input(): string | null;
  /** Dwuklik na kanwie. */
  dblclick?(e: MouseEvent): void;
  /** Pozycje menu kontekstowego (prawy przycisk); null = przesuwanie widoku. */
  context?(e: MouseEvent): { action: string; label: string; run: () => void }[] | null;
  /** Dodatkowe pola w pasku opcji narzędzia. */
  options?(): HTMLElement[];
  /** Znaczniki o stałym rozmiarze na ekranie (węzły, uchwyty). */
  markers?(): { x: number; y: number; kind: "node" | "node-sel" | "handle" }[];
  /** Nowa scena z hosta. */
  onScene?(): void;
}

export class SelectTool implements Tool {
  id = "select";
  cursor = "default";
  down() {}
  move() {}
  up() {}
  key() {
    return false;
  }
  preview() {
    return [];
  }
  extraNodes() {
    return [];
  }
  input() {
    return null;
  }
}

// ---------- ustawienia narzędzi ----------

export interface ToolOptions {
  sides: number;
  points: number;
  ratio: number;
  /** Szerokość gumki (mm). */
  eraserWidth: number;
  /** Rodzaj wypaczania: "4", "mesh", "envelope". */
  warpMode: string;
}

const OPT_KEY = "bc.tools";

export function loadOptions(): ToolOptions {
  const d: ToolOptions = { sides: 6, points: 5, ratio: 0.5, eraserWidth: 2, warpMode: "4" };
  try {
    return { ...d, ...JSON.parse(localStorage.getItem(OPT_KEY) ?? "{}") };
  } catch {
    return d;
  }
}

export function saveOptions(o: ToolOptions) {
  try {
    localStorage.setItem(OPT_KEY, JSON.stringify(o));
  } catch {
    // tylko w tej sesji
  }
}

// ---------- kształty przeciągane ----------

type ShapeKind = "rect" | "ellipse" | "polygon" | "star";

const round = (v: number) => Number(v.toFixed(9)) + 0;
const deg = (r: number) => (r * 180) / Math.PI;

function ring(cx: number, cy: number, radii: number[], n: number, rot: number): number[] {
  const out: number[] = [];
  for (let i = 0; i <= n; i++) {
    const a = rot + (2 * Math.PI * i) / n;
    const r = radii[i % radii.length];
    out.push(cx + r * Math.cos(a), cy + r * Math.sin(a));
  }
  return out;
}

export class ShapeTool implements Tool {
  cursor = "crosshair";
  private a: Pt | null = null;
  private b: Pt | null = null;
  private square = false;
  private centered = false;

  constructor(public id: ShapeKind, private host: ToolHost, private opts: () => ToolOptions) {}

  down(p: SnapResult) {
    this.a = [p.x, p.y];
    this.b = [p.x, p.y];
  }

  move(p: SnapResult, e: PointerEvent, pressed: boolean) {
    if (!pressed || !this.a) return;
    this.b = [p.x, p.y];
    this.square = e.shiftKey;
    this.centered = e.ctrlKey || e.metaKey;
    this.host.redraw();
  }

  up(p: SnapResult, e: PointerEvent) {
    if (!this.a) return;
    this.b = [p.x, p.y];
    this.square = e.shiftKey;
    this.centered = e.ctrlKey || e.metaKey;
    const params = this.params();
    this.a = this.b = null;
    this.host.redraw();
    if (params) this.host.create(`shape.${this.id}`, { ...params, layer: this.host.layer });
  }

  key(e: KeyboardEvent) {
    if (e.key === "Escape" && this.a) {
      this.a = this.b = null;
      this.host.redraw();
      return true;
    }
    return false;
  }

  /** Prostokąt (x0,y0,x1,y1) z uwzględnieniem Shift (kwadrat) i Ctrl (od środka). */
  private box(): [number, number, number, number] | null {
    if (!this.a || !this.b) return null;
    let [ax, ay] = this.a;
    let dx = this.b[0] - ax;
    let dy = this.b[1] - ay;
    if (this.square) {
      const s = Math.max(Math.abs(dx), Math.abs(dy));
      dx = Math.sign(dx || 1) * s;
      dy = Math.sign(dy || 1) * s;
    }
    if (this.centered) {
      ax -= dx;
      ay -= dy;
      dx *= 2;
      dy *= 2;
    }
    const x0 = Math.min(ax, ax + dx), x1 = Math.max(ax, ax + dx);
    const y0 = Math.min(ay, ay + dy), y1 = Math.max(ay, ay + dy);
    if (x1 - x0 < 1e-6 || y1 - y0 < 1e-6) return null;
    return [round(x0), round(y0), round(x1), round(y1)];
  }

  private radial(): { cx: number; cy: number; r: number; rot: number } | null {
    if (!this.a || !this.b) return null;
    const dx = this.b[0] - this.a[0];
    const dy = this.b[1] - this.a[1];
    const r = Math.hypot(dx, dy);
    if (r < 1e-6) return null;
    return { cx: this.a[0], cy: this.a[1], r: round(r), rot: round(deg(Math.atan2(dy, dx))) };
  }

  private params(): Record<string, unknown> | null {
    const o = this.opts();
    if (this.id === "rect" || this.id === "ellipse") {
      const b = this.box();
      if (!b) return null;
      const [x0, y0, x1, y1] = b;
      return this.id === "rect"
        ? { x: x0, y: y0, w: round(x1 - x0), h: round(y1 - y0) }
        : { cx: round((x0 + x1) / 2), cy: round((y0 + y1) / 2), rx: round((x1 - x0) / 2), ry: round((y1 - y0) / 2) };
    }
    const r = this.radial();
    if (!r) return null;
    return this.id === "polygon"
      ? { cx: r.cx, cy: r.cy, r: r.r, sides: o.sides, rotation: r.rot }
      : { cx: r.cx, cy: r.cy, r_outer: r.r, r_inner: round(r.r * o.ratio), points: o.points, rotation: r.rot };
  }

  preview(): number[][] {
    const o = this.opts();
    if (this.id === "rect" || this.id === "ellipse") {
      const b = this.box();
      if (!b) return [];
      const [x0, y0, x1, y1] = b;
      if (this.id === "rect") return [[x0, y0, x1, y0, x1, y1, x0, y1, x0, y0]];
      const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2, rx = (x1 - x0) / 2, ry = (y1 - y0) / 2;
      const out: number[] = [];
      for (let i = 0; i <= 72; i++) {
        const t = (2 * Math.PI * i) / 72;
        out.push(cx + rx * Math.cos(t), cy + ry * Math.sin(t));
      }
      return [out];
    }
    const r = this.radial();
    if (!r) return [];
    const rot = (r.rot * Math.PI) / 180;
    return this.id === "polygon"
      ? [ring(r.cx, r.cy, [r.r], o.sides, rot)]
      : [ring(r.cx, r.cy, [r.r, r.r * o.ratio], o.points * 2, rot)];
  }

  extraNodes() {
    return [];
  }

  input() {
    return null;
  }
}

// ---------- linie i krzywe Béziera ----------

interface Node {
  p: Pt;
  hin?: Pt;
  hout?: Pt;
}

const INPUT_CHARS = /^[0-9.,<@-]$/;
const fmt = (v: number) => String(round(v));

/** Linia/polilinia z Bézierami: klik = węzeł ostry, wciśnięcie z przeciągnięciem = węzeł gładki. */
export class LineTool implements Tool {
  id = "line";
  cursor = "crosshair";
  private nodes: Node[] = [];
  private cursorPt: Pt | null = null;
  private pressed: { start: Pt; moved: boolean; close: boolean } | null = null;
  private text: string | null = null;

  constructor(private host: ToolHost) {}

  get drawing() {
    return this.nodes.length > 0;
  }

  down(p: SnapResult) {
    const q: Pt = [p.x, p.y];
    const first = this.nodes[0];
    const close = !!first && this.nodes.length >= 2 && Math.hypot(q[0] - first.p[0], q[1] - first.p[1]) <= this.host.pickTolerance();
    if (!close) this.nodes.push({ p: q });
    this.pressed = { start: q, moved: false, close };
    this.text = this.drawing ? "" : null;
    this.host.redraw();
  }

  move(p: SnapResult, _e: PointerEvent, pressed: boolean) {
    this.cursorPt = [p.x, p.y];
    const pr = this.pressed;
    if (pressed && pr && !pr.close) {
      const n = this.nodes[this.nodes.length - 1];
      const d = Math.hypot(p.x - n.p[0], p.y - n.p[1]);
      if (d > this.host.pickTolerance() / 2) {
        pr.moved = true;
        n.hout = [p.x, p.y];
        n.hin = [2 * n.p[0] - p.x, 2 * n.p[1] - p.y];
      }
    }
    this.host.redraw();
  }

  up() {
    const pr = this.pressed;
    this.pressed = null;
    if (pr?.close) this.finish(true);
  }

  key(e: KeyboardEvent): boolean {
    if (!this.drawing) return false;
    if (e.key === "Escape") {
      this.reset();
      return true;
    }
    if (e.key === "Enter") {
      const t = (this.text ?? "").trim();
      if (t) {
        const q = this.parse(t);
        if (q) this.nodes.push({ p: q });
        this.text = "";
      } else {
        this.finish(false);
      }
      this.host.redraw();
      return true;
    }
    if (e.key === "Backspace") {
      if (this.text) this.text = this.text.slice(0, -1);
      else if (this.nodes.length > 1) this.nodes.pop();
      else this.reset();
      this.host.redraw();
      return true;
    }
    if (!e.ctrlKey && !e.metaKey && !e.altKey && INPUT_CHARS.test(e.key)) {
      this.text = (this.text ?? "") + e.key;
      this.host.redraw();
      return true;
    }
    return false;
  }

  /** „L” (wzdłuż poprzedniego odcinka), „L<A” (długość, kąt CCW od +X), „@dx,dy” (przyrost). */
  private parse(t: string): Pt | null {
    const last = this.nodes[this.nodes.length - 1].p;
    if (t.startsWith("@")) {
      const parts = t.slice(1).split(",").map(Number);
      if (parts.length !== 2 || parts.some((v) => !Number.isFinite(v))) return null;
      return [round(last[0] + parts[0]), round(last[1] + parts[1])];
    }
    const [ls, as] = t.replace(/,/g, ".").split("<");
    const len = Number(ls);
    if (!Number.isFinite(len)) return null;
    let ang: number;
    if (as !== undefined) {
      ang = (Number(as) * Math.PI) / 180;
      if (!Number.isFinite(ang)) return null;
    } else {
      const prev = this.nodes[this.nodes.length - 2]?.p;
      ang = prev ? Math.atan2(last[1] - prev[1], last[0] - prev[0]) : 0;
    }
    return [round(last[0] + len * Math.cos(ang)), round(last[1] + len * Math.sin(ang))];
  }

  private segment(a: Node, b: Node): string {
    const [x, y] = b.p;
    if (!a.hout && !b.hin) return ` L ${fmt(x)} ${fmt(y)}`;
    const c1 = a.hout ?? a.p;
    const c2 = b.hin ?? b.p;
    return ` C ${fmt(c1[0])} ${fmt(c1[1])} ${fmt(c2[0])} ${fmt(c2[1])} ${fmt(x)} ${fmt(y)}`;
  }

  private finish(closed: boolean) {
    const n = this.nodes;
    if (n.length >= 2) {
      let d = `M ${fmt(n[0].p[0])} ${fmt(n[0].p[1])}`;
      for (let i = 1; i < n.length; i++) d += this.segment(n[i - 1], n[i]);
      if (closed) d += this.segment(n[n.length - 1], n[0]) + " Z";
      this.host.create("shape.path", { d, layer: this.host.layer });
    }
    this.reset();
  }

  private reset() {
    this.nodes = [];
    this.text = null;
    this.pressed = null;
    this.host.redraw();
  }

  /** Spłaszczona ścieżka z gumką do kursora. */
  preview(): number[][] {
    if (!this.drawing) return [];
    const n = [...this.nodes];
    if (this.cursorPt && !this.pressed) n.push({ p: this.cursorPt });
    const out: number[] = [n[0].p[0], n[0].p[1]];
    for (let i = 1; i < n.length; i++) {
      const a = n[i - 1], b = n[i];
      const c1 = a.hout ?? a.p, c2 = b.hin ?? b.p;
      for (let k = 1; k <= 24; k++) {
        const t = k / 24, u = 1 - t;
        out.push(
          u * u * u * a.p[0] + 3 * u * u * t * c1[0] + 3 * u * t * t * c2[0] + t * t * t * b.p[0],
          u * u * u * a.p[1] + 3 * u * u * t * c1[1] + 3 * u * t * t * c2[1] + t * t * t * b.p[1],
        );
      }
    }
    const lines = [out];
    const last = this.nodes[this.nodes.length - 1];
    if (last.hin && last.hout) lines.push([last.hin[0], last.hin[1], last.hout[0], last.hout[1]]);
    return lines;
  }

  extraNodes(): number[] {
    return this.nodes.flatMap((n) => n.p);
  }

  input() {
    return this.text;
  }
}
