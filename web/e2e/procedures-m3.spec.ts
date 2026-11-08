// Zrzuty ekranu do procedur ręcznych M3 (docs/procedures/M3-gui-checks.md): kreator USB,
// przewodnik sterownika, K40 bez PWM (pole mocy nieaktywne, REQ-13-045) i płyta K40. Uruchamiane tylko na żądanie: BC_SCREENSHOTS=1 npx playwright test procedures-m3
// Urządzenia: fikcyjne USB (BURNCRAFT_FAKE_USB=1 w konfiguracji Playwright).

import { expect, Page, test } from "@playwright/test";
import { bc, resetDocument } from "./helpers";

declare const process: { env: Record<string, string | undefined> };

const OUT = "../docs/procedures/img/m3";

test.skip(!process.env.BC_SCREENSHOTS, "zrzuty do procedur: BC_SCREENSHOTS=1");
test.setTimeout(120_000);

async function lang(page: Page, l: string) {
  await bc(page, `async (b) => { await b.rpc.execute("prefs.set", { key: "ui.lang", value: "${l}" }); await b.loadPrefs(); }`);
}

test.beforeEach(async ({ page }) => {
  await resetDocument(page);
  await lang(page, "pl");
});

test.afterEach(async ({ page }) => {
  await lang(page, "pl");
});

test("screens_m3_usb_find_and_driver_guide", async ({ page }) => {
  await page.getByTestId("laser-find").click();
  await expect(page.getByTestId("laser-usb-0")).toBeVisible({ timeout: 30_000 });
  await page.waitForTimeout(200);
  const panel = page.getByTestId("panel-laser");
  await panel.screenshot({ path: `${OUT}/usb-found.png` });
  await page.getByTestId("laser-usb-0-guide").click();
  await expect(page.getByTestId("laser-usb-guide")).toBeVisible();
  await page.waitForTimeout(200);
  await page.locator(".dialog.usb-guide").screenshot({ path: `${OUT}/usb-guide-winusb.png` });
  await page.getByTestId("laser-usb-guide-close").click();
  // ten sam przewodnik po angielsku
  await lang(page, "en");
  await page.getByTestId("laser-find").click();
  await expect(page.getByTestId("laser-usb-0")).toBeVisible({ timeout: 30_000 });
  await page.getByTestId("laser-usb-0-guide").click();
  await expect(page.getByTestId("laser-usb-guide")).toBeVisible();
  await page.waitForTimeout(200);
  await page.locator(".dialog.usb-guide").screenshot({ path: `${OUT}/usb-guide-winusb-en.png` });
  await page.getByTestId("laser-usb-guide-close").click();
});

async function k40Cleanup(page: Page) {
  await bc(page, async (b) => {
    const l = await b.rpc.execute("machine.list", {});
    for (const n of l.connected) await b.rpc.execute("machine.disconnect", { device: n });
    await b.refresh();
  });
}

test("screens_m3_k40_no_pwm_power_field", async ({ page }) => {
  await k40Cleanup(page);
  await bc(page, async (b) => {
    await b.rpc.execute("shape.rect", { x: 20, y: 20, w: 40, h: 25, layer: 0 });
    await b.rpc.execute("layer.set", { index: 0, speed: 100, power_max: 40, air_assist: false });
    await b.refresh();
  });
  for (const l of ["pl", "en"]) {
    await lang(page, l);
    await page.getByTestId("laser-device").selectOption("Symulator K40 (Lihuiyu)");
    await page.getByTestId("laser-connect").click();
    await expect(page.getByTestId("laser-state")).toHaveAttribute("data-state", "idle", { timeout: 15_000 });
    await expect(page.getByTestId("layers-no-pwm")).toBeVisible();
    await expect(page.getByTestId("layer-0-power")).toBeDisabled();
    await page.waitForTimeout(200);
    const sfx = l === "pl" ? "" : "-en";
    await page.getByTestId("panel-layers").screenshot({ path: `${OUT}/k40-layers-no-pwm${sfx}.png` });
    await page.getByTestId("panel-laser").screenshot({ path: `${OUT}/k40-laser-board${sfx}.png` });
    await bc(page, (b) => b.exec("layer.settings_open", { index: 0 }));
    await expect(page.getByTestId("ls-no-pwm")).toBeVisible();
    await page.waitForTimeout(200);
    await page.locator(".dialog.layer-settings").screenshot({ path: `${OUT}/k40-layer-settings-no-pwm${sfx}.png` });
    await bc(page, (b) => b.exec("layer.settings_close"));
    await k40Cleanup(page);
  }
  // urządzenie z PWM (GRBL): pole mocy aktywne, bez komunikatu
  await lang(page, "pl");
  await page.getByTestId("laser-device").selectOption("Symulator");
  await page.getByTestId("laser-connect").click();
  await expect(page.getByTestId("layer-0-power")).toBeEnabled();
  await page.waitForTimeout(200);
  await page.getByTestId("panel-layers").screenshot({ path: `${OUT}/k40-layers-grbl-pwm.png` });
  await k40Cleanup(page);
});