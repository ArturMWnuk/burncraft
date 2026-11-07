// Zrzuty ekranu do procedur ręcznych M1 (docs/procedures/M1-manual-checks.md).
// Uruchamiane tylko na żądanie: BC_SCREENSHOTS=1 npx playwright test procedures
// (w CI pomijane — zrzuty są częścią dokumentacji, nie testem).

import { expect, Locator, Page, test } from "@playwright/test";
import { bc, resetDocument, screenOf } from "./helpers";

declare const process: { env: Record<string, string | undefined> };

const OUT = "../docs/procedures/img/m1";

test.skip(!process.env.BC_SCREENSHOTS, "zrzuty do procedur: BC_SCREENSHOTS=1");
test.setTimeout(120_000);

async function setPrefs(page: Page, prefs: [string, unknown][]) {
  await bc(page, `async (b) => {
    for (const [key, value] of ${JSON.stringify(prefs)}) await b.rpc.execute("prefs.set", { key, value });
    await b.loadPrefs();
  }`);
}

const DEFAULT_PREFS: [string, unknown][] = [
  ["ui.lang", "pl"], ["ui.theme", "dark"], ["ui.units", "mm"], ["ui.mode", "full"],
  ["ui.toolbars", null], ["ui.toolbar_layouts", null], ["ui.screen_scale", 1],
];

/** Przykładowy projekt: kształty parametryczne na kilku warstwach i krzywa. */
async function sample(page: Page) {
  await bc(page, async (b) => {
    const r = (id: string, p: unknown) => b.rpc.execute(id, p);
    await r("shape.rect", { x: 40, y: 230, w: 130, h: 90, radius: 8, layer: 0 });
    await r("shape.ellipse", { cx: 270, cy: 280, rx: 70, ry: 45, layer: 1 });
    await r("shape.polygon", { cx: 100, cy: 120, r: 55, sides: 6, layer: 2 });
    await r("shape.star", { cx: 280, cy: 130, r_outer: 65, r_inner: 30, points: 5, layer: 3 });
    await r("shape.path", { d: "M 30 30 C 120 90 220 -10 370 50", layer: 4 });
    await r("selection.clear", {});
    await b.refresh();
    b.zoomPage();
  });
  await settle(page);
}

async function settle(page: Page) {
  await bc(page, async (b) => {
    await b.refresh();
    b.paint();
  });
  await page.waitForTimeout(150);
}

const shot = (page: Page, name: string, opts: Parameters<Page["screenshot"]>[0] = {}) =>
  page.screenshot({ path: `${OUT}/${name}.png`, ...opts });

const el = (loc: Locator, name: string) => loc.screenshot({ path: `${OUT}/${name}.png` });

/** Fragment kanwy wokół punktu świata (mm). */
async function around(page: Page, name: string, x: number, y: number, w = 360, h = 220) {
  const [sx, sy] = await screenOf(page, x, y);
  await shot(page, name, { clip: { x: Math.max(0, sx - w / 2), y: Math.max(0, sy - h / 2), width: w, height: h } });
}

async function canvasShot(page: Page, name: string) {
  await el(page.locator("#canvas"), name);
}

async function fit(page: Page, r: [number, number, number, number]) {
  await bc(page, `(b) => { b.view.fit(${JSON.stringify(r)}); b.redraw(); }`);
  await settle(page);
}

/** Najechanie narzędziem rysowania w pobliże punktu (marker przyciągania). */
async function hover(page: Page, x: number, y: number, dx = 4, dy = -4) {
  const [sx, sy] = await screenOf(page, x, y);
  await page.mouse.move(sx + dx + 20, sy + dy + 20);
  await page.mouse.move(sx + dx, sy + dy, { steps: 4 });
  await page.waitForTimeout(150);
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
  await setPrefs(page, DEFAULT_PREFS);
});

test.afterEach(async ({ page }) => {
  await setPrefs(page, DEFAULT_PREFS);
  await bc(page, (b) => b.rpc.execute("document.new"));
});

