// Testy E2E GUI (REQ-19-013): frontend z `burncraft-cli serve` w Chromium.
// Stan żyje w hoście, więc każdy test zaczyna od czystego dokumentu.

import { expect, Page, test } from "@playwright/test";

type Bc = any;

async function bc<T>(page: Page, f: (b: Bc) => T | Promise<T>): Promise<T> {
  return page.evaluate(`(${f.toString()})(window.burncraft)`) as Promise<T>;
}

/** Drugi klient kanału (jak agent podłączony przez `mcp --connect`). */
async function agent(page: Page, method: string, params: unknown): Promise<any> {
  return page.evaluate(
    async ([method, params]) => {
      const ws = new WebSocket(`ws://${location.host}/rpc?token=${(window as any).BURNCRAFT_TOKEN}`);
      await new Promise((ok, fail) => {
        ws.onopen = ok;
        ws.onerror = fail;
      });
      const reply = await new Promise<any>((ok) => {
        ws.onmessage = (ev) => {
          const m = JSON.parse(ev.data);
          if (m.id === 1) ok(m);
        };
        ws.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }));
      });
      ws.close();
      return reply;
    },
    [method, params] as const,
  );
}

const objects = (page: Page) => bc(page, (b) => b.scene?.objects.length ?? -1);
const bbox = (page: Page, id: number) => bc(page, `(b) => b.scene.objects.find(o => o.id === ${id})?.bbox` as any) as Promise<number[]>;

/** Punkt świata (mm) → współrzędne strony. */
async function screenOf(page: Page, x: number, y: number): Promise<[number, number]> {
  return page.evaluate(
    ([x, y]) => {
      const b = (window as any).burncraft;
      const r = document.getElementById("canvas")!.getBoundingClientRect();
      const [sx, sy] = b.view.toScreen(x, y);
      return [r.left + sx, r.top + sy] as [number, number];
    },
    [x, y] as const,
  );
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#status")).toHaveAttribute("data-connected", "true");
  await expect.poll(() => bc(page, (b) => b.scene !== null)).toBe(true);
  await bc(page, async (b) => {
    while (b.scene.docs.length > 1) {
      await b.rpc.execute("doc.close", { index: b.scene.docs.length - 1, force: true });
      await b.refresh();
    }
    await b.rpc.execute("document.new");
    await b.refresh();
    b.zoomPage();
  });
  await expect.poll(() => objects(page)).toBe(0);
});

