// Każda operacja użytkownika jest poleceniem rejestru (REQ-01-029): przyciski,
// skróty, kółko myszy, gesty i pola wywołują polecenia — dokumentu przez host,
// widoku i narzędzi przez okno (te same id, ten sam schemat; agent MCP też je wywoła).

import { expect, Page, test } from "@playwright/test";
import { bc, resetDocument, screenOf } from "./helpers";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await resetDocument(page);
  await bc(page, (b) => (b.commandLog = []));
});

const log = (page: Page) => bc<string[]>(page, (b) => [...b.commandLog]);

/** Drugi klient kanału (jak agent podłączony przez `mcp --connect`). */
async function agentExecute(page: Page, id: string, params: unknown): Promise<any> {
  return page.evaluate(
    async ([id, params]) => {
      const ws = new WebSocket(`ws://${location.host}/rpc?token=${(window as any).BURNCRAFT_TOKEN}`);
      await new Promise((ok, fail) => {
        ws.onopen = ok;
        ws.onerror = fail;
      });
      const send = (n: number, method: string, p: unknown) =>
        new Promise<any>((ok) => {
          ws.onmessage = (ev) => {
            const m = JSON.parse(ev.data);
            if (m.id === n) ok(m);
          };
          ws.send(JSON.stringify({ jsonrpc: "2.0", id: n, method, params: p }));
        });
      await send(1, "mcp", { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18" } });
      const r = await send(2, "mcp", { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "execute", arguments: { id, params } } });
      ws.close();
      return r.result.result;
    },
    [id, params] as const,
  );
}

test("req_01_029_every_toolbar_control_is_a_registry_command", async ({ page }) => {
  const missing = await bc<string[]>(page, (b) => {
    const out: string[] = [];
    const controls = document.querySelectorAll<HTMLElement>(
      "[role=toolbar] button:not(.grip):not(.tb-menu), [role=toolbar] input, [role=toolbar] select",
    );
    for (const el of controls) {
      const id = el.dataset.command;
      if (!id || !b.commands.has(id)) out.push(`${el.dataset.testid ?? el.outerHTML.slice(0, 60)} → ${id ?? "brak"}`);
    }
    return controls.length > 20 ? out : [`za mało kontrolek: ${controls.length}`];
  });
  expect(missing).toEqual([]);
});

test("req_01_029_view_tool_and_snap_actions_run_registry_commands", async ({ page }) => {
  await page.getByTestId("zoom-in").click();
  await page.getByTestId("zoom-out").click();
  await page.getByTestId("zoom-100").click();
  await page.getByTestId("zoom-selection").click();
  await page.getByTestId("zoom-page").click();
  await page.getByTestId("tool-rect").click();
  await page.locator("#canvas").focus();
  await page.keyboard.press("Escape");
  await page.getByTestId("snap-grid").click();
  // kółko myszy i przesuwanie środkowym przyciskiem
  const [sx, sy] = await screenOf(page, 200, 200);
  await page.mouse.move(sx, sy);
  await page.mouse.wheel(0, -200);
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(sx + 40, sy + 20, { steps: 3 });
  await page.mouse.up({ button: "middle" });
  await page.getByTestId("palette-4").click();
  await page.getByTestId("open-settings").click();
  const ids = await log(page);
  for (const id of ["view.zoom", "view.actual_size", "view.zoom_selection", "view.zoom_page", "tool.select", "snap.set", "view.pan", "layer.set_active", "dialog.open"]) {
    expect(ids, `${id} w ${ids.join(", ")}`).toContain(id);
  }
  // wyniki widać w oknie
  await expect(page.getByTestId("settings-dialog")).toBeVisible();
  expect(await bc(page, (b) => b.tool.id)).toBe("select");
  expect(await bc(page, (b) => b.snapSettings.grid)).toBe(false);
  expect(await bc(page, (b) => b.layer)).toBe(4);
});

test("req_01_029_agent_drives_view_and_tools_of_the_window", async ({ page }) => {
  const z = await agentExecute(page, "view.actual_size", {});
  expect(z.isError, JSON.stringify(z)).toBe(false);
  await expect.poll(() => bc(page, (b) => Math.round(b.view.zoomPercent))).toBe(100);
  const t = await agentExecute(page, "tool.select", { tool: "ellipse" });
  expect(t.isError, JSON.stringify(t)).toBe(false);
  await expect(page.getByTestId("tool-ellipse")).toHaveAttribute("aria-pressed", "true");
  const v = await agentExecute(page, "view.get", {});
  expect(v.structuredContent.result.zoom).toBeCloseTo(100, 6);
  expect(v.structuredContent.result.visible).toHaveLength(4);
  const bad = await agentExecute(page, "tool.select", { tool: "nope" });
  expect(bad.isError).toBe(true);
});
