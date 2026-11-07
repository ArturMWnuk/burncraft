// Narzędzie edycji węzłów (M1.4b2, REQ-04-023…035, REQ-03-056): węzły
// i uchwyty edytowanego obiektu, przeciąganie, dodawanie dwuklikiem, menu
// kontekstowe. Każda zmiana to polecenie `node.*` z rejestru (REQ-01-036).

import { h } from "./dom";
import { pick } from "./hit";
import { t } from "./i18n";
import { fmtLen, parseLen } from "./units";
import type { Scene } from "./scene";
import type { SnapOptions, SnapResult } from "./snap";
import type { Tool } from "./tools";
import type { View } from "./view";

export interface NodeOut {
  p: [number, number];
  hin: [number, number] | null;
  hout: [number, number] | null;
  smooth: boolean;
}

export interface ContourOut {
  closed: boolean;
  nodes: NodeOut[];
}

export interface MenuItem {
  action: string;
  label: string;
  run: () => void;
}

export interface Marker {
  x: number;
  y: number;
  kind: "node" | "node-sel" | "handle";
}

export interface NodeHost {
  view: View;
  scene: Scene | null;
  run(id: string, params?: unknown): Promise<any>;
  batch(steps: { id: string; params: unknown }[]): Promise<any>;
  snapWorld(x: number, y: number, opt?: SnapOptions): SnapResult;
  selectObjects(ids: number[]): void;
  redraw(): void;
  refreshTools(): void;
}

type Hit =
  | { kind: "handle"; node: number; side: "in" | "out" }
  | { kind: "node"; node: number }
  | { kind: "segment"; segment: number; at: [number, number] }
  | { kind: "object"; id: number }
  | { kind: "none" };

type Drag =
  | { kind: "nodes"; start: [number, number]; nodes: number[]; offset: [number, number]; moved: boolean }
  | { kind: "handle"; node: number; side: "in" | "out"; to: [number, number] | null };

const HIT_PX = 6;

/** Punkt na krzywej Béziera (t ∈ [0, 1]). */
function bez(a: [number, number], b: [number, number], c: [number, number], d: [number, number], t: number): [number, number] {
  const u = 1 - t;
  return [
    u * u * u * a[0] + 3 * u * u * t * b[0] + 3 * u * t * t * c[0] + t * t * t * d[0],
    u * u * u * a[1] + 3 * u * u * t * b[1] + 3 * u * t * t * c[1] + t * t * t * d[1],
  ];
}

export class NodeTool implements Tool {
  id = "node";
  cursor = "default";
  /** Edytowany obiekt. */
  edited: number | null = null;
  contours: ContourOut[] = [];
  selected = new Set<number>();
  private drag: Drag | null = null;
  private loading = 0;

  constructor(private host: NodeHost) {}

  // ---------- stan ----------

  /** Węzły kolejno we wszystkich konturach (numeracja poleceń node.*). */
  private flat(): { n: NodeOut; c: number; i: number }[] {
    return this.contours.flatMap((c, ci) => c.nodes.map((n, i) => ({ n, c: ci, i })));
  }

  /** Rozpoczyna edycję obiektu (albo kończy dla `null`). */
  async edit(id: number | null) {
    if (id !== this.edited) this.selected.clear();
    this.edited = id;
    await this.reload();
  }

  /** Węzły na nowo z rdzenia (po każdej zmianie dokumentu). */
  async reload() {
    const id = this.edited;
    const ticket = ++this.loading;
    if (id === null || !this.host.scene?.objects.some((o) => o.id === id)) {
      this.edited = null;
      this.contours = [];
      this.selected.clear();
    } else {
      const r = await this.host.run("node.list", { id });
      if (ticket !== this.loading) return; // nowsze odświeżenie w toku
      this.contours = r?.contours ?? [];
      const n = this.flat().length;
      for (const s of [...this.selected]) if (s >= n) this.selected.delete(s);
    }
    this.host.refreshTools();
    this.host.redraw();
  }

  /** Wywoływane po nowej scenie. */
  onScene() {
    if (this.edited === null) {
      // narzędzie włączone przy zaznaczonym jednym obiekcie — edytuj go
      const sel = this.host.scene?.selection ?? [];
      const o = sel.length === 1 ? this.host.scene?.objects.find((x) => x.id === sel[0]) : undefined;
      if (o && o.type !== "group" && o.type !== "array") void this.edit(o.id);
      return;
    }
    void this.reload();
  }

