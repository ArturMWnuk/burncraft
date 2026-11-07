// Okno ustawień warstwy (REQ-03-032, §3 „Warstwy/Cięcia”): dwuklik na wierszu warstwy
// otwiera okno ze wszystkimi parametrami `layer.set` i `layer.params` z §7.2 w zakładkach
// Ogólne, Cięcie, Grawer/Skanowanie, Zaawansowane, Pod-warstwy. Każde pole jest
// poleceniem rejestru (REQ-01-029).

import { expect, Page, test } from "@playwright/test";
import { bc, objects, resetDocument } from "./helpers";

const layer0 = (page: Page) => bc<any>(page, (b) => b.scene.layers.find((l: any) => l.index === 0));
const param = (page: Page, path: string) =>
  bc<any>(page, `(b) => { let v = b.scene.layers.find((l) => l.index === 0).params; for (const k of ${JSON.stringify(path.split("."))}) v = v?.[k]; return v; }`);

/** Pole liczbowe/tekstowe: wpisanie i zatwierdzenie (change). */
async function set(page: Page, id: string, value: string) {
  const f = page.getByTestId(id);
  await f.fill(value);
  await f.press("Enter");
}

async function openTab(page: Page, tab: string) {
  await page.getByTestId(`ls-tab-${tab}`).click();
  await expect(page.getByTestId(`ls-page-${tab}`)).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "pl" });
    await b.loadPrefs();
    await b.rpc.execute("shape.rect", { x: 10, y: 10, w: 40, h: 30, layer: 0 });
    await b.refresh();
    b.commandLog = [];
  });
  await expect.poll(() => objects(page)).toBe(1);
});

test.afterEach(async ({ page }) => {
  await bc(page, (b) => b.exec("layer.settings_close"));
});

test("req_03_032_layer_settings_window_opens_on_double_click", async ({ page }) => {
  await page.getByTestId("layer-0").locator("td").nth(1).dblclick();
  await expect(page.getByTestId("layer-dialog")).toBeVisible();
  await expect(page.getByTestId("layer-dialog")).toContainText("C00");
  expect(await bc<string[]>(page, (b) => [...b.commandLog])).toContain("layer.settings_open");
  for (const tab of ["general", "cut", "scan", "advanced", "sublayers"]) await expect(page.getByTestId(`ls-tab-${tab}`)).toBeVisible();
  // każde pole i przycisk okna to polecenie rejestru (REQ-01-029)
  const bad = await bc<string[]>(page, (b) => {
    const root = document.querySelector("[data-testid=layer-dialog]")!;
    return [...root.querySelectorAll("input, select, textarea, button")]
      .filter((e) => !b.commands.get(e.getAttribute("data-command") ?? ""))
      .map((e) => e.getAttribute("data-testid") ?? e.outerHTML.slice(0, 60));
  });
  expect(bad).toEqual([]);
  // Escape zamyka okno
  await page.getByTestId("ls-name").press("Escape");
  await expect(page.getByTestId("layer-dialog")).toHaveCount(0);
});

