// Zrzuty ekranu do procedur ręcznych M3 (docs/procedures/M3-gui-checks.md): kreator USB
// i przewodnik sterownika. Uruchamiane tylko na żądanie: BC_SCREENSHOTS=1 npx playwright test procedures-m3
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