  select(nodes: number[]) {
    this.selected = new Set(nodes);
    this.host.refreshTools();
    this.host.redraw();
  }

  // ---------- trafienia ----------

  private hit(x: number, y: number): Hit {
    const tol = HIT_PX / this.host.view.scale;
    const d = (p: [number, number]) => Math.hypot(p[0] - x, p[1] - y);
    const flat = this.flat();
    for (const k of this.selected) {
      const n = flat[k]?.n;
      if (!n) continue;
      if (n.hout && d(n.hout) <= tol) return { kind: "handle", node: k, side: "out" };
      if (n.hin && d(n.hin) <= tol) return { kind: "handle", node: k, side: "in" };
    }
    let best = -1;
    let bd = tol;
    flat.forEach(({ n }, k) => {
      const dd = d(n.p);
      if (dd <= bd) {
        bd = dd;
        best = k;
      }
    });
    if (best >= 0) return { kind: "node", node: best };
    const seg = this.nearestSegment(x, y, tol);
    if (seg) return { kind: "segment", ...seg };
    const o = this.host.scene ? pick(this.host.scene, x, y, tol) : null;
    if (o) return { kind: "object", id: o.id };
    return { kind: "none" };
  }

  private nearestSegment(x: number, y: number, tol: number): { segment: number; at: [number, number] } | null {
    let best: { segment: number; at: [number, number] } | null = null;
    let bd = tol;
    let base = 0;
    for (const c of this.contours) {
      const n = c.nodes.length;
      const segs = c.closed ? n : n - 1;
      for (let i = 0; i < segs; i++) {
        const a = c.nodes[i];
        const b = c.nodes[(i + 1) % n];
        const c1 = a.hout ?? a.p;
        const c2 = b.hin ?? b.p;
        for (let k = 0; k <= 32; k++) {
          const q = bez(a.p, c1, c2, b.p, k / 32);
          const dd = Math.hypot(q[0] - x, q[1] - y);
          if (dd <= bd) {
            bd = dd;
            best = { segment: base + i, at: q };
          }
        }
      }
      base += n;
    }
    return best;
  }

  private segmentIsLine(seg: number): boolean {
    let base = 0;
    for (const c of this.contours) {
      if (seg < base + c.nodes.length) {
        const i = seg - base;
        return !c.nodes[i].hout && !c.nodes[(i + 1) % c.nodes.length].hin;
      }
      base += c.nodes.length;
    }
    return true;
  }

  // ---------- zdarzenia ----------

  private world(e: PointerEvent | MouseEvent): [number, number] {
    const r = (e.target as HTMLElement).getBoundingClientRect();
    return this.host.view.toWorld(e.clientX - r.left, e.clientY - r.top);
  }

  /** Przyciąganie bez edytowanego obiektu; jego pozostałe węzły jako kandydaci. */
  private snapExcept(x: number, y: number, moving: Set<number>): [number, number] {
    const extra = this.flat().filter((_, k) => !moving.has(k)).flatMap(({ n }) => n.p);
    const s = this.host.snapWorld(x, y, { exclude: new Set(this.edited === null ? [] : [this.edited]), extraNodes: extra });
    return [s.x, s.y];
  }

  down(_p: SnapResult, e: PointerEvent) {
    const [x, y] = this.world(e);
    const h = this.hit(x, y);
    if (h.kind === "handle") {
      this.drag = { kind: "handle", node: h.node, side: h.side, to: null };
    } else if (h.kind === "node") {
      if (e.shiftKey) {
        if (this.selected.has(h.node)) this.selected.delete(h.node);
        else this.selected.add(h.node);
        this.host.refreshTools();
        this.host.redraw();
        return;
      }
      if (!this.selected.has(h.node)) this.selected = new Set([h.node]);
      const p = this.flat()[h.node].n.p;
      this.drag = { kind: "nodes", start: [p[0], p[1]], nodes: [...this.selected], offset: [0, 0], moved: false };
      this.host.refreshTools();
    } else if (h.kind === "object") {
      this.host.selectObjects([h.id]);
      void this.edit(h.id);
    } else if (h.kind === "none" && !e.shiftKey) {
      this.selected.clear();
      this.host.refreshTools();
    }
    this.host.redraw();
  }

  move(_p: SnapResult, e: PointerEvent, pressed: boolean) {
    const d = this.drag;
    if (!d || !pressed) return;
    const [x, y] = this.world(e);
    if (d.kind === "nodes") {
      const t = this.snapExcept(x, y, new Set(d.nodes));
      d.offset = [t[0] - d.start[0], t[1] - d.start[1]];
      if (Math.hypot(d.offset[0], d.offset[1]) * this.host.view.scale > 1) d.moved = true;
    } else {
      d.to = this.snapExcept(x, y, new Set());
    }
    this.host.redraw();
  }