test("req_03_032_layer_settings_general_and_cut", async ({ page }) => {
  await bc(page, (b) => b.exec("layer.settings_open", { index: 0 }));
  await expect(page.getByTestId("layer-dialog")).toBeVisible();
  // Ogólne: pola layer.set i podstawowe parametry przebiegu
  await set(page, "ls-name", "Cięcie sklejki");
  await set(page, "ls-speed", "6");
  await set(page, "ls-power_max", "100");
  await set(page, "ls-power_min", "15");
  await set(page, "ls-passes", "2");
  await page.getByTestId("ls-air_assist").uncheck();
  await set(page, "ls-pass_delay", "1.5");
  await set(page, "ls-priority", "-1");
  await set(page, "ls-pause_message", "Zmień materiał");
  await expect.poll(async () => {
    const l = await layer0(page);
    return [l.name, l.speed, l.power_max, l.power_min, l.passes, l.air_assist, l.params.pass_delay, l.params.priority, l.params.pause_message];
  }).toEqual(["Cięcie sklejki", 6, 100, 15, 2, false, 1.5, -1, "Zmień materiał"]);
  // Cięcie: kerf, mostki, lead-in, overcut, perforacja, tryb punktowy, rampa
  await openTab(page, "cut");
  await set(page, "ls-kerf", "0.15");
  await page.getByTestId("ls-tabs-enabled").check();
  await set(page, "ls-tabs-size", "0.8");
  await set(page, "ls-tabs-count", "4");
  await page.getByTestId("ls-lead_in-kind").selectOption("line");
  await set(page, "ls-lead_in-length", "2");
  await set(page, "ls-overcut", "1");
  await page.getByTestId("ls-perforation-on").check();
  await set(page, "ls-perforation-cut", "2");
  await set(page, "ls-perforation-gap", "1");
  await page.getByTestId("ls-ramp-on").check();
  await set(page, "ls-ramp-length", "3");
  await page.getByTestId("ls-corner_ramp").check();
  await expect.poll(() => param(page, "kerf")).toBe(0.15);
  await expect.poll(() => param(page, "tabs")).toMatchObject({ enabled: true, size: 0.8, count: 4 });
  await expect.poll(() => param(page, "lead_in")).toMatchObject({ kind: "line", length: 2 });
  await expect.poll(() => param(page, "overcut")).toBe(1);
  await expect.poll(() => param(page, "perforation")).toEqual({ cut: 2, gap: 1 });
  await expect.poll(() => param(page, "ramp")).toMatchObject({ length: 3 });
  await expect.poll(() => param(page, "corner_ramp")).toBe(true);
  // wyłączenie perforacji usuwa ją z parametrów
  await page.getByTestId("ls-perforation-on").uncheck();
  await expect.poll(() => param(page, "perforation")).toBeNull();
  // błędna wartość: komunikat, parametr bez zmian
  await set(page, "ls-kerf", "50");
  await expect(page.getByTestId("ls-error")).toContainText("kerf");
  expect(await param(page, "kerf")).toBe(0.15);
});

test("req_03_032_layer_settings_scan_advanced_sublayers", async ({ page }) => {
  await bc(page, (b) => b.exec("layer.settings_open", { index: 0, tab: "scan" }));
  await expect(page.getByTestId("ls-page-scan")).toBeVisible();
  await page.getByTestId("ls-bidirectional").uncheck();
  await set(page, "ls-scan_angle", "45");
  await page.getByTestId("ls-cross_hatch").check();
  await set(page, "ls-lpi", "254");
  await expect.poll(() => param(page, "interval")).toBeCloseTo(0.1, 9);
  await expect(page.getByTestId("ls-interval")).toHaveValue("0.1");
  await page.getByTestId("ls-overscan-kind").selectOption("percent");
  await set(page, "ls-overscan-percent", "2.5");
  await set(page, "ls-fill_kerf", "0.05");
  await page.getByTestId("ls-fill_grouping").selectOption("each");
  await page.getByTestId("ls-scan_offset-add").click();
  await set(page, "ls-scan_offset-0-speed", "200");
  await set(page, "ls-scan_offset-0-offset", "0.04");
  await expect.poll(async () => [await param(page, "bidirectional"), await param(page, "scan_angle"), await param(page, "cross_hatch"), await param(page, "fill_grouping")])
    .toEqual([false, 45, true, "each"]);
  await expect.poll(() => param(page, "overscan")).toEqual({ kind: "percent", percent: 2.5 });
  await expect.poll(() => param(page, "scan_offsets")).toEqual([{ speed: 200, offset: 0.04 }]);
  // Zaawansowane: drugi laser, PWM, M3/M4, Z, makra G-code
  await openTab(page, "advanced");
  await page.getByTestId("ls-power2-on").check();
  await set(page, "ls-power2_max", "40");
  await set(page, "ls-pwm_hz", "1000");
  await page.getByTestId("ls-dynamic_power").uncheck();
  await set(page, "ls-z_offset", "1.2");
  await set(page, "ls-z_step", "0.5");
  // pole wielowierszowe: Enter to nowy wiersz, zmiana zapisuje się po wyjściu z pola
  await page.getByTestId("ls-gcode_pre").fill("M8");
  await page.getByTestId("ls-gcode_pre").blur();
  await expect.poll(async () => [await param(page, "power2_max"), await param(page, "pwm_hz"), await param(page, "dynamic_power"), await param(page, "z_offset"), await param(page, "z_step"), await param(page, "gcode_pre")])
    .toEqual([40, 1000, false, 1.2, 0.5, "M8"]);
  // Pod-warstwy: dodanie, zmiana, usunięcie
  await openTab(page, "sublayers");
  await page.getByTestId("ls-sub-add").click();
  await expect(page.getByTestId("ls-sub-0-mode")).toBeVisible();
  await page.getByTestId("ls-sub-0-mode").selectOption("fill");
  await set(page, "ls-sub-0-name", "Grawer krawędzi");
  await set(page, "ls-sub-0-speed", "150");
  await set(page, "ls-sub-0-power_max", "20");
  await expect.poll(() => param(page, "sublayers")).toEqual([
    expect.objectContaining({ name: "Grawer krawędzi", mode: "fill", speed: 150, power_max: 20, output: true }),
  ]);
  await page.getByTestId("ls-sub-0-delete").click();
  await expect.poll(() => param(page, "sublayers")).toEqual([]);
  // zmiany są cofalne jak każde polecenie dokumentu
  await bc(page, (b) => b.rpc.execute("edit.undo", {}));
  await expect.poll(() => param(page, "sublayers")).toHaveLength(1);
});

