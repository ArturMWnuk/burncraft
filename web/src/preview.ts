// Okno Podgląd (REQ-03-046): tor głowicy z `plan.preview` z odtwarzaniem — suwak
// czasu i prędkość (REQ-09-001), przejazdy (002), kolejność (003), kerf (004,
// REQ-07-030), kierunek (005), punkty startowe (006) i „rozpocznij od tego
// miejsca” (013). Sterowanie to polecenia okna `preview.*` (REQ-01-029).

import { h } from "./dom";
import { displayColor } from "./renderer";
import { hexToRgb } from "./scene";
import { t } from "./i18n";

interface Seg {
  op: number;
  kind: "travel" | "cut" | "dwell";
  from: [number, number];
  to: [number, number];
  power: number;
  t0: number;
  t1: number;
  dir: [number, number] | null;
}

interface PreviewData {
  total_s: number;
  ops: { layer: number; label: string; pass: number; passes: number; color: string }[];
  segments: Seg[];
  parts: { index: number; op: number; segment: number; start: [number, number] }[];
  kerf: { layer: number; object: number; points: [number, number][]; closed: boolean }[];
  kerf_width: number[];
}

export interface PreviewHost {
  rpc: { execute<T = any>(id: string, params?: unknown): Promise<T> };
  exec(id: string, params?: unknown): unknown;
  /** „Rozpocznij od tego miejsca” — `job.start {from_segment}`. */
  startFrom(segment: number): Promise<void>;
  /** Kontury projektu obiektów (płaskie x,y w mm) — odniesienie dla kerfu. */
  designOutlines(ids: number[]): { points: number[]; closed: boolean }[];
  modal: boolean;
}

/** Kolor warstwy widoczny na tle podglądu (jak na kanwie edytora). */
function layerColor(hex: string): string {
  const [r, g, b] = displayColor(hexToRgb(hex));
  return `rgb(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)})`;
}

export const SPEEDS = [1, 2, 5, 10, 50, 100, 500];

const TOGGLES = ["travel", "order", "kerf", "direction", "starts"] as const;
type Toggle = (typeof TOGGLES)[number];

