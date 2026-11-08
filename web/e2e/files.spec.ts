// M3.4 — okno „Lista plików na sterowniku” (REQ-03-042) na emulatorze FluidNC jako
// urządzeniu (profil z transportem emulator, odmiana fluidnc — karta SD i XMODEM):
// lista, wysyłka projektu i pliku z dysku, uruchomienie, usunięcie z potwierdzeniem.
// Każda kontrolka to polecenie rejestru (`data-command`, REQ-01-029). K40 bez pamięci
// plików: komunikat zamiast listy.

import { expect, Page, test } from "@playwright/test";
import { bc, objects, resetDocument } from "./helpers";

declare const Buffer: { from(data: string): any };

test.setTimeout(60_000);

const FLUID = "FluidNC (emulator)";

async function cleanup(page: Page) {
  await bc(page, async (b) => {
    if (b.files?.state?.open) b.exec("files.close");
    const l = await b.rpc.execute("machine.list", {});
    for (const n of l.connected) await b.rpc.execute("machine.disconnect", { device: n });
    for (const p of l.profiles) if (p.name === "FluidNC (emulator)") await b.rpc.execute("machine.profile_delete", { name: p.name });
    await b.machine.reload();
  });
}

async function connect(page: Page, name: string) {
  await page.getByTestId("laser-device").selectOption(name);
  await page.getByTestId("laser-connect").click();
  await expect(page.getByTestId("laser-state")).toHaveAttribute("data-state", "idle", { timeout: 15_000 });
}

async function fluid(page: Page) {
  await bc(page, async (b) => {
    await b.rpc.execute("machine.profile_save", {
      profile: { name: "FluidNC (emulator)", driver: "grbl", transport: { kind: "emulator", variant: "fluidnc" }, workspace: [400, 300] },
    });
    await b.rpc.execute("shape.rect", { x: 20, y: 20, w: 30, h: 20, layer: 0 });
    await b.rpc.execute("layer.set", { index: 0, speed: 100, power_max: 40, air_assist: false });
    await b.refresh();
    await b.machine.reload();
  });
  await expect.poll(() => objects(page)).toBe(1);
  await connect(page, FLUID);
}

const rows = (page: Page) => page.locator("[data-testid^='files-row-']");

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

