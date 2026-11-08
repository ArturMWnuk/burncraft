// Okno ustawień warstwy (REQ-03-032, §3 „Warstwy/Cięcia”): wszystkie parametry
// `layer.set` i `layer.params` z §7.2 w zakładkach. Każde pole wykonuje polecenie
// rejestru (REQ-01-029) — zmiany są cofalne jak każda zmiana dokumentu. Okno nie
// przebudowuje się po każdej zmianie: pola bez fokusu dostają wartości z dokumentu,
// a listy (pod-warstwy, scan offset) przebudowują się, gdy zmieni się ich długość.

import { h, num, parseNum } from "./dom";
import { t } from "./i18n";
import { Layer, Scene, noPwm } from "./scene";

export interface LayerSettingsHost {
  scene: Scene | null;
  modal: boolean;
  /** Polecenie hosta; błąd = wyjątek z komunikatem. */
  call<T = any>(id: string, params?: unknown): Promise<T>;
  /** Polecenie okna albo hosta (REQ-01-029). */
  exec(id: string, params?: unknown): unknown;
  refresh(): Promise<void>;
}

export const LAYER_TABS = ["general", "cut", "scan", "advanced", "sublayers"] as const;
export type LayerTab = (typeof LAYER_TABS)[number];

const MODES = ["line", "fill", "offset_fill", "image", "fill_line", "flood_fill"];
const LEADS = ["none", "line", "arc"];
const OVERSCAN = ["off", "auto", "percent"];
const GROUPING = ["all", "groups", "each"];

type Params = Record<string, any>;
type Kind = "num" | "int" | "opt" | "optint" | "text" | "opttext" | "area" | "bool" | "select" | "color";

/** Pole okna: odczyt z warstwy, zapis poleceniem, synchronizacja z dokumentem. */
interface Field {
  el: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
  kind: Kind;
  get(l: Layer): unknown;
  enabled?(l: Layer): boolean;
}

export class LayerSettings {
  state = { open: false, index: 0, tab: "general" as LayerTab };
  private root: HTMLElement | null = null;
  private fields: Field[] = [];
  private pages = new Map<LayerTab, HTMLElement>();
  private tabs = new Map<LayerTab, HTMLButtonElement>();
  private error = h("p", { class: "error", "data-testid": "ls-error", role: "alert" });
  private title = h("h2", {});
  private subBox = h("div", { class: "ls-list" });
  private offBox = h("div", { class: "ls-list" });
  private subKey = "";
  private offKey = "";
  private queue: Promise<void> = Promise.resolve();
  private pending = 0;
  /** Warstwa z wyniku ostatniego polecenia, dopóki kolejka zmian nie opustoszeje. */
  private latest: Layer | null = null;

  constructor(private host: LayerSettingsHost) {}

  open(p: { index?: number; tab?: LayerTab }) {
    const index = Number(p.index);
    if (!Number.isInteger(index) || !this.host.scene?.layers.some((l) => l.index === index)) throw new Error(`index: brak warstwy ${p.index}`);
    if (p.tab !== undefined && !LAYER_TABS.includes(p.tab)) throw new Error(`tab: ${LAYER_TABS.join(" | ")}`);
    if (this.root && index !== this.state.index) this.close();
    this.state = { open: true, index, tab: p.tab ?? (this.root ? this.state.tab : "general") };
    this.build();
    this.showTab();
    this.sync();
    return {};
  }

  close() {
    this.state.open = false;
    this.root?.remove();
    this.root = null;
    this.fields = [];
    this.subKey = this.offKey = "";
    this.latest = null;
    this.host.modal = false;
    return {};
  }

  /** Dokument się zmienił: wartości pól z warstwy. */
  refresh() {
    if (!this.root) return;
    if (!this.layer()) {
      this.close();
      return;
    }
    this.sync();
  }

  private layer(): Layer | undefined {
    return this.latest ?? this.host.scene?.layers.find((l) => l.index === this.state.index);
  }

  private params(): Params {
    return ((this.layer() as any)?.params ?? {}) as Params;
  }

  // ---------- zapis ----------