test("screens_overview_themes_language_mode", async ({ page }) => {
  await sample(page);
  await bc(page, async (b) => {
    await b.rpc.execute("selection.set", { ids: [1] });
    await b.refresh();
  });
  await settle(page);
  await shot(page, "overview");
  // ten sam projekt do zrzutu okna Tauri (REQ-01-021): burncraft --open test-results/sample.bcraft
  await bc(page, (b) => b.rpc.execute("file.save", { path: "test-results/sample.bcraft" }));
  await setPrefs(page, [["ui.theme", "light"]]);
  await settle(page);
  await shot(page, "overview-light");
  await setPrefs(page, [["ui.theme", "dark"], ["ui.lang", "en"]]);
  await settle(page);
  await shot(page, "overview-en");
  await setPrefs(page, [["ui.lang", "pl"], ["ui.mode", "beginner"]]);
  await settle(page);
  await shot(page, "beginner");
});

test("screens_snapping_and_guides", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 100, y: 100, w: 80, h: 50 }));
  await bc(page, (b) => b.rpc.execute("selection.clear"));
  await fit(page, [60, 60, 260, 200]);
  await page.getByTestId("tool-line").click();
  await hover(page, 100, 100);
  await around(page, "snap-node", 100, 100);
  await hover(page, 140, 100);
  await around(page, "snap-mid", 140, 100);
  await hover(page, 140, 125);
  await around(page, "snap-center", 140, 125);
  await hover(page, 117, 150, 0, 4);
  await around(page, "snap-edge", 117, 150);
  // siatka: znacznik nie jest rysowany (byłby wszędzie) — linia zaczęta obok węzła siatki
  // zaczyna się dokładnie w nim, a rodzaj przyciągnięcia podaje pasek stanu
  await hover(page, 220, 70);
  await page.mouse.down();
  await page.mouse.up();
  await hover(page, 240, 90);
  await around(page, "snap-grid", 230, 80);
  await el(page.locator("#status"), "snap-grid-status");
  await page.keyboard.press("Escape");
  await page.getByTestId("tool-line").click();
  // prowadnica automatyczna: w linii z lewą krawędzią, daleko od obiektu
  await hover(page, 100, 185, 3, 0.4);
  await around(page, "snap-align", 100, 160, 360, 260);
  await page.keyboard.press("Escape");
  // prowadnica ręczna przeciągnięta z linijki
  const top = await page.locator("#ruler-top").boundingBox();
  const [, gy] = await screenOf(page, 0, 80);
  await page.mouse.move(top!.x + 300, top!.y + 10);
  await page.mouse.down();
  await page.mouse.move(top!.x + 300, gy, { steps: 6 });
  await page.mouse.up();
  await expect.poll(() => bc(page, (b) => b.scene.objects.length)).toBe(2);
  await settle(page);
  await shot(page, "guide-manual", { clip: { x: 0, y: 0, width: 1280, height: Math.min(800, gy + 120) } });
});

test("screens_zoom_and_calibration", async ({ page }) => {
  await sample(page);
  await bc(page, async (b) => {
    await b.rpc.execute("selection.set", { ids: [4] });
    await b.refresh();
  });
  await page.getByTestId("zoom-selection").click();
  await settle(page);
  await canvasShot(page, "zoom-selection");
  await page.getByTestId("zoom-page").click();
  await settle(page);
  await canvasShot(page, "zoom-page");
  await page.getByTestId("zoom-100").click();
  await settle(page);
  await shot(page, "zoom-100");
  await page.getByTestId("open-settings").click();
  const dlg = page.getByTestId("settings-dialog");
  await dlg.getByTestId("calibration-measured").fill("96");
  await el(dlg, "calibration");
  await dlg.getByTestId("settings-close").click();
});

test("screens_hidpi", async ({ browser }) => {
  const ctx = await browser.newContext({ deviceScaleFactor: 2, viewport: { width: 1280, height: 800 } });
  const page = await ctx.newPage();
  await resetDocument(page);
  await setPrefs(page, DEFAULT_PREFS);
  await sample(page);
  await fit(page, [200, 60, 360, 200]);
  await canvasShot(page, "hidpi-2x");
  const dpr = await page.evaluate(() => ({ dpr: devicePixelRatio, w: (document.getElementById("canvas") as HTMLCanvasElement).width,
    css: document.getElementById("canvas")!.clientWidth }));
  console.log(`HiDPI: devicePixelRatio=${dpr.dpr}, kanwa ${dpr.w} px dla ${dpr.css} px CSS`);
  await bc(page, (b) => b.rpc.execute("document.new"));
  await ctx.close();
});

