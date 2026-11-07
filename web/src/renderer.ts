// Renderer obszaru roboczego w WebGL2 (REQ-01-022): tło, obszar maszyny,
// siatka, polilinie obiektów w kolorach warstw, zaznaczenie, ramka zaznaczania.
// Kafelkowanie i LOD dla dużych scen — M1.4c.

import { hexToRgb, Scene, selectionBox } from "./scene";
import { gridStep, Rect, View } from "./view";

const VS = `#version 300 es
in vec2 a_pos;
uniform vec2 u_center;
uniform vec2 u_k;      // 2*scale/width, 2*scale/height
uniform vec2 u_offset; // przesunięcie podglądu przeciągania (mm)
void main() {
  gl_Position = vec4((a_pos + u_offset - u_center) * u_k, 0.0, 1.0);
}`;

const FS = `#version 300 es
precision mediump float;
uniform vec4 u_color;
out vec4 o;
void main() { o = u_color; }`;

type Rgba = [number, number, number, number];

const DARK = {
  background: [0.118, 0.122, 0.133, 1] as Rgba,
  workspace: [0.165, 0.173, 0.188, 1] as Rgba,
  gridMinor: [1, 1, 1, 0.045] as Rgba,
  gridMajor: [1, 1, 1, 0.11] as Rgba,
  border: [0.55, 0.58, 0.62, 1] as Rgba,
  selection: [0.35, 0.65, 1, 1] as Rgba,
  marquee: [0.35, 0.65, 1, 0.8] as Rgba,
  preview: [1, 0.78, 0.3, 1] as Rgba,
  snap: [0.3, 1, 0.55, 1] as Rgba,
  node: [0.95, 0.95, 0.97, 1] as Rgba,
  /** Obrys kerfu (linia środka wiązki, REQ-07-029). */
  kerf: [0.25, 0.9, 0.95, 0.85] as Rgba,
  /** Położenie głowicy lasera (REQ-03-004). */
  head: [1, 0.25, 0.25, 1] as Rgba,
  /** Linijki (CSS): tło, podziałka, opisy, znacznik kursora. */
  ruler: { bg: "#26282c", tick: "#6b7078", text: "#a9adb4", cursor: "#5aa7ff" },
};

const LIGHT: typeof DARK = {
  background: [0.9, 0.91, 0.925, 1],
  workspace: [1, 1, 1, 1],
  gridMinor: [0, 0, 0, 0.05],
  gridMajor: [0, 0, 0, 0.13],
  border: [0.45, 0.48, 0.52, 1],
  selection: [0.1, 0.43, 0.85, 1],
  marquee: [0.1, 0.43, 0.85, 0.8],
  preview: [0.85, 0.45, 0, 1],
  snap: [0, 0.6, 0.25, 1],
  node: [0.15, 0.16, 0.18, 1],
  kerf: [0, 0.55, 0.65, 0.9],
  head: [0.85, 0.05, 0.05, 1],
  ruler: { bg: "#eceef1", tick: "#8a9099", text: "#4b5059", cursor: "#1f6fd1" },
};

/** Kolory bieżącego motywu (REQ-03-016, ciemne tło domyślnie — REQ-03-017). */
export const THEME: typeof DARK = { ...DARK, ruler: { ...DARK.ruler } };

export function setCanvasTheme(light: boolean) {
  const src = light ? LIGHT : DARK;
  Object.assign(THEME, src, { ruler: { ...src.ruler } });
}

/**
 * Kolor warstwy na kanwie: bardzo ciemny na ciemnym obszarze roboczym (czarna
 * C00) jest rozjaśniany, bardzo jasny na jasnym — przyciemniany. Barwy
 * nasycone zostają bez zmian. Tylko na ekranie, kolor warstwy w dokumencie
 * się nie zmienia (REQ-03-017).
 */
export function displayColor(rgb: [number, number, number]): [number, number, number] {
  const darkBg = THEME.workspace[0] + THEME.workspace[1] + THEME.workspace[2] < 1.5;
  const close = darkBg ? Math.max(...rgb) < 0.25 : Math.min(...rgb) > 0.85;
  if (!close) return rgb;
  const target = darkBg ? 1 : 0;
  const k = 0.7;
  return [rgb[0] + (target - rgb[0]) * k, rgb[1] + (target - rgb[1]) * k, rgb[2] + (target - rgb[2]) * k];
}

