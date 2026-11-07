// Wspólne pomocniki testów E2E.

import { expect, Page } from "@playwright/test";

export async function bc<T>(page: Page, f: ((b: any) => T | Promise<T>) | string): Promise<T> {
  return page.evaluate(`(${f.toString()})(window.burncraft)`) as Promise<T>;
}

export const objects = (page: Page) => bc<number>(page, (b) => b.scene?.objects.length ?? -1);

export const obj = (page: Page, i: number) => bc<any>(page, `(b) => b.scene.objects[${i}]`);

/** Punkt świata (mm) → współrzędne strony. */
export async function screenOf(page: Page, x: number, y: number): Promise<[number, number]> {
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

/** Przeciągnięcie myszą między punktami świata (mm). */
export async function dragWorld(page: Page, a: [number, number], b: [number, number], mods: string[] = []) {
  const [ax, ay] = await screenOf(page, a[0], a[1]);
  const [bx, by] = await screenOf(page, b[0], b[1]);
  for (const m of mods) await page.keyboard.down(m);
  await page.mouse.move(ax, ay);
  await page.mouse.down();
  await page.mouse.move(bx, by, { steps: 6 });
  await page.mouse.up();
  for (const m of mods) await page.keyboard.up(m);
}

export async function clickWorld(page: Page, x: number, y: number) {
  const [sx, sy] = await screenOf(page, x, y);
  await page.mouse.click(sx, sy);
}

/** Pary [x, y] z płaskiej tablicy, zaokrąglone do 1 µm i posortowane. */
export function pairs(flat: number[]): [number, number][] {
  const out: [number, number][] = [];
  for (let i = 0; i + 1 < flat.length; i += 2) out.push([Math.round(flat[i] * 1000) / 1000, Math.round(flat[i + 1] * 1000) / 1000]);
  return out.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

export async function resetDocument(page: Page) {
  await page.goto("/");
  await expect(page.locator("#status")).toHaveAttribute("data-connected", "true");
  // pierwsza scena dużego dokumentu (host w kompilacji debug) może iść kilka sekund
  await expect.poll(() => bc(page, (b) => b.scene !== null), { timeout: 30_000 }).toBe(true);
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
}
