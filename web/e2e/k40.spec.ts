// M3.3 — K40 Lihuiyu i Moshi w GUI: urządzenia fikcyjne z emulatorem płyty (REQ-19-009),
// pole mocy nieaktywne bez PWM (REQ-13-045), wariant i ID płyty, korekcja skali
// (REQ-13-046, 047), dodanie K40 z kreatora USB (REQ-13-039). Każda kontrolka to polecenie
// rejestru (REQ-01-029).

import { expect, Page, test } from "@playwright/test";
import { bc, objects, resetDocument } from "./helpers";

test.setTimeout(60_000);

const K40 = "Symulator K40 (Lihuiyu)";

async function cleanup(page: Page) {
  await bc(page, async (b) => {
    const l = await b.rpc.execute("machine.list", {});
    for (const n of l.connected) await b.rpc.execute("machine.disconnect", { device: n });
    for (const p of l.profiles) {
      if (["K40 test", "K40 (USB)", "Moshi (USB)"].includes(p.name)) await b.rpc.execute("machine.profile_delete", { name: p.name });
    }
    await b.refresh();
  });
}

async function rect(page: Page) {
  await bc(page, async (b) => {
    await b.rpc.execute("shape.rect", { x: 20, y: 20, w: 10, h: 6, layer: 0 });
    await b.rpc.execute("layer.set", { index: 0, speed: 100, power_max: 40, air_assist: false });
    await b.refresh();
  });
  await expect.poll(() => objects(page)).toBe(1);
}

async function connect(page: Page, name: string) {
  await page.getByTestId("laser-device").selectOption(name);
  await page.getByTestId("laser-connect").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-state", "idle", { timeout: 15_000 });
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
  await cleanup(page);
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "pl" });
    await b.loadPrefs();
    b.commandLog = [];
  });
});

test.afterEach(async ({ page }) => {
  await cleanup(page);
});

test("req_13_045_no_pwm_power_field_disabled_with_message", async ({ page }) => {
  await rect(page);
  const power = page.getByTestId("layer-0-power");
  await expect(power).toBeEnabled();
  await expect(page.getByTestId("layers-no-pwm")).toHaveCount(0);
  await connect(page, K40);
  // M2 Nano: brak PWM — pole mocy nieaktywne z komunikatem
  await expect(power).toBeDisabled();
  const note = page.getByTestId("layers-no-pwm");
  await expect(note).toBeVisible();
  await expect(note).toContainText("M2 Nano");
  await expect(note).toContainText("pokrętłem");
  await expect(power).toHaveAttribute("title", /PWM/);
  // okno ustawień warstwy: pola mocy też nieaktywne
  await bc(page, (b) => b.exec("layer.settings_open", { index: 0 }));
  await expect(page.getByTestId("ls-power_max")).toBeDisabled();
  await expect(page.getByTestId("ls-power_min")).toBeDisabled();
  await expect(page.getByTestId("ls-speed")).toBeEnabled();
  await expect(page.getByTestId("ls-no-pwm")).toBeVisible();
  await bc(page, (b) => b.exec("layer.settings_close"));
  // inne urządzenie (GRBL z PWM): pole mocy znów aktywne
  await page.getByTestId("laser-disconnect").click();
  await connect(page, "Symulator");
  await expect(power).toBeEnabled();
  await expect(page.getByTestId("layers-no-pwm")).toHaveCount(0);
  // ten sam komunikat po angielsku
  await page.getByTestId("laser-disconnect").click();
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "en" });
    await b.loadPrefs();
  });
  await connect(page, K40);
  await expect(page.getByTestId("layers-no-pwm")).toContainText("knob");
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "pl" });
    await b.loadPrefs();
  });
});

test("req_19_009_k40_simulator_runs_job", async ({ page }) => {
  await rect(page);
  await connect(page, K40);
  await page.getByTestId("laser-start").click();
  await expect
    .poll(async () => bc<string>(page, (b) => b.machine.job?.status ?? ""), { timeout: 30_000 })
    .toBe("done");
  const caps = await bc<any>(page, (b) => b.machine.status?.capabilities);
  expect(caps.native_raster).toBe(true);
  expect(caps.pwm).toBe(false);
  expect(await bc<string[]>(page, (b) => [...b.commandLog])).toContain("job.start");
  // Moshi: urządzenie fikcyjne z emulatorem MS10105
  await page.getByTestId("laser-disconnect").click();
  await connect(page, "Symulator Moshi");
  await page.getByTestId("laser-start").click();
  await expect
    .poll(async () => bc<string>(page, (b) => b.machine.job?.status ?? ""), { timeout: 30_000 })
    .toBe("done");
});

test("req_13_046_047_k40_board_variant_id_and_scale", async ({ page }) => {
  await bc(page, async (b) => {
    await b.rpc.execute("machine.profile_save", {
      profile: { name: "K40 test", driver: "lihuiyu", transport: { kind: "emulator", variant: "lihuiyu" }, workspace: [310, 210], k40: { board: "m2" } },
    });
    await b.machine.reload();
  });
  await page.getByTestId("laser-device").selectOption("K40 test");
  const board = page.getByTestId("k40-board");
  await expect(board).toBeVisible();
  await expect(board).toHaveAttribute("data-command", "machine.profile_save");
  await expect(board.locator("option")).toHaveCount(10);
  await board.selectOption("6c6879_m3");
  const prof = async () =>
    bc<any>(page, async (b) => (await b.rpc.execute("machine.list", {})).profiles.find((p: any) => p.name === "K40 test"));
  await expect.poll(async () => (await prof()).k40.board).toBe("6c6879_m3");
  await page.getByTestId("k40-board-id").fill("AB12CD34");
  await page.getByTestId("k40-board-id").press("Enter");
  await page.getByTestId("k40-board-id").blur();
  await expect.poll(async () => (await prof()).k40.board_id).toBe("AB12CD34");
  await page.getByTestId("k40-scale-x").fill("1,02");
  await page.getByTestId("k40-scale-x").blur();
  await expect.poll(async () => (await prof()).k40.scale[0]).toBe(1.02);
  // urządzenie fikcyjne: ustawienia stałe
  await page.getByTestId("laser-device").selectOption(K40);
  await expect(page.getByTestId("k40-board")).toBeDisabled();
  await expect(page.getByTestId("k40-fixed")).toBeVisible();
});

test("req_13_039_add_k40_from_usb_finder", async ({ page }) => {
  await page.getByTestId("laser-find").click();
  await expect(page.getByTestId("laser-usb-0")).toBeVisible({ timeout: 30_000 });
  // sterownik K40 jest w tej wersji: bez dopisku „w etapie …”
  await expect(page.getByTestId("laser-usb-0")).not.toContainText("M3.3");
  const add = page.getByTestId("laser-usb-0-add-k40");
  await expect(add).toHaveAttribute("data-command", "machine.profile_save");
  await add.click();
  const p = await bc<any>(page, async (b) => (await b.rpc.execute("machine.list", {})).profiles.find((p: any) => p.name === "K40 (USB)"));
  expect(p.driver).toBe("lihuiyu");
  expect(p.transport).toEqual({ kind: "usb", vid: 0x1a86, pid: 0x5512, serial: null });
  expect(p.k40.board).toBe("m2");
});