/**
 * Geometria sceny jako jedna lista odcinków (gl.LINES) z kolorem przy każdym
 * wierzchołku — jedno wywołanie rysowania niezależnie od liczby obiektów
 * (REQ-01-023). Flaga `a_move` przesuwa zaznaczone obiekty podczas przeciągania.
 */
const VS_SCENE = `#version 300 es
in vec2 a_pos;
in vec4 a_color;
in float a_move;
uniform vec2 u_center;
uniform vec2 u_k;
uniform vec2 u_offset;
out vec4 v_color;
void main() {
  v_color = a_color;
  gl_Position = vec4((a_pos + u_offset * a_move - u_center) * u_k, 0.0, 1.0);
}`;

const FS_SCENE = `#version 300 es
precision mediump float;
in vec4 v_color;
out vec4 o;
void main() { o = v_color; }`;

export class Renderer {
  readonly gl: WebGL2RenderingContext;
  private prog: WebGLProgram;
  private sceneBuf: WebGLBuffer;
  private dynBuf: WebGLBuffer;
  private vao: WebGLVertexArrayObject;
  private loc: { center: WebGLUniformLocation; k: WebGLUniformLocation; offset: WebGLUniformLocation; color: WebGLUniformLocation };
  private sceneProg: WebGLProgram;
  private sceneLoc: { center: WebGLUniformLocation; k: WebGLUniformLocation; offset: WebGLUniformLocation };
  private sceneVao: WebGLVertexArrayObject;
  private colorBuf: WebGLBuffer;
  private moveBuf: WebGLBuffer;
  /** Zakres wierzchołków każdego obiektu [pierwszy, liczba] — do flag przesuwania. */
  private ranges = new Map<number, [number, number]>();
  private vertexCount = 0;
  private moveKey = "";

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", { antialias: true, preserveDrawingBuffer: true, alpha: false });
    if (!gl) throw new Error("WebGL2 niedostępny");
    this.gl = gl;
    this.prog = link(gl, VS, FS);
    const u = (n: string) => gl.getUniformLocation(this.prog, n)!;
    this.loc = { center: u("u_center"), k: u("u_k"), offset: u("u_offset"), color: u("u_color") };
    this.sceneProg = link(gl, VS_SCENE, FS_SCENE, ["a_pos", "a_color", "a_move"]);
    const us = (n: string) => gl.getUniformLocation(this.sceneProg, n)!;
    this.sceneLoc = { center: us("u_center"), k: us("u_k"), offset: us("u_offset") };
    this.sceneBuf = gl.createBuffer()!;
    this.colorBuf = gl.createBuffer()!;
    this.moveBuf = gl.createBuffer()!;
    this.dynBuf = gl.createBuffer()!;
    this.vao = gl.createVertexArray()!;
    this.sceneVao = gl.createVertexArray()!;
    gl.bindVertexArray(this.sceneVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.sceneBuf);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.colorBuf);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 4, gl.UNSIGNED_BYTE, true, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.moveBuf);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 1, gl.UNSIGNED_BYTE, false, 0, 0);
    gl.bindVertexArray(null);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  /** Wgrywa geometrię sceny do buforów GPU (po zmianie geometrii, warstw albo kolejności). */
  upload(scene: Scene) {
    const layers = new Map(scene.layers.map((l) => [l.index, l]));
    let n = 0;
    for (const o of scene.objects) for (const p of o.polylines) n += Math.max(0, p.length / 2 - 1) * 2;
    const pos = new Float32Array(n * 2);
    const col = new Uint8Array(n * 4);
    this.ranges.clear();
    let v = 0;
    for (const o of scene.objects) {
      if (!o.visible) continue;
      const first = v;
      o.polylines.forEach((p, i) => {
        const layer = layers.get(o.polyline_layers[i] ?? o.layer);
        if (layer && !layer.show) return;
        const [r, g, b] = displayColor(hexToRgb(layer?.color ?? "#ffffff"));
        const rgba = [r * 255, g * 255, b * 255, o.helper ? 128 : 255];
        // pasek linii → pary wierzchołków (gl.LINES)
        for (let k = 0; k + 3 < p.length; k += 2) {
          pos[v * 2] = p[k];
          pos[v * 2 + 1] = p[k + 1];
          pos[v * 2 + 2] = p[k + 2];
          pos[v * 2 + 3] = p[k + 3];
          col.set(rgba, v * 4);
          col.set(rgba, v * 4 + 4);
          v += 2;
        }
      });
      this.ranges.set(o.id, [first, v - first]);
    }
    this.vertexCount = v;
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.sceneBuf);
    gl.bufferData(gl.ARRAY_BUFFER, pos.subarray(0, v * 2), gl.STATIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.colorBuf);
    gl.bufferData(gl.ARRAY_BUFFER, col.subarray(0, v * 4), gl.STATIC_DRAW);
    this.moveKey = "\u0000"; // flagi przesuwania do przeliczenia
  }

  /** Flagi wierzchołków obiektów przesuwanych podczas przeciągania. */
  private setMoving(ids: number[]) {
    const key = ids.join(",");
    if (key === this.moveKey) return;
    this.moveKey = key;
    const flags = new Uint8Array(this.vertexCount);
    for (const id of ids) {
      const r = this.ranges.get(id);
      if (r) flags.fill(1, r[0], r[0] + r[1]);
    }
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.moveBuf);
    gl.bufferData(gl.ARRAY_BUFFER, flags, gl.DYNAMIC_DRAW);
  }

  draw(
    view: View,
    scene: Scene | null,
    opts: {
      dragOffset?: [number, number];
      marquee?: Rect | null;
      /** Podgląd rysowanego kształtu (polilinie w mm). */
      preview?: number[][];
      /** Punkt przyciągnięcia (znacznik o stałym rozmiarze na ekranie). */
      snapMark?: { x: number; y: number } | null;
      /** Węzły i uchwyty edytowanego obiektu. */
      markers?: { x: number; y: number; kind: "node" | "node-sel" | "handle" }[];
      /** Obrysy kerfu (polilinie w mm, REQ-07-029). */
      kerf?: { points: number[]; closed: boolean }[];
      /** Położenie głowicy lasera w mm (REQ-03-004). */
      head?: [number, number] | null;
    } = {},
  ) {
    const gl = this.gl;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(view.width * dpr));
    const h = Math.max(1, Math.round(view.height * dpr));
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    gl.viewport(0, 0, w, h);
    const bg = THEME.background;
    gl.clearColor(bg[0], bg[1], bg[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(this.prog);
    gl.bindVertexArray(this.vao);
    gl.uniform2f(this.loc.center, view.cx, view.cy);
    gl.uniform2f(this.loc.k, (2 * view.scale) / view.width, (2 * view.scale) / view.height);
    gl.uniform2f(this.loc.offset, 0, 0);
    if (!scene) return;

    const [ww, wh] = scene.workspace;
    this.dyn(gl.TRIANGLE_STRIP, [0, 0, ww, 0, 0, wh, ww, wh], THEME.workspace);
    // siatka tylko w obszarze maszyny; główne linie co 10 oczek
    const step = gridStep(view.scale);
    const vis = view.visible();
    const minor: number[] = [];
    const major: number[] = [];
    const x0 = Math.max(0, Math.floor(vis[0] / step) * step);
    const x1 = Math.min(ww, vis[2]);
    const y0 = Math.max(0, Math.floor(vis[1] / step) * step);
    const y1 = Math.min(wh, vis[3]);
    const isMajor = (v: number) => Math.abs(v / (step * 10) - Math.round(v / (step * 10))) < 1e-6;
    for (let x = x0; x <= x1 + 1e-9; x += step) (isMajor(x) ? major : minor).push(x, 0, x, wh);
    for (let y = y0; y <= y1 + 1e-9; y += step) (isMajor(y) ? major : minor).push(0, y, ww, y);
    this.dyn(gl.LINES, minor, THEME.gridMinor);
    this.dyn(gl.LINES, major, THEME.gridMajor);
    this.dyn(gl.LINE_LOOP, [0, 0, ww, 0, ww, wh, 0, wh], THEME.border);

    // obiekty: jedno wywołanie rysowania
    const off = opts.dragOffset ?? [0, 0];
    if (this.vertexCount) {
      this.setMoving(scene.selection);
      gl.useProgram(this.sceneProg);
      gl.bindVertexArray(this.sceneVao);
      gl.uniform2f(this.sceneLoc.center, view.cx, view.cy);
      gl.uniform2f(this.sceneLoc.k, (2 * view.scale) / view.width, (2 * view.scale) / view.height);
      gl.uniform2f(this.sceneLoc.offset, off[0], off[1]);
      gl.drawArrays(gl.LINES, 0, this.vertexCount);
      gl.useProgram(this.prog);
      gl.bindVertexArray(this.vao);
    }

    // ramka zaznaczenia (z marginesem 3 px)
    const sb = selectionBox(scene);
    if (sb) {
      const m = 3 / view.scale;
      const [a, b2, c, d] = [sb[0] - m + off[0], sb[1] - m + off[1], sb[2] + m + off[0], sb[3] + m + off[1]];
      this.dyn(gl.LINE_LOOP, [a, b2, c, b2, c, d, a, d], THEME.selection);
    }
    if (opts.marquee) {
      const [a, b2, c, d] = opts.marquee;
      this.dyn(gl.TRIANGLE_STRIP, [a, b2, c, b2, a, d, c, d], [0.35, 0.65, 1, 0.08]);
      this.dyn(gl.LINE_LOOP, [a, b2, c, b2, c, d, a, d], THEME.marquee);
    }
    for (const k of opts.kerf ?? []) this.dyn(k.closed ? gl.LINE_LOOP : gl.LINE_STRIP, k.points, THEME.kerf);
    for (const p of opts.preview ?? []) this.dyn(gl.LINE_STRIP, p, THEME.preview);
    if (opts.head) {
      // krzyż i okrąg o stałym rozmiarze na ekranie
      const [x, y] = opts.head;
      const r = 9 / view.scale;
      const ring: number[] = [];
      for (let i = 0; i < 24; i++) {
        const a = (i / 24) * Math.PI * 2;
        ring.push(x + Math.cos(a) * r * 0.6, y + Math.sin(a) * r * 0.6);
      }
      this.dyn(gl.LINE_LOOP, ring, THEME.head);
      this.dyn(gl.LINES, [x - r, y, x + r, y, x, y - r, x, y + r], THEME.head);
    }
    if (opts.markers?.length) {
      const r = 3.5 / view.scale;
      const fill: number[] = [];
      const outline: number[] = [];
      const handles: number[] = [];
      for (const m of opts.markers) {
        const [x0, y0, x1, y1] = [m.x - r, m.y - r, m.x + r, m.y + r];
        if (m.kind === "node-sel") fill.push(x0, y0, x1, y0, x0, y1, x1, y0, x1, y1, x0, y1);
        const target = m.kind === "handle" ? handles : outline;
        target.push(x0, y0, x1, y0, x1, y0, x1, y1, x1, y1, x0, y1, x0, y1, x0, y0);
      }
      this.dyn(gl.TRIANGLES, fill, THEME.selection);
      this.dyn(gl.LINES, outline, THEME.node);
      this.dyn(gl.LINES, handles, THEME.preview);
    }
    if (opts.snapMark) {
      const { x, y } = opts.snapMark;
      const m = 4 / view.scale;
      this.dyn(gl.LINE_LOOP, [x - m, y - m, x + m, y - m, x + m, y + m, x - m, y + m], THEME.snap);
    }
  }

  private bindAttr(buf: WebGLBuffer) {
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  }

  private dyn(mode: number, pts: number[], color: Rgba) {
    if (pts.length < 4) return;
    const gl = this.gl;
    this.bindAttr(this.dynBuf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(pts), gl.STREAM_DRAW);
    gl.uniform4fv(this.loc.color, color);
    gl.drawArrays(mode, 0, pts.length / 2);
  }
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string, attrs: string[] = ["a_pos"]): WebGLProgram {
  const sh = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? "shader");
    return s;
  };
  const p = gl.createProgram()!;
  gl.attachShader(p, sh(gl.VERTEX_SHADER, vs));
  gl.attachShader(p, sh(gl.FRAGMENT_SHADER, fs));
  attrs.forEach((a, i) => gl.bindAttribLocation(p, i, a));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? "program");
  return p;
}
