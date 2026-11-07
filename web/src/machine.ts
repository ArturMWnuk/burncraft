// Okna maszyny (M2.5): Laser (REQ-03-033, 052–054), Przesuń (REQ-03-034…039),
// Konsola (REQ-03-040) i Makra (REQ-03-041). Każda kontrolka wywołuje polecenie
// rejestru (`data-command`, REQ-01-029); stan urządzenia odświeża `machine.status`.

import { h, num, parseNum } from "./dom";
import { t } from "./i18n";
import { Rpc } from "./rpc";

export interface MachineHost {
  rpc: Rpc;
  /** Polecenie z rejestru (log poleceń, błąd na pasku stanu). */
  run(id: string, params?: unknown): Promise<unknown>;
  /** Polecenie okna albo hosta (REQ-01-029). */
  exec(id: string, params?: unknown): unknown;
  setPref(key: string, value: unknown): Promise<void>;
  redraw(): void;
  /** Zmiana położenia głowicy (znacznik na kanwie). */
  onHead(): void;
}

export interface JogSettings {
  step_xy: number;
  step_z: number;
  slow: number;
  fast: number;
  mode: "slow" | "fast";
}

export const DEFAULT_JOG: JogSettings = { step_xy: 10, step_z: 1, slow: 5, fast: 50, mode: "fast" };

/** Domyślne kolory przycisków Start i Stop (REQ-03-052). */
export const START_COLOR = "#1f9d55";
export const STOP_COLOR = "#d63031";

const ORIGINS = ["top_left", "top", "top_right", "left", "center", "right", "bottom_left", "bottom", "bottom_right"];

interface Status {
  device: string;
  state: { state: string; wpos: number[]; mpos: number[]; alarm: [number, string] | null };
  job: { status: string };
  head: [number, number] | null;
}

interface JobStatus {
  status: string;
  percent: number;
  elapsed: string;
  pass?: number;
  passes?: number;
  layer?: number;
}

/** Fragment wiersza konsoli z klasą kolorowania. */
export interface Token {
  cls: string;
  text: string;
}

/** Kolorowanie składni wiersza konsoli (REQ-03-040): G-code wysłany i odpowiedzi GRBL. */
export function highlight(line: string, sent: boolean): Token[] {
  const out: Token[] = [];
  if (!sent) {
    const l = line.trim();
    const cls = /^ok$/i.test(l) ? "tok-ok" : /^error/i.test(l) ? "tok-error" : /^alarm/i.test(l) ? "tok-alarm" : /^\[.*\]$/.test(l) || /^</.test(l) ? "tok-msg" : /^\$/.test(l) ? "tok-dollar" : "tok-text";
    out.push({ cls, text: line });
    return out;
  }
  const re = /(;.*$|\(.*?\))|([GM]\d+(?:\.\d+)?)|([XYZIJKFSPRTABCHLN])(\s*[-+]?\d*\.?\d+)|(\$\S*)|(\s+)|(.)/giy;
  let m: RegExpExecArray | null;
  let plain = "";
  const flush = () => {
    if (plain) out.push({ cls: "tok-text", text: plain });
    plain = "";
  };
  while ((m = re.exec(line))) {
    if (m[1]) {
      flush();
      out.push({ cls: "tok-comment", text: m[1] });
    } else if (m[2]) {
      flush();
      out.push({ cls: /^m/i.test(m[2]) ? "tok-m" : "tok-g", text: m[2] });
    } else if (m[3]) {
      flush();
      out.push({ cls: "tok-axis", text: m[3] }, { cls: "tok-num", text: m[4].trim() });
    } else if (m[5]) {
      flush();
      out.push({ cls: "tok-dollar", text: m[5] });
    } else {
      plain += m[0];
    }
  }
  flush();
  return out;
}

function section(id: string, titleKey: string) {
  const body = h("div", { class: "panel-body" });
  const head = h("button", { class: "panel-head", "aria-expanded": "true", title: t("objects-toggle") }, t(titleKey));
  const root = h("section", { class: "panel machine", id: `panel-${id}`, "data-testid": `panel-${id}` }, head, body);
  head.addEventListener("click", () => {
    const closed = root.classList.toggle("collapsed");
    head.setAttribute("aria-expanded", String(!closed));
  });
  return { root, body, head, titleKey };
}

