// M1.4b3b: paski narzędzi (REQ-03-020…024, 029, 030), dokowanie i zapisane
// układy (REQ-03-031), tryb początkującego (REQ-03-051), podpowiedzi do pól
// (REQ-03-057), HiDPI (REQ-03-018). Układ pasków w PrefStore hosta.

import { expect, Page, test } from "@playwright/test";
import { bc, obj, objects, resetDocument, screenOf } from "./helpers";

async function resetPrefs(page: Page) {
  await bc(page, async (b) => {
    for (const [key, value] of [["ui.toolbars", null], ["ui.toolbar_layouts", null], ["ui.mode", "full"], ["ui.lang", "pl"]]) {
      await b.rpc.execute("prefs.set", { key, value });
    }
    await b.loadPrefs();
  });
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
  await resetPrefs(page);
});

test.afterEach(async ({ page }) => {
  await resetPrefs(page);
});

const rect = (page: Page, x: number, y: number, w: number, h: number) =>
  bc(page, `(b) => b.rpc.execute("shape.rect", {x: ${x}, y: ${y}, w: ${w}, h: ${h}})`);

const dockOf = (page: Page, id: string) =>
  page.getByTestId(`toolbar-${id}`).evaluate((el) => el.closest(".dock")?.id ?? null);

test("req_03_020_021_022_023_024_029_030_toolbars_and_their_actions", async ({ page }) => {
  for (const id of ["main", "create", "modes", "modifiers", "numeric", "align", "node"]) {
    await expect(page.getByTestId(`toolbar-${id}`)).toBeVisible();
  }
  // modyfikatory: suma dwóch nachodzących prostokątów
  await rect(page, 10, 10, 40, 40);
  await rect(page, 30, 30, 40, 40);
  await bc(page, (b) => b.rpc.execute("selection.all"));
  await page.getByTestId("mod-union").click();
  await expect.poll(() => objects(page)).toBe(1);
  // pasek numeryczny
  await expect(page.getByTestId("num-w")).toHaveValue("60");
  await page.getByTestId("num-lock").uncheck();
  await page.getByTestId("num-w").fill("80");
  await page.getByTestId("num-w").press("Enter");
  await expect.poll(async () => { const b = (await obj(page, 0)).bbox; return b[2] - b[0]; }).toBeCloseTo(80, 6);
  // wyrównywanie
  await rect(page, 200, 10, 10, 10);
  await rect(page, 230, 50, 10, 10);
  await expect.poll(() => objects(page)).toBe(3);
  const [, id2, id3] = await bc<number[]>(page, (b) => b.scene.objects.map((o: any) => o.id));
  await bc(page, `(b) => b.rpc.execute("selection.set", { ids: [${id2}, ${id3}] })`);
  await page.getByTestId("align-left").click();
  await expect.poll(async () => [(await obj(page, 1)).bbox[0], (await obj(page, 2)).bbox[0]]).toEqual([200, 200]);
  await page.getByTestId("align-top").click();
  await expect.poll(async () => [(await obj(page, 1)).bbox[3], (await obj(page, 2)).bbox[3]]).toEqual([60, 60]);
  // pasek węzłów działa na zaznaczonych węzłach narzędzia węzłów
  await bc(page, `(b) => b.rpc.execute("selection.set", { ids: [${id2}] })`);
  await page.getByTestId("tool-node").click();
  await expect.poll(() => bc(page, (b) => b.nodeEdit().id)).toBe(id2);
  await bc(page, (b) => b.nodeSelect([1]));
  await page.getByTestId("nodebar-smooth").click();
  await expect.poll(() => bc(page, (b) => b.nodeEdit().contours[0].nodes[1].smooth)).toBe(true);
  await page.getByTestId("nodebar-corner").click();
  await expect.poll(() => bc(page, (b) => b.nodeEdit().contours[0].nodes[1].hin)).toBeNull();
});

