// Zrzuty ekranu do procedur ręcznych M2 (docs/procedures/M2-gui-checks.md): okna
// Laser, Przesuń, Konsola, Makra, Podgląd, położenie głowicy i kerf w edytorze.
// Uruchamiane tylko na żądanie: BC_SCREENSHOTS=1 npx playwright test procedures
// (w CI pomijane — zrzuty są częścią dokumentacji, nie testem). Urządzenie: „Symulator”.

import { expect, Locator, Page, test } from "@playwright/test";
import { bc, screenOf } from "./helpers";
import { resetDocument } from "./helpers";

declare const process: { env: Record<string, string | undefined> };

const OUT = "../docs/procedures/img/m2";

test.skip(!process.env.BC_SCREENSHOTS, "zrzuty do procedur: BC_SCREENSHOTS=1");
test.setTimeout(120_000);

const el = (loc: Locator, name: string) => loc.screenshot({ path: `${OUT}/${name}.png` });

async function prefs(page: Page, list: [string, unknown][]) {
  await bc(page, `async (b) => {
    for (const [key, value] of ${JSON.stringify(list)}) await b.rpc.execute("prefs.set", { key, value });
    await b.loadPrefs();
  }`);
}

const DEFAULTS: [string, unknown][] = [
  ["ui.lang", "pl"], ["ui.theme", "dark"], ["ui.units", "mm"], ["ui.mode", "full"], ["ui.toolbars", null],
  ["ui.start_color", null], ["ui.stop_color", null], ["ui.console_history", null],
  ["ui.jog", { step_xy: 10, step_z: 1, slow: 5, fast: 50, mode: "fast" }],
];

async function clean(page: Page) {
  await bc(page, async (b) => {
    const l = await b.rpc.execute("machine.list", {});
    for (const n of l.connected) await b.rpc.execute("machine.disconnect", { device: n });
    for (const m of (await b.rpc.execute("macro.list", {})).macros) await b.rpc.execute("macro.delete", { name: m.name });
    for (const p of (await b.rpc.execute("machine.positions", {})).positions) await b.rpc.execute("machine.position_delete", { name: p.name });
  });
}

async function connect(page: Page) {
  await page.getByTestId("laser-device").selectOption("Symulator");
  await page.getByTestId("laser-connect").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-state", "idle", { timeout: 15_000 });
}

/** Projekt: tablica z otworem (C00, kerf 0,4 mm, 2 przejścia) i grawer napisu-ramki (C01). */
async function sample(page: Page) {
  await bc(page, async (b) => {
    const r = (id: string, p: unknown) => b.rpc.execute(id, p);
    // tablica z otworem: jeden kontur zewnętrzny i otwór (kerf na zewnątrz i do wewnątrz)
    const a = await r("shape.rect", { x: 40, y: 40, w: 120, h: 80, radius: 6, layer: 0 });
    const c = await r("shape.ellipse", { cx: 100, cy: 80, rx: 22, ry: 22, layer: 0 });
    await r("path.boolean", { ids: [a.id, c.id], op: "difference" });
    await r("shape.star", { cx: 220, cy: 80, r_outer: 40, r_inner: 18, points: 5, layer: 1 });
    await r("layer.set", { index: 0, speed: 25, power_max: 60, passes: 2, air_assist: false });
    await r("layer.set", { index: 1, speed: 60, power_max: 25, passes: 1, air_assist: false });
    await r("layer.params", { index: 0, params: { kerf: 0.4 } });
    await r("selection.clear", {});
    await b.refresh();
    b.zoomPage();
  });
}

async function settle(page: Page) {
  await bc(page, async (b) => {
    await b.refresh();
    b.paint();
  });
  await page.waitForTimeout(250);
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
  await clean(page);
  await prefs(page, DEFAULTS);
});

test.afterEach(async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("job.stop", {}));
  await clean(page);
  await prefs(page, DEFAULTS);
  await bc(page, (b) => b.rpc.execute("document.new"));
});

