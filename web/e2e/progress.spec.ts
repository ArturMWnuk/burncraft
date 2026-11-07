// Długie operacje (> 100 ms) bez blokowania UI: pasek postępu i anulowanie (REQ-16-006).

import { expect, test } from "@playwright/test";
import { bc, objects, resetDocument } from "./helpers";

test.beforeEach(async ({ page }) => {
  await resetDocument(page);
});

test("req_16_006_long_operation_shows_progress_and_can_be_cancelled", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.polygon", { cx: 1, cy: 1, r: 0.4, sides: 64 }));
  await expect.poll(() => objects(page)).toBe(1);
  // tablica 300 × 300 kopii — kilkaset ms w hoście; UI nie czeka na wynik
  await bc(page, (b) => {
    (window as any).__grid = b.run("array.grid", { columns: 300, rows: 300, dx: 1, dy: 1 });
  });
  const bar = page.getByTestId("progress");
  await expect(bar).toBeVisible({ timeout: 20_000 });
  await expect(bar).toHaveAttribute("data-command", "array.grid");
  const cancel = page.getByTestId("progress-cancel");
  await expect(cancel).toHaveAttribute("data-command", "job.cancel");
  await cancel.click();
  await bc(page, () => (window as any).__grid);
  await expect(bar).toBeHidden();
  // anulowane polecenie nie zostawia zmian, a pasek stanu mówi dlaczego
  await expect.poll(() => objects(page)).toBe(1);
  await expect(page.getByTestId("status-error")).toContainText(/anulowano|cancelled/i);
  expect(await bc(page, (b) => b.commandLog.includes("job.cancel"))).toBe(true);
});

test("req_16_006_progress_reaches_the_end_without_cancel", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.polygon", { cx: 1, cy: 1, r: 0.4, sides: 64 }));
  const r = await bc<{ seen: number[]; created: number }>(page, async (b) => {
    const seen: number[] = [];
    b.rpc.on("gui.progress", (p: any) => seen.push(p.fraction));
    // test dotyczy postępu, nie rysowania: bez pobierania sceny 90 tys. wielokątów
    const refresh = b.refresh;
    b.refresh = () => Promise.resolve();
    const res = await b.rpc.execute("array.grid", { columns: 300, rows: 300, dx: 1, dy: 1 });
    await b.rpc.execute("edit.undo");
    b.refresh = refresh;
    await b.refresh();
    return { seen, created: res.ids.length };
  });
  expect(r.created).toBe(89_999);
  expect(r.seen.length, "co najmniej jedno powiadomienie o postępie").toBeGreaterThan(0);
  for (let i = 1; i < r.seen.length; i++) expect(r.seen[i]).toBeGreaterThanOrEqual(r.seen[i - 1]);
  expect(Math.max(...r.seen)).toBeLessThanOrEqual(1);
  await expect(page.getByTestId("progress")).toBeHidden();
});
