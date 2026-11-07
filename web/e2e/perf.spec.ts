// Wydajność (M1.4c2): REQ-01-023 — płynne rysowanie przy 200 tys. węzłów,
// REQ-16-006 — zmiany w dużym dokumencie nie blokują UI (>100 ms).
// Progi: sprzętowy GPU (maszyna referencyjna, ADR 0007) 16,7 ms na klatkę;
// renderer programowy (SwiftShader w CI) — luźniejszy próg, żeby wychwycić regresje.

import { expect, Page, test } from "@playwright/test";
import { bc, objects, resetDocument } from "./helpers";

test.setTimeout(180_000);

test.beforeEach(async ({ page }) => {
  await resetDocument(page);
});

// duży dokument zostaje w hoście — następny test nie może zaczynać od niego
test.afterEach(async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("document.new"));
});

/** n obiektów po k węzłów (zamknięte, falowane okręgi) jednym wsadem. */
async function bigDocument(page: Page, n: number, k: number) {
  await bc(page, `async (b) => {
    const steps = [];
    const cols = Math.ceil(Math.sqrt(${n}));
    const cell = 380 / cols;
    for (let i = 0; i < ${n}; i++) {
      const cx = 10 + (i % cols) * cell, cy = 10 + Math.floor(i / cols) * cell, r = cell * 0.4;
      const pts = [];
      for (let j = 0; j < ${k}; j++) {
        const a = 2 * Math.PI * j / ${k};
        pts.push([cx + r * Math.cos(a), cy + r * Math.sin(a) * (1 + 0.2 * Math.sin(5 * a))]);
      }
      steps.push({ id: "shape.path", params: { points: pts, closed: true } });
    }
    await b.rpc.call("gui.batch", { steps });
  }`);
  await expect.poll(() => objects(page), { timeout: 60_000 }).toBe(n);
}

const software = (page: Page) =>
  page.evaluate(() => {
    const gl = (document.getElementById("canvas") as HTMLCanvasElement).getContext("webgl2")!;
    const ext = gl.getExtension("WEBGL_debug_renderer_info");
    return /swiftshader|llvmpipe|software/i.test(String(ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : ""));
  });

/** Czasy klatek (ms) przy przesuwaniu widoku, z czekaniem na GPU. */
const frameTimes = (page: Page, frames: number) =>
  bc<number[]>(page, `(b) => {
    b.paint(); b.renderer.gl.finish();
    const t = [];
    for (let i = 0; i < ${frames}; i++) {
      b.view.pan(2, 1);
      const a = performance.now();
      b.paint();
      b.renderer.gl.finish();
      t.push(performance.now() - a);
    }
    return t.sort((x, y) => x - y);
  }`);

const p95 = (t: number[]) => t[Math.floor(t.length * 0.95) - 1];

test("req_01_023_60fps_with_200k_nodes", async ({ page }) => {
  const limit = (await software(page)) ? 120 : 16.7;
  // 2000 obiektów × 100 węzłów
  await bigDocument(page, 2000, 100);
  const a = await frameTimes(page, 60);
  expect(p95(a), `p95 klatki ${p95(a).toFixed(1)} ms`).toBeLessThan(limit);
  // 20 000 obiektów × 10 węzłów — liczba obiektów nie może mnożyć wywołań rysowania
  await resetDocument(page);
  await bigDocument(page, 20000, 10);
  const b = await frameTimes(page, 60);
  expect(p95(b), `p95 klatki ${p95(b).toFixed(1)} ms`).toBeLessThan(limit);
});

test("req_16_006_edits_in_large_document_do_not_block_ui", async ({ page }) => {
  const limit = (await software(page)) ? 250 : 100;
  await bigDocument(page, 2000, 100);
  // obserwacja długich zadań wątku UI przy zmianie jednego obiektu
  await page.evaluate(() => {
    (window as any).__long = [];
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) (window as any).__long.push(e.duration);
    }).observe({ type: "longtask", buffered: false });
  });
  const before = await bc<number>(page, (b) => b.sceneBytes);
  const x0 = await bc<number>(page, (b) => b.scene.objects.find((o: any) => o.id === 7).bbox[0]);
  await bc(page, (b) => b.rpc.execute("object.transform", { ids: [7], translate: [1, 0] }));
  // czekaj na scenę po zmianie (na wolnych maszynach przychodzi później niż odpowiedź polecenia)
  await expect.poll(() => bc(page, (b) => b.scene.objects.find((o: any) => o.id === 7).bbox[0]), { timeout: 30_000 }).toBeCloseTo(x0 + 1, 6);
  await page.waitForTimeout(500);
  const after = await bc<number>(page, (b) => b.sceneBytes);
  // przyrostowa scena: po zmianie jednego obiektu przychodzi ułamek danych
  expect(after, `${after} B wobec ${before} B`).toBeLessThan(before / 20);
  const long = await page.evaluate(() => (window as any).__long as number[]);
  expect(Math.max(0, ...long), `długie zadania: ${long.map((d) => d.toFixed(0)).join(", ")} ms`).toBeLessThan(limit);
  // zaznaczenie wszystkiego i przewinięcie drzewa obiektów też bez blokady
  await bc(page, (b) => b.rpc.execute("selection.all"));
  await page.waitForTimeout(800);
  const long2 = await page.evaluate(() => (window as any).__long as number[]);
  expect(Math.max(0, ...long2), `długie zadania: ${long2.map((d) => d.toFixed(0)).join(", ")} ms`).toBeLessThan(limit);
  // drzewo obiektów jest wirtualizowane: w DOM tylko widoczne wiersze
  const rows = await page.locator('[data-testid="panel-objects"] .row').count();
  expect(rows).toBeLessThan(200);
});