test("req_03_031_toolbars_dock_hide_and_saved_layouts", async ({ page }) => {
  expect(await dockOf(page, "align")).toBe("dock-top");
  // dokowanie z menu paska
  await page.getByTestId("tb-menu-align").click();
  await page.getByTestId("dock-left").click();
  await expect.poll(() => dockOf(page, "align")).toBe("dock-left");
  // przeciągnięcie uchwytu paska do strefy dokowania
  await page.getByTestId("tb-grip-modifiers").dragTo(page.locator("#dock-right"));
  await expect.poll(() => dockOf(page, "modifiers")).toBe("dock-right");
  // układ przetrwa przeładowanie (preferencje hosta)
  await page.reload();
  await expect(page.locator("#status")).toHaveAttribute("data-connected", "true");
  await expect.poll(() => dockOf(page, "align")).toBe("dock-left");
  await expect.poll(() => dockOf(page, "modifiers")).toBe("dock-right");
  // zapis układu pod nazwą
  await page.getByTestId("open-settings").click();
  await page.getByTestId("layout-name").fill("Warsztat");
  await page.getByTestId("layout-save").click();
  // ukrycie paska i przywrócenie domyślnego układu
  await page.getByTestId("toolbar-visible-align").uncheck();
  await expect(page.getByTestId("toolbar-align")).toBeHidden();
  await page.getByTestId("toolbars-reset").click();
  await expect.poll(() => dockOf(page, "align")).toBe("dock-top");
  await expect(page.getByTestId("toolbar-align")).toBeVisible();
  // wczytanie zapisanego układu
  await page.getByTestId("layout-load").selectOption("Warsztat");
  await expect.poll(() => dockOf(page, "align")).toBe("dock-left");
  await expect.poll(() => dockOf(page, "modifiers")).toBe("dock-right");
  await page.getByTestId("settings-close").click();
  // menu paska: ukryj
  await page.getByTestId("tb-menu-numeric").click();
  await page.getByTestId("dock-hidden").click();
  await expect(page.getByTestId("toolbar-numeric")).toBeHidden();
});

test("req_03_051_beginner_mode_hides_advanced_options", async ({ page }) => {
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-mode").selectOption("beginner");
  await page.getByTestId("settings-close").click();
  for (const id of ["modifiers", "node"]) await expect(page.getByTestId(`toolbar-${id}`)).toBeHidden();
  for (const tool of ["node", "trim", "eraser", "warp", "rotate2", "scale2"]) await expect(page.getByTestId(`tool-${tool}`)).toBeHidden();
  await expect(page.getByTestId("tool-rect")).toBeVisible();
  // zaawansowane kolumny warstw ukryte
  await rect(page, 10, 10, 10, 10);
  await expect(page.getByTestId("layer-0-passes")).toBeHidden();
  await expect(page.getByTestId("layer-0-speed")).toBeVisible();
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-mode").selectOption("full");
  await page.getByTestId("settings-close").click();
  await expect(page.getByTestId("toolbar-modifiers")).toBeVisible();
  await expect(page.getByTestId("tool-warp")).toBeVisible();
  await expect(page.getByTestId("layer-0-passes")).toBeVisible();
});

test("req_03_057_every_field_has_contextual_hint", async ({ page }) => {
  await rect(page, 10, 10, 10, 10);
  await page.getByTestId("tool-star").click(); // pola opcji narzędzia
  const missing = async () =>
    page.evaluate(() =>
      [...document.querySelectorAll<HTMLElement>("input, select, textarea, button")]
        .filter((el) => el.offsetParent !== null)
        .filter((el) => !(el.getAttribute("title") ?? "").trim())
        .map((el) => el.dataset.testid ?? el.outerHTML.slice(0, 80)),
    );
  expect(await missing()).toEqual([]);
  await page.getByTestId("open-settings").click();
  expect(await missing()).toEqual([]);
  await page.getByTestId("settings-close").click();
  await page.getByTestId("open-shortcuts").click();
  await page.getByTestId("shortcuts-search").fill("cofnij");
  expect(await missing()).toEqual([]);
});

test.describe("HiDPI", () => {
  test.use({ deviceScaleFactor: 2 });

  test("req_03_018_canvas_and_rulers_render_at_device_resolution", async ({ page }) => {
    const sizes = await page.evaluate(() => {
      const css = (id: string) => document.getElementById(id)!.getBoundingClientRect();
      const px = (id: string) => document.getElementById(id) as HTMLCanvasElement;
      return {
        dpr: window.devicePixelRatio,
        canvas: [px("canvas").width, Math.round(css("canvas").width * 2)],
        top: [px("ruler-top").height, Math.round(css("ruler-top").height * 2)],
      };
    });
    expect(sizes.dpr).toBe(2);
    expect(Math.abs(sizes.canvas[0] - sizes.canvas[1])).toBeLessThanOrEqual(1);
    expect(Math.abs(sizes.top[0] - sizes.top[1])).toBeLessThanOrEqual(1);
    // trafienie myszą w obiekt działa przy skali 2
    await rect(page, 100, 100, 40, 40);
    await bc(page, (b) => b.rpc.execute("selection.clear"));
    // współrzędne liczone przy każdej próbie: panel boczny (okna maszyny) może jeszcze
    // zmieniać układ po wczytaniu, a z nim położenie kanwy
    await expect(async () => {
      const [x, y] = await screenOf(page, 120, 100);
      await page.mouse.click(x, y);
      await expect.poll(() => bc(page, (b) => b.scene.selection), { timeout: 1_000 }).toEqual([1]);
    }).toPass({ timeout: 10_000 });
  });
});
