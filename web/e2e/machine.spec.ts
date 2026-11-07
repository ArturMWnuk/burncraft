// M2.5 — GUI maszyny z emulatorem GRBL jako urządzeniem („Symulator”): okno Laser
// (start/pauza/stop, ramka, punkt startowy, pozycja zadania, licznik przejść, czas),
// okno Przesuń (jog XY i Z, kroki i prędkości, wolno/szybko, zapamiętane pozycje),
// Konsola (kolorowanie, historia), Makra, Podgląd z odtwarzaniem, położenie głowicy,
// kerf w edytorze i podglądzie, wyrównanie do pozycji lasera. Każda kontrolka
// to polecenie rejestru (REQ-01-029).

import { expect, Page, test } from "@playwright/test";
import { bc, objects, resetDocument } from "./helpers";

test.setTimeout(60_000);

const log = (page: Page) => bc<string[]>(page, (b) => [...b.commandLog]);

async function disconnectAll(page: Page) {
  await bc(page, async (b) => {
    const l = await b.rpc.execute("machine.list", {});
    for (const n of l.connected) await b.rpc.execute("machine.disconnect", { device: n });
    for (const m of (await b.rpc.execute("macro.list", {})).macros) await b.rpc.execute("macro.delete", { name: m.name });
    for (const p of (await b.rpc.execute("machine.positions", {})).positions) await b.rpc.execute("machine.position_delete", { name: p.name });
    await b.rpc.execute("job.start_settings", { from: "absolute", origin: "bottom_left" });
  });
}

/** Połączenie z urządzeniem fikcyjnym z okna Laser. */
async function connect(page: Page) {
  await page.getByTestId("laser-device").selectOption("Symulator");
  await page.getByTestId("laser-connect").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-state", "idle", { timeout: 15_000 });
}

/** Mały prostokąt na warstwie 0 (krótkie zadanie na emulatorze w czasie rzeczywistym). */
async function smallJob(page: Page, passes = 1, size = 6) {
  await bc(page, `async (b) => {
    await b.rpc.execute("shape.rect", { x: 20, y: 20, w: ${size}, h: ${size}, layer: 0 });
    await b.rpc.execute("layer.set", { index: 0, speed: 40, power_max: 20, passes: ${passes}, air_assist: false });
    await b.refresh();
  }`);
  await expect.poll(() => objects(page)).toBe(1);
}

async function head(page: Page): Promise<number[]> {
  return bc<number[]>(page, (b) => b.machine.status?.state?.wpos ?? []);
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
  await disconnectAll(page);
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "pl" });
    await b.loadPrefs();
    b.commandLog = [];
  });
});

test.afterEach(async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("prefs.set", { key: "ui.start_color", value: null }));
  await bc(page, (b) => b.rpc.execute("prefs.set", { key: "ui.stop_color", value: null }));
  await disconnectAll(page);
});

test("req_01_029_machine_windows_controls_are_registry_commands", async ({ page }) => {
  await connect(page);
  await page.getByTestId("laser-preview").click();
  await expect(page.getByTestId("preview-dialog")).toBeVisible();
  const missing = await bc<string[]>(page, (b) => {
    const out: string[] = [];
    const roots = ["panel-laser", "panel-move", "panel-console", "panel-macros", "preview-dialog"].map((id) => document.querySelector(`[data-testid=${id}]`));
    let n = 0;
    for (const root of roots) {
      if (!root) {
        out.push("brak okna");
        continue;
      }
      for (const el of root.querySelectorAll<HTMLElement>("button:not(.panel-head), input, select, textarea")) {
        n++;
        const id = el.dataset.command;
        if (!id || !b.commands.has(id)) out.push(`${el.dataset.testid ?? el.outerHTML.slice(0, 60)} → ${id ?? "brak"}`);
      }
    }
    return n > 30 ? out : [`za mało kontrolek: ${n}`];
  });
  expect(missing).toEqual([]);
});

test("req_03_033_laser_window_start_pause_resume_stop", async ({ page }) => {
  await smallJob(page, 1, 30);
  await connect(page);
  await page.getByTestId("laser-start").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-job", "running", { timeout: 10_000 });
  await page.getByTestId("laser-pause").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-job", "paused", { timeout: 10_000 });
  await page.getByTestId("laser-resume").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-job", "running", { timeout: 10_000 });
  await page.getByTestId("laser-stop").click();
  await expect(page.getByTestId("laser-state")).not.toHaveAttribute("data-job", /running|paused/, { timeout: 10_000 });
  const ids = await log(page);
  for (const id of ["machine.connect", "job.start", "job.pause", "job.resume", "job.stop"]) expect(ids).toContain(id);
});