test("screens_m2_laser_window_job_progress_and_time", async ({ page }) => {
  await sample(page);
  await connect(page);
  await page.getByTestId("laser-start").click();
  await expect(page.getByTestId("laser-pass")).toHaveText(/przejście 2\/2/, { timeout: 30_000 });
  await page.waitForTimeout(400);
  await el(page.getByTestId("panel-laser"), "laser-running");
  await page.getByTestId("laser-pause").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-job", "paused", { timeout: 10_000 });
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-state", "hold", { timeout: 10_000 });
  await el(page.getByTestId("panel-laser"), "laser-paused");
  await page.getByTestId("laser-resume").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-job", "done", { timeout: 60_000 });
  await page.waitForTimeout(300);
  await el(page.getByTestId("panel-laser"), "laser-done");
  // ramka w trakcie: głowica na obrysie zadania
  await page.getByTestId("laser-frame").click();
  await page.waitForTimeout(1200);
  await settle(page);
  await page.screenshot({ path: `${OUT}/laser-overview.png` });
});

test("screens_m2_start_stop_colors", async ({ page }) => {
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-start-color").fill("#2050c0");
  await page.getByTestId("pref-stop-color").fill("#ff8800");
  await el(page.locator(".dialog").first(), "settings-colors");
  await page.getByTestId("settings-close").click();
  await page.waitForTimeout(200);
  await el(page.getByTestId("panel-laser"), "laser-custom-colors");
});

test("screens_m2_start_point_and_job_position", async ({ page }) => {
  await sample(page);
  await connect(page);
  await page.getByTestId("laser-start-from").selectOption("user_origin");
  await page.getByTestId("laser-job-origin-center").click();
  await page.waitForTimeout(300);
  await el(page.getByTestId("panel-laser"), "laser-start-from");
  await bc(page, (b) => b.rpc.execute("job.start_settings", { from: "absolute", origin: "bottom_left" }));
});

test("screens_m2_move_window_jog_and_positions", async ({ page }) => {
  await connect(page);
  await page.getByTestId("jog-x-plus").click();
  await page.waitForTimeout(800);
  await page.getByTestId("jog-y-plus").click();
  await page.waitForTimeout(800);
  await page.getByTestId("pos-name").fill("Narożnik płyty");
  await page.getByTestId("pos-save").click();
  await bc(page, (b) => b.rpc.execute("device.move_to", { x: 150, y: 120 }));
  await page.waitForTimeout(2500);
  await page.getByTestId("pos-name").fill("Środek stołu");
  await page.getByTestId("pos-save").click();
  await page.getByTestId("jog-mode-slow").click();
  await page.getByTestId("jog-z-plus").click();
  await page.waitForTimeout(600);
  await el(page.getByTestId("panel-move"), "move-window");
  await page.getByTestId("pos-go-0").click();
  await page.waitForTimeout(3000);
  await el(page.getByTestId("panel-laser"), "move-went-to-position");
});

test("screens_m2_console_and_macros", async ({ page }) => {
  await connect(page);
  const input = page.getByTestId("console-input");
  for (const line of ["$$", "$I", "G0 X10 Y5 ; dojazd", "G1 X20 F600", "M5", "G999"]) {
    await input.fill(line);
    await input.press("Enter");
    await page.waitForTimeout(250);
  }
  await input.press("ArrowUp");
  await input.press("ArrowUp");
  await page.waitForTimeout(400);
  await el(page.getByTestId("panel-console"), "console");
  await page.getByTestId("macro-name").fill("Do narożnika");
  await page.getByTestId("macro-lines").fill("G0 X0 Y0\n@device.home");
  await page.getByTestId("macro-save").click();
  await expect(page.getByTestId("macro-run-0")).toBeVisible();
  await page.getByTestId("macro-name").fill("Test wiązki 1%");
  await page.getByTestId("macro-lines").fill("M3 S10\nG4 P0.2\nM5");
  await page.getByTestId("macro-save").click();
  await page.waitForTimeout(300);
  await el(page.getByTestId("panel-macros"), "macros");
});

