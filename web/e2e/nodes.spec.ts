// Narzędzie edycji węzłów (M1.4b2): zaznaczanie i przeciąganie węzłów i uchwytów,
// dodawanie dwuklikiem, usuwanie, menu kontekstowe, pozycja liczbowa.

import { expect, Page, test } from "@playwright/test";
import { bc, objects, resetDocument, screenOf } from "./helpers";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
});

/** Stan narzędzia: edytowany obiekt, zaznaczone węzły, kontury z node.list. */
const ne = (page: Page) => bc<any>(page, (b) => b.nodeEdit());

const nodePts = async (page: Page): Promise<[number, number][]> => {
  const s = await ne(page);
  return s.contours.flatMap((c: any) => c.nodes.map((n: any) => [Math.round(n.p[0] * 1000) / 1000, Math.round(n.p[1] * 1000) / 1000]));
};

async function at(page: Page, x: number, y: number) {
  return screenOf(page, x, y);
}

async function startEditing(page: Page) {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 10, w: 30, h: 20 }));
  await expect.poll(() => objects(page)).toBe(1);
  await page.getByTestId("tool-node").click();
  const [x, y] = await at(page, 25, 10); // krawędź prostokąta
  await page.mouse.click(x, y + 1);
  await expect.poll(async () => (await ne(page)).id).toBe(1);
  await expect.poll(async () => (await nodePts(page)).length).toBe(4);
}

test("req_04_032_node_tool_selects_drags_and_types_position", async ({ page }) => {
  await startEditing(page);
  // przeciągnięcie węzła (40,30) do (50,40) — z przyciąganiem do siatki
  const [sx, sy] = await at(page, 40, 30);
  const [tx, ty] = await at(page, 50, 40);
  await page.mouse.move(sx, sy);
  await page.mouse.down();
  await page.mouse.move(tx + 1, ty - 1, { steps: 6 });
  await page.mouse.up();
  await expect.poll(() => nodePts(page)).toContainEqual([50, 40]);
  expect((await ne(page)).selected).toEqual([2]);
  // wpisanie pozycji zaznaczonego węzła
  await page.getByTestId("node-x").fill("55");
  await page.getByTestId("node-x").press("Enter");
  await expect.poll(() => nodePts(page)).toContainEqual([55, 40]);
  await page.getByTestId("node-y").fill("45,5");
  await page.getByTestId("node-y").press("Enter");
  await expect.poll(() => nodePts(page)).toContainEqual([55, 45.5]);
  // cofnięcie przez rejestr
  await page.locator("#canvas").focus();
  await page.keyboard.press("Control+z");
  await page.keyboard.press("Control+z");
  await page.keyboard.press("Control+z");
  await expect.poll(() => bc(page, (b) => b.scene.objects[0].type)).toBe("rect");
});

test("req_04_023_024_double_click_adds_and_delete_removes_nodes", async ({ page }) => {
  await startEditing(page);
  const [x, y] = await at(page, 25, 10);
  await page.mouse.dblclick(x, y + 1);
  await expect.poll(async () => (await nodePts(page)).length).toBe(5);
  expect(await nodePts(page)).toContainEqual([25, 10]);
  expect((await ne(page)).selected).toEqual([1]);
  await page.keyboard.press("Delete");
  await expect.poll(async () => (await nodePts(page)).length).toBe(4);
  // zaznaczenie dwóch węzłów (Shift) i usunięcie obu
  const [ax, ay] = await at(page, 10, 10);
  const [bx, by] = await at(page, 40, 10);
  await page.mouse.click(ax, ay);
  await page.keyboard.down("Shift");
  await page.mouse.click(bx, by);
  await page.keyboard.up("Shift");
  expect((await ne(page)).selected.sort()).toEqual([0, 1]);
  await page.keyboard.press("Delete");
  await expect.poll(async () => (await nodePts(page)).length).toBe(2);
  expect(await objects(page)).toBe(1);
});