export class Machine {
  /** Ostatni `machine.status` (null = brak urządzenia). */
  status: Status | null = null;
  job: JobStatus | null = null;
  jog: JogSettings = { ...DEFAULT_JOG };
  /** Parametry ostatniego jogu i startu (diagnostyka, testy). */
  lastJog: { axis: string; distance: number; speed: number } | null = null;
  lastStart: Record<string, unknown> | null = null;
  profiles: string[] = [];
  connected: string[] = [];
  positions: { name: string; x: number; y: number }[] = [];
  macros: { name: string; steps: unknown[] }[] = [];
  start = { from: "absolute", origin: "bottom_left" };
  private console: string[] = [];
  private consoleKey = "";
  history: string[] = [];
  private histPos = -1;
  private timer = 0;
  private polling = false;
  private selectionOnly = false;

  private laser = section("laser", "panel-laser");
  private move = section("move", "panel-move");
  private cons = section("console", "panel-console");
  private mac = section("macros", "panel-macros");
  private consoleLog = h("div", { class: "console-log", "data-testid": "console-log", role: "log" });
  private consoleInput = h("input", { type: "text", class: "console-input", "data-testid": "console-input", "data-command": "device.raw", spellcheck: "false", autocomplete: "off" });
  private live: Record<string, HTMLElement> = {};
  // pola tekstowe trwałe: przebudowa okna po odświeżeniu listy nie kasuje wpisanego tekstu
  private posName = h("input", { type: "text", "data-testid": "pos-name", "data-command": "machine.position_save" });
  private macroName = h("input", { type: "text", "data-testid": "macro-name", "data-command": "macro.save" });
  private macroLines = h("textarea", { rows: 3, "data-testid": "macro-lines", "data-command": "macro.save" });
  private portInput = h("input", { type: "text", "data-testid": "laser-port", "data-command": "machine.profile_save" });
  /** Wynik kreatora „Znajdź mój laser” (REQ-13-192). */
  found: { port: { port: string; chip: string | null }; baud: number | null; variant: string | null; version: string | null; error: string | null; profile: unknown }[] | null = null;