test("req_03_033_laser_window_frame_start_point_and_job_position", async ({ page }) => {
  await smallJob(page);
  await connect(page);
  // punkt startowy (Absolute / Current / User Origin) i pozycja zadania (9 punktów)
  await page.getByTestId("laser-start-from").selectOption("user_origin");
  await page.getByTestId("laser-job-origin-center").click();
  const st = await bc<any>(page, (b) => b.rpc.execute("job.start_settings", {}));
  expect(st.from).toBe("user_origin");
  expect(st.origin).toBe("center");
  await expect(page.getByTestId("laser-job-origin-center")).toHaveAttribute("aria-pressed", "true");
  // zero użytkownika w bieżącej pozycji i ramka wokół zadania
  await page.getByTestId("laser-set-origin").click();
  await page.getByTestId("laser-frame").click();
  await expect.poll(() => log(page)).toContain("device.frame");
  await page.getByTestId("laser-home").click();
  await page.getByTestId("laser-go-origin").click();
  const ids = await log(page);
  for (const id of ["job.start_settings", "device.set_origin", "device.frame", "device.home", "device.go_origin"]) expect(ids).toContain(id);
});

test("req_13_196_laser_window_adds_serial_profile", async ({ page }) => {
  await expect(page.getByTestId("laser-find")).toHaveAttribute("data-command", "machine.find");
  await page.getByTestId("laser-port").fill("COM99");
  await page.getByTestId("laser-add-port").click();
  await expect(page.getByTestId("laser-device").locator("option", { hasText: "Laser COM99" })).toHaveCount(1);
  const l = await bc<any>(page, (b) => b.rpc.execute("machine.list", {}));
  const p = l.profiles.find((x: any) => x.name === "Laser COM99");
  expect(p.transport).toEqual({ kind: "serial", port: "COM99", baud: 115200 });
  await bc(page, (b) => b.rpc.execute("machine.profile_delete", { name: "Laser COM99" }));
  expect(await log(page)).toContain("machine.profile_save");
});

test("req_03_052_start_green_stop_red_configurable_colors", async ({ page }) => {
  const bg = (id: string) => page.getByTestId(id).evaluate((e) => getComputedStyle(e).backgroundColor);
  expect(await bg("laser-start")).toBe("rgb(31, 157, 85)");
  expect(await bg("laser-stop")).toBe("rgb(214, 48, 49)");
  // kolory w oknie Ustawienia
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-start-color").fill("#2050c0");
  await page.getByTestId("pref-stop-color").fill("#ff8800");
  await page.getByTestId("settings-close").click();
  await expect.poll(() => bg("laser-start")).toBe("rgb(32, 80, 192)");
  expect(await bg("laser-stop")).toBe("rgb(255, 136, 0)");
  const p = await bc<any>(page, (b) => b.rpc.execute("prefs.get", {}));
  expect(p.value.ui.start_color).toBe("#2050c0");
});

test("req_03_053_054_pass_counter_and_time_after_finish", async ({ page }) => {
  await smallJob(page, 3, 12);
  await connect(page);
  await page.getByTestId("laser-start").click();
  // licznik przejść obok procentu w trakcie zadania
  await expect(page.getByTestId("laser-pass")).toHaveText(/przejście [123]\/3/, { timeout: 10_000 });
  await expect(page.getByTestId("laser-progress")).toHaveText(/\d+%/);
  // po zakończeniu: czas zadania
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-job", "done", { timeout: 30_000 });
  await expect(page.getByTestId("laser-time")).toHaveText(/\d\d:\d\d:\d\d/);
  await expect(page.getByTestId("laser-pass")).toHaveText("przejście 3/3");
  await expect(page.getByTestId("laser-progress")).toHaveText("100%");
});

