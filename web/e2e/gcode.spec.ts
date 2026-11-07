// Zapis pliku G-code z paska narzędzi (REQ-10-019, REQ-01-029: przycisk to polecenie rejestru
// gcode.export) i import G-code przez „Importuj…” (REQ-02-089: tor jako ścieżki na kanwie).

import { expect, test } from "@playwright/test";
import { bc, objects, resetDocument } from "./helpers";

test.beforeEach(async ({ page }) => {
  await resetDocument(page);
});

test("req_10_019_toolbar_save_gcode_file", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 10, w: 30, h: 20, layer: 0 }));
  await expect.poll(() => objects(page)).toBe(1);
  const btn = page.getByTestId("tb-gcode.export");
  await expect(btn).toHaveAttribute("data-command", "gcode.export");
  // ścieżka względem katalogu hosta (web/)
  const path = `test-results/e2e-zapis-${Date.now()}.gcode`;
  page.once("dialog", (d) => d.accept(path));
  await btn.click();
  await expect.poll(() => bc(page, (b) => b.commandLog.includes("gcode.export"))).toBe(true);
  // plik istnieje i zawiera tor prostokąta (odczyt przez host)
  const r = await bc<any>(page, `(b) => b.rpc.execute("gcode.import", { path: "${path}", to_document: false })`);
  expect(Math.abs(r.cut_length - 100)).toBeLessThan(1e-3);
  expect(r.dynamic_power).toBe(true);
});

test("req_02_089_import_gcode_from_toolbar_shows_paths", async ({ page }) => {
  const path = `test-results/e2e-import-${Date.now()}.gcode`;
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 5, y: 5, w: 20, h: 10, layer: 0 }));
  await bc<any>(page, `(b) => b.rpc.execute("gcode.export", { path: "${path}" })`);
  await bc(page, async (b) => {
    await b.rpc.execute("document.new");
    await b.refresh();
  });
  await expect.poll(() => objects(page)).toBe(0);
  const btn = page.getByTestId("tb-file.import");
  await expect(btn).toHaveAttribute("data-command", "file.import");
  page.once("dialog", (d) => d.accept(path));
  await btn.click();
  await expect.poll(() => objects(page)).toBe(1);
  const prog = await bc<any>(page, (b) => b.rpc.execute("gcode.program", {}));
  expect(prog.loaded).toBe(true);
});