  constructor(root: HTMLElement, private host: MachineHost) {
    root.append(this.laser.root, this.move.root, this.cons.root, this.mac.root);
    for (const el of [this.posName, this.macroName, this.macroLines, this.portInput]) el.addEventListener("keydown", (e) => e.stopPropagation());
    this.consoleInput.addEventListener("keydown", (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === "Enter") void this.send();
      else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
        e.preventDefault();
        this.recall(e.key === "ArrowUp" ? 1 : -1);
      }
    });
    this.render();
  }

  get device(): string | null {
    return this.status?.device ?? null;
  }

  /** Położenie głowicy w układzie dokumentu (REQ-03-004). */
  get head(): [number, number] | null {
    return this.status?.head ?? null;
  }

  /** Preferencje: jog i historia konsoli (wywołuje okno po `prefs.get`). */
  applyPrefs(ui: Record<string, unknown>) {
    const j = ui.jog as Partial<JogSettings> | undefined;
    this.jog = { ...DEFAULT_JOG, ...(j && typeof j === "object" ? j : {}) };
    if (Array.isArray(ui.console_history)) this.history = ui.console_history.filter((x) => typeof x === "string").slice(-100);
    this.render();
  }

  /** Polecenie okna `jog.settings` (REQ-03-037/038). */
  setJog(p: Partial<JogSettings>) {
    const pos = (v: unknown, max: number) => typeof v === "number" && Number.isFinite(v) && v > 0 && v <= max;
    const next = { ...this.jog };
    if (p.step_xy !== undefined) {
      if (!pos(p.step_xy, 1000)) throw new Error("step_xy: 0–1000 mm");
      next.step_xy = p.step_xy;
    }
    if (p.step_z !== undefined) {
      if (!pos(p.step_z, 100)) throw new Error("step_z: 0–100 mm");
      next.step_z = p.step_z;
    }
    if (p.slow !== undefined) {
      if (!pos(p.slow, 1000)) throw new Error("slow: 0–1000 mm/s");
      next.slow = p.slow;
    }
    if (p.fast !== undefined) {
      if (!pos(p.fast, 1000)) throw new Error("fast: 0–1000 mm/s");
      next.fast = p.fast;
    }
    if (p.mode !== undefined) {
      if (p.mode !== "slow" && p.mode !== "fast") throw new Error("mode: slow | fast");
      next.mode = p.mode;
    }
    this.jog = next;
    void this.host.setPref("ui.jog", next);
    this.render();
    return {};
  }

  /** Odświeża listy (profile, pozycje, makra, ustawienia startu). */
  async reload() {
    try {
      const l = await this.host.rpc.execute<{ profiles: { name: string }[]; connected: string[]; selected: string | null }>("machine.list", {});
      this.profiles = l.profiles.map((p) => p.name);
      this.connected = l.connected;
      this.positions = (await this.host.rpc.execute<{ positions: Machine["positions"] }>("machine.positions", {})).positions;
      this.macros = (await this.host.rpc.execute<{ macros: Machine["macros"] }>("macro.list", {})).macros;
      this.start = await this.host.rpc.execute("job.start_settings", {});
    } catch {
      // host bez urządzeń
    }
    await this.poll();
    this.ensurePolling();
    this.render();
  }

  private ensurePolling() {
    if (this.timer) return;
    this.timer = window.setInterval(() => void this.poll(), 200);
  }

  /** Stan urządzenia, zadania i konsoli (co 200 ms). */
  async poll() {
    if (this.polling) return;
    this.polling = true;
    const before = JSON.stringify(this.head);
    try {
      this.status = await this.host.rpc.execute<Status>("machine.status", {});
      this.job = await this.host.rpc.execute<JobStatus>("job.status", {});
      const c = await this.host.rpc.execute<{ lines: string[] }>("machine.console", {});
      this.console = c.lines;
    } catch {
      this.status = null;
      this.job = null;
    } finally {
      this.polling = false;
    }
    if (JSON.stringify(this.head) !== before) this.host.onHead();
    this.update();
  }

  // ---------- akcje ----------

  private async jogAxis(axis: "x" | "y" | "z", sign: number) {
    const step = axis === "z" ? this.jog.step_z : this.jog.step_xy;
    const speed = this.jog.mode === "slow" ? this.jog.slow : this.jog.fast;
    this.lastJog = { axis, distance: sign * step, speed };
    await this.host.run("device.jog", this.lastJog);
  }

  private async startJob() {
    this.lastStart = this.selectionOnly ? { selection_only: true } : {};
    await this.host.run("job.start", this.lastStart);
    await this.poll();
  }

  /** „Rozpocznij od tego miejsca” z okna Podgląd (REQ-09-013). */
  async startFrom(segment: number) {
    this.lastStart = { from_segment: segment, ...(this.selectionOnly ? { selection_only: true } : {}) };
    await this.host.run("job.start", this.lastStart);
    await this.poll();
  }

  private async send() {
    const line = this.consoleInput.value.trim();
    if (!line) return;
    this.history = [...this.history.filter((x) => x !== line), line].slice(-100);
    this.histPos = -1;
    this.consoleInput.value = "";
    void this.host.rpc.execute("prefs.set", { key: "ui.console_history", value: this.history }).catch(() => undefined);
    await this.host.run("device.raw", { line });
    await this.poll();
  }

  private recall(dir: number) {
    if (!this.history.length) return;
    const n = this.history.length;
    this.histPos = Math.max(-1, Math.min(n - 1, this.histPos + dir));
    this.consoleInput.value = this.histPos < 0 ? "" : this.history[n - 1 - this.histPos];
  }

  private async afterList() {
    await this.reload();
  }

  // ---------- widok ----------

  retranslate() {
    for (const s of [this.laser, this.move, this.cons, this.mac]) {
      s.head.textContent = t(s.titleKey);
      s.head.title = t("objects-toggle");
    }
    this.render();
  }

  /** Kolory Start/Stop z preferencji (REQ-03-052). */
  setColors(start: string, stop: string) {
    document.documentElement.style.setProperty("--start-color", start);
    document.documentElement.style.setProperty("--stop-color", stop);
  }

  private btn(testid: string, command: string, label: string, onclick: () => void, cls = "") {
    return h("button", { class: cls, "data-testid": testid, "data-command": command, title: t(`${label}.title`) === `${label}.title` ? t(label) : t(`${label}.title`), "aria-label": t(label), onclick }, t(label));
  }

  private numInput(testid: string, label: string, value: number, apply: (v: number) => void) {
    const input = h("input", { type: "text", inputmode: "decimal", class: "cell", value: num(value), "data-testid": testid, "data-command": "jog.settings", title: t(label), "aria-label": t(label) });
    const commit = () => {
      const v = parseNum(input.value);
      if (v === null || v <= 0) {
        input.value = num(value);
        return;
      }
      apply(v);
    };
    input.addEventListener("keydown", (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === "Enter") commit();
    });
    input.addEventListener("change", commit);
    return h("label", { class: "field" }, h("span", {}, t(label)), input);
  }

  render() {
    // pola trwałe (Konsola, nazwy pozycji i makr, port) są przenoszone do nowych wierszy:
    // przeniesienie w DOM zabiera fokus — po przebudowie wraca razem z kursorem
    const keep = [this.consoleInput, this.posName, this.macroName, this.macroLines, this.portInput]
      .find((el) => el === document.activeElement);
    const caret = keep ? [keep.selectionStart, keep.selectionEnd] : null;
    this.renderLaser();
    this.renderMove();
    this.renderConsole();
    this.renderMacros();
    if (keep?.isConnected && document.activeElement !== keep) {
      keep.focus({ preventScroll: true });
      if (caret && caret[0] !== null && caret[1] !== null) keep.setSelectionRange(caret[0], caret[1]);
    }
    this.update();
  }

  private renderLaser() {
    const dev = h("select", { "data-testid": "laser-device", "data-command": "machine.connect", title: t("laser-device"), "aria-label": t("laser-device") },
      ...this.profiles.map((p) => h("option", { value: p, selected: p === (this.device ?? this.connected[0] ?? this.profiles[0]) }, p)));
    const live = (key: string, testid: string) => (this.live[key] = h("span", { "data-testid": testid, class: `live ${key}` }));
    const startFrom = h("select", { "data-testid": "laser-start-from", "data-command": "job.start_settings", title: t("laser-start-from"), "aria-label": t("laser-start-from"),
      onchange: async (e: Event) => {
        await this.host.run("job.start_settings", { from: (e.target as HTMLSelectElement).value });
        await this.afterList();
      } },
      ...["absolute", "current", "user_origin"].map((v) => h("option", { value: v, selected: v === this.start.from }, t(`start-from-${v}`))));
    const origins = h("div", { class: "job-origin", title: t("laser-job-origin"), role: "group", "aria-label": t("laser-job-origin") },
      ...ORIGINS.map((o) => h("button", { "data-testid": `laser-job-origin-${o}`, "data-command": "job.start_settings", "aria-pressed": String(o === this.start.origin),
        title: `${t("laser-job-origin")}: ${t(`origin-${o}`)}`, "aria-label": t(`origin-${o}`),
        onclick: async () => {
          await this.host.run("job.start_settings", { origin: o });
          await this.afterList();
        } })));
    const sel = h("input", { type: "checkbox", checked: this.selectionOnly, "data-testid": "laser-selection-only", "data-command": "job.start", title: t("laser-selection-only"), "aria-label": t("laser-selection-only"),
      onchange: (e: Event) => (this.selectionOnly = (e.target as HTMLInputElement).checked) });
    this.laser.body.replaceChildren(
      h("div", { class: "row" }, dev,
        this.btn("laser-connect", "machine.connect", "laser-connect", async () => {
          await this.host.run("machine.connect", { name: dev.value });
          await this.afterList();
        }),
        this.btn("laser-disconnect", "machine.disconnect", "laser-disconnect", async () => {
          await this.host.run("machine.disconnect", {});
          await this.afterList();
        })),
      h("div", { class: "machine-state" }, live("state", "laser-state"), live("pos", "laser-pos")),
      h("div", { class: "row job-progress" }, live("progress", "laser-progress"), live("pass", "laser-pass"), live("time", "laser-time")),
      h("div", { class: "row big" },
        this.btn("laser-start", "job.start", "laser-start", () => void this.startJob(), "start-btn"),
        this.btn("laser-pause", "job.pause", "laser-pause", () => void this.host.run("job.pause", {})),
        this.btn("laser-resume", "job.resume", "laser-resume", () => void this.host.run("job.resume", {})),
        this.btn("laser-stop", "job.stop", "laser-stop", () => void this.host.run("job.stop", {}), "stop-btn")),
      h("div", { class: "row wrap" },
        this.btn("laser-frame", "device.frame", "laser-frame", () => void this.host.run("device.frame", this.selectionOnly ? { selection_only: true } : {})),
        this.btn("laser-home", "device.home", "laser-home", () => void this.host.run("device.home", {})),
        this.btn("laser-set-origin", "device.set_origin", "laser-set-origin", () => void this.host.run("device.set_origin", {})),
        this.btn("laser-go-origin", "device.go_origin", "laser-go-origin", () => void this.host.run("device.go_origin", {})),
        this.btn("laser-preview", "preview.open", "laser-preview", () => this.host.exec("preview.open", this.selectionOnly ? { selection_only: true } : {}))),
      h("label", { class: "field wide" }, h("span", {}, t("laser-start-from")), startFrom),
      h("div", { class: "row" }, h("span", { class: "muted" }, t("laser-job-origin")), origins),
      h("label", { class: "opt" }, sel, t("laser-selection-only")),
      ...this.setupRows(),
    );
  }

  /** Nowe urządzenie: kreator „Znajdź mój laser” i profil portu szeregowego. */
  private setupRows(): HTMLElement[] {
    const serial = (port: string, baud: number) => ({ name: `Laser ${port}`, driver: "grbl", transport: { kind: "serial", port, baud } });
    const add = async (profile: unknown) => {
      await this.host.run("machine.profile_save", { profile });
      await this.afterList();
    };
    this.portInput.placeholder = t("laser-port");
    this.portInput.title = t("laser-port");
    this.portInput.setAttribute("aria-label", t("laser-port"));
    const found = (this.found ?? []).map((f, i) =>
      h("div", { class: "row" },
        h("span", { class: "muted" }, `${f.port.port}${f.port.chip ? ` (${f.port.chip})` : ""}: ${f.variant ? `${f.variant} ${f.version ?? ""} @ ${f.baud}` : f.error ?? t("laser-found-none")}`),
        h("button", { "data-testid": `laser-found-${i}`, "data-command": "machine.profile_save", title: t("laser-found-add"), "aria-label": t("laser-found-add"),
          onclick: () => void add(f.profile ?? serial(f.port.port, f.baud ?? 115200)) }, t("laser-found-add"))));
    return [
      h("h4", {}, t("laser-setup")),
      h("div", { class: "row" },
        this.btn("laser-find", "machine.find", "laser-find", async () => {
          const r = (await this.host.run("machine.find", {})) as { found: Machine["found"] } | undefined;
          this.found = r?.found ?? [];
          this.render();
        }),
        this.portInput,
        this.btn("laser-add-port", "machine.profile_save", "laser-add-port", () => {
          const p = this.portInput.value.trim();
          if (p) void add(serial(p, 115200));
        })),
      ...(this.found && !this.found.length ? [h("p", { class: "empty", "data-testid": "laser-found-empty" }, t("laser-found-empty"))] : found),
    ];
  }

  private renderMove() {
    const jb = (testid: string, axis: "x" | "y" | "z", sign: number, label: string) =>
      h("button", { class: "jog", "data-testid": testid, "data-command": "device.jog", title: t(`jog-${axis}`, { dir: label }), "aria-label": t(`jog-${axis}`, { dir: label }),
        onclick: () => void this.jogAxis(axis, sign) }, `${axis.toUpperCase()}${label}`);
    const mode = (m: "slow" | "fast") =>
      h("button", { "data-testid": `jog-mode-${m}`, "data-command": "jog.settings", "aria-pressed": String(this.jog.mode === m), title: t(`jog-mode-${m}`), "aria-label": t(`jog-mode-${m}`),
        onclick: () => this.host.exec("jog.settings", { mode: m }) }, t(`jog-mode-${m}`));
    const name = this.posName;
    name.placeholder = t("pos-name");
    name.title = t("pos-name");
    name.setAttribute("aria-label", t("pos-name"));
    const list = this.positions.map((p, i) =>
      h("div", { class: "row" },
        h("button", { class: "pos", "data-testid": `pos-go-${i}`, "data-command": "device.position_go", title: t("pos-go"), "aria-label": `${t("pos-go")}: ${p.name}`,
          onclick: () => void this.host.run("device.position_go", { name: p.name }) }, `${p.name} (${num(p.x)}, ${num(p.y)})`),
        h("button", { class: "icon", "data-testid": `pos-del-${i}`, "data-command": "machine.position_delete", title: t("pos-delete"), "aria-label": t("pos-delete"),
          onclick: async () => {
            await this.host.run("machine.position_delete", { name: p.name });
            await this.afterList();
          } }, "✕")));
    this.move.body.replaceChildren(
      h("div", { class: "jog-grid" },
        h("span", {}), jb("jog-y-plus", "y", 1, "+"), h("span", {}), jb("jog-z-plus", "z", 1, "+"),
        jb("jog-x-minus", "x", -1, "−"), h("span", { class: "muted" }, "XY"), jb("jog-x-plus", "x", 1, "+"), h("span", { class: "muted" }, "Z"),
        h("span", {}), jb("jog-y-minus", "y", -1, "−"), h("span", {}), jb("jog-z-minus", "z", -1, "−")),
      h("div", { class: "row wrap" },
        this.numInput("jog-step-xy", "jog-step-xy", this.jog.step_xy, (v) => this.host.exec("jog.settings", { step_xy: v })),
        this.numInput("jog-step-z", "jog-step-z", this.jog.step_z, (v) => this.host.exec("jog.settings", { step_z: v }))),
      h("div", { class: "row wrap" },
        this.numInput("jog-slow", "jog-slow", this.jog.slow, (v) => this.host.exec("jog.settings", { slow: v })),
        this.numInput("jog-fast", "jog-fast", this.jog.fast, (v) => this.host.exec("jog.settings", { fast: v })),
        h("div", { class: "seg", role: "group", "aria-label": t("jog-speed") }, mode("slow"), mode("fast"))),
      h("h4", {}, t("pos-title")),
      h("div", { class: "row" }, name,
        this.btn("pos-save", "machine.position_save", "pos-save", async () => {
          if (!name.value.trim()) return;
          await this.host.run("machine.position_save", { name: name.value.trim() });
          name.value = "";
          await this.afterList();
        })),
      ...list,
    );
  }

  private renderConsole() {
    this.consoleInput.placeholder = t("console-placeholder");
    this.consoleInput.title = t("console-input");
    this.consoleInput.setAttribute("aria-label", t("console-input"));
    this.consoleKey = "";
    this.cons.body.replaceChildren(
      this.consoleLog,
      h("div", { class: "row" }, this.consoleInput, this.btn("console-send", "device.raw", "console-send", () => void this.send())),
    );
  }

  private renderMacros() {
    const name = this.macroName;
    const lines = this.macroLines;
    name.placeholder = t("macro-name");
    name.title = t("macro-name");
    name.setAttribute("aria-label", t("macro-name"));
    lines.placeholder = t("macro-lines-hint");
    lines.title = t("macro-lines-hint");
    lines.setAttribute("aria-label", t("macro-lines"));
    const list = this.macros.map((m, i) =>
      h("div", { class: "row" },
        h("button", { class: "macro", "data-testid": `macro-run-${i}`, "data-command": "macro.run", title: t("macro-run"), "aria-label": `${t("macro-run")}: ${m.name}`,
          onclick: async () => {
            await this.host.run("macro.run", { name: m.name });
            await this.poll();
          } }, m.name),
        h("button", { class: "icon", "data-testid": `macro-del-${i}`, "data-command": "macro.delete", title: t("macro-delete"), "aria-label": t("macro-delete"),
          onclick: async () => {
            await this.host.run("macro.delete", { name: m.name });
            await this.afterList();
          } }, "✕")));
    this.mac.body.replaceChildren(
      ...(list.length ? list : [h("p", { class: "empty" }, t("macro-empty"))]),
      h("h4", {}, t("macro-new")),
      name,
      lines,
      this.btn("macro-save", "macro.save", "macro-save", async () => {
        const steps = parseMacro(lines.value);
        if (!name.value.trim() || !steps.length) return;
        const ok = await this.host.run("macro.save", { macro: { name: name.value.trim(), steps } });
        if (ok !== undefined) {
          name.value = "";
          lines.value = "";
        }
        await this.afterList();
      }),
    );
  }

  /** Aktualizacja stanu bez przebudowy kontrolek (co 200 ms). */
  private update() {
    const st = this.status;
    const job = this.job;
    const l = this.live;
    if (!l.state) return;
    const state = st?.state.state ?? "disconnected";
    l.state.dataset.state = state;
    l.state.dataset.job = job?.status ?? "none";
    const js = job?.status ?? "none";
    l.state.textContent = st
      ? `${st.device}: ${t(`state-${state}`)}${st.state.alarm ? ` — ${st.state.alarm[1]}` : ""}${js !== "none" ? ` · ${t(`job-${js}`)}` : ""}`
      : t("state-disconnected");
    l.pos.textContent = st ? `X ${num(st.state.wpos[0])}  Y ${num(st.state.wpos[1])}  Z ${num(st.state.wpos[2] ?? 0)}` : "";
    const active = job && job.status !== "none";
    l.progress.textContent = active ? `${Math.floor(job.percent)}%` : "";
    l.pass.textContent = active && job.pass ? t("laser-pass", { pass: job.pass, passes: job.passes ?? job.pass }) : "";
    // czas widoczny także po zakończeniu zadania (REQ-03-053)
    l.time.textContent = active ? `${t(job.status === "done" ? "laser-time-done" : "laser-time")} ${job.elapsed}` : "";
    l.time.dataset.status = job?.status ?? "none";
    // konsola: tylko nowe wiersze
    const lines = this.console.filter((x) => !/^<\s*</.test(x)).slice(-300);
    const key = `${lines.length}:${lines[lines.length - 1] ?? ""}`;
    if (key !== this.consoleKey) {
      this.consoleKey = key;
      const atEnd = this.consoleLog.scrollTop + this.consoleLog.clientHeight >= this.consoleLog.scrollHeight - 4;
      this.consoleLog.replaceChildren(...lines.map((x) => {
        const sent = x.startsWith(">");
        const body = x.replace(/^[<>]\s?/, "");
        return h("div", { class: sent ? "line-sent" : "line-recv" }, h("span", { class: "dir" }, sent ? "›" : "‹"),
          ...highlight(body, sent).map((tk) => h("span", { class: tk.cls }, tk.text)));
      }));
      if (atEnd) this.consoleLog.scrollTop = this.consoleLog.scrollHeight;
    }
  }
}

/** Wiersze makra: surowe polecenia sterownika albo `@id {json}` — polecenie rejestru. */
export function parseMacro(text: string): unknown[] {
  return text.split(/\r?\n/).map((l) => l.trim()).filter(Boolean).map((l) => {
    const m = /^@([\w.]+)\s*(\{.*\})?$/.exec(l);
    if (m) return { kind: "command", id: m[1], params: m[2] ? JSON.parse(m[2]) : {} };
    return { kind: "raw", line: l };
  });
}