test("req_03_034_037_038_move_window_jog_xy_steps_and_speeds", async ({ page }) => {
  await connect(page);
  await page.getByTestId("jog-step-xy").fill("10");
  await page.getByTestId("jog-step-xy").press("Enter");
  await page.getByTestId("jog-fast").fill("80");
  await page.getByTestId("jog-fast").press("Enter");
  await page.getByTestId("jog-slow").fill("4");
  await page.getByTestId("jog-slow").press("Enter");
  await page.getByTestId("jog-mode-fast").click();
  await page.getByTestId("jog-x-plus").click();
  await expect.poll(() => head(page), { timeout: 10_000 }).toEqual([10, 0, 0]);
  expect(await bc<any>(page, (b) => b.machine.lastJog)).toEqual({ axis: "x", distance: 10, speed: 80 });
  await page.getByTestId("jog-mode-slow").click();
  await page.getByTestId("jog-y-plus").click();
  await expect.poll(() => head(page), { timeout: 15_000 }).toEqual([10, 10, 0]);
  expect(await bc<any>(page, (b) => b.machine.lastJog)).toEqual({ axis: "y", distance: 10, speed: 4 });
  await page.getByTestId("jog-x-minus").click();
  await page.getByTestId("jog-y-minus").click();
  await expect.poll(() => head(page), { timeout: 15_000 }).toEqual([0, 0, 0]);
  // ustawienia zapamiętane (preferencje)
  const p = await bc<any>(page, (b) => b.rpc.execute("prefs.get", {}));
  expect(p.value.ui.jog).toMatchObject({ step_xy: 10, slow: 4, fast: 80, mode: "slow" });
  const ids = await log(page);
  expect(ids).toContain("device.jog");
  expect(ids).toContain("jog.settings");
});

test("req_03_035_move_window_jog_z", async ({ page }) => {
  await connect(page);
  await page.getByTestId("jog-step-z").fill("2");
  await page.getByTestId("jog-step-z").press("Enter");
  await page.getByTestId("jog-z-plus").click();
  await expect.poll(() => bc<any>(page, (b) => b.machine.lastJog)).toMatchObject({ axis: "z", distance: 2 });
  await page.getByTestId("jog-z-minus").click();
  await expect.poll(() => bc<any>(page, (b) => b.machine.lastJog)).toMatchObject({ axis: "z", distance: -2 });
});

test("req_03_039_move_window_saved_positions", async ({ page }) => {
  await connect(page);
  await bc(page, (b) => b.rpc.execute("device.move_to", { x: 40, y: 30 }));
  await expect.poll(() => head(page), { timeout: 10_000 }).toEqual([40, 30, 0]);
  await page.getByTestId("pos-name").fill("Narożnik");
  await page.getByTestId("pos-save").click();
  await expect(page.getByTestId("pos-go-0")).toHaveText(/Narożnik/);
  await expect(page.getByTestId("pos-go-0")).toHaveText(/40.*30/);
  await bc(page, (b) => b.rpc.execute("device.move_to", { x: 0, y: 0 }));
  await expect.poll(() => head(page), { timeout: 10_000 }).toEqual([0, 0, 0]);
  await page.getByTestId("pos-go-0").click();
  await expect.poll(() => head(page), { timeout: 10_000 }).toEqual([40, 30, 0]);
  await page.getByTestId("pos-del-0").click();
  await expect(page.getByTestId("pos-go-0")).toHaveCount(0);
  const ids = await log(page);
  for (const id of ["machine.position_save", "device.position_go", "machine.position_delete"]) expect(ids).toContain(id);
});

test("req_03_040_console_syntax_highlight_and_history", async ({ page }) => {
  await connect(page);
  const input = page.getByTestId("console-input");
  await input.fill("G0 X5 Y2 ; komentarz");
  await input.press("Enter");
  await input.fill("$I");
  await input.press("Enter");
  const logEl = page.getByTestId("console-log");
  // kolorowanie: wysłane / odebrane, słowa G, liczby, komentarze, ok
  await expect(logEl.locator(".tok-g").first()).toHaveText("G0");
  await expect(logEl.locator(".tok-axis").first()).toHaveText("X");
  await expect(logEl.locator(".tok-num").first()).toHaveText("5");
  await expect(logEl.locator(".tok-comment").first()).toContainText("komentarz");
  await expect(logEl.locator(".tok-ok").first()).toHaveText("ok");
  await expect(logEl.locator(".line-sent").first()).toBeVisible();
  await expect(logEl.locator(".line-recv").first()).toBeVisible();
  // błąd na czerwono
  await input.fill("G999");
  await input.press("Enter");
  await expect(logEl.locator(".tok-error").first()).toContainText("error");
  // historia: strzałki w górę i w dół
  await input.press("ArrowUp");
  await expect(input).toHaveValue("G999");
  await input.press("ArrowUp");
  await expect(input).toHaveValue("$I");
  await input.press("ArrowUp");
  await expect(input).toHaveValue("G0 X5 Y2 ; komentarz");
  await input.press("ArrowDown");
  await expect(input).toHaveValue("$I");
  expect(await log(page)).toContain("device.raw");
  // historia przetrwa przeładowanie okna
  await page.reload();
  await expect(page.locator("#status")).toHaveAttribute("data-connected", "true");
  await expect.poll(() => bc<number>(page, (b) => b.machine.history.length)).toBeGreaterThan(2);
  await page.getByTestId("console-input").press("ArrowUp");
  await expect(page.getByTestId("console-input")).toHaveValue("G999");
});