function hms(s: number): string {
  const v = Math.max(0, Math.round(s));
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(v / 3600))}:${p(Math.floor(v / 60) % 60)}:${p(v % 60)}`;
}

export class Preview {
  state = {
    open: false,
    time: 0,
    total: 0,
    speed: 10,
    playing: false,
    travel: true,
    order: true,
    kerf: true,
    direction: true,
    starts: true,
    /** Segment w bieżącym czasie. */
    segment: 0,
    /** Położenie głowicy w bieżącym czasie (mm). */
    head: [0, 0] as [number, number],
  };
  /** Liczba narysowanych elementów (diagnostyka, testy). */
  stats = { cut: 0, travel: 0, arrows: 0, starts: 0, labels: 0, kerf: 0, kerfWidthMm: [] as number[] };
  data: PreviewData | null = null;
  private root: HTMLElement | null = null;
  private canvas = h("canvas", { class: "preview-canvas", "data-testid": "preview-canvas" });
  private slider = h("input", { type: "range", min: "0", max: "1", step: "any", "data-testid": "preview-slider", "data-command": "preview.set" });
  private timeLabel = h("span", { "data-testid": "preview-time", class: "preview-time" });
  private playBtn = h("button", { "data-testid": "preview-play", "data-command": "preview.set" });
  private raf = 0;
  private last = 0;
  private selectionOnly = false;
  private loading: Promise<void> | null = null;

  constructor(private host: PreviewHost) {
    this.slider.addEventListener("input", () => this.host.exec("preview.set", { time: Number(this.slider.value) }));
  }

  /** Polecenie `preview.open`. */
  open(p: { selection_only?: boolean }) {
    this.selectionOnly = Boolean(p.selection_only);
    this.state.open = true;
    this.state.playing = false;
    this.loading = this.load(true);
    return this.public();
  }

  close() {
    this.state.open = false;
    this.state.playing = false;
    cancelAnimationFrame(this.raf);
    this.raf = 0;
    this.root?.remove();
    this.root = null;
    this.host.modal = false;
    return {};
  }

  /** Dokument się zmienił: przeliczenie podglądu (okno otwarte). */
  refresh() {
    if (this.state.open && !this.loading) this.loading = this.load(false);
  }

  private async load(reset: boolean) {
    try {
      this.data = await this.host.rpc.execute<PreviewData>("plan.preview", this.selectionOnly ? { selection_only: true } : {});
    } catch (e) {
      this.data = { total_s: 0, ops: [], segments: [], parts: [], kerf: [], kerf_width: [] };
    } finally {
      this.loading = null;
    }
    if (!this.state.open) return;
    this.state.total = this.data.total_s;
    if (reset || this.state.time > this.state.total) this.state.time = this.state.total;
    this.build();
    this.draw();
  }

  /** Polecenie `preview.set`. */
  set(p: Record<string, unknown>) {
    if (!this.state.open) throw new Error("okno Podgląd jest zamknięte — preview.open");
    const s = this.state;
    if (p.time !== undefined) {
      const v = Number(p.time);
      if (typeof p.time !== "number" || !Number.isFinite(v) || v < 0) throw new Error("time: ≥ 0");
      s.time = Math.min(v, s.total);
    }
    if (p.speed !== undefined) {
      const v = p.speed;
      if (typeof v !== "number" || !(v >= 1 && v <= 1000)) throw new Error("speed: 1–1000");
      s.speed = v;
    }
    for (const k of TOGGLES) {
      if (p[k] === undefined) continue;
      if (typeof p[k] !== "boolean") throw new Error(`${k}: true/false`);
      s[k] = p[k] as boolean;
    }
    if (p.playing !== undefined) {
      if (typeof p.playing !== "boolean") throw new Error("playing: true/false");
      if (p.playing && s.time >= s.total) s.time = 0;
      s.playing = p.playing;
      if (s.playing) this.play();
    }
    this.syncControls();
    this.draw();
    return this.public();
  }

  /** Czas początku części `i` (od 0) w kolejności cięcia. */
  partTime(i: number): number {
    const d = this.data;
    const part = d?.parts[i];
    return part ? d!.segments[part.segment].t0 : 0;
  }

  private public() {
    const s = this.state;
    return { open: s.open, time: s.time, total: s.total, speed: s.speed, playing: s.playing, segment: s.segment };
  }

  private play() {
    cancelAnimationFrame(this.raf);
    this.last = performance.now();
    const step = (now: number) => {
      const s = this.state;
      if (!s.playing || !s.open) return;
      s.time = Math.min(s.total, s.time + ((now - this.last) / 1000) * s.speed);
      this.last = now;
      if (s.time >= s.total) s.playing = false;
      this.syncControls();
      this.draw();
      if (s.playing) this.raf = requestAnimationFrame(step);
    };
    this.raf = requestAnimationFrame(step);
  }

  private build() {
    if (this.root) {
      this.syncControls();
      return;
    }
    this.host.modal = true;
    const toggle = (k: Toggle) =>
      h("label", { class: "opt", title: t(`preview-${k}`) },
        h("input", { type: "checkbox", checked: this.state[k], "data-testid": `preview-${k}`, "data-command": "preview.set", title: t(`preview-${k}`), "aria-label": t(`preview-${k}`),
          onchange: (e: Event) => this.host.exec("preview.set", { [k]: (e.target as HTMLInputElement).checked }) }),
        t(`preview-${k}`));
    const speed = h("select", { "data-testid": "preview-speed", "data-command": "preview.set", title: t("preview-speed"), "aria-label": t("preview-speed"),
      onchange: (e: Event) => this.host.exec("preview.set", { speed: Number((e.target as HTMLSelectElement).value) }) },
      ...SPEEDS.map((v) => h("option", { value: String(v), selected: v === this.state.speed }, `×${v}`)));
    this.playBtn.onclick = () => this.host.exec("preview.set", { playing: !this.state.playing });
    const box = h("div", { class: "dialog preview", role: "dialog", "aria-label": t("preview-title") },
      h("div", { class: "row" }, h("h2", {}, t("preview-title")), h("span", { class: "muted", "data-testid": "preview-summary" }, this.summary())),
      this.canvas,
      h("div", { class: "row" }, this.playBtn, this.slider, this.timeLabel, speed),
      h("div", { class: "row wrap" }, ...TOGGLES.map(toggle)),
      h("div", { class: "row" },
        h("button", { "data-testid": "preview-start-here", "data-command": "job.start", class: "start-btn", title: t("preview-start-here.title"), "aria-label": t("preview-start-here"),
          onclick: () => void this.host.startFrom(this.state.segment) }, t("preview-start-here")),
        h("button", { "data-testid": "preview-close", "data-command": "preview.close", title: t("dialog-close"), onclick: () => this.host.exec("preview.close") }, t("dialog-close"))),
    );
    this.root = h("div", { class: "dialog-backdrop", "data-testid": "preview-dialog" }, box);
    this.root.addEventListener("keydown", (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === "Escape") this.host.exec("preview.close");
    });
    document.body.append(this.root);
    new ResizeObserver(() => this.draw()).observe(this.canvas);
    this.syncControls();
  }

  private summary(): string {
    const d = this.data;
    if (!d) return "";
    return t("preview-summary", { time: hms(d.total_s), parts: d.parts.length, ops: d.ops.length });
  }

  private syncControls() {
    const s = this.state;
    this.slider.max = String(Math.max(s.total, 0.01));
    this.slider.value = String(s.time);
    this.slider.title = t("preview-slider");
    this.slider.setAttribute("aria-label", t("preview-slider"));
    this.playBtn.textContent = s.playing ? "⏸" : "▶";
    this.playBtn.title = t(s.playing ? "preview-pause" : "preview-play");
    this.playBtn.setAttribute("aria-label", this.playBtn.title);
    this.timeLabel.textContent = `${hms(s.time)} / ${hms(s.total)}`;
  }

  /** Rysuje tor do bieżącego czasu (2D, oś Y w górę). */
  draw() {
    const d = this.data;
    const s = this.state;
    const st = { cut: 0, travel: 0, arrows: 0, starts: 0, labels: 0, kerf: 0, kerfWidthMm: [] as number[] };
    this.stats = st;
    if (!d) return;
    // głowica i segment w bieżącym czasie
    let seg = 0;
    while (seg < d.segments.length - 1 && d.segments[seg].t1 < s.time) seg++;
    s.segment = seg;
    const g = d.segments[seg];
    if (g) {
      const f = g.t1 > g.t0 ? Math.min(1, Math.max(0, (s.time - g.t0) / (g.t1 - g.t0))) : 1;
      s.head = [g.from[0] + (g.to[0] - g.from[0]) * f, g.from[1] + (g.to[1] - g.from[1]) * f];
    }
    if (s.kerf) st.kerfWidthMm = [...d.kerf_width];
    const c = this.canvas;
    const dpr = window.devicePixelRatio || 1;
    const cw = Math.max(1, c.clientWidth);
    const ch = Math.max(1, c.clientHeight);
    if (c.width !== Math.round(cw * dpr) || c.height !== Math.round(ch * dpr)) {
      c.width = Math.round(cw * dpr);
      c.height = Math.round(ch * dpr);
    }
    const ctx = c.getContext("2d");
    // dopasowanie: ramka toru i kerfu
    let b = [Infinity, Infinity, -Infinity, -Infinity];
    const grow = (p: [number, number]) => {
      b = [Math.min(b[0], p[0]), Math.min(b[1], p[1]), Math.max(b[2], p[0]), Math.max(b[3], p[1])];
    };
    for (const q of d.segments) {
      grow(q.from);
      grow(q.to);
    }
    for (const k of d.kerf) k.points.forEach(grow);
    if (!Number.isFinite(b[0])) b = [0, 0, 1, 1];
    const pad = 16;
    const k = Math.min((cw - 2 * pad) / Math.max(b[2] - b[0], 1e-6), (ch - 2 * pad) / Math.max(b[3] - b[1], 1e-6));
    const ox = (cw - (b[2] - b[0]) * k) / 2;
    const oy = (ch - (b[3] - b[1]) * k) / 2;
    const X = (p: [number, number]) => ox + (p[0] - b[0]) * k;
    const Y = (p: [number, number]) => ch - oy - (p[1] - b[1]) * k;
    const css = getComputedStyle(document.documentElement);
    const muted = css.getPropertyValue("--muted").trim() || "#888";
    if (ctx) {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = css.getPropertyValue("--bg").trim() || "#111";
      ctx.fillRect(0, 0, cw, ch);
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
    }
    // kerf (REQ-07-030): linia projektu jako odniesienie, cięcie w szerokości wiązki
    if (s.kerf && d.kerf.length) {
      st.kerf = d.kerf.length;
      if (ctx) {
        const ids = [...new Set(d.kerf.map((o) => o.object))];
        ctx.strokeStyle = css.getPropertyValue("--text").trim() || "#ddd";
        ctx.globalAlpha = 0.55;
        ctx.lineWidth = 1;
        ctx.setLineDash([2, 3]);
        for (const o of this.host.designOutlines(ids)) {
          ctx.beginPath();
          for (let i = 0; i + 1 < o.points.length; i += 2) {
            const p: [number, number] = [o.points[i], o.points[i + 1]];
            if (i) ctx.lineTo(X(p), Y(p));
            else ctx.moveTo(X(p), Y(p));
          }
          if (o.closed) ctx.closePath();
          ctx.stroke();
        }
        ctx.setLineDash([]);
        ctx.globalAlpha = 1;
      }
    }
    // segmenty do bieżącego czasu
    for (let i = 0; i < d.segments.length; i++) {
      const q = d.segments[i];
      if (q.t0 > s.time) break;
      const f = q.t1 > q.t0 ? Math.min(1, (s.time - q.t0) / (q.t1 - q.t0)) : 1;
      const end: [number, number] = [q.from[0] + (q.to[0] - q.from[0]) * f, q.from[1] + (q.to[1] - q.from[1]) * f];
      if (q.kind === "travel") {
        if (!s.travel || (q.from[0] === q.to[0] && q.from[1] === q.to[1])) continue;
        st.travel++;
        if (!ctx) continue;
        ctx.strokeStyle = muted;
        ctx.globalAlpha = 0.7;
        ctx.lineWidth = 1;
        ctx.setLineDash([3, 4]);
        ctx.beginPath();
        ctx.moveTo(X(q.from), Y(q.from));
        ctx.lineTo(X(end), Y(end));
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 1;
      } else if (q.kind === "cut" && q.power > 0) {
        st.cut++;
        if (!ctx) continue;
        const width = s.kerf ? (d.kerf_width[q.op] ?? 0) * k : 0;
        ctx.strokeStyle = layerColor(d.ops[q.op]?.color ?? "#ffffff");
        // szerokość cięcia (2 × kerf) półprzezroczysta, linia środka wiązki cienka
        if (width > 1.5) {
          ctx.globalAlpha = 0.45;
          ctx.lineWidth = width;
          ctx.beginPath();
          ctx.moveTo(X(q.from), Y(q.from));
          ctx.lineTo(X(end), Y(end));
          ctx.stroke();
          ctx.globalAlpha = 1;
        }
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.moveTo(X(q.from), Y(q.from));
        ctx.lineTo(X(end), Y(end));
        ctx.stroke();
      }
    }
    // części: kierunek, punkty startowe, kolejność
    const labels = new Map<string, { at: [number, number]; text: string[] }>();
    const arrow = (q: Seg, f: number) => {
      st.arrows++;
      if (!ctx || !q.dir) return;
      const m: [number, number] = [q.from[0] + (q.to[0] - q.from[0]) * f, q.from[1] + (q.to[1] - q.from[1]) * f];
      const [ux, uy] = [q.dir[0], -q.dir[1]];
      const [mx, my] = [X(m), Y(m)];
      const a = 6;
      ctx.fillStyle = css.getPropertyValue("--accent").trim() || "#5af";
      ctx.beginPath();
      ctx.moveTo(mx + ux * a, my + uy * a);
      ctx.lineTo(mx - ux * a - uy * a * 0.7, my - uy * a + ux * a * 0.7);
      ctx.lineTo(mx - ux * a + uy * a * 0.7, my - uy * a - ux * a * 0.7);
      ctx.closePath();
      ctx.fill();
    };
    for (const p of d.parts) {
      const q = d.segments[p.segment];
      if (!q || q.t0 > s.time) continue;
      if (s.direction) {
        // strzałki wzdłuż części co ~90 px (co najmniej jedna)
        let run = 45;
        let placed = false;
        for (let i = p.segment; i < d.segments.length; i++) {
          const g = d.segments[i];
          if (g.kind !== "cut" || g.op !== q.op || g.t0 > s.time || (i > p.segment && d.parts.some((x) => x.segment === i))) break;
          const len = Math.hypot(X(g.to) - X(g.from), Y(g.to) - Y(g.from));
          let at = run;
          while (at < len) {
            arrow(g, at / len);
            placed = true;
            at += 90;
          }
          run = at - len;
        }
        if (!placed && q.dir) arrow(q, 0.5);
      }
      if (s.starts) {
        st.starts++;
        if (ctx) {
          ctx.fillStyle = "#3ddc84";
          ctx.beginPath();
          ctx.arc(X(p.start), Y(p.start), 4, 0, Math.PI * 2);
          ctx.fill();
        }
      }
      if (s.order) {
        // kilka przejść z tego samego punktu: jeden napis „1, 3”
        const near = [...labels.values()].find((l) => Math.hypot(X(l.at) - X(p.start), Y(l.at) - Y(p.start)) < 14);
        if (near) near.text.push(String(p.index));
        else labels.set(String(p.index), { at: p.start, text: [String(p.index)] });
      }
    }
    st.labels = labels.size;
    if (ctx) {
      ctx.font = "600 12px system-ui, sans-serif";
      for (const l of labels.values()) {
        const txt = l.text.join(", ");
        const [x, y] = [X(l.at) + 7, Y(l.at) - 7];
        ctx.fillStyle = "rgba(0, 0, 0, 0.55)";
        ctx.fillRect(x - 2, y - 11, ctx.measureText(txt).width + 4, 14);
        ctx.fillStyle = "#fff";
        ctx.fillText(txt, x, y);
      }
    }
    // głowica
    if (ctx && g) {
      ctx.strokeStyle = "#ff4040";
      ctx.lineWidth = 1.5;
      const [hx, hy] = [X(s.head), Y(s.head)];
      ctx.beginPath();
      ctx.arc(hx, hy, 5, 0, Math.PI * 2);
      ctx.moveTo(hx - 9, hy);
      ctx.lineTo(hx + 9, hy);
      ctx.moveTo(hx, hy - 9);
      ctx.lineTo(hx, hy + 9);
      ctx.stroke();
    }
  }
}
