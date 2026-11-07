// M1.4c3: parametry kształtów (REQ-03-047), podgląd stanów historii (REQ-03-048),
// obrazy w notatkach (REQ-03-049), kalibracja rzeczywistego rozmiaru (REQ-03-015),
// gesty dotykowe (REQ-03-019).

import { expect, Page, test } from "@playwright/test";
import { bc, objects, resetDocument } from "./helpers";

declare const Buffer: { from(data: string, encoding: "base64"): any };

// 1×1 px PNG
const PNG_1PX = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

async function resetScale(page: Page) {
  await bc(page, async (b) => {
    await b.rpc.execute("prefs.set", { key: "ui.screen_scale", value: 1 });
    await b.loadPrefs();
  });
}

test.beforeEach(async ({ page }) => {
  await resetDocument(page);
});

test("req_03_047_shape_parameters_panel_edits_rect_and_polygon", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 20, w: 40, h: 20 }));
  await expect(page.getByTestId("shape-params")).toBeVisible();
  await expect(page.getByTestId("param-w")).toHaveValue("40");
  await page.getByTestId("param-radius").fill("3");
  await page.getByTestId("param-radius").press("Enter");
  await expect.poll(() => bc(page, (b) => b.scene.objects[0].params?.radius)).toBe(3);
  await bc(page, (b) => b.rpc.execute("shape.polygon", { cx: 100, cy: 100, r: 20, sides: 6 }));
  await expect(page.getByTestId("param-sides")).toHaveValue("6");
  await page.getByTestId("param-sides").fill("8");
  await page.getByTestId("param-sides").press("Enter");
  await expect.poll(() => bc(page, (b) => b.scene.objects[1].params?.sides)).toBe(8);
  // nieprawidłowa wartość: komunikat, kształt bez zmian
  await page.getByTestId("param-sides").fill("2");
  await page.getByTestId("param-sides").press("Enter");
  await expect(page.locator("#status")).toContainText("sides");
  await expect.poll(() => bc(page, (b) => b.scene.objects[1].params?.sides)).toBe(8);
  // ścieżka nie ma parametrów
  await bc(page, (b) => b.rpc.execute("shape.path", { d: "M10 10 L50 10" }));
  await expect(page.getByTestId("shape-params")).toBeHidden();
});

test("req_03_048_history_hover_shows_state_preview", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 10, w: 100, h: 100 }));
  await bc(page, (b) => b.rpc.execute("shape.ellipse", { cx: 250, cy: 250, rx: 60, ry: 60 }));
  await expect(page.getByTestId("history-1")).toBeVisible();
  await page.getByTestId("history-0").hover();
  const prev = page.getByTestId("history-preview");
  await expect(prev).toBeVisible();
  await expect(prev.locator("img")).toHaveAttribute("src", /^data:image\/png;base64,/);
  await expect(prev).toContainText("1"); // liczba obiektów w tym stanie
  await page.getByTestId("history-start").hover();
  await expect(prev).toContainText("0");
  await page.locator("#canvas").hover();
  await expect(prev).toBeHidden();
  // podgląd niczego nie cofa
  expect(await objects(page)).toBe(2);
});

test("req_03_049_notes_images_add_show_remove", async ({ page }) => {
  await page.getByTestId("notes").fill("Sklejka 3 mm");
  await page.getByTestId("notes-image-input").setInputFiles({
    name: "probka.png", mimeType: "image/png", buffer: Buffer.from(PNG_1PX, "base64"),
  });
  await expect(page.getByTestId("note-image-0").locator("img")).toHaveAttribute("src", /^data:image\/png;base64,/);
  await expect.poll(() => bc(page, (b) => b.scene.note_images.length)).toBe(1);
  await expect(page.getByTestId("notes")).toHaveValue(/Sklejka 3 mm[\s\S]*!\[\]\(notes\/.+\.png\)/);
  await page.getByTestId("note-image-0-remove").click();
  await expect.poll(() => bc(page, (b) => b.scene.note_images.length)).toBe(0);
  await expect(page.getByTestId("note-image-0")).toBeHidden();
});