test("req_03_056_and_req_04_025_to_029_context_menu_actions", async ({ page }) => {
  await startEditing(page);
  const ctx = async (x: number, y: number, action: string) => {
    const [sx, sy] = await at(page, x, y);
    await page.mouse.click(sx, sy, { button: "right" });
    await expect(page.getByTestId("ctxmenu")).toBeVisible();
    await page.getByTestId(`ctx-${action}`).click();
    await expect(page.getByTestId("ctxmenu")).toBeHidden();
  };
  // wygładź węzeł (40,10)
  await ctx(40, 10, "smooth");
  await expect.poll(async () => (await ne(page)).contours[0].nodes[1].smooth).toBe(true);
  // narożnik
  await ctx(40, 10, "corner");
  await expect.poll(async () => (await ne(page)).contours[0].nodes[1].hin).toBeNull();
  // segment górny (40,30)→(10,30) na krzywą i z powrotem na linię
  await ctx(25, 30, "curve");
  await expect.poll(async () => (await ne(page)).contours[0].nodes[2].hout).not.toBeNull();
  await ctx(25, 30, "line");
  await expect.poll(async () => (await ne(page)).contours[0].nodes[2].hout).toBeNull();
  // przełam w węźle (10,30) → kontur otwarty
  await ctx(10, 30, "break");
  await expect.poll(async () => (await ne(page)).contours[0].closed).toBe(false);
  // połącz końce: zaznacz pierwszy i ostatni węzeł, „Połącz”
  const s = await ne(page);
  const last = s.contours[0].nodes.length - 1;
  expect(s.contours[0].nodes[0].p).toEqual(s.contours[0].nodes[last].p);
  await bc(page, `(b) => b.nodeSelect([0, ${last}])`);
  await ctx(10, 20, "join"); // menu na segmencie lewym; akcja dotyczy zaznaczonych końców
  await expect.poll(async () => (await ne(page)).contours[0].closed).toBe(true);
  await expect.poll(async () => (await nodePts(page)).length).toBe(4);
  // nożyczki na segmencie dolnym
  await ctx(25, 10, "cut");
  await expect.poll(async () => (await ne(page)).contours[0].closed).toBe(false);
  expect((await nodePts(page))[0]).toEqual([25, 10]);
  // Escape zamyka menu bez akcji
  const [mx, my] = await at(page, 40, 30);
  await page.mouse.click(mx, my, { button: "right" });
  await expect(page.getByTestId("ctxmenu")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("ctxmenu")).toBeHidden();
});

test("req_04_033_align_selected_nodes_and_drag_handle", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.path", { points: [[10, 10], [30, 14], [50, 18]] }));
  await expect.poll(() => objects(page)).toBe(1);
  await page.getByTestId("tool-node").click();
  const [x, y] = await at(page, 30, 14);
  await page.mouse.click(x, y);
  await expect.poll(async () => (await ne(page)).id).toBe(1);
  await bc(page, (b) => b.nodeSelect([0, 1, 2]));
  const [sx, sy] = await at(page, 30, 14);
  await page.mouse.click(sx, sy, { button: "right" });
  await page.getByTestId("ctx-align-y").click();
  await expect.poll(() => nodePts(page)).toEqual([[10, 14], [30, 14], [50, 14]]);
  // wygładzony węzeł środkowy ma uchwyty; przeciągnięcie uchwytu zmienia krzywą
  await bc(page, (b) => b.nodeSelect([1]));
  await page.mouse.click(sx, sy, { button: "right" });
  await page.getByTestId("ctx-smooth").click();
  await expect.poll(async () => (await ne(page)).contours[0].nodes[1].hout).not.toBeNull();
  const h = (await ne(page)).contours[0].nodes[1].hout as [number, number];
  const [hx, hy] = await at(page, h[0], h[1]);
  const [tx, ty] = await at(page, h[0], h[1] + 10);
  await page.mouse.move(hx, hy);
  await page.mouse.down();
  await page.mouse.move(tx, ty, { steps: 5 });
  await page.mouse.up();
  await expect.poll(async () => (await ne(page)).contours[0].nodes[1].hout[1]).toBeGreaterThan(h[1] + 5);
  // węzeł gładki zostaje gładki
  expect((await ne(page)).contours[0].nodes[1].smooth).toBe(true);
});

test("node_tool_escape_and_switching_objects", async ({ page }) => {
  await startEditing(page);
  await bc(page, (b) => b.rpc.execute("shape.ellipse", { cx: 100, cy: 100, rx: 10, ry: 10 }));
  await expect.poll(() => objects(page)).toBe(2);
  // klik w inny obiekt przełącza edytowany obiekt
  const [x, y] = await at(page, 110, 100);
  await page.mouse.click(x, y);
  await expect.poll(async () => (await ne(page)).id).toBe(2);
  // Escape: najpierw czyści zaznaczenie węzłów, potem wraca do narzędzia zaznaczania
  await bc(page, (b) => b.nodeSelect([0]));
  await page.locator("#canvas").focus();
  await page.keyboard.press("Escape");
  expect((await ne(page)).selected).toEqual([]);
  await expect(page.getByTestId("tool-node")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("tool-select")).toHaveAttribute("aria-pressed", "true");
});