  /**
   * Zmiany idą po kolei: każda jest składana dopiero po wykonaniu poprzedniej, z warstwy
   * zwróconej przez poprzednie polecenie (scena może jeszcze nie być odświeżona). Bez tego
   * dwie szybkie zmiany pól jednego obiektu (np. mostków) nadpisywały się nawzajem.
   */
  private send(id: "layer.set" | "layer.params", p: Params | (() => Params)) {
    this.pending++;
    this.queue = this.queue.then(async () => {
      try {
        const r = await this.host.call(id, { index: this.state.index, ...(typeof p === "function" ? p() : p) });
        if (r && typeof r === "object" && (r as Layer).index === this.state.index) this.latest = r as Layer;
        this.error.textContent = "";
      } catch (e) {
        this.error.textContent = String((e as Error).message ?? e);
      }
      this.sync();
      if (--this.pending === 0) {
        await this.host.refresh();
        if (this.pending === 0) this.latest = null;
        this.sync();
      }
    });
    return this.queue;
  }

  private setLayer(key: string, v: unknown) {
    return this.send("layer.set", { [key]: v });
  }

  /** Parametr §7.2; zagnieżdżony obiekt wysyłany w całości (pola z bieżących wartości). */
  private setParam(path: string, v: unknown) {
    const [top, sub] = path.split(".");
    if (!sub) return this.send("layer.params", { params: { [top]: v } });
    return this.send("layer.params", () => ({ params: { [top]: { ...(this.params()[top] ?? {}), [sub]: v } } }));
  }

  /** Zmiana parametru wyliczana z jego bieżącej wartości w chwili wysłania (listy). */
  private updateParam(top: string, f: (cur: any) => unknown) {
    return this.send("layer.params", () => ({ params: { [top]: f(this.params()[top]) } }));
  }

  // ---------- pola ----------

  private label(key: string, el: HTMLElement) {
    return h("label", { class: "ls-field" }, h("span", {}, t(`ls-${key}`)), el);
  }

  /** Pole edycji: kind decyduje o parsowaniu; pusta wartość pola opcjonalnego = null. */
  private input(
    testid: string,
    key: string,
    kind: Kind,
    command: string,
    get: (l: Layer) => unknown,
    save: (v: unknown) => Promise<void>,
    extra: { options?: string[]; optionKey?: (o: string) => string; enabled?: (l: Layer) => boolean } = {},
  ): HTMLElement {
    const title = t(`ls-${key}`);
    const common = { "data-testid": testid, "data-command": command, title, "aria-label": title };
    let el: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
    if (kind === "bool") {
      el = h("input", { type: "checkbox", ...common });
      el.addEventListener("change", () => void save((el as HTMLInputElement).checked));
    } else if (kind === "select") {
      el = h("select", common, ...(extra.options ?? []).map((o) => h("option", { value: o }, t(extra.optionKey ? extra.optionKey(o) : o))));
      el.addEventListener("change", () => void save(el.value));
    } else if (kind === "color") {
      el = h("input", { type: "color", ...common });
      el.addEventListener("change", () => void save(el.value));
    } else if (kind === "area") {
      el = h("textarea", { rows: 3, spellcheck: "false", ...common });
      el.addEventListener("change", () => void save(el.value));
    } else {
      const numeric = kind !== "text" && kind !== "opttext";
      el = h("input", { type: "text", inputmode: numeric ? "decimal" : "text", class: numeric ? "" : "wide", ...common });
      el.addEventListener("change", () => {
        const raw = el.value.trim();
        if (kind === "text") return void save(el.value);
        if (kind === "opttext") return void save(raw ? el.value : null);
        if (!raw && (kind === "opt" || kind === "optint")) return void save(null);
        const n = parseNum(raw);
        if (n === null) {
          this.error.textContent = t("ls-not-number", { field: title });
          return;
        }
        void save(kind === "int" || kind === "optint" ? Math.round(n) : n);
      });
    }
    el.addEventListener("keydown", (e: Event) => {
      const k = e as KeyboardEvent;
      if (k.key === "Escape") return;
      k.stopPropagation();
      if (k.key === "Enter" && !(el instanceof HTMLTextAreaElement)) (el as HTMLElement).blur();
    });
    this.fields.push({ el, kind, get, enabled: extra.enabled });
    return this.label(key, el);
  }