test("req_01_022_and_req_03_001_002_003_017_and_req_19_013_webgl2_workspace", async ({ page }) => {
  const info = await page.evaluate(() => {
    const c = document.getElementById("canvas") as HTMLCanvasElement;
    const gl = c.getContext("webgl2");
    const top = document.getElementById("ruler-top") as HTMLCanvasElement;
    const left = document.getElementById("ruler-left") as HTMLCanvasElement;
    return { webgl2: gl !== null && gl instanceof WebGL2RenderingContext, w: c.width, top: top.width * top.height, left: left.width * left.height };
  });
  expect(info.webgl2).toBe(true);
  expect(info.w).toBeGreaterThan(100);
  expect(info.top).toBeGreaterThan(0);
  expect(info.left).toBeGreaterThan(0);
  // okno nie przewija się (panele przewijają się w sobie) — inaczej kliknięcie przesuwa kanwę
  for (let i = 0; i < 12; i++) await bc(page, (b) => b.rpc.execute("shape.rect", { x: 1, y: 1, w: 1, h: 1 }));
  await expect.poll(() => objects(page)).toBe(12);
  expect(await page.evaluate(() => document.scrollingElement!.scrollHeight <= window.innerHeight)).toBe(true);
  // kanwa mieści się w oknie i zmniejsza się razem z nim
  const fits = () => page.evaluate(() => document.getElementById("canvas")!.getBoundingClientRect().bottom <= window.innerHeight);
  expect(await fits()).toBe(true);
  // zmiana rozmiaru kanwy nie przesuwa rysunku względem jej lewego górnego rogu
  const local = () => bc<number[]>(page, (b) => { b.paint(); return b.view.toScreen(100, 300).map((v: number) => Math.round(v)); });
  const before = await local();
  await page.setViewportSize({ width: 1100, height: 650 });
  await expect.poll(fits).toBe(true);
  expect(await local()).toEqual(before);
  await page.setViewportSize({ width: 1280, height: 800 });
  await bc(page, (b) => b.rpc.execute("document.new"));
  // tło poza obszarem maszyny jest ciemne, obszar maszyny jaśniejszy od tła
  const px = await page.evaluate(() => {
    const b = (window as any).burncraft;
    b.paint();
    const c = document.getElementById("canvas") as HTMLCanvasElement;
    const o = document.createElement("canvas");
    o.width = c.width;
    o.height = c.height;
    const g = o.getContext("2d")!;
    g.drawImage(c, 0, 0);
    const dpr = c.width / c.getBoundingClientRect().width;
    const diag: string[] = [`canvas ${c.width}x${c.height} dpr ${dpr} view ${JSON.stringify([b.view.width, b.view.height, b.view.scale, b.view.cx, b.view.cy])}`];
    const at = (x: number, y: number) => {
      const [sx, sy] = b.view.toScreen(x, y);
      const d = g.getImageData(Math.round(sx * dpr), Math.round(sy * dpr), 1, 1).data;
      diag.push(`(${x},${y})→(${Math.round(sx)},${Math.round(sy)}) rgba ${Array.from(d).join(",")}`);
      return (d[0] + d[1] + d[2]) / 3;
    };
    const r = { outside: at(-5, -5), inside: at(3.3, 3.3), center: at(203.3, 196.7) };
    return { ...r, diag: diag.join(" | ") };
  });
  expect(px.outside, px.diag).toBeLessThan(50);
  expect(px.inside, px.diag).toBeGreaterThan(px.outside);
  expect(px.center, px.diag).toBeGreaterThan(px.outside);
});

test("req_01_036_and_req_15_002_agent_changes_appear_live", async ({ page }) => {
  const r = await agent(page, "mcp", {
    jsonrpc: "2.0", id: 7, method: "tools/call",
    params: { name: "execute", arguments: { id: "shape.rect", params: { x: 10, y: 10, w: 40, h: 20, layer: 2 } } },
  });
  expect(r.result.result.isError).toBe(false);
  await expect(page.getByTestId("panel-objects").locator(".row")).toHaveCount(1);
  await expect(page.getByTestId("layer-2")).toBeVisible();
});

test("select_click_drag_move_and_undo_with_keyboard", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 100, y: 100, w: 50, h: 40 }));
  await bc(page, (b) => b.rpc.execute("selection.clear"));
  await expect.poll(() => bc(page, (b) => b.scene.selection.length)).toBe(0);
  const [sx, sy] = await screenOf(page, 125, 120);
  await page.mouse.move(sx, sy);
  await page.mouse.down();
  await page.mouse.move(sx + 40, sy - 20, { steps: 5 });
  await page.mouse.up();
  const scale = await bc(page, (b) => b.view.scale);
  await expect.poll(async () => (await bbox(page, 1))[0]).toBeCloseTo(100 + 40 / scale, 1);
  expect((await bbox(page, 1))[1]).toBeCloseTo(100 + 20 / scale, 1);
  await expect.poll(() => bc(page, (b) => b.scene.selection)).toEqual([1]);
  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await bbox(page, 1))[0]).toBeCloseTo(100, 6);
  // klik w puste miejsce czyści zaznaczenie; ramka zaznacza
  const [ex, ey] = await screenOf(page, 300, 300);
  await page.mouse.click(ex, ey);
  await expect.poll(() => bc(page, (b) => b.scene.selection.length)).toBe(0);
  const [ax, ay] = await screenOf(page, 90, 160);
  const [bx, by] = await screenOf(page, 160, 90);
  await page.mouse.move(ax, ay);
  await page.mouse.down();
  await page.mouse.move(bx, by, { steps: 4 });
  await page.mouse.up();
  await expect.poll(() => bc(page, (b) => b.scene.selection)).toEqual([1]);
  // skróty z rejestru: Delete usuwa, Ctrl+Z przywraca, Ctrl+A zaznacza wszystko
  await page.locator("#canvas").focus();
  await page.keyboard.press("Delete");
  await expect.poll(() => objects(page)).toBe(0);
  await page.keyboard.press("Control+z");
  await expect.poll(() => objects(page)).toBe(1);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+a");
  await expect.poll(() => bc(page, (b) => b.scene.selection)).toEqual([1]);
  // strzałka przesuwa o 1 mm
  await page.keyboard.press("ArrowRight");
  await expect.poll(async () => (await bbox(page, 1))[0]).toBeCloseTo(101, 6);
});