/** Zdarzenie wskaźnika dotykowego na kanwie. */
async function touch(page: Page, type: string, id: number, x: number, y: number) {
  await page.evaluate(([type, id, x, y]) => {
    const c = document.getElementById("canvas")!;
    const r = c.getBoundingClientRect();
    c.dispatchEvent(new PointerEvent(type as string, {
      bubbles: true, cancelable: true, pointerId: id as number, pointerType: "touch", isPrimary: id === 1,
      clientX: r.left + (x as number), clientY: r.top + (y as number), button: type === "pointermove" ? -1 : 0,
      buttons: type === "pointerup" ? 0 : 1,
    }));
  }, [type, id, x, y] as const);
}

test("screens_touch_pinch", async ({ page }) => {
  await sample(page);
  await canvasShot(page, "touch-before");
  await touch(page, "pointerdown", 1, 300, 300);
  await touch(page, "pointerdown", 2, 400, 300);
  for (let i = 1; i <= 10; i++) {
    await touch(page, "pointermove", 1, 300 - 10 * i, 300 - 3 * i);
    await touch(page, "pointermove", 2, 400 + 10 * i, 300 - 3 * i);
  }
  await touch(page, "pointerup", 2, 500, 270);
  await touch(page, "pointerup", 1, 200, 270);
  await settle(page);
  await canvasShot(page, "touch-after");
});

test("screens_toolbars_numeric_units_docking", async ({ page }) => {
  await sample(page);
  for (const id of ["main", "create", "modes", "modifiers", "align"]) await el(page.getByTestId(`toolbar-${id}`), `toolbar-${id}`);
  await bc(page, async (b) => {
    await b.rpc.execute("selection.set", { ids: [1] });
    await b.refresh();
  });
  await page.getByTestId("num-anchor-cc").click();
  await settle(page);
  await el(page.getByTestId("toolbar-numeric"), "toolbar-numeric");
  // cale i przeliczanie wartości z jednostką
  await setPrefs(page, [["ui.units", "in"]]);
  await settle(page);
  await el(page.getByTestId("toolbar-numeric"), "units-in");
  await page.getByTestId("num-lock").uncheck();
  await page.getByTestId("num-w").fill("127mm");
  await el(page.getByTestId("toolbar-numeric"), "units-typed");
  await page.getByTestId("num-w").press("Enter");
  await settle(page);
  await el(page.getByTestId("toolbar-numeric"), "units-converted");
  await setPrefs(page, [["ui.units", "mm"]]);
  // pasek węzłów z zaznaczonym węzłem krzywej
  await bc(page, async (b) => {
    await b.rpc.execute("selection.set", { ids: [5] });
    await b.refresh();
  });
  await page.getByTestId("tool-node").click();
  await expect.poll(() => bc(page, (b) => b.nodeEdit()?.id)).toBe(5);
  await fit(page, [0, -40, 400, 160]);
  await bc(page, (b) => b.nodeSelect([0]));
  await settle(page);
  await el(page.getByTestId("toolbar-node"), "toolbar-node");
  // menu kontekstowe węzła
  const n0 = await bc<[number, number]>(page, (b) => b.nodeEdit().contours[0].nodes[0].p);
  const [nx, ny] = await screenOf(page, n0[0], n0[1]);
  await page.mouse.click(nx, ny, { button: "right" });
  await expect(page.locator("#ctxmenu")).toBeVisible();
  await shot(page, "node-context-menu", { clip: { x: Math.max(0, nx - 60), y: Math.max(0, ny - 60), width: 420, height: 300 } });
  await page.keyboard.press("Escape");
  await page.getByTestId("tool-select").click();
  // dokowanie: modyfikatory w prawym doku, wyrównywanie w lewym, układ zapisany pod nazwą
  await page.getByTestId("tb-menu-align").click();
  await page.getByTestId("dock-left").click();
  await page.getByTestId("tb-grip-modifiers").dragTo(page.locator("#dock-right"));
  await settle(page);
  await shot(page, "docking");
  await page.getByTestId("open-settings").click();
  const dlg = page.getByTestId("settings-dialog");
  await dlg.getByTestId("layout-name").fill("Cięcie");
  await dlg.getByTestId("layout-save").click();
  await el(dlg, "settings-toolbars");
  await dlg.getByTestId("settings-close").click();
});

