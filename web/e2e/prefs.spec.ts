// M1.4b3a: edytor skrótów (REQ-01-039), drzewo obiektów (REQ-02-040…044),
// tłumaczenia (REQ-03-059, 061), motywy (REQ-03-016), mm/in (REQ-03-027, 028),
// 9-punktowy punkt odniesienia (REQ-03-025). Preferencje w PrefStore hosta.

import { expect, Page, test } from "@playwright/test";
import { bc, obj, objects, resetDocument } from "./helpers";

async function resetPrefs(page: Page) {
  await bc(page, async (b) => {
    await b.rpc.execute("shortcuts.reset");
    for (const [key, value] of [["ui.lang", "pl"], ["ui.theme", "dark"], ["ui.units", "mm"]]) {
      await b.rpc.execute("prefs.set", { key, value });
    }
    await b.loadPrefs();
  });
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
  await resetPrefs(page);
});

test.afterEach(async ({ page }) => {
  await resetPrefs(page);
});

const rect = (page: Page, x: number, y: number, w: number, h: number) =>
  bc(page, `(b) => b.rpc.execute("shape.rect", {x: ${x}, y: ${y}, w: ${w}, h: ${h}})`);

test("req_01_039_shortcut_editor_assigns_conflicts_and_persists", async ({ page }) => {
  await rect(page, 10, 10, 10, 10);
  await rect(page, 30, 10, 10, 10);
  await expect.poll(() => objects(page)).toBe(2);
  await page.getByTestId("open-shortcuts").click();
  const dlg = page.getByTestId("shortcuts-dialog");
  await expect(dlg).toBeVisible();
  // wyszukiwanie po tytule i id
  await dlg.getByTestId("shortcuts-search").fill("zaznacz wszystko");
  await expect(dlg.locator("tbody tr")).toHaveCount(1);
  await expect(dlg.getByTestId("shortcut-selection.all")).toHaveText("Ctrl+A");
  // przechwycenie nowego skrótu
  await dlg.getByTestId("shortcut-edit-selection.all").click();
  await page.keyboard.press("F2");
  await expect(dlg.getByTestId("shortcut-selection.all")).toHaveText("F2");
  // konflikt z Ctrl+Z (edit.undo) — komunikat i przeniesienie skrótu
  await dlg.getByTestId("shortcut-edit-selection.all").click();
  await page.keyboard.press("Control+z");
  await expect(dlg.getByTestId("shortcut-conflict")).toContainText("edit.undo");
  await dlg.getByTestId("shortcut-cancel-conflict").click();
  await expect(dlg.getByTestId("shortcut-selection.all")).toHaveText("F2");
  await dlg.getByTestId("shortcuts-close").click();
  // skrót działa po przeładowaniu (zapis w preferencjach hosta)
  await page.reload();
  await expect(page.locator("#status")).toHaveAttribute("data-connected", "true");
  await expect.poll(() => bc(page, (b) => b.scene !== null)).toBe(true); // połączenie przychodzi przed sceną
  await bc(page, (b) => b.rpc.execute("selection.clear"));
  await expect.poll(() => bc(page, (b) => b.scene.selection.length)).toBe(0);
  await page.locator("#canvas").focus();
  await page.keyboard.press("F2");
  await expect.poll(() => bc(page, (b) => b.scene.selection.length)).toBe(2);
  // stary skrót nie działa
  await bc(page, (b) => b.rpc.execute("selection.clear"));
  await page.keyboard.press("Control+a");
  await page.waitForTimeout(300);
  expect(await bc(page, (b) => b.scene.selection.length)).toBe(0);
  // przywrócenie domyślnych
  await page.getByTestId("open-shortcuts").click();
  await page.getByTestId("shortcuts-reset-all").click();
  await expect(page.getByTestId("shortcut-selection.all")).toHaveText("Ctrl+A");
});

test("req_02_040_042_044_object_tree_with_search_hide_and_lock", async ({ page }) => {
  await rect(page, 10, 10, 10, 10);
  await bc(page, (b) => b.rpc.execute("shape.ellipse", { cx: 50, cy: 50, rx: 5, ry: 5 }));
  await expect.poll(() => objects(page)).toBe(2);
  await bc(page, (b) => b.rpc.execute("object.group", { ids: [1, 2] }));
  await expect.poll(() => objects(page)).toBe(1);
  const panel = page.getByTestId("panel-objects");
  // grupa z dwojgiem dzieci, zwijana
  await expect(panel.getByTestId("object-3")).toBeVisible();
  await expect(panel.getByTestId("object-1")).toBeVisible();
  await panel.getByTestId("object-3-toggle").click();
  await expect(panel.getByTestId("object-1")).toBeHidden();
  await panel.getByTestId("object-3-toggle").click();
  await expect(panel.getByTestId("object-2")).toBeVisible();
  // wyszukiwanie: pokazuje trafienie z rodzicami
  await panel.getByTestId("objects-search").fill("elipsa");
  await expect(panel.getByTestId("object-2")).toBeVisible();
  await expect(panel.getByTestId("object-1")).toBeHidden();
  await expect(panel.getByTestId("object-3")).toBeVisible();
  await panel.getByTestId("objects-search").fill("");
  // ukrywanie i blokowanie dziecka w grupie
  await panel.getByTestId("object-1-visible").click();
  await expect.poll(() => bc(page, (b) => b.scene.tree[0].children[0].visible)).toBe(false);
  await panel.getByTestId("object-2-locked").click();
  await expect.poll(() => bc(page, (b) => b.scene.tree[0].children[1].locked)).toBe(true);
});

