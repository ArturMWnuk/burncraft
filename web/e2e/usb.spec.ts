// M3.1 — kreator „Znajdź mój laser” z urządzeniami USB (REQ-13-193) i przewodnikiem
// sterownika (REQ-13-020, REQ-13-021). Host uruchomiony z BURNCRAFT_FAKE_USB=1: urządzenia
// fikcyjne — K40 (CH341) bez sterownika i galvo BJJCZ gotowe do pracy.

import { expect, test } from "@playwright/test";
import { bc, resetDocument } from "./helpers";

test.beforeEach(async ({ page }) => {
  await resetDocument(page);
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.lang", value: "pl" });
    await b.loadPrefs();
    b.commandLog = [];
  });
});

test("req_13_193_find_my_laser_shows_usb_lasers", async ({ page }) => {
  await page.getByTestId("laser-find").click();
  const k40 = page.getByTestId("laser-usb-0");
  await expect(k40).toBeVisible({ timeout: 30_000 });
  await expect(k40).toContainText("K40");
  await expect(k40).toContainText("1A86:5512");
  await expect(k40).toHaveAttribute("data-state", "driver_missing");
  await expect(page.getByTestId("laser-usb-1")).toHaveAttribute("data-state", "ready");
  // nieznane urządzenia USB (mysz, klawiatura) nie są pokazywane
  await expect(page.getByTestId("laser-usb-2")).toHaveCount(0);
});

test("req_13_020_driver_guide_for_usb_laser", async ({ page }) => {
  await page.getByTestId("laser-find").click();
  await expect(page.getByTestId("laser-usb-0")).toBeVisible({ timeout: 30_000 });
  const btn = page.getByTestId("laser-usb-0-guide");
  await expect(btn).toHaveAttribute("data-command", "machine.usb_guide");
  await btn.click();
  const guide = page.getByTestId("laser-usb-guide");
  await expect(guide).toBeVisible();
  await expect(guide).toContainText("Zadig");
  await expect(guide).toContainText("WinUSB");
  await expect(guide.locator("li")).not.toHaveCount(0);
  expect(await bc<string[]>(page, (b) => [...b.commandLog])).toContain("machine.usb_guide");
  // gotowe urządzenie nie ma przewodnika
  await expect(page.getByTestId("laser-usb-1-guide")).toHaveCount(0);
  await page.getByTestId("laser-usb-guide-close").click();
  await expect(guide).toHaveCount(0);
});
