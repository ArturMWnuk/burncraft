// Narzędzia rysowania, wpisywanie długości/kątów i przyciąganie (M1.4b1).
// Przy „zoom do strony” oczko siatki ma 10 mm, więc punkty testowe leżą na siatce.

import { expect, test } from "@playwright/test";
import { bc, clickWorld, dragWorld, obj, objects, pairs, resetDocument, screenOf } from "./helpers";

test.beforeEach(async ({ page }) => {
  // ustawienia narzędzi i przyciągania są w localStorage — każdy test od domyślnych
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
});

const cursor = (page: any) => page.locator("#canvas").evaluate((c: HTMLElement) => getComputedStyle(c).cursor);

test("req_03_055_and_req_03_021_creation_toolbar_and_tool_cursor", async ({ page }) => {
  await expect(page.getByTestId("tool-select")).toHaveAttribute("aria-pressed", "true");
  expect(await cursor(page)).toBe("default");
  await page.getByTestId("tool-rect").click();
  await expect(page.getByTestId("tool-rect")).toHaveAttribute("aria-pressed", "true");
  expect(await cursor(page)).toMatch(/^url\(.+\) 7 7, crosshair$/); // własny kursor narzędzia
  await page.locator("#canvas").focus();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("tool-select")).toHaveAttribute("aria-pressed", "true");
  expect(await cursor(page)).toBe("default");
});

test("req_04_005_rect_tool_drag_square_and_layer", async ({ page }) => {
  await page.getByTestId("palette-3").click(); // bez zaznaczenia: aktywna warstwa dla nowych kształtów
  await page.getByTestId("tool-rect").click();
  await dragWorld(page, [10, 10], [60, 40]);
  await expect.poll(() => objects(page)).toBe(1);
  const r = await obj(page, 0);
  expect(r.type).toBe("rect");
  expect(r.bbox).toEqual([10, 10, 60, 40]);
  expect(r.layer).toBe(3);
  // Shift = kwadrat, narzędzie zostaje aktywne
  await dragWorld(page, [100, 100], [140, 130], ["Shift"]);
  await expect.poll(() => objects(page)).toBe(2);
  expect((await obj(page, 1)).bbox).toEqual([100, 100, 140, 140]);
  // przeciągnięcie w przeciwną stronę normalizuje narożnik
  await dragWorld(page, [300, 300], [250, 280]);
  await expect.poll(() => objects(page)).toBe(3);
  expect((await obj(page, 2)).bbox).toEqual([250, 280, 300, 300]);
});

test("req_04_006_ellipse_tool_from_corner_and_from_center", async ({ page }) => {
  await page.getByTestId("tool-ellipse").click();
  await dragWorld(page, [100, 100], [160, 140]);
  await expect.poll(() => objects(page)).toBe(1);
  // ramka w scenie pochodzi z polilinii spłaszczonej z tolerancją 0,02 mm
  const e = await obj(page, 0);
  expect(e.type).toBe("ellipse");
  e.bbox.forEach((v: number, i: number) => expect(v).toBeCloseTo([100, 100, 160, 140][i], 1));
  // Ctrl = od środka
  await dragWorld(page, [250, 250], [270, 260], ["Control"]);
  await expect.poll(() => objects(page)).toBe(2);
  (await obj(page, 1)).bbox.forEach((v: number, i: number) => expect(v).toBeCloseTo([230, 240, 270, 260][i], 1));
});

test("req_04_007_008_polygon_and_star_tools_with_options", async ({ page }) => {
  await page.getByTestId("tool-polygon").click();
  await page.getByTestId("opt-sides").fill("5");
  await page.getByTestId("opt-sides").press("Enter");
  await dragWorld(page, [200, 200], [240, 200]);
  await expect.poll(() => objects(page)).toBe(1);
  const p = await obj(page, 0);
  expect(p.type).toBe("polygon");
  expect(p.nodes.length / 2).toBe(5);
  expect(pairs(p.nodes)).toContainEqual([240, 200]); // pierwszy wierzchołek w kierunku przeciągnięcia
  await page.getByTestId("tool-star").click();
  await page.getByTestId("opt-points").fill("6");
  await page.getByTestId("opt-points").press("Enter");
  await page.getByTestId("opt-ratio").fill("0.5");
  await page.getByTestId("opt-ratio").press("Enter");
  await dragWorld(page, [100, 300], [100, 340]);
  await expect.poll(() => objects(page)).toBe(2);
  const s = await obj(page, 1);
  expect(s.type).toBe("star");
  expect(s.nodes.length / 2).toBe(12);
  expect(s.bbox[3]).toBeCloseTo(340, 6);
});