test("req_03_013_014_015_zoom_page_selection_and_actual_size", async ({ page }) => {
  await page.getByTestId("zoom-100").click();
  await expect.poll(() => bc(page, (b) => Math.round(b.view.zoomPercent))).toBe(100);
  await page.getByTestId("zoom-page").click();
  const fit = await bc(page, (b) => ({ s: b.view.scale, w: b.view.width, h: b.view.height }));
  expect(400 * fit.s).toBeLessThanOrEqual(Math.min(fit.w, fit.h));
  expect(400 * fit.s).toBeGreaterThan(Math.min(fit.w, fit.h) * 0.8);
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 10, w: 5, h: 5 }));
  await expect.poll(() => bc(page, (b) => b.scene.selection.length)).toBe(1);
  await page.getByTestId("zoom-selection").click();
  const vis = await bc(page, (b) => b.view.visible());
  expect(vis[0]).toBeLessThanOrEqual(10);
  expect(vis[2]).toBeGreaterThanOrEqual(15);
  expect(vis[2] - vis[0]).toBeLessThan(60);
  // kółko myszy powiększa wokół kursora
  const before = await bc(page, (b) => b.view.scale);
  const [sx, sy] = await screenOf(page, 12, 12);
  await page.mouse.move(sx, sy);
  await page.mouse.wheel(0, -300);
  await expect.poll(() => bc(page, (b) => b.view.scale)).toBeGreaterThan(before);
});

test("req_03_024_026_and_req_03_047_properties_numeric_edit", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 20, w: 40, h: 20 }));
  await expect(page.getByTestId("prop-w")).toHaveValue("40");
  await page.getByTestId("prop-w").fill("80");
  await page.getByTestId("prop-w").press("Enter");
  await expect.poll(() => bbox(page, 1)).toEqual([10, 20, 90, 60]);
  await page.getByTestId("prop-lock").uncheck();
  await page.getByTestId("prop-h").fill("10");
  await page.getByTestId("prop-h").press("Enter");
  await expect.poll(() => bbox(page, 1)).toEqual([10, 20, 90, 30]);
  await page.getByTestId("prop-x").fill("0");
  await page.getByTestId("prop-x").press("Enter");
  await expect.poll(async () => (await bbox(page, 1))[0]).toBeCloseTo(0, 6);
  await page.getByTestId("prop-name").fill("Podstawa");
  await page.getByTestId("prop-name").press("Enter");
  await expect(page.getByTestId("object-1")).toContainText("Podstawa");
});