test("req_03_040_console_keeps_focus_when_window_rerenders", async ({ page }) => {
  // przebudowa okien (odświeżenie list, preferencje) nie może zabierać fokusu z Konsoli
  await connect(page);
  const input = page.getByTestId("console-input");
  await input.fill("$I");
  await input.press("Enter");
  await input.fill("G0 X1");
  await bc(page, async (b) => {
    await b.machine.reload();
    b.machine.applyPrefs({});
  });
  await expect(input).toBeFocused();
  await page.keyboard.type(" Y2");
  await expect(input).toHaveValue("G0 X1 Y2");
});

test("req_03_041_macros_window", async ({ page }) => {
  await connect(page);
  await page.getByTestId("macro-name").fill("Do zera");
  await page.getByTestId("macro-lines").fill("G0 X12 Y7\nG0 X3");
  await page.getByTestId("macro-save").click();
  await expect(page.getByTestId("macro-run-0")).toHaveText(/Do zera/);
  await page.getByTestId("macro-run-0").click();
  await expect.poll(() => head(page), { timeout: 10_000 }).toEqual([3, 7, 0]);
  const m = await bc<any>(page, (b) => b.rpc.execute("macro.list", {}));
  expect(m.macros[0].steps).toEqual([{ kind: "raw", line: "G0 X12 Y7" }, { kind: "raw", line: "G0 X3" }]);
  await page.getByTestId("macro-del-0").click();
  await expect(page.getByTestId("macro-run-0")).toHaveCount(0);
  const ids = await log(page);
  for (const id of ["macro.save", "macro.run", "macro.delete"]) expect(ids).toContain(id);
});

// ---------- podgląd ----------

async function openPreview(page: Page) {
  await page.getByTestId("tb-preview").click();
  await expect(page.getByTestId("preview-dialog")).toBeVisible();
}

async function twoParts(page: Page) {
  await bc(page, async (b) => {
    await b.rpc.execute("shape.rect", { x: 10, y: 10, w: 30, h: 20, layer: 0 });
    await b.rpc.execute("shape.rect", { x: 60, y: 10, w: 20, h: 20, layer: 1 });
    await b.rpc.execute("layer.params", { index: 0, params: { kerf: 0.5 } });
    await b.rpc.execute("selection.clear", {});
    await b.refresh();
  });
  await expect.poll(() => objects(page)).toBe(2);
}

const pv = (page: Page) => bc<any>(page, (b) => ({ ...b.preview.state, stats: b.preview.stats }));

test("req_03_046_preview_window_playback_req_09_001_slider_speed", async ({ page }) => {
  await twoParts(page);
  await openPreview(page);
  const s0 = await pv(page);
  expect(s0.total).toBeGreaterThan(1);
  expect(s0.time).toBeCloseTo(s0.total, 6); // otwarcie: całe zadanie
  // suwak: połowa czasu → część segmentów
  const slider = page.getByTestId("preview-slider");
  await slider.fill((s0.total / 2).toFixed(3));
  await slider.dispatchEvent("input");
  const s1 = await pv(page);
  expect(s1.time).toBeCloseTo(s0.total / 2, 3);
  expect(s1.stats.cut).toBeLessThan(s0.stats.cut);
  expect(s1.head).toHaveLength(2);
  // odtwarzanie z prędkością ×2: zadanie spowolnione (10 mm/s, kilkadziesiąt sekund), żeby
  // pauza wypadła przed końcem także na wolnym runnerze CI
  await bc(page, async (b) => {
    for (const index of [0, 1]) await b.rpc.execute("layer.set", { index, speed: 10 });
    await b.refresh();
  });
  await expect.poll(async () => (await pv(page)).total, { timeout: 10_000 }).toBeGreaterThan(15);
  await page.getByTestId("preview-speed").selectOption("2");
  await slider.fill("0");
  await slider.dispatchEvent("input");
  await page.getByTestId("preview-play").click();
  await expect.poll(async () => (await pv(page)).time, { timeout: 10_000 }).toBeGreaterThan(0.5);
  expect((await pv(page)).speed).toBe(2);
  expect((await pv(page)).playing).toBe(true);
  await page.getByTestId("preview-play").click();
  expect((await pv(page)).playing).toBe(false);
  await expect(page.getByTestId("preview-time")).toHaveText(/\d\d:\d\d:\d\d \/ \d\d:\d\d:\d\d/);
  await page.getByTestId("preview-close").click();
  await expect(page.getByTestId("preview-dialog")).toHaveCount(0);
  const ids = await log(page);
  for (const id of ["preview.open", "preview.set", "preview.close"]) expect(ids).toContain(id);
});