test("req_04_001_line_tool_polyline_bezier_and_closing", async ({ page }) => {
  await page.getByTestId("tool-line").click();
  await clickWorld(page, 10, 100);
  await clickWorld(page, 50, 100);
  // wciśnięcie z przeciągnięciem = węzeł gładki z uchwytami
  await dragWorld(page, [90, 100], [90, 130]);
  await page.keyboard.press("Enter");
  await expect.poll(() => objects(page)).toBe(1);
  const open = await obj(page, 0);
  expect(open.type).toBe("path");
  expect(open.closed).toEqual([false]);
  expect(pairs(open.nodes)).toEqual([[10, 100], [50, 100], [90, 100]]);
  expect(open.bbox[3] - open.bbox[1]).toBeGreaterThan(1); // odcinek krzywej, nie prosta
  // kliknięcie w pierwszy punkt zamyka kontur
  for (const [x, y] of [[200, 100], [250, 100], [250, 150], [200, 100]]) await clickWorld(page, x, y);
  await expect.poll(() => objects(page)).toBe(2);
  const closed = await obj(page, 1);
  expect(closed.closed).toEqual([true]);
  expect(pairs(closed.nodes)).toEqual([[200, 100], [250, 100], [250, 150]]);
  // Escape przerywa rysowanie bez tworzenia obiektu
  await clickWorld(page, 300, 300);
  await clickWorld(page, 320, 300);
  await page.keyboard.press("Escape");
  await page.waitForTimeout(200);
  expect(await objects(page)).toBe(2);
});

test("req_04_002_numeric_length_and_angle_while_drawing", async ({ page }) => {
  await page.getByTestId("tool-line").click();
  await clickWorld(page, 100, 300);
  for (const entry of ["50", "30<90", "@-50,0"]) {
    await page.keyboard.type(entry);
    await expect(page.getByTestId("tool-input")).toHaveText(entry);
    await page.keyboard.press("Enter");
  }
  await page.keyboard.press("Enter"); // puste pole kończy ścieżkę
  await expect.poll(() => objects(page)).toBe(1);
  const p = await obj(page, 0);
  expect(pairs(p.nodes)).toEqual([[100, 300], [100, 330], [150, 300], [150, 330]]);
  expect(p.closed).toEqual([false]);
});

test("req_03_006_to_010_snapping_grid_nodes_midpoints_centers_edges", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 20, w: 40, h: 20 }));
  await expect.poll(() => objects(page)).toBe(1);
  const near = async (x: number, y: number, dx = 3, dy = -3) => {
    const [sx, sy] = await screenOf(page, x, y);
    return bc<any>(page, `(b) => b.snapAt(${sx + dx}, ${sy + dy})`);
  };
  expect(await near(10, 20)).toEqual({ x: 10, y: 20, kind: "node" });
  expect(await near(30, 20)).toEqual({ x: 30, y: 20, kind: "mid" });
  expect(await near(30, 30)).toEqual({ x: 30, y: 30, kind: "center" });
  const edge = await near(17.3, 40, 0, 3);
  expect(edge.kind).toBe("edge");
  expect(edge.y).toBeCloseTo(40, 6);
  expect(edge.x).toBeCloseTo(17.3, 0);
  const grid = await near(73.4, 88.2, 0, 0);
  expect(grid).toEqual({ x: 70, y: 90, kind: "grid" });
  // przełącznik przyciągania do siatki
  await page.getByTestId("snap-grid").uncheck();
  const free = await near(73.4, 88.2, 0, 0);
  expect(free.kind).toBe("none");
  expect(free.x).toBeCloseTo(73.4, 0);
  await page.getByTestId("snap-grid").check();
  // przyciąganie działa w narzędziu: prostokąt zaczęty 3 px od narożnika startuje dokładnie w nim
  await page.getByTestId("tool-rect").click();
  const [cx, cy] = await screenOf(page, 50, 40);
  const [ex, ey] = await screenOf(page, 80, 60);
  await page.mouse.move(cx + 3, cy + 2);
  await page.mouse.down();
  await page.mouse.move(ex, ey, { steps: 5 });
  await page.mouse.up();
  await expect.poll(() => objects(page)).toBe(2);
  expect((await obj(page, 1)).bbox).toEqual([50, 40, 80, 60]);
});

test("move_drag_snaps_grabbed_point", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 10, w: 20, h: 20 }));
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 100, y: 100, w: 20, h: 20 }));
  await expect.poll(() => objects(page)).toBe(2);
  // chwyć narożnik (30,30) pierwszego i upuść 2 px od narożnika (100,100) drugiego
  const [sx, sy] = await screenOf(page, 30, 30);
  const [tx, ty] = await screenOf(page, 100, 100);
  await page.mouse.move(sx - 1, sy + 1);
  await page.mouse.down();
  await page.mouse.move(tx + 2, ty - 2, { steps: 8 });
  await page.mouse.up();
  await expect.poll(async () => (await obj(page, 0)).bbox).toEqual([80, 80, 100, 100]);
});