  async up() {
    const d = this.drag;
    this.drag = null;
    const id = this.edited;
    if (!d || id === null) return this.host.redraw();
    if (d.kind === "nodes" && d.moved) {
      const [dx, dy] = d.offset;
      if (d.nodes.length === 1) {
        await this.host.run("node.move", { id, node: d.nodes[0], to: [d.start[0] + dx, d.start[1] + dy] });
      } else {
        await this.host.batch(d.nodes.map((node) => ({ id: "node.move", params: { id, node, by: [dx, dy] } })));
      }
    } else if (d.kind === "handle" && d.to) {
      await this.host.run("node.set_handle", { id, node: d.node, side: d.side, to: d.to });
    }
    this.host.redraw();
  }

  async dblclick(e: MouseEvent) {
    const [x, y] = this.world(e);
    const h = this.hit(x, y);
    if (h.kind !== "segment" || this.edited === null) return;
    // punkt na segmencie przyciągnięty do środka odcinka / krawędzi
    const s = this.host.snapWorld(h.at[0], h.at[1], { grid: false });
    const r = await this.host.run("node.add", { id: this.edited, at: s.kind === "none" ? h.at : [s.x, s.y] });
    if (r) this.select([r.node]);
  }

  key(e: KeyboardEvent): boolean {
    if (this.edited === null) return false;
    if ((e.key === "Delete" || e.key === "Backspace") && this.selected.size) {
      const nodes = [...this.selected];
      this.selected.clear();
      void this.host.run("node.delete", { id: this.edited, nodes });
      return true;
    }
    if (e.key === "Escape" && this.selected.size) {
      this.select([]);
      return true;
    }
    return false;
  }

  /** Pozycje menu kontekstowego w punkcie (REQ-03-056). */
  context(e: MouseEvent): MenuItem[] | null {
    const id = this.edited;
    if (id === null) return null;
    const [x, y] = this.world(e);
    const h = this.hit(x, y);
    const items: MenuItem[] = [];
    const run = (cmd: string, params: Record<string, unknown>) => () => void this.host.run(cmd, { id, ...params });
    if (h.kind === "node" && !this.selected.has(h.node)) this.select([h.node]);
    const sel = [...this.selected].sort((a, b) => a - b);
    // etykieta z tłumaczeń: ctx-<akcja>
    const item = (action: string, fn: () => void): MenuItem => ({ action, label: t(`ctx-${action}`), run: fn });
    if (h.kind === "node" || h.kind === "handle") {
      items.push(
        item("delete", () => { this.selected.clear(); void this.host.run("node.delete", { id, nodes: sel }); }),
        item("smooth", run("node.smooth", { nodes: sel })),
        item("corner", run("node.corner", { nodes: sel })),
      );
      if (sel.length === 1) items.push(item("break", () => { this.selected.clear(); void this.host.run("node.break", { id, node: sel[0] }); }));
    }
    if (h.kind === "segment") {
      const line = this.segmentIsLine(h.segment);
      items.push(
        line
          ? item("curve", run("node.segment", { segment: h.segment, kind: "curve" }))
          : item("line", run("node.segment", { segment: h.segment, kind: "line" })),
        item("add", async () => {
          const r = await this.host.run("node.add", { id, at: h.at });
          if (r) this.select([r.node]);
        }),
        item("cut", () => { this.selected.clear(); void this.host.run("path.cut", { id, at: h.at }); }),
      );
    }
    if (sel.length === 2) items.push(item("join", () => { this.selected.clear(); void this.host.run("node.join", { id, a: sel[0], b: sel[1] }); }));
    if (sel.length >= 2) {
      items.push(item("align-y", run("node.align", { nodes: sel, axis: "y" })), item("align-x", run("node.align", { nodes: sel, axis: "x" })));
    }
    return items.length ? items : null;
  }