test("screens_m2_preview_playback", async ({ page }) => {
  await sample(page);
  await page.getByTestId("tb-preview").click();
  await expect(page.getByTestId("preview-dialog")).toBeVisible();
  await page.waitForTimeout(400);
  await el(page.locator(".dialog.preview"), "preview-full");
  // połowa zadania: tor do bieżącego miejsca, głowica, numery kolejności
  const total = await bc<number>(page, (b) => b.preview.state.total);
  await page.getByTestId("preview-slider").fill((total * 0.55).toFixed(3));
  await page.waitForTimeout(300);
  await el(page.locator(".dialog.preview"), "preview-half");
  // bez przejazdów, kerfu i strzałek
  for (const id of ["preview-travel", "preview-kerf", "preview-direction"]) await page.getByTestId(id).uncheck();
  await page.waitForTimeout(200);
  await el(page.locator(".dialog.preview"), "preview-options-off");
  for (const id of ["preview-travel", "preview-kerf", "preview-direction"]) await page.getByTestId(id).check();
  await page.getByTestId("preview-close").click();
});

test("screens_m2_layer_settings_window", async ({ page }) => {
  await sample(page);
  await bc(page, async (b) => {
    await b.rpc.execute("layer.set", { index: 0, name: "Cięcie sklejki 3 mm" });
    await b.rpc.execute("layer.params", { index: 0, params: { tabs: { enabled: true, size: 0.8, count: 4 }, lead_in: { kind: "line", length: 2, angle: 90 }, pass_delay: 1, pause_message: "Zmień materiał" } });
    await b.rpc.execute("layer.params", { index: 0, params: { sublayers: [{ name: "Grawer krawędzi", mode: "fill", output: true, speed: 150, power_max: 20 }] } });
    await b.refresh();
  });
  await page.getByTestId("layer-0").locator("td").nth(1).dblclick();
  await expect(page.getByTestId("layer-dialog")).toBeVisible();
  const box = page.locator(".dialog.layer-settings");
  for (const tab of ["general", "cut", "scan", "advanced", "sublayers"]) {
    await page.getByTestId(`ls-tab-${tab}`).click();
    await page.waitForTimeout(200);
    await el(box, `layer-settings-${tab}`);
  }
  // błędna wartość: komunikat w oknie
  await page.getByTestId("ls-tab-cut").click();
  await page.getByTestId("ls-kerf").fill("50");
  await page.getByTestId("ls-kerf").press("Enter");
  await expect(page.getByTestId("ls-error")).toContainText("kerf");
  await el(box, "layer-settings-error");
  await page.getByTestId("ls-close").click();
});

test("screens_m2_head_and_kerf_on_workspace", async ({ page }) => {
  await sample(page);
  await connect(page);
  await bc(page, (b) => b.rpc.execute("device.move_to", { x: 190, y: 130 }));
  await expect.poll(() => bc<any>(page, (b) => b.machine.head), { timeout: 15_000 }).toEqual([190, 130]);
  await bc(page, (b) => {
    b.view.fit([20, 20, 280, 150]);
    b.redraw();
  });
  await settle(page);
  await el(page.locator("#workspace"), "head-and-kerf");
  // zbliżenie na kerf: obrys środka wiązki 0,4 mm na zewnątrz prostokąta i do wewnątrz otworu
  await bc(page, (b) => {
    b.view.fit([28, 28, 64, 56]);
    b.redraw();
  });
  await settle(page);
  const [sx, sy] = await screenOf(page, 46, 42);
  await page.screenshot({ path: `${OUT}/kerf-zoom.png`, clip: { x: sx - 260, y: sy - 200, width: 520, height: 400 } });
  // otwór: obrys do wewnątrz
  await bc(page, (b) => {
    b.view.fit([74, 54, 126, 106]);
    b.redraw();
  });
  await settle(page);
  const [hx, hy] = await screenOf(page, 100, 80);
  await page.screenshot({ path: `${OUT}/kerf-hole.png`, clip: { x: hx - 260, y: hy - 200, width: 520, height: 400 } });
  // wyrównanie zaznaczenia do pozycji lasera
  await bc(page, async (b) => {
    const s = b.scene.objects.find((o: any) => o.type === "star" || o.layer === 1);
    await b.rpc.execute("selection.set", { ids: [s.id] });
    await b.refresh();
    b.view.fit([20, 20, 280, 180]);
    b.redraw();
  });
  await page.getByTestId("align-laser").click();
  await settle(page);
  await el(page.locator("#workspace"), "align-to-laser");
});