test("req_03_032_layer_settings_quick_edits_are_not_lost", async ({ page }) => {
  // kilka zmian jednego obiektu parametrów jedna po drugiej, bez czekania na odświeżenie
  // sceny (szybki użytkownik, wolny host) — żadna nie może nadpisać poprzedniej
  await bc(page, (b) => b.exec("layer.settings_open", { index: 0, tab: "cut" }));
  await expect(page.getByTestId("ls-page-cut")).toBeVisible();
  await bc(page, () => {
    const fire = (id: string, v: string | boolean) => {
      const el = document.querySelector(`[data-testid=${id}]`) as HTMLInputElement;
      if (typeof v === "boolean") el.checked = v;
      else el.value = v;
      el.dispatchEvent(new Event("change"));
    };
    fire("ls-tabs-enabled", true);
    fire("ls-tabs-size", "0.8");
    fire("ls-tabs-count", "4");
    fire("ls-tabs-spacing", "30");
    fire("ls-lead_in-length", "3");
    fire("ls-lead_in-angle", "45");
  });
  await expect.poll(() => param(page, "tabs")).toMatchObject({ enabled: true, size: 0.8, count: 4, spacing: 30 });
  await expect.poll(() => param(page, "lead_in")).toMatchObject({ length: 3, angle: 45 });
  // to samo dla wierszy listy
  await openTab(page, "scan");
  await page.getByTestId("ls-scan_offset-add").click();
  await expect(page.getByTestId("ls-scan_offset-0-speed")).toBeVisible();
  await bc(page, () => {
    for (const [id, v] of [["ls-scan_offset-0-speed", "200"], ["ls-scan_offset-0-offset", "0.04"]]) {
      const el = document.querySelector(`[data-testid=${id}]`) as HTMLInputElement;
      el.value = v;
      el.dispatchEvent(new Event("change"));
    }
  });
  await expect.poll(() => param(page, "scan_offsets")).toEqual([{ speed: 200, offset: 0.04 }]);
});

test("req_03_032_layer_settings_translations_en", async ({ page }) => {
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "en" });
    await b.loadPrefs();
    b.exec("layer.settings_open", { index: 0 });
  });
  await expect(page.getByTestId("layer-dialog")).toContainText("Layer settings");
  await expect(page.getByTestId("ls-tab-cut")).toHaveText("Cut");
  await bc(page, async (b) => {
    b.exec("layer.settings_close");
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "pl" });
    await b.loadPrefs();
  });
});