test("req_02_041_043_object_tree_drag_to_reparent_and_reorder", async ({ page }) => {
  await rect(page, 10, 10, 10, 10); // 1
  await rect(page, 30, 10, 10, 10); // 2
  await bc(page, (b) => b.rpc.execute("object.group", { ids: [1, 2] })); // 3
  await rect(page, 50, 10, 10, 10); // 4
  await rect(page, 70, 10, 10, 10); // 5
  await expect.poll(() => objects(page)).toBe(3);
  const panel = page.getByTestId("panel-objects");
  // przeciągnięcie 4 na grupę → dziecko grupy
  await panel.getByTestId("object-4").dragTo(panel.getByTestId("object-3"));
  await expect.poll(() => bc(page, (b) => b.scene.tree.map((n: any) => n.id))).toEqual([3, 5]);
  expect(await bc(page, (b) => b.scene.tree[0].children.map((n: any) => n.id))).toContain(4);
  expect((await obj(page, 0)).bbox[2]).toBeCloseTo(60, 6); // położenie bez zmian
  // przeciągnięcie 5 na dziecko 1 → nad nim w kolejności rysowania grupy
  await panel.getByTestId("object-5").dragTo(panel.getByTestId("object-1"));
  await expect.poll(() => bc(page, (b) => b.scene.tree[0].children.map((n: any) => n.id))).toEqual([1, 5, 2, 4]);
  // przyciski kolejności rysowania
  await panel.getByTestId("object-1").click();
  await panel.getByTestId("order-top").click();
  await expect.poll(() => bc(page, (b) => b.scene.tree[0].children.map((n: any) => n.id))).toEqual([5, 2, 4, 1]);
});

test("req_03_059_061_english_translation", async ({ page }) => {
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-lang").selectOption("en");
  await expect(page.getByTestId("panel-layers").locator(".panel-head")).toHaveText("Layers");
  await expect(page.getByTestId("tool-rect")).toHaveAttribute("title", /Rectangle/);
  await expect(page.locator('[data-command="edit.undo"]')).toContainText("Undo");
  // po przeładowaniu język zostaje (preferencje hosta)
  await page.reload();
  await expect(page.getByTestId("panel-layers").locator(".panel-head")).toHaveText("Layers");
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-lang").selectOption("pl");
  await expect(page.getByTestId("panel-layers").locator(".panel-head")).toHaveText("Warstwy");
});

test("req_03_016_light_dark_and_system_theme", async ({ page }) => {
  const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const canvasBg = () => bc<number>(page, (b) => {
    b.paint();
    const c = document.getElementById("canvas") as HTMLCanvasElement;
    const o = document.createElement("canvas");
    o.width = c.width;
    o.height = c.height;
    const g = o.getContext("2d")!;
    g.drawImage(c, 0, 0);
    const d = g.getImageData(2, 2, 1, 1).data;
    return (d[0] + d[1] + d[2]) / 3;
  });
  const dark = await bg();
  expect(await canvasBg()).toBeLessThan(80);
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-theme").selectOption("light");
  await expect.poll(bg).not.toBe(dark);
  await expect.poll(canvasBg).toBeGreaterThan(180);
  await page.emulateMedia({ colorScheme: "dark" });
  await page.getByTestId("pref-theme").selectOption("system");
  await expect.poll(bg).toBe(dark);
  await page.emulateMedia({ colorScheme: "light" });
  await expect.poll(bg).not.toBe(dark);
});

test("req_03_027_028_units_toggle_and_inline_conversion", async ({ page }) => {
  await rect(page, 10, 20, 40, 20);
  await expect(page.getByTestId("prop-w")).toHaveValue("40");
  // wpisanie wartości w calach przy jednostce mm
  await page.getByTestId("prop-w").fill("2in");
  await page.getByTestId("prop-w").press("Enter");
  await expect.poll(async () => (await obj(page, 0)).bbox[2]).toBeCloseTo(10 + 50.8, 3);
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-units").selectOption("in");
  await page.getByTestId("settings-close").click(); // okno ustawień jest modalne
  await expect(page.getByTestId("prop-w")).toHaveValue("2");
  await expect(page.getByTestId("prop-x")).toHaveValue("0.394");
  // w trybie cali wartość w mm też jest przeliczana
  await page.getByTestId("prop-lock").uncheck();
  await page.getByTestId("prop-h").fill("25.4mm");
  await page.getByTestId("prop-h").press("Enter");
  await expect.poll(async () => { const b = (await obj(page, 0)).bbox; return b[3] - b[1]; }).toBeCloseTo(25.4, 3);
  await expect(page.getByTestId("prop-h")).toHaveValue("1");
  // linijka i pasek stanu w calach
  await expect(page.getByTestId("status-text")).toContainText("%");
});