  /** Akcje paska węzłów (REQ-03-030) na zaznaczonych węzłach; `null` = niedostępna. */
  actions(): Record<string, (() => void) | null> {
    const id = this.edited;
    const sel = [...this.selected].sort((a, b) => a - b);
    const on = (ok: boolean, f: () => void) => (id !== null && ok ? f : null);
    const run = (cmd: string, params: Record<string, unknown>, clear = false) => () => {
      if (clear) this.selected.clear();
      void this.host.run(cmd, { id, ...params });
    };
    return {
      delete: on(sel.length > 0, run("node.delete", { nodes: sel }, true)),
      smooth: on(sel.length > 0, run("node.smooth", { nodes: sel })),
      corner: on(sel.length > 0, run("node.corner", { nodes: sel })),
      break: on(sel.length === 1, run("node.break", { node: sel[0] }, true)),
      join: on(sel.length === 2, run("node.join", { a: sel[0], b: sel[1] }, true)),
      "align-y": on(sel.length >= 2, run("node.align", { nodes: sel, axis: "y" })),
      "align-x": on(sel.length >= 2, run("node.align", { nodes: sel, axis: "x" })),
    };
  }

  /** Pola X/Y zaznaczonego węzła (REQ-04-032). */
  options(): HTMLElement[] {
    const flat = this.flat();
    const one = this.selected.size === 1 ? flat[[...this.selected][0]] : undefined;
    const field = (axis: 0 | 1, label: string) => {
      const input = h("input", {
        type: "text", inputmode: "decimal", class: "cell", "data-testid": `node-${label.toLowerCase()}`, "data-command": "node.move",
        "aria-label": t("node-field", { axis: label }), title: t("node-field", { axis: label }),
        value: one ? fmtLen(one.n.p[axis]) : "", disabled: !one,
      });
      input.addEventListener("keydown", (e: KeyboardEvent) => {
        e.stopPropagation();
        if (e.key !== "Enter" || !one || this.edited === null) return;
        const v = parseLen(input.value);
        if (v === null) return;
        const to: [number, number] = [one.n.p[0], one.n.p[1]];
        to[axis] = v;
        void this.host.run("node.move", { id: this.edited, node: [...this.selected][0], to });
      });
      return h("label", { class: "opt" }, label, input);
    };
    return [
      h("span", { class: "muted" }, this.edited === null ? t("node-click-object") : t("node-count", { n: flat.length, sel: this.selected.size })),
      field(0, "X"),
      field(1, "Y"),
    ];
  }

  // ---------- rysowanie ----------

  /** Kontury z uwzględnieniem przeciągania (podgląd) i linie uchwytów. */
  preview(): number[][] {
    const d = this.drag;
    const moved = d?.kind === "nodes" && d.moved ? new Set(d.nodes) : new Set<number>();
    const off = d?.kind === "nodes" ? d.offset : [0, 0];
    const lines: number[][] = [];
    let k = 0;
    for (const c of this.contours) {
      const pts = c.nodes.map((n) => {
        const m = moved.has(k++);
        const sh = (p: [number, number] | null): [number, number] | null => (p && m ? [p[0] + off[0], p[1] + off[1]] : p);
        let hin = sh(n.hin);
        let hout = sh(n.hout);
        if (d?.kind === "handle" && d.to && d.node === k - 1) {
          if (d.side === "in") hin = d.to;
          else hout = d.to;
        }
        return { p: sh(n.p)!, hin, hout };
      });
      if (d) {
        const n = pts.length;
        const segs = c.closed ? n : n - 1;
        const out: number[] = [pts[0].p[0], pts[0].p[1]];
        for (let i = 0; i < segs; i++) {
          const a = pts[i];
          const b = pts[(i + 1) % n];
          for (let s = 1; s <= 24; s++) out.push(...bez(a.p, a.hout ?? a.p, b.hin ?? b.p, b.p, s / 24));
        }
        lines.push(out);
      }
    }
    // uchwyty zaznaczonych węzłów
    const flat = this.flat();
    for (const s of this.selected) {
      const n = flat[s]?.n;
      if (!n) continue;
      if (n.hin) lines.push([n.hin[0], n.hin[1], n.p[0], n.p[1]]);
      if (n.hout) lines.push([n.p[0], n.p[1], n.hout[0], n.hout[1]]);
    }
    return lines;
  }

  markers(): Marker[] {
    const out: Marker[] = [];
    this.flat().forEach(({ n }, k) => {
      const sel = this.selected.has(k);
      out.push({ x: n.p[0], y: n.p[1], kind: sel ? "node-sel" : "node" });
      if (sel) {
        if (n.hin) out.push({ x: n.hin[0], y: n.hin[1], kind: "handle" });
        if (n.hout) out.push({ x: n.hout[0], y: n.hout[1], kind: "handle" });
      }
    });
    return out;
  }

  extraNodes() {
    return [];
  }

  input() {
    return null;
  }
}