test("req_09_002_003_005_006_preview_travel_order_direction_start_points", async ({ page }) => {
  await twoParts(page);
  await openPreview(page);
  const s = await pv(page);
  expect(s.stats.travel).toBeGreaterThanOrEqual(2);
  expect(s.stats.labels).toBe(2); // numery kolejności części
  expect(s.stats.arrows).toBeGreaterThanOrEqual(2);
  expect(s.stats.starts).toBe(2);
  for (const [id, key] of [["preview-travel", "travel"], ["preview-order", "labels"], ["preview-direction", "arrows"], ["preview-starts", "starts"]]) {
    await page.getByTestId(id).uncheck();
    expect((await pv(page)).stats[key], id).toBe(0);
    await page.getByTestId(id).check();
    expect((await pv(page)).stats[key], id).toBeGreaterThan(0);
  }
});

test("req_07_030_preview_shows_kerf_req_09_004", async ({ page }) => {
  await twoParts(page);
  await openPreview(page);
  expect((await pv(page)).stats.kerf).toBe(1);
  // szerokość linii cięcia = 2 × kerf w skali podglądu
  expect((await pv(page)).stats.kerfWidthMm[0]).toBeCloseTo(1, 6);
  await page.getByTestId("preview-kerf").uncheck();
  expect((await pv(page)).stats.kerf).toBe(0);
});

test("req_09_013_preview_start_from_here", async ({ page }) => {
  await twoParts(page);
  await bc(page, (b) => b.rpc.execute("layer.set", { index: 1, speed: 60, power_max: 20, air_assist: false }));
  await connect(page);
  await openPreview(page);
  // druga część: suwak na jej początek
  const t = await bc<number>(page, (b) => b.preview.partTime(1));
  const slider = page.getByTestId("preview-slider");
  await slider.fill((t + 0.01).toFixed(3));
  await slider.dispatchEvent("input");
  const seg = await bc<number>(page, (b) => b.preview.state.segment);
  await page.getByTestId("preview-start-here").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-job", /running|done/, { timeout: 10_000 });
  expect(await bc<any>(page, (b) => b.machine.lastStart)).toMatchObject({ from_segment: seg });
});

test("req_07_029_editor_shows_kerf_outline", async ({ page }) => {
  await twoParts(page);
  await expect.poll(() => bc<number>(page, (b) => b.kerf.length)).toBe(1);
  const k = await bc<any>(page, (b) => b.kerf[0]);
  expect(k.layer).toBe(0);
  await bc(page, (b) => b.rpc.execute("layer.params", { index: 0, params: { kerf: 0 } }));
  await expect.poll(() => bc<number>(page, (b) => b.kerf.length)).toBe(0);
});

test("req_03_004_head_position_on_workspace", async ({ page }) => {
  await connect(page);
  await bc(page, (b) => b.rpc.execute("device.move_to", { x: 50, y: 40 }));
  await expect.poll(() => bc<any>(page, (b) => b.machine.head), { timeout: 10_000 }).toEqual([50, 40]);
  // znacznik głowicy na kanwie
  expect(await bc<any>(page, (b) => b.headMarker())).toEqual([50, 40]);
  await page.getByTestId("laser-disconnect").click();
  await expect.poll(() => bc<any>(page, (b) => b.headMarker())).toBeNull();
});

test("req_04_089_align_to_laser_position_button", async ({ page }) => {
  await bc(page, async (b) => {
    const r = await b.rpc.execute("shape.rect", { x: 10, y: 10, w: 20, h: 10, layer: 0 });
    await b.rpc.execute("selection.set", { ids: [r.id] });
    await b.refresh();
  });
  await connect(page);
  await bc(page, (b) => b.rpc.execute("device.move_to", { x: 70, y: 60 }));
  await expect.poll(() => bc<any>(page, (b) => b.machine.head), { timeout: 10_000 }).toEqual([70, 60]);
  const btn = page.getByTestId("align-laser");
  await expect(btn).toHaveAttribute("data-command", "object.align_to_laser");
  await btn.click();
  await expect.poll(() => bc<number[]>(page, (b) => b.scene.objects[0].bbox.map((v: number) => Math.round(v * 1000) / 1000))).toEqual([60, 55, 80, 65]);
});