test("req_03_042_driver_files_window_list_upload_run_delete", async ({ page }) => {
  await fluid(page);
  const open = page.getByTestId("laser-files");
  await expect(open).toHaveAttribute("data-command", "files.open");
  await open.click();
  const dlg = page.getByTestId("files-dialog");
  await expect(dlg).toBeVisible();
  await expect(page.getByTestId("files-device")).toContainText(FLUID);
  await expect(page.getByTestId("files-empty")).toBeVisible();
  expect(await bc<string[]>(page, (b) => [...b.commandLog])).toContain("machine.files");
  // wysłanie projektu (kompilacja G-code dla urządzenia, XMODEM)
  const name = page.getByTestId("files-name");
  await expect(name).toHaveAttribute("data-command", "device.sd_upload");
  await name.fill("/sd/prostokat.gcode");
  const up = page.getByTestId("files-upload-project");
  await expect(up).toHaveAttribute("data-command", "device.sd_upload");
  await up.click();
  await expect(rows(page)).toHaveCount(1);
  await expect(page.getByTestId("files-name-0")).toHaveText("/sd/prostokat.gcode");
  await expect(page.getByTestId("files-size-0")).toContainText("B");
  await expect(page.getByTestId("files-status")).toContainText("/sd/prostokat.gcode");
  // wysłanie pliku z dysku: nazwa = katalog z pola + nazwa pliku
  const text = "G21 G90\nG0 X5 Y5\nG1 X25 F600 S300 M3\nM5\n";
  await expect(page.getByTestId("files-upload-file")).toHaveAttribute("data-command", "device.sd_upload");
  await page.getByTestId("files-file-input").setInputFiles({ name: "reczny.gcode", mimeType: "text/plain", buffer: Buffer.from(text) });
  await expect(rows(page)).toHaveCount(2);
  await expect(page.getByTestId("files-name-1")).toHaveText("/sd/reczny.gcode");
  await expect(page.getByTestId("files-size-1")).toHaveText(`${text.length} B`);
  await expect(page.getByTestId("files-total")).toContainText("2");
  const sd = await bc<any>(page, (b) => b.rpc.execute("machine.files", {}));
  expect(sd.files.map((f: any) => f.name)).toEqual(["/sd/prostokat.gcode", "/sd/reczny.gcode"]);
  expect(sd.files[1].size).toBe(text.length);
  // uruchomienie pliku z karty: głowica kończy w X 25
  const runBtn = page.getByTestId("files-run-1");
  await expect(runBtn).toHaveAttribute("data-command", "device.sd_run");
  await runBtn.click();
  await expect
    .poll(async () => bc<number>(page, (b) => (b.machine.status?.state?.state === "idle" ? b.machine.status.state.wpos[0] : -1)), { timeout: 15_000 })
    .toBe(25);
  // usunięcie z potwierdzeniem; „Nie” zostawia plik
  const del = page.getByTestId("files-delete-1");
  await expect(del).toHaveAttribute("data-command", "device.sd_delete");
  await del.click();
  await expect(page.getByTestId("files-delete-confirm-1")).toHaveAttribute("data-command", "device.sd_delete");
  await page.getByTestId("files-delete-cancel-1").click();
  await expect(rows(page)).toHaveCount(2);
  await page.getByTestId("files-delete-1").click();
  await page.getByTestId("files-delete-confirm-1").click();
  await expect(rows(page)).toHaveCount(1);
  await expect(page.getByTestId("files-name-0")).toHaveText("/sd/prostokat.gcode");
  // odświeżenie listy
  await expect(page.getByTestId("files-refresh")).toHaveAttribute("data-command", "machine.files");
  await page.getByTestId("files-refresh").click();
  await expect(rows(page)).toHaveCount(1);
  // błąd sterownika w oknie (zła nazwa nie trafia do sterownika)
  await name.fill("");
  await up.click();
  await expect(page.getByTestId("files-error")).not.toBeEmpty();
  // każda kontrolka okna to polecenie rejestru
  const missing = await page.locator(".dialog.driver-files button, .dialog.driver-files input").evaluateAll((els) =>
    els.filter((e) => !e.getAttribute("data-command")).map((e) => e.getAttribute("data-testid") ?? e.outerHTML));
  expect(missing).toEqual([]);
  const log = await bc<string[]>(page, (b) => [...b.commandLog]);
  for (const id of ["files.open", "machine.files", "device.sd_upload", "device.sd_run", "device.sd_delete"]) expect(log).toContain(id);
  // zamknięcie: przycisk i Esc
  await page.getByTestId("files-close").click();
  await expect(dlg).toHaveCount(0);
  await bc(page, (b) => b.exec("files.open"));
  await expect(dlg).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
});

test("req_03_042_driver_files_window_unsupported_and_english", async ({ page }) => {
  // bez urządzenia
  await page.getByTestId("laser-files").click();
  await expect(page.getByTestId("files-no-device")).toBeVisible();
  await page.getByTestId("files-close").click();
  // K40 (M2 Nano): brak pamięci plików — komunikat, bez listy i przycisków wysyłki
  await connect(page, "Symulator K40 (Lihuiyu)");
  await page.getByTestId("laser-files").click();
  await expect(page.getByTestId("files-unsupported")).toBeVisible();
  await expect(page.getByTestId("files-unsupported")).toContainText("K40");
  await expect(page.getByTestId("files-upload-project")).toHaveCount(0);
  await expect(rows(page)).toHaveCount(0);
  await page.getByTestId("files-close").click();
  // okno po angielsku
  await page.getByTestId("laser-disconnect").click();
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "en" });
    await b.loadPrefs();
  });
  await fluid(page);
  await expect(page.getByTestId("laser-files")).toHaveText("Controller files");
  await page.getByTestId("laser-files").click();
  await expect(page.getByTestId("files-empty")).toContainText("No files");
  await expect(page.getByTestId("files-upload-project")).toHaveText("Send project");
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "pl" });
    await b.loadPrefs();
  });
  await expect(page.getByTestId("files-upload-project")).toHaveText("Wyślij projekt");
});