  private lset(key: string, kind: Kind, extra: Parameters<LayerSettings["input"]>[6] = {}) {
    return this.input(`ls-${key}`, key, kind, "layer.set", (l) => (l as any)[key], (v) => this.setLayer(key, v), extra);
  }

  private par(path: string, kind: Kind, extra: Parameters<LayerSettings["input"]>[6] = {}) {
    const [top, sub] = path.split(".");
    return this.input(
      `ls-${path.replace(".", "-")}`,
      path.replace(".", "-"),
      kind,
      "layer.params",
      (l) => (sub ? (l as any).params?.[top]?.[sub] : (l as any).params?.[top]),
      (v) => this.setParam(path, v),
      extra,
    );
  }

  /** Opcjonalny obiekt (perforacja, tryb punktowy, rampa): przełącznik i pola. */
  private optional(key: string, def: Params, fields: [string, Kind][]) {
    const on = this.input(`ls-${key}-on`, `${key}-on`, "bool", "layer.params", (l) => (l as any).params?.[key] != null,
      (v) => this.send("layer.params", { params: { [key]: v ? def : null } }));
    const enabled = (l: Layer) => (l as any).params?.[key] != null;
    return h("fieldset", { class: "ls-group" }, h("legend", {}, t(`ls-${key}`)), on,
      ...fields.map(([f, kind]) => this.par(`${key}.${f}`, kind, { enabled })));
  }

  private group(key: string, ...children: HTMLElement[]) {
    return h("fieldset", { class: "ls-group" }, h("legend", {}, t(`ls-group-${key}`)), ...children);
  }

  // ---------- zakładki ----------

  private general(): HTMLElement[] {
    return [
      this.group("layer", this.lset("name", "text"), this.lset("color", "color"),
        this.lset("mode", "select", { options: MODES, optionKey: (m) => `mode-${m}` }),
        this.lset("output", "bool"), this.lset("show", "bool"), this.lset("air_assist", "bool")),
      // brak PWM (K40 M2 Nano): moc z pokrętła lasera — pola mocy nieaktywne (REQ-13-045)
      this.group("power", this.noPwmNote, this.lset("speed", "num"), this.lset("power_max", "num", { enabled: () => !noPwm(this.host.scene) }),
        this.lset("power_min", "num", { enabled: () => !noPwm(this.host.scene) }),
        this.lset("passes", "int"), this.par("pass_delay", "num"), this.par("priority", "int"), this.par("pause_message", "opttext")),
    ];
  }

  private cut(): HTMLElement[] {
    const lead = (k: string) =>
      this.group(k, this.par(`${k}.kind`, "select", { options: LEADS, optionKey: (o) => `ls-lead-${o}` }),
        this.par(`${k}.length`, "num"), this.par(`${k}.angle`, "num"));
    return [
      this.group("kerf", this.par("kerf", "num"), this.par("kerf_x", "opt"), this.par("kerf_y", "opt")),
      this.group("tabs", this.par("tabs.enabled", "bool"), this.par("tabs.size", "num"), this.par("tabs.count", "int"),
        this.par("tabs.spacing", "num"), this.par("tabs.skip_inner", "bool")),
      lead("lead_in"),
      lead("lead_out"),
      this.group("cut-more", this.par("overcut", "num"), this.par("corner_ramp", "bool")),
      this.optional("perforation", { cut: 2, gap: 1 }, [["cut", "num"], ["gap", "num"]]),
      this.optional("dot_mode", { dwell_ms: 1, spacing: 0.5 }, [["dwell_ms", "num"], ["spacing", "num"]]),
      this.optional("ramp", { length: 2, outer_only: false }, [["length", "num"], ["outer_only", "bool"]]),
    ];
  }

