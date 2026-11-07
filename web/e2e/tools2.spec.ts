// M1.4b2b: lasso, prowadnice ręczne i automatyczne, obrót/skala dwupunktowa,
// gumka, przycinanie, wypaczanie (4 punkty, siatka, obwiednia) z tymczasowym cofaniem.

import { expect, Page, test } from "@playwright/test";
import { bc, clickWorld, dragWorld, obj, objects, resetDocument, screenOf } from "./helpers";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
});

const rect = (page: Page, x: number, y: number, w: number, h: number) =>
  bc(page, `(b) => b.rpc.execute("shape.rect", {x: ${x}, y: ${y}, w: ${w}, h: ${h}})`);

const close = (a: number[], b: number[], eps = 0.05) => a.every((v, i) => Math.abs(v - b[i]) <= eps);

test("req_04_022_lasso_selects_objects_inside_freehand_outline", async ({ page }) => {
  await rect(page, 10, 10, 20, 20);
  await rect(page, 50, 10, 20, 20);
  await rect(page, 150, 10, 20, 20);
  await expect.poll(() => objects(page)).toBe(3);
  await page.getByTestId("tool-lasso").click();
  const pts: [number, number][] = [[0, 0], [90, 0], [90, 40], [40, 60], [0, 40]];
  const scr = await Promise.all(pts.map(([x, y]) => screenOf(page, x, y)));
  await page.mouse.move(...scr[0]);
  await page.mouse.down();
  for (const p of scr.slice(1)) await page.mouse.move(...p, { steps: 4 });
  await page.mouse.up();
  await expect.poll(() => bc(page, (b) => b.scene.selection)).toEqual([1, 2]);
});

test("req_03_012_and_req_04_020_manual_guides_dragged_from_rulers", async ({ page }) => {
  const top = await page.locator("#ruler-top").boundingBox();
  const left = await page.locator("#ruler-left").boundingBox();
  const [, gy] = await screenOf(page, 0, 100);
  await page.mouse.move(top!.x + 200, top!.y + 10);
  await page.mouse.down();
  await page.mouse.move(top!.x + 200, gy + 1, { steps: 6 });
  await page.mouse.up();
  await expect.poll(() => objects(page)).toBe(1);
  const g = await obj(page, 0);
  expect(g.helper).toBe(true);
  expect(g.bbox[1]).toBeCloseTo(100, 6);
  expect(g.bbox[3]).toBeCloseTo(100, 6);
  expect(g.bbox[2] - g.bbox[0]).toBeGreaterThanOrEqual(400);
  const [gx] = await screenOf(page, 50, 0);
  await page.mouse.move(left!.x + 10, left!.y + 200);
  await page.mouse.down();
  await page.mouse.move(gx - 1, left!.y + 200, { steps: 6 });
  await page.mouse.up();
  await expect.poll(() => objects(page)).toBe(2);
  const v = await obj(page, 1);
  expect(v.bbox[0]).toBeCloseTo(50, 6);
  expect(v.bbox[2]).toBeCloseTo(50, 6);
  // prowadnica przyciąga: punkt 3 px od niej ląduje dokładnie na niej
  const [sx, sy] = await screenOf(page, 50, 237);
  const s = await bc<any>(page, `(b) => b.snapAt(${sx + 3}, ${sy})`);
  expect(s.x).toBeCloseTo(50, 6);
});

test("req_03_011_automatic_alignment_guides", async ({ page }) => {
  await rect(page, 100, 100, 50, 40);
  await expect.poll(() => objects(page)).toBe(1);
  // daleko od obiektu, ale w linii z jego lewą krawędzią (x = 100)
  const [sx, sy] = await screenOf(page, 100, 300);
  const s = await bc<any>(page, `(b) => b.snapAt(${sx + 3}, ${sy + 0.4})`);
  expect(s.kind).toBe("align");
  expect(s.x).toBe(100);
  expect(s.guides.length).toBeGreaterThan(0);
  // w linii ze środkiem (y = 120)
  const [tx, ty] = await screenOf(page, 330, 120);
  const t = await bc<any>(page, `(b) => b.snapAt(${tx + 0.4}, ${ty - 3})`);
  expect(t.kind).toBe("align");
  expect(t.y).toBe(120);
});

