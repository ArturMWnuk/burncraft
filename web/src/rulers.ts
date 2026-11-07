// Linijki (REQ-03-002): podziałka w mm zgodna z siatką, znacznik kursora.

import { THEME } from "./renderer";
import { gridStep, View } from "./view";

export const RULER = 22;

export function drawRulers(top: HTMLCanvasElement, left: HTMLCanvasElement, view: View, cursor: [number, number] | null) {
  const dpr = window.devicePixelRatio || 1;
  const prep = (c: HTMLCanvasElement, w: number, h: number) => {
    const W = Math.round(w * dpr);
    const H = Math.round(h * dpr);
    if (c.width !== W || c.height !== H) {
      c.width = W;
      c.height = H;
    }
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = THEME.ruler.bg;
    g.fillRect(0, 0, w, h);
    g.strokeStyle = THEME.ruler.tick;
    g.fillStyle = THEME.ruler.text;
    g.font = "10px system-ui, sans-serif";
    g.lineWidth = 1;
    return g;
  };
  const step = gridStep(view.scale, 8);
  const label = step * 10;
  const vis = view.visible();

  const gt = prep(top, view.width, RULER);
  gt.beginPath();
  for (let x = Math.floor(vis[0] / step) * step; x <= vis[2]; x += step) {
    const [sx] = view.toScreen(x, 0);
    const big = isMultiple(x, label);
    const mid = isMultiple(x, label / 2);
    const len = big ? RULER : mid ? RULER / 2 : RULER / 4;
    gt.moveTo(Math.round(sx) + 0.5, RULER);
    gt.lineTo(Math.round(sx) + 0.5, RULER - len);
    if (big) gt.fillText(fmt(x), sx + 2, 10);
  }
  gt.stroke();

  const gl = prep(left, RULER, view.height);
  gl.beginPath();
  for (let y = Math.floor(vis[1] / step) * step; y <= vis[3]; y += step) {
    const [, sy] = view.toScreen(0, y);
    const big = isMultiple(y, label);
    const mid = isMultiple(y, label / 2);
    const len = big ? RULER : mid ? RULER / 2 : RULER / 4;
    gl.moveTo(RULER, Math.round(sy) + 0.5);
    gl.lineTo(RULER - len, Math.round(sy) + 0.5);
    if (big) {
      gl.save();
      gl.translate(10, sy - 2);
      gl.rotate(-Math.PI / 2);
      gl.fillText(fmt(y), 0, 0);
      gl.restore();
    }
  }
  gl.stroke();

  if (cursor) {
    const [sx, sy] = view.toScreen(cursor[0], cursor[1]);
    gt.strokeStyle = gl.strokeStyle = THEME.ruler.cursor;
    gt.beginPath();
    gt.moveTo(sx + 0.5, 0);
    gt.lineTo(sx + 0.5, RULER);
    gt.stroke();
    gl.beginPath();
    gl.moveTo(0, sy + 0.5);
    gl.lineTo(RULER, sy + 0.5);
    gl.stroke();
  }
}

function isMultiple(v: number, m: number): boolean {
  return Math.abs(v / m - Math.round(v / m)) < 1e-6;
}

function fmt(v: number): string {
  const r = Math.round(v * 1000) / 1000;
  return String(Object.is(r, -0) ? 0 : r);
}