  private scan(): HTMLElement[] {
    const percent = (l: Layer) => (l as any).params?.overscan?.kind === "percent";
    const overscanKind = this.input("ls-overscan-kind", "overscan-kind", "select", "layer.params",
      (l) => (l as any).params?.overscan?.kind,
      (v) => this.send("layer.params", { params: { overscan: v === "percent" ? { kind: "percent", percent: 2.5 } : { kind: v } } }),
      { options: OVERSCAN, optionKey: (o) => `ls-overscan-opt-${o}` });
    const overscanPct = this.input("ls-overscan-percent", "overscan-percent", "num", "layer.params",
      (l) => (l as any).params?.overscan?.percent,
      (v) => this.send("layer.params", { params: { overscan: { kind: "percent", percent: v } } }), { enabled: percent });
    const lpi = this.input("ls-lpi", "lpi", "num", "layer.params", (l) => 25.4 / ((l as any).params?.interval ?? 0.1),
      (v) => this.send("layer.params", { lpi: v }));
    return [
      this.group("scan", this.par("bidirectional", "bool"), this.par("scan_angle", "num"), this.par("cross_hatch", "bool"),
        this.par("interval", "num"), lpi, overscanKind, overscanPct),
      this.group("fill", this.par("fill_kerf", "num"),
        this.par("fill_grouping", "select", { options: GROUPING, optionKey: (o) => `ls-grouping-${o}` })),
      this.group("scan_offsets", this.offBox,
        h("button", { "data-testid": "ls-scan_offset-add", "data-command": "layer.params", title: t("ls-scan_offset-add"),
          onclick: () => void this.updateParam("scan_offsets", (cur) => [...(cur ?? []), { speed: 100, offset: 0 }]) },
        t("ls-scan_offset-add"))),
    ];
  }

  private advanced(): HTMLElement[] {
    const on = (l: Layer) => (l as any).params?.power2_max != null;
    const power2 = this.input("ls-power2-on", "power2-on", "bool", "layer.params", on,
      (v) => this.send("layer.params", () => ({ params: v ? { power2_max: this.layer()?.power_max ?? 100, power2_min: 0 } : { power2_max: null, power2_min: null } })));
    return [
      this.group("laser2", power2, this.par("power2_max", "opt", { enabled: on }), this.par("power2_min", "opt", { enabled: on })),
      this.group("output", this.par("pwm_hz", "optint"), this.par("dynamic_power", "bool")),
      this.group("z", this.par("z_offset", "num"), this.par("z_step", "num")),
      this.group("gcode", this.par("gcode_pre", "area"), this.par("gcode_post", "area")),
    ];
  }

  private sublayers(): HTMLElement[] {
    return [
      h("p", { class: "muted" }, t("ls-sub-hint")),
      this.subBox,
      h("button", { "data-testid": "ls-sub-add", "data-command": "layer.params", title: t("ls-sub-add"),
        onclick: () => void this.updateParam("sublayers", (cur) => [...(cur ?? []), { name: "", output: true, mode: "line" }]) },
      t("ls-sub-add")),
    ];
  }

  /** Lista elementów tablicy parametru: wiersze z polami i przyciskiem usuń. */
  private listRows(key: "sublayers" | "scan_offsets", prefix: string, cols: [string, Kind, string[]?][]): HTMLElement[] {
    const items: Params[] = this.params()[key] ?? [];
    return items.map((_, i) => {
      const update = (f: string, v: unknown) =>
        this.updateParam(key, (list) => (list ?? []).map((x: Params, k: number) => (k === i ? { ...x, [f]: v } : x)));
      const cells = cols.map(([f, kind, options]) =>
        this.input(`ls-${prefix}-${i}-${f}`, `${prefix}-${f}`, kind, "layer.params",
          (l) => (l as any).params?.[key]?.[i]?.[f], (v) => update(f, v),
          options ? { options, optionKey: (m) => `mode-${m}` } : {}));
      const del = h("button", { class: "icon", "data-testid": `ls-${prefix}-${i}-delete`, "data-command": "layer.params", title: t("ls-delete"), "aria-label": t("ls-delete"),
        onclick: () => void this.updateParam(key, (list) => (list ?? []).filter((_: unknown, k: number) => k !== i)) }, "✕");
      return h("div", { class: "ls-row", "data-testid": `ls-${prefix}-${i}` }, h("span", { class: "muted" }, `${i + 1}.`), ...cells, del);
    });
  }

