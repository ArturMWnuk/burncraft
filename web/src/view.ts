// Widok obszaru roboczego: świat w mm (oś Y w górę, ADR 0003) ↔ ekran w pikselach CSS.

export type Rect = [number, number, number, number]; // x0, y0, x1, y1 (mm)

/** 1 mm w pikselach CSS przy 100% (96 dpi wg CSS). */
export const PX_PER_MM_100 = 96 / 25.4;

export class View {
  /** Środek widoku w mm. */
  cx = 200;
  cy = 200;
  /** Piksele CSS na mm. */
  scale = 1.5;
  width = 1;
  height = 1;
  /**
   * Kalibracja ekranu (`ui.screen_scale`, REQ-03-015): ile pikseli CSS tego
   * monitora odpowiada 1/96 cala. 1 = deklarowane 96 dpi CSS.
   */
  calibration = 1;

  /** 1 mm w pikselach CSS przy 100% po kalibracji. */
  get pxPerMm100(): number {
    return PX_PER_MM_100 * this.calibration;
  }

  toScreen(x: number, y: number): [number, number] {
    return [(x - this.cx) * this.scale + this.width / 2, this.height / 2 - (y - this.cy) * this.scale];
  }

  toWorld(sx: number, sy: number): [number, number] {
    return [(sx - this.width / 2) / this.scale + this.cx, (this.height / 2 - sy) / this.scale + this.cy];
  }

  /** Powiększenie względem rzeczywistego rozmiaru (100% = 1 mm na ekranie to 1 mm). */
  get zoomPercent(): number {
    return (this.scale / this.pxPerMm100) * 100;
  }

  zoomAt(sx: number, sy: number, factor: number) {
    const [wx, wy] = this.toWorld(sx, sy);
    this.scale = clamp(this.scale * factor, 0.01, 2000);
    const [nx, ny] = this.toWorld(sx, sy);
    this.cx += wx - nx;
    this.cy += wy - ny;
  }

  pan(dxPx: number, dyPx: number) {
    this.cx -= dxPx / this.scale;
    this.cy += dyPx / this.scale;
  }

  /** Dopasowuje prostokąt świata do widoku z marginesem (zoom do strony / zaznaczenia). */
  fit(r: Rect, marginPx = 24) {
    const w = Math.max(r[2] - r[0], 1e-3);
    const h = Math.max(r[3] - r[1], 1e-3);
    const m = Math.min(marginPx, this.width / 4, this.height / 4);
    const sx = (this.width - 2 * m) / w;
    const sy = (this.height - 2 * m) / h;
    this.scale = clamp(Math.min(sx, sy), 0.01, 2000);
    this.cx = (r[0] + r[2]) / 2;
    this.cy = (r[1] + r[3]) / 2;
  }

  /** Zoom 100% (rzeczywisty rozmiar po kalibracji ekranu) wokół bieżącego środka. */
  actualSize() {
    this.scale = this.pxPerMm100;
  }

  /** Widoczny prostokąt świata. */
  visible(): Rect {
    const [x0, y1] = this.toWorld(0, 0);
    const [x1, y0] = this.toWorld(this.width, this.height);
    return [x0, y0, x1, y1];
  }
}

export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/** Krok siatki: 1-2-5 × 10^n, tak aby oczko miało co najmniej `minPx` pikseli. */
export function gridStep(scale: number, minPx = 12): number {
  const raw = minPx / scale;
  const p = Math.pow(10, Math.floor(Math.log10(raw)));
  for (const m of [1, 2, 5, 10]) if (m * p >= raw) return m * p;
  return 10 * p;
}