test("req_03_015_screen_calibration_makes_100_percent_physical", async ({ page }) => {
  try {
    await page.getByTestId("open-settings").click();
    const dlg = page.getByTestId("settings-dialog");
    // pasek kalibracji ma 100 mm przy bieżącej skali; użytkownik mierzy go linijką
    await expect(dlg.getByTestId("calibration-bar")).toBeVisible();
    await dlg.getByTestId("calibration-measured").fill("50");
    await dlg.getByTestId("calibration-apply").click();
    await expect.poll(() => bc(page, (b) => b.prefs.screenScale)).toBeCloseTo(2, 6);
    await expect(dlg.getByTestId("calibration-scale")).toContainText("2");
    await dlg.getByTestId("settings-close").click();
    await page.getByTestId("zoom-100").click();
    await expect.poll(() => bc(page, (b) => b.view.scale)).toBeCloseTo((2 * 96) / 25.4, 6);
    await expect.poll(() => bc(page, (b) => Math.round(b.view.zoomPercent))).toBe(100);
    // po przeładowaniu kalibracja zostaje (preferencje hosta)
    await page.reload();
    await expect.poll(() => bc(page, (b) => b.prefs?.screenScale)).toBeCloseTo(2, 6);
  } finally {
    await resetScale(page);
  }
});

test("req_03_055_each_tool_has_its_own_cursor", async ({ page }) => {
  const ids = await page.locator("[data-testid^='tool-']").evaluateAll((els) =>
    els.map((e) => (e as HTMLElement).dataset.testid!.slice(5)));
  expect(ids.length).toBeGreaterThanOrEqual(13);
  const cursors = new Map<string, string>();
  for (const id of ids) {
    await page.getByTestId(`tool-${id}`).click();
    await expect(page.getByTestId(`tool-${id}`)).toHaveAttribute("aria-pressed", "true");
    cursors.set(id, await page.locator("#canvas").evaluate((c) => (c as HTMLElement).style.cursor));
  }
  expect(cursors.get("select")).toBe("default");
  // każde narzędzie inne; własne kursory (SVG) z systemowym zapasowym
  expect(new Set(cursors.values()).size).toBe(ids.length);
  for (const [id, c] of cursors) {
    if (id !== "select") expect(c, id).toMatch(/^url\("data:image\/svg\+xml.*"\) \d+ \d+, (crosshair|default)$/);
  }
});

/** Najjaśniejszy i najciemniejszy piksel kanwy w kwadracie 9×9 px wokół punktu świata. */
async function brightness(page: Page, x: number, y: number) {
  return page.evaluate(([x, y]) => {
    const b = (window as any).burncraft;
    b.paint();
    const c = document.getElementById("canvas") as HTMLCanvasElement;
    const o = document.createElement("canvas");
    o.width = c.width;
    o.height = c.height;
    const g = o.getContext("2d")!;
    g.drawImage(c, 0, 0);
    const dpr = c.width / c.getBoundingClientRect().width;
    const [sx, sy] = b.view.toScreen(x, y);
    const d = g.getImageData(Math.round(sx * dpr) - 4, Math.round(sy * dpr) - 4, 9, 9).data;
    let max = 0;
    let min = 255;
    for (let i = 0; i < d.length; i += 4) {
      const v = (d[i] + d[i + 1] + d[i + 2]) / 3;
      max = Math.max(max, v);
      min = Math.min(min, v);
    }
    return { max, min };
  }, [x, y] as const);
}