test("screens_panels_layers_properties_history_notes", async ({ page }) => {
  await sample(page);
  await el(page.getByTestId("panel-layers"), "panel-layers");
  await bc(page, async (b) => {
    await b.rpc.execute("selection.set", { ids: [3] });
    await b.refresh();
  });
  await settle(page);
  await el(page.getByTestId("panel-properties"), "panel-properties");
  // historia: podgląd stanu po najechaniu
  await page.getByTestId("history-1").hover();
  await expect(page.getByTestId("history-preview")).toBeVisible();
  const ol = await page.getByTestId("panel-history").boundingBox();
  const pv = await page.getByTestId("history-preview").boundingBox();
  const x0 = Math.min(ol!.x, pv!.x) - 4;
  const y0 = Math.min(ol!.y, pv!.y) - 4;
  await shot(page, "history-preview", { clip: { x: x0, y: y0, width: Math.max(ol!.x + ol!.width, pv!.x + pv!.width) - x0 + 4,
    height: Math.max(ol!.y + ol!.height, pv!.y + pv!.height) - y0 + 4 } });
  await page.locator("#canvas").hover();
  // notatki z obrazem (miniatura projektu jako obraz)
  await bc(page, async (b) => {
    const p = await b.rpc.execute("edit.history_preview", { offset: 0, max_size: 240 });
    await b.rpc.execute("document.set_notes", { text: "Sklejka 3 mm — test mocy przed cięciem.\nOstrość po wymianie stołu: 12 mm." });
    await b.rpc.execute("document.add_note_image", { data_base64: p.png_base64, name: "projekt.png" });
    await b.refresh();
  });
  await expect(page.getByTestId("note-image-0").locator("img")).toBeVisible();
  await page.getByTestId("panel-notes").scrollIntoViewIfNeeded();
  await el(page.getByTestId("panel-notes"), "panel-notes");
});

test("screens_tool_cursors", async ({ page }) => {
  const ids = await page.locator("[data-testid^='tool-']").evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.testid!.slice(5)));
  const cursors: [string, string][] = [];
  for (const id of ids) {
    await page.getByTestId(`tool-${id}`).click();
    cursors.push([id, await page.locator("#canvas").evaluate((c) => (c as HTMLElement).style.cursor)]);
  }
  await page.getByTestId("tool-select").click();
  // kursory jako obrazy (zrzut ekranu nie obejmuje kursora myszy)
  await page.evaluate((list) => {
    const box = document.createElement("div");
    box.id = "cursor-strip";
    box.style.cssText = "position:fixed;left:20px;top:20px;z-index:999;display:flex;gap:14px;padding:12px;background:#3a3d42;border-radius:6px;font:12px sans-serif;color:#ddd";
    for (const [id, c] of list) {
      const m = /url\("(.+?)"\)/.exec(c);
      const cell = document.createElement("div");
      cell.style.cssText = "display:flex;flex-direction:column;align-items:center;gap:4px";
      if (m) {
        const img = document.createElement("img");
        img.src = m[1];
        img.style.cssText = "width:48px;height:48px;image-rendering:pixelated;background:#8a8f96";
        cell.append(img);
      } else {
        const s = document.createElement("div");
        s.textContent = c;
        s.style.cssText = "width:48px;height:48px;display:flex;align-items:center;justify-content:center;background:#8a8f96";
        cell.append(s);
      }
      const label = document.createElement("span");
      label.textContent = id;
      cell.append(label);
      box.append(cell);
    }
    document.body.append(box);
  }, cursors);
  await el(page.locator("#cursor-strip"), "cursors");
});