  private syncLists() {
    const p = this.params();
    const subKey = String((p.sublayers ?? []).length);
    if (subKey !== this.subKey) {
      this.subKey = subKey;
      this.fields = this.fields.filter((f) => !this.subBox.contains(f.el));
      this.subBox.replaceChildren(...this.listRows("sublayers", "sub", [
        ["name", "text"], ["mode", "select", MODES], ["output", "bool"], ["speed", "opt"], ["power_max", "opt"],
        ["power_min", "opt"], ["passes", "optint"], ["interval", "opt"], ["scan_angle", "opt"],
      ]));
    }
    const offKey = String((p.scan_offsets ?? []).length);
    if (offKey !== this.offKey) {
      this.offKey = offKey;
      this.fields = this.fields.filter((f) => !this.offBox.contains(f.el));
      this.offBox.replaceChildren(...this.listRows("scan_offsets", "scan_offset", [["speed", "num"], ["offset", "num"]]));
    }
  }

  /** Wartości pól z dokumentu (pole z fokusem zostaje — użytkownik je edytuje). */
  /** Komunikat o braku PWM w zakładce Ogólne (REQ-13-045). */
  private noPwmNote = h("p", { class: "ls-note no-pwm", "data-testid": "ls-no-pwm", role: "note" });

  private sync() {
    const l = this.layer();
    if (!this.root || !l) return;
    const dev = noPwm(this.host.scene);
    this.noPwmNote.hidden = !dev;
    this.noPwmNote.textContent = dev ? t("power-no-pwm", { device: dev.name, board: dev.board ?? dev.driver }) : "";
    this.title.textContent = t("ls-title", { name: l.name });
    this.syncLists();
    for (const f of this.fields) {
      const v = f.get(l);
      f.el.disabled = f.enabled ? !f.enabled(l) : false;
      if (document.activeElement === f.el) continue;
      if (f.kind === "bool") (f.el as HTMLInputElement).checked = Boolean(v);
      else if (typeof v === "number") f.el.value = num(v);
      else f.el.value = v == null ? "" : String(v);
    }
  }

  private showTab() {
    for (const [k, page] of this.pages) page.hidden = k !== this.state.tab;
    for (const [k, b] of this.tabs) b.setAttribute("aria-selected", String(k === this.state.tab));
  }

  private build() {
    if (this.root) return;
    this.host.modal = true;
    this.fields = [];
    this.pages.clear();
    this.tabs.clear();
    const make: Record<LayerTab, () => HTMLElement[]> = {
      general: () => this.general(),
      cut: () => this.cut(),
      scan: () => this.scan(),
      advanced: () => this.advanced(),
      sublayers: () => this.sublayers(),
    };
    const bar = h("div", { class: "ls-tabs", role: "tablist" });
    const body = h("div", { class: "ls-body" });
    for (const k of LAYER_TABS) {
      const b = h("button", { role: "tab", "data-testid": `ls-tab-${k}`, "data-command": "layer.settings_open", title: t(`ls-tab-${k}`),
        onclick: () => this.host.exec("layer.settings_open", { index: this.state.index, tab: k }) }, t(`ls-tab-${k}`));
      this.tabs.set(k, b);
      bar.append(b);
      const page = h("div", { class: "ls-page", role: "tabpanel", "data-testid": `ls-page-${k}` }, ...make[k]());
      this.pages.set(k, page);
      body.append(page);
    }
    const box = h("div", { class: "dialog layer-settings", role: "dialog", "aria-label": t("ls-dialog") },
      h("div", { class: "row" }, this.title),
      bar,
      body,
      this.error,
      h("div", { class: "row" },
        h("button", { "data-testid": "ls-close", "data-command": "layer.settings_close", title: t("dialog-close"),
          onclick: () => this.host.exec("layer.settings_close") }, t("dialog-close"))));
    this.root = h("div", { class: "dialog-backdrop", "data-testid": "layer-dialog" }, box);
    this.root.addEventListener("keydown", (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === "Escape") this.host.exec("layer.settings_close");
    });
    document.body.append(this.root);
  }
}