test("req_03_032_layers_panel_edits_layer_settings", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 10, w: 10, h: 10, layer: 0 }));
  await expect(page.getByTestId("layer-0")).toBeVisible();
  await page.getByTestId("layer-0-speed").fill("250");
  await page.getByTestId("layer-0-speed").press("Enter");
  await expect.poll(() => bc(page, (b) => b.scene.layers.find((l: any) => l.index === 0).speed)).toBe(250);
  await page.getByTestId("layer-0-mode").selectOption("fill");
  await expect.poll(() => bc(page, (b) => b.scene.layers.find((l: any) => l.index === 0).mode)).toBe("fill");
  await page.getByTestId("layer-0-output").uncheck();
  await expect.poll(() => bc(page, (b) => b.scene.layers.find((l: any) => l.index === 0).output)).toBe(false);
  // paleta przypisuje zaznaczenie do warstwy
  await page.getByTestId("palette-5").click();
  await expect.poll(() => bc(page, (b) => b.scene.objects[0].layer)).toBe(5);
  await expect(page.getByTestId("layer-5")).toBeVisible();
});

test("req_03_048_history_panel_jumps_between_states", async ({ page }) => {
  for (const x of [10, 30, 50]) await bc(page, `(b) => b.rpc.execute("shape.rect", {x: ${x}, y: 10, w: 10, h: 10})` as any);
  await expect(page.getByTestId("history-2")).toBeVisible();
  await page.getByTestId("history-0").click();
  await expect.poll(() => objects(page)).toBe(1);
  await expect(page.getByTestId("history-redo-1")).toBeVisible();
  await page.getByTestId("history-redo-1").click();
  await expect.poll(() => objects(page)).toBe(3);
  await page.getByTestId("history-start").click();
  await expect.poll(() => objects(page)).toBe(0);
});

test("req_03_049_notes_panel_saves_document_notes", async ({ page }) => {
  await page.getByTestId("notes").fill("Sklejka 3 mm, test mocy przed cięciem");
  await expect.poll(() => bc(page, (b) => b.scene.notes)).toBe("Sklejka 3 mm, test mocy przed cięciem");
});

test("objects_panel_selects_hides_and_locks", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 10, y: 10, w: 10, h: 10 }));
  await bc(page, (b) => b.rpc.execute("shape.ellipse", { cx: 50, cy: 50, rx: 5, ry: 5 }));
  await page.getByTestId("object-1").click();
  await expect.poll(() => bc(page, (b) => b.scene.selection)).toEqual([1]);
  await page.getByTestId("object-2").click({ modifiers: ["Shift"] });
  await expect.poll(() => bc(page, (b) => b.scene.selection)).toEqual([1, 2]);
  await page.getByTestId("object-2-visible").click();
  await expect.poll(() => bc(page, (b) => b.scene.objects[1].visible)).toBe(false);
  await page.getByTestId("object-1-locked").click();
  await expect.poll(() => bc(page, (b) => b.scene.objects[0].locked)).toBe(true);
});

test("document_tabs_new_switch_close", async ({ page }) => {
  await page.locator(".tab-new").click();
  await expect(page.getByTestId("tab-1")).toBeVisible();
  await bc(page, (b) => b.rpc.execute("shape.rect", { x: 1, y: 1, w: 1, h: 1 }));
  await expect.poll(() => objects(page)).toBe(1);
  await page.getByTestId("tab-0").locator(".tab-name").click();
  await expect.poll(() => bc(page, (b) => b.scene.active)).toBe(0);
  await expect.poll(() => objects(page)).toBe(0);
});

test("req_15_026_screenshot_tool_returns_window_png", async ({ page }) => {
  await bc(page, (b) => b.rpc.execute("shape.ellipse", { cx: 100, cy: 100, rx: 40, ry: 30 }));
  const r = await agent(page, "mcp", {
    jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "screenshot", arguments: {} },
  });
  const res = r.result.result;
  expect(res.isError).toBe(false);
  expect(res.content[0].type).toBe("image");
  const head = await page.evaluate((d) => Array.from(atob(d).slice(0, 4)).map((c) => c.charCodeAt(0)), res.content[0].data);
  expect(head).toEqual([0x89, 0x50, 0x4e, 0x47]);
});