test("req_03_017_dark_layer_colors_stay_visible_on_dark_background", async ({ page }) => {
  // warstwa C00 jest czarna (paleta), tło obszaru roboczego w motywie ciemnym też ciemne
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 100, y: 100, w: 200, h: 200, layer: 0 }));
  await bc(page, (b) => b.rpc.execute("selection.clear"));
  await expect.poll(() => objects(page)).toBe(1);
  expect((await brightness(page, 200, 100)).max).toBeGreaterThan(120);
  // w motywie jasnym czarna linia zostaje ciemna
  try {
    await bc(page, async (b) => {
      await b.rpc.execute("prefs.set", { key: "ui.theme", value: "light" });
      await b.loadPrefs();
    });
    expect((await brightness(page, 200, 100)).min).toBeLessThan(80);
  } finally {
    await bc(page, async (b) => {
      await b.rpc.execute("prefs.set", { key: "ui.theme", value: "dark" });
      await b.loadPrefs();
    });
  }
});

/** Zdarzenie wskaźnika dotykowego na kanwie (punkt względem kanwy w px CSS). */
async function touch(page: Page, type: string, id: number, x: number, y: number) {
  await page.evaluate(([type, id, x, y]) => {
    const c = document.getElementById("canvas")!;
    const r = c.getBoundingClientRect();
    c.dispatchEvent(new PointerEvent(type as string, {
      bubbles: true, cancelable: true, pointerId: id as number, pointerType: "touch", isPrimary: id === 1,
      clientX: r.left + (x as number), clientY: r.top + (y as number), button: type === "pointermove" ? -1 : 0,
      buttons: type === "pointerup" ? 0 : 1,
    }));
  }, [type, id, x, y] as const);
}

test("req_03_019_touch_pinch_zooms_and_two_finger_pans", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 150, y: 150, w: 100, h: 100 }));
  await bc(page, (b) => b.rpc.execute("selection.clear"));
  const v0 = await bc<any>(page, (b) => ({ s: b.view.scale, w: b.view.toWorld(300, 250) }));
  // dwa palce 100 px od siebie, rozsunięte do 200 px — dwukrotne powiększenie wokół środka
  await touch(page, "pointerdown", 1, 250, 250);
  await touch(page, "pointerdown", 2, 350, 250);
  for (let i = 1; i <= 5; i++) {
    await touch(page, "pointermove", 1, 250 - 10 * i, 250);
    await touch(page, "pointermove", 2, 350 + 10 * i, 250);
  }
  const v1 = await bc<any>(page, (b) => ({ s: b.view.scale, w: b.view.toWorld(300, 250) }));
  expect(v1.s / v0.s).toBeCloseTo(2, 2);
  expect(v1.w[0]).toBeCloseTo(v0.w[0], 3);
  expect(v1.w[1]).toBeCloseTo(v0.w[1], 3);
  // przesunięcie obu palców o 40 px w dół przesuwa widok
  await touch(page, "pointermove", 1, 200, 290);
  await touch(page, "pointermove", 2, 400, 290);
  const v2 = await bc<any>(page, (b) => ({ s: b.view.scale, w: b.view.toWorld(300, 290) }));
  expect(v2.w[0]).toBeCloseTo(v0.w[0], 3);
  expect(v2.w[1]).toBeCloseTo(v0.w[1], 3);
  await touch(page, "pointerup", 2, 400, 290);
  await touch(page, "pointerup", 1, 200, 290);
  // gest nie zaznacza ani nie przesuwa obiektów
  expect(await bc(page, (b) => b.scene.selection.length)).toBe(0);
  expect(await bc(page, (b) => b.scene.objects[0].bbox)).toEqual([150, 150, 250, 250]);
  // jeden palec na pustym miejscu działa jak zwykłe zaznaczanie ramką
  const [ax, ay] = await bc<[number, number]>(page, (b) => b.view.toScreen(140, 260));
  const [bx, by] = await bc<[number, number]>(page, (b) => b.view.toScreen(260, 140));
  await touch(page, "pointerdown", 3, ax, ay);
  await touch(page, "pointermove", 3, (ax + bx) / 2, (ay + by) / 2);
  await touch(page, "pointermove", 3, bx, by);
  await touch(page, "pointerup", 3, bx, by);
  await expect.poll(() => bc(page, (b) => b.scene.selection.length)).toBe(1);
});