test("req_02_056_backup_browser_diff_and_restore_with_confirmation", async ({ page }) => {
  // wszystkie akcje okna idą przez polecenia rejestru
  await bc(page, (b) => {
    const orig = b.rpc.execute.bind(b.rpc);
    (window as any).__calls = [];
    b.rpc.execute = (id: string, params: unknown) => {
      (window as any).__calls.push(id);
      return orig(id, params);
    };
  });
  const calls = () => page.evaluate(() => (window as any).__calls as string[]);
  await page.getByTestId("open-settings").click();
  await page.getByTestId("pref-theme").selectOption("light"); // kopia z theme=dark
  await page.getByTestId("open-backups").click();
  const dlg = page.getByTestId("backups-dialog");
  await expect(dlg).toBeVisible();
  await expect(dlg.locator("h2")).toHaveText("Kopie zapasowe preferencji");
  const rows = dlg.getByTestId("backup-row");
  await expect(rows.first()).toBeVisible();
  const count = await rows.count();
  // najnowsza kopia na górze, z datą utworzenia
  await expect(rows.first()).toContainText(String(new Date().getFullYear()));
  expect(await calls()).toContain("prefs.backups");
  // podgląd różnic względem bieżących ustawień
  await rows.first().click();
  const diff = dlg.getByTestId("backup-diff");
  const theme = diff.locator('[data-key="ui.theme"]');
  await expect(theme).toBeVisible();
  await expect(theme.getByTestId("diff-backup")).toHaveText('"dark"');
  await expect(theme.getByTestId("diff-current")).toHaveText('"light"');
  expect(await calls()).toContain("prefs.backup_diff");
  // przywrócenie wymaga potwierdzenia — anulowanie nic nie zmienia
  await dlg.getByTestId("backup-restore").click();
  await expect(dlg.getByTestId("backup-confirm")).toBeVisible();
  await dlg.getByTestId("backup-confirm-no").click();
  await expect(dlg.getByTestId("backup-confirm")).toBeHidden();
  expect(await calls()).not.toContain("prefs.restore");
  expect(await bc(page, (b) => b.prefs.theme)).toBe("light");
  // potwierdzenie przywraca kopię i stosuje ją w interfejsie
  await dlg.getByTestId("backup-restore").click();
  await dlg.getByTestId("backup-confirm-yes").click();
  await expect.poll(() => bc(page, (b) => b.prefs.theme)).toBe("dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  expect(await calls()).toContain("prefs.restore");
  expect(await bc(page, async (b) => (await b.rpc.execute("prefs.get", { key: "ui.theme" })).value)).toBe("dark");
  // przywrócenie też tworzy kopię (można je cofnąć) — lista się odświeża
  await expect(dlg.getByTestId("backup-status")).toBeVisible();
  await expect.poll(() => rows.count()).toBe(Math.min(count + 1, 20));
  await rows.first().click();
  await expect(theme.getByTestId("diff-backup")).toHaveText('"light"');
  await expect(theme.getByTestId("diff-current")).toHaveText('"dark"');
  await dlg.getByTestId("backups-close").click();
  await expect(dlg).toBeHidden();
});

test("req_02_056_backup_browser_english", async ({ page }) => {
  await bc(page, (b) => b.setPref("ui.lang", "en"));
  await page.getByTestId("open-settings").click();
  await page.getByTestId("open-backups").click();
  const dlg = page.getByTestId("backups-dialog");
  await expect(dlg.locator("h2")).toHaveText("Preferences backups");
  await dlg.getByTestId("backup-row").first().click();
  await expect(dlg.getByTestId("backup-restore")).toHaveText("Restore…");
  await dlg.getByTestId("backup-restore").click();
  await expect(dlg.getByTestId("backup-confirm-yes")).toHaveText("Restore");
  await dlg.getByTestId("backup-confirm-no").click();
  await dlg.getByTestId("backups-close").click();
});

test("req_03_025_nine_point_reference", async ({ page }) => {
  await rect(page, 10, 20, 40, 20);
  await expect(page.getByTestId("prop-x")).toHaveValue("10");
  // prawy górny narożnik jako punkt odniesienia
  await page.getByTestId("anchor-tr").click();
  await expect(page.getByTestId("prop-x")).toHaveValue("50");
  await expect(page.getByTestId("prop-y")).toHaveValue("40");
  // zmiana szerokości trzyma prawą krawędź
  await page.getByTestId("prop-lock").uncheck();
  await page.getByTestId("prop-w").fill("20");
  await page.getByTestId("prop-w").press("Enter");
  await expect.poll(async () => (await obj(page, 0)).bbox).toEqual([30, 20, 50, 40]);
  // środek
  await page.getByTestId("anchor-cc").click();
  await expect(page.getByTestId("prop-x")).toHaveValue("40");
  await page.getByTestId("prop-x").fill("100");
  await page.getByTestId("prop-x").press("Enter");
  await expect.poll(async () => (await obj(page, 0)).bbox).toEqual([90, 20, 110, 40]);
});