test("req_04_050_051_two_point_rotate_and_scale", async ({ page }) => {
  await rect(page, 100, 100, 40, 20);
  await expect.poll(() => objects(page)).toBe(1);
  await page.getByTestId("tool-rotate2").click();
  await clickWorld(page, 100, 100); // środek obrotu
  await clickWorld(page, 140, 100); // punkt odniesienia
  await clickWorld(page, 100, 140); // cel: +90°
  await expect.poll(async () => close((await obj(page, 0)).bbox, [80, 100, 100, 140])).toBe(true);
  await page.getByTestId("tool-scale2").click();
  await clickWorld(page, 100, 100);
  await clickWorld(page, 100, 140);
  await clickWorld(page, 100, 120); // skala 0,5
  await expect.poll(async () => close((await obj(page, 0)).bbox, [90, 100, 100, 120])).toBe(true);
  // dwa kroki historii
  await expect.poll(() => bc(page, (b) => b.scene.history.undo.slice(-2))).toEqual(["object.transform", "object.transform"]);
});

test("req_04_021_eraser_tool", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.path", { points: [[0, 200], [100, 200]] }));
  await expect.poll(() => objects(page)).toBe(1);
  await page.getByTestId("tool-eraser").click();
  await page.getByTestId("opt-eraser-width").fill("4");
  await page.getByTestId("opt-eraser-width").press("Enter");
  await dragWorld(page, [50, 190], [50, 210]);
  await expect.poll(async () => (await obj(page, 0)).closed.length).toBe(2);
  const b = (await obj(page, 0)).bbox;
  expect(b[0]).toBeCloseTo(0, 3);
  expect(b[2]).toBeCloseTo(100, 3);
});

test("req_04_034_trim_tool", async ({ page }) => {
  await rect(page, 100, 100, 40, 40);
  await bc(page, (b) => b.rpc.execute("shape.path", { points: [[120, 80], [120, 160]] }));
  await expect.poll(() => objects(page)).toBe(2);
  await page.getByTestId("tool-trim").click();
  await clickWorld(page, 100, 117); // lewy bok prostokąta
  await expect.poll(async () => (await obj(page, 0)).bbox[0]).toBeCloseTo(120, 6);
  expect((await obj(page, 0)).closed).toEqual([false]);
});

test("req_04_045_048_warp_with_temporary_undo_then_apply", async ({ page }) => {
  await rect(page, 100, 100, 40, 40);
  await expect.poll(() => objects(page)).toBe(1);
  await page.getByTestId("tool-warp").click();
  await expect.poll(async () => (await bc<any>(page, (b) => b.warpState())).grid.length).toBe(4);
  // prawy górny róg (140,140) → (160,140)
  await dragWorld(page, [140, 140], [160, 140]);
  await expect.poll(async () => (await bc<any>(page, (b) => b.warpState())).grid[3]).toEqual([160, 140]);
  // tymczasowe cofanie w trybie wypaczania: dokument bez zmian
  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await bc<any>(page, (b) => b.warpState())).grid[3]).toEqual([140, 140]);
  expect(await bc(page, (b) => b.scene.history.undo.length)).toBe(1); // tylko shape.rect
  await dragWorld(page, [140, 140], [160, 140]);
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await obj(page, 0)).bbox[2]).toBeCloseTo(160, 3);
  expect(await bc(page, (b) => b.scene.history.undo.at(-1))).toBe("path.warp");
});

test("req_04_046_047_mesh_and_envelope_modes", async ({ page }) => {
  await rect(page, 100, 100, 40, 40);
  await expect.poll(() => objects(page)).toBe(1);
  await page.getByTestId("tool-warp").click();
  await page.getByTestId("opt-warp-mode").selectOption("mesh");
  await expect.poll(async () => (await bc<any>(page, (b) => b.warpState())).grid.length).toBe(9);
  // środek siatki (120,120) w górę o 10
  await dragWorld(page, [120, 120], [120, 130]);
  await page.keyboard.press("Enter");
  await expect.poll(() => bc(page, (b) => b.scene.history.undo.at(-1))).toBe("path.warp");
  await page.getByTestId("opt-warp-mode").selectOption("envelope");
  await expect.poll(async () => (await bc<any>(page, (b) => b.warpState())).grid.length).toBe(8);
  const s = await bc<any>(page, (b) => b.warpState());
  expect([s.cols, s.rows]).toEqual([3, 1]);
  // Escape porzuca wypaczanie bez zmian
  await page.keyboard.press("Escape");
  expect(await bc(page, (b) => b.scene.history.undo.at(-1))).toBe("path.warp");
});
