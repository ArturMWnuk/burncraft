// BurnCraft GUI — punkt wejścia. Stan dokumentu żyje w hoście (bc-host);
// frontend tylko go wyświetla i wywołuje polecenia z rejestru.

import "./style.css";
import { toolCursor } from "./cursors";
import { DialogHost, keyCombo, openSettings, openShortcuts } from "./dialogs";
import { h } from "./dom";
import { inRect, pick } from "./hit";
import { setLang, t } from "./i18n";
import { fmtLen, getUnit, setUnit } from "./units";
import { Machine, MachineHost, START_COLOR, STOP_COLOR } from "./machine";
import { NodeHost, NodeTool } from "./nodeedit";
import { disabledTransformFields, Panels, transformFields, TransformState } from "./panels";
import { DEFAULT_LAYOUT, Dock, Layout, ToolbarDef, ToolbarHost, Toolbars } from "./toolbars";
import { EraserTool, Host2, LassoTool, TrimTool, TwoPointTool, WarpTool } from "./tools2";
import { Renderer, setCanvasTheme, THEME } from "./renderer";
import { Preview, PreviewHost } from "./preview";
import { LayerSettings } from "./layersettings";
import { DriverFiles, FilesHost } from "./files";
import { Rpc, RpcError } from "./rpc";
import { drawRulers } from "./rulers";
import { Scene, selectionBox } from "./scene";
import { loadSnap, saveSnap, snap, SNAP_PX, SnapOptions, SnapResult, SnapSettings } from "./snap";
import { LineTool, loadOptions, saveOptions, SelectTool, ShapeTool, Tool, ToolHost, ToolOptions } from "./tools";
import { Rect, View } from "./view";

interface CommandInfo {
  id: string;
  /** "document" | "workbench" | "window" — polecenia okna wykonuje to okno. */
  executor?: string;
  title: string;
  shortcut: string | null;
  params_schema: { required?: string[] };
}

type Drag =
  | { kind: "move"; ids: number[]; start: [number, number]; offset: [number, number]; moved: boolean }
  | { kind: "pan"; last: [number, number] }
  | { kind: "marquee"; start: [number, number]; rect: Rect; add: boolean };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** Pasek tworzenia: id narzędzia i ikona; opis z tłumaczeń (`tool-<id>`). */
const TOOLS: { id: string; label: string }[] = [
  { id: "select", label: "↖" },
  { id: "line", label: "✎" },
  { id: "node", label: "⌖" },
  { id: "lasso", label: "➰" },
  { id: "trim", label: "✂" },
  { id: "eraser", label: "⌫" },
  { id: "rotate2", label: "⟳" },
  { id: "scale2", label: "⤢" },
  { id: "warp", label: "⌗" },
  { id: "rect", label: "▭" },
  { id: "ellipse", label: "◯" },
  { id: "polygon", label: "⬠" },
  { id: "star", label: "☆" },
];

const SNAP_LABELS: (keyof SnapSettings)[] = ["grid", "nodes", "mids", "centers", "edges", "align"];

/** Narzędzia ukryte w trybie początkującego (REQ-03-051). */
const ADVANCED_TOOLS = new Set(["node", "trim", "eraser", "warp", "rotate2", "scale2"]);

class App implements ToolHost, NodeHost, Host2, DialogHost, ToolbarHost, MachineHost, PreviewHost, FilesHost {
  rpc = Rpc.fromPage();
  view = new View();
  scene: Scene | null = null;
  commands = new Map<string, CommandInfo>();
  /** Id wykonanych poleceń w kolejności (diagnostyka, testy REQ-01-029). */
  commandLog: string[] = [];
  /** Trwająca długa operacja w hoście (REQ-16-006). */
  private progress: { command: string; fraction: number } | null = null;
  private progressUi?: { root: HTMLElement; bar: HTMLProgressElement; label: HTMLElement; cancel: HTMLElement };
  private canvas = $<HTMLCanvasElement>("canvas");
  /** `null`, gdy przeglądarka nie ma WebGL2 — panele i kanał działają dalej. */
  private renderer = createRenderer(this.canvas);
  /** Punkt odniesienia i blokada proporcji: panel Właściwości i pasek numeryczny. */
  transform: TransformState = { anchor: "bl", lockAspect: true };
  private panels = new Panels($("panels"), this, this.transform);
  /** Okna maszyny: Laser, Przesuń, Konsola, Makra (M2.5). */
  machine = new Machine($("panels"), this);
  /** Okno Podgląd (REQ-03-046). */
  preview = new Preview(this);
  layerSettings = new LayerSettings(this);
  /** Okno Lista plików na sterowniku (REQ-03-042). */
  files = new DriverFiles(this);
  /** Obrysy kerfu w edytorze (REQ-07-029). */
  kerf: { layer: number; object: number; closed: boolean; points: [number, number][] }[] = [];
  private kerfFlat: { points: number[]; closed: boolean }[] = [];
  private toolbars?: Toolbars;
  private drag: Drag | null = null;
  private cursor: [number, number] | null = null;
  private frame = 0;
  private refreshing: Promise<void> | null = null;
  private refreshAgain = false;
  private message = "";
  private connected = false;
  private fitted = false;
  /** Aktywna warstwa dla nowych kształtów (paleta bez zaznaczenia). */
  layer = 0;
  snapSettings: SnapSettings = loadSnap();
  toolOpts: ToolOptions = loadOptions();
  private snapMark: SnapResult | null = null;
  private tools: Record<string, Tool> = {
    select: new SelectTool(),
    line: new LineTool(this),
    node: new NodeTool(this),
    lasso: new LassoTool(this),
    trim: new TrimTool(this),
    eraser: new EraserTool(this),
    rotate2: new TwoPointTool("rotate2", this),
    scale2: new TwoPointTool("scale2", this),
    warp: new WarpTool(this),
    rect: new ShapeTool("rect", this, () => this.toolOpts),
    ellipse: new ShapeTool("ellipse", this, () => this.toolOpts),
    polygon: new ShapeTool("polygon", this, () => this.toolOpts),
    star: new ShapeTool("star", this, () => this.toolOpts),
  };
  tool: Tool = this.tools.select;
  /** Preferencje interfejsu (PrefStore hosta: ui.lang, ui.theme, ui.units, ui.mode, ui.screen_scale). */
  prefs = { lang: "pl", theme: "dark", units: "mm", mode: "full", screenScale: 1, startColor: START_COLOR, stopColor: STOP_COLOR };
  /** Układ pasków (`ui.toolbars`) i zapisane układy (`ui.toolbar_layouts`). */
  layout: Layout = { ...DEFAULT_LAYOUT };
  layouts: Record<string, Layout> = {};

  get beginner(): boolean {
    return this.prefs.mode === "beginner";
  }

  saveLayout(l: Layout) {
    void this.setPref("ui.toolbars", l);
  }

  /** Paski: id i nazwa (okno Ustawienia). */
  toolbarList(): { id: string; title: string; dock: Dock }[] {
    return (this.toolbars?.ids ?? []).map((id) => ({ id, title: t(`toolbar-${id}`), dock: (this.layout[id] ?? DEFAULT_LAYOUT[id]).dock }));
  }

  setToolbarVisible(id: string, visible: boolean) {
    if (visible) this.toolbars?.moveTo(id, DEFAULT_LAYOUT[id]?.dock ?? "top");
    else this.toolbars?.moveTo(id, "hidden");
  }

  async saveNamedLayout(name: string) {
    await this.setPref("ui.toolbar_layouts", { ...this.layouts, [name]: this.layout });
  }
  /** Okno dialogowe otwarte — skróty aplikacji wyłączone. */
  modal = false;
  /** Geometria obiektów z poprzednich scen (id → obiekt ze skrótem). */
  private sceneCache = new Map<number, Scene["objects"][number]>();
  private sceneOrder = "";
  private sceneLayers = "";
  private treeCache: unknown = [];
  private treeHash = "";
  /** Rozmiar ostatniej odpowiedzi `gui.scene` (pomiar, testy wydajności). */
  sceneBytes = 0;
  /** Obowiązujące skróty z `shortcuts.list` (id polecenia → skrót). */
  private shortcuts = new Map<string, string | null>();

  async start() {
    if (!this.renderer) this.message = t("app-webgl-missing");
    window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", () => this.applyTheme());
    this.rpc.onState = (c) => {
      this.connected = c;
      this.status();
    };
    this.rpc.on("doc.changed", () => this.refresh());
    // pasek postępu operacji dłuższych niż 100 ms (REQ-16-006)
    this.rpc.on("gui.progress", (p: { command: string; fraction: number; done: boolean }) => {
      this.progress = p.done ? null : { command: p.command, fraction: p.fraction };
      this.status();
    });
    this.rpc.handle("ui.screenshot", () => ({ png: this.screenshot() }));
    // polecenia okna wywołane przez agenta MCP albo inny klient (REQ-01-029)
    this.rpc.handle("ui.execute", (p: any) => this.windowExec(String(p?.id), p?.params ?? {}));
    this.bindCanvas();
    this.bindRulers();
    this.bindKeys();
    new ResizeObserver(() => this.resize()).observe($("workspace"));
    this.resize();
    try {
      await this.rpc.connect();
    } catch (e) {
      this.message = String((e as Error).message);
      this.status();
      return;
    }
    await this.rpc.call("hello", { role: "window" });
    const { commands } = await this.rpc.call<{ commands: CommandInfo[] }>("gui.commands");
    for (const c of commands) this.commands.set(c.id, c);
    await this.loadPrefs();
    await this.refresh();
    await this.machine.reload();
  }

  /** Położenie głowicy zmieniło się (REQ-03-004). */
  onHead() {
    this.redraw();
  }

  /** Znacznik głowicy na kanwie (diagnostyka, testy). */
  headMarker(): [number, number] | null {
    return this.machine.head;
  }

  /** Kontury obiektów ze sceny (odniesienie kerfu w podglądzie). */
  designOutlines(ids: number[]): { points: number[]; closed: boolean }[] {
    const set = new Set(ids);
    const out: { points: number[]; closed: boolean }[] = [];
    for (const o of this.scene?.objects ?? []) {
      if (set.has(o.id)) o.polylines.forEach((p, i) => out.push({ points: p, closed: o.closed[i] ?? false }));
    }
    return out;
  }

  /** Wybrane urządzenie (okno plików na sterowniku). */
  get deviceName(): string | null {
    return this.machine.device;
  }

  async afterRun() {
    await this.machine.poll();
  }

  async startFrom(segment: number) {
    await this.machine.startFrom(segment);
  }

  /** Obrysy kerfu warstw z kerfem (REQ-07-029). */
  private async loadKerf() {
    try {
      const r = await this.rpc.execute<{ outlines: App["kerf"] }>("plan.kerf", {});
      this.kerf = r.outlines;
    } catch {
      this.kerf = [];
    }
    this.kerfFlat = this.kerf.map((k) => ({ points: k.points.flat(), closed: k.closed }));
    this.redraw();
  }

  // ---------- preferencje: język, motyw, jednostki, skróty ----------

  /** Wczytuje preferencje z hosta i stosuje je (także po zmianie z zewnątrz). */
  async loadPrefs() {
    try {
      const r = await this.rpc.execute<{ value: any }>("prefs.get", {});
      const ui = r?.value?.ui ?? {};
      this.prefs = {
        lang: typeof ui.lang === "string" ? ui.lang : navigator.language?.startsWith("pl") === false ? "en" : "pl",
        theme: typeof ui.theme === "string" ? ui.theme : "dark",
        units: typeof ui.units === "string" ? ui.units : "mm",
        mode: typeof ui.mode === "string" ? ui.mode : "full",
        screenScale: validScale(ui.screen_scale),
        startColor: validColor(ui.start_color, START_COLOR),
        stopColor: validColor(ui.stop_color, STOP_COLOR),
      };
      this.machine.applyPrefs(ui);
      this.layout = ui.toolbars && typeof ui.toolbars === "object" ? { ...DEFAULT_LAYOUT, ...ui.toolbars } : { ...DEFAULT_LAYOUT };
      this.layouts = ui.toolbar_layouts && typeof ui.toolbar_layouts === "object" ? ui.toolbar_layouts : {};
    } catch {
      // host bez magazynu preferencji — wartości domyślne
    }
    await this.reloadShortcuts();
    this.applyPrefs();
  }

  async setPref(key: string, value: unknown) {
    await this.rpc.execute("prefs.set", { key, value });
    const k = key.replace(/^ui\./, "");
    if (k === "toolbars") this.layout = value && typeof value === "object" ? { ...DEFAULT_LAYOUT, ...(value as Layout) } : { ...DEFAULT_LAYOUT };
    else if (k === "toolbar_layouts") this.layouts = (value as Record<string, Layout>) ?? {};
    else if (k === "screen_scale") this.prefs.screenScale = validScale(value);
    else if (k === "start_color") this.prefs.startColor = validColor(value, START_COLOR);
    else if (k === "stop_color") this.prefs.stopColor = validColor(value, STOP_COLOR);
    else if (k === "lang" || k === "theme" || k === "units" || k === "mode") this.prefs[k] = String(value);
    this.applyPrefs();
  }

  async reloadShortcuts() {
    try {
      const r = await this.rpc.execute<{ commands: { id: string; shortcut: string | null }[] }>("shortcuts.list");
      this.shortcuts = new Map(r.commands.map((c) => [c.id, c.shortcut]));
    } catch {
      this.shortcuts = new Map([...this.commands.values()].map((c) => [c.id, c.shortcut]));
    }
    this.buildToolbar();
  }

  private applyPrefs() {
    setLang(this.prefs.lang);
    setUnit(this.prefs.units);
    this.view.calibration = this.prefs.screenScale;
    this.applyTheme();
    document.documentElement.dataset.mode = this.beginner ? "beginner" : "full";
    if (this.beginner && ADVANCED_TOOLS.has(this.tool.id)) this.tool = this.tools.select;
    this.panels.retranslate();
    this.machine.setColors(this.prefs.startColor, this.prefs.stopColor);
    this.machine.retranslate();
    this.files.retranslate();
    this.buildToolbar();
    if (this.scene) {
      this.panels.update(this.scene);
      this.renderTabs(this.scene);
    }
    this.redraw();
  }

  /** Motyw ciemny / jasny / systemowy (REQ-03-016). */
  private applyTheme() {
    const system = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? true;
    const dark = this.prefs.theme === "dark" || (this.prefs.theme !== "light" && system);
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    setCanvasTheme(!dark);
    // kolory warstw na kanwie zależą od tła motywu
    if (this.scene) this.renderer?.upload(this.scene);
    this.redraw();
  }

  /** Polecenie z rejestru zwracające błąd (okna dialogowe). */
  call<T = any>(id: string, params: unknown = {}): Promise<T> {
    this.commandLog.push(id);
    return this.rpc.execute<T>(id, params);
  }

  /**
   * Akcja użytkownika (REQ-01-029): polecenie okna wykonuje się tutaj, pozostałe
   * w hoście. Błąd trafia na pasek stanu.
   */
  exec(id: string, params: unknown = {}): unknown {
    if (this.commands.get(id)?.executor !== "window") return this.run(id, params);
    try {
      const r = this.windowExec(id, params);
      this.message = "";
      this.status();
      return r;
    } catch (e) {
      this.message = t("msg-cmd-error", { title: this.commands.get(id)?.title ?? id, error: String((e as Error).message ?? e) });
      this.status();
      return undefined;
    }
  }

  /** Wykonuje polecenie okna z rejestru; niepoprawne parametry = wyjątek. */
  windowExec(id: string, params: any): unknown {
    const f = this.windowHandlers[id];
    if (!f || this.commands.get(id)?.executor !== "window") throw new Error(`${id} nie jest poleceniem okna`);
    this.commandLog.push(id);
    const r = f(params && typeof params === "object" ? params : {});
    this.redraw();
    return r;
  }

  private viewState() {
    this.measure();
    const v = this.view;
    return { cx: v.cx, cy: v.cy, zoom: v.zoomPercent, visible: v.visible() };
  }

  /** Wykonawcy poleceń okna (opis i schematy w rejestrze: `bc-commands/src/window.rs`). */
  private windowHandlers: Record<string, (p: any) => unknown> = {
    "view.zoom": (p) => {
      const f = Number(p.factor);
      if (!(Number.isFinite(f) && f > 0)) throw new Error("factor musi być > 0");
      this.measure();
      const at = Array.isArray(p.at) ? this.view.toScreen(finite(p.at[0]), finite(p.at[1])) : [this.view.width / 2, this.view.height / 2];
      this.view.zoomAt(at[0], at[1], f);
      return this.viewState();
    },
    "view.pan": (p) => {
      this.view.cx -= finite(p.dx);
      this.view.cy -= finite(p.dy);
      return this.viewState();
    },
    "view.zoom_page": () => {
      this.zoomPage();
      return this.viewState();
    },
    "view.zoom_selection": () => {
      this.zoomSelection();
      return this.viewState();
    },
    "view.actual_size": () => {
      this.view.actualSize();
      return this.viewState();
    },
    "view.get": () => this.viewState(),
    "tool.select": (p) => {
      const id = String(p.tool);
      if (!this.tools[id] || (this.beginner && ADVANCED_TOOLS.has(id))) throw new Error(`narzędzie ${id} nie istnieje albo jest niedostępne`);
      this.setTool(id);
      return { tool: id };
    },
    "tool.set_option": (p) => {
      const v = p.value;
      const int = (min: number) => Number.isInteger(v) && v >= min && v <= 1000;
      const ok: Record<string, boolean> = {
        sides: int(3),
        points: int(2),
        ratio: typeof v === "number" && v > 0 && v < 1,
        eraserWidth: typeof v === "number" && v > 0 && v <= 1000,
        warpMode: v === "4" || v === "mesh" || v === "envelope",
      };
      if (!ok[p.key]) throw new Error(`niepoprawna opcja ${p.key} = ${JSON.stringify(v)}`);
      this.toolOpts = { ...this.toolOpts, [p.key]: v };
      saveOptions(this.toolOpts);
      this.toolbars?.render();
      return {};
    },
    "snap.set": (p) => {
      if (!SNAP_LABELS.includes(p.key) || typeof p.enabled !== "boolean") throw new Error("snap.set: key i enabled");
      this.snapSettings = { ...this.snapSettings, [p.key]: p.enabled };
      saveSnap(this.snapSettings);
      this.toolbars?.render();
      return {};
    },
    "layer.set_active": (p) => {
      const l = p.layer;
      if (!Number.isInteger(l) || l < 0 || l > 255) throw new Error("layer: 0–255");
      this.layer = l;
      if (this.scene) this.panels.update(this.scene);
      return {};
    },
    "transform.set_anchor": (p) => {
      if (!["tl", "tc", "tr", "cl", "cc", "cr", "bl", "bc", "br"].includes(p.anchor)) throw new Error("anchor: tl…br");
      this.transform.anchor = p.anchor;
      this.onTransformState();
      return {};
    },
    "transform.lock_aspect": (p) => {
      if (typeof p.locked !== "boolean") throw new Error("locked: true/false");
      this.transform.lockAspect = p.locked;
      this.onTransformState();
      return {};
    },
    "job.cancel": () => {
      // gui.cancel omija kolejkę hosta zajętą przez anulowane polecenie
      void this.rpc.call("gui.cancel");
      return {};
    },
    "preview.open": (p) => this.preview.open(p),
    "preview.close": () => this.preview.close(),
    "layer.settings_open": (p) => this.layerSettings.open(p),
    "layer.settings_close": () => this.layerSettings.close(),
    "files.open": () => this.files.open(),
    "files.close": () => this.files.close(),
    "preview.set": (p) => this.preview.set(p),
    "jog.settings": (p) => this.machine.setJog(p),
    "dialog.open": (p) => {
      if (p.dialog === "settings") openSettings(this);
      else if (p.dialog === "shortcuts") openShortcuts(this);
      else throw new Error("dialog: settings | shortcuts");
      return {};
    },
  };

  /** Polecenie z rejestru; błąd trafia na pasek stanu. */
  async run(id: string, params: unknown = {}): Promise<unknown> {
    this.commandLog.push(id);
    try {
      const r = await this.rpc.execute(id, params);
      this.message = "";
      this.status();
      return r;
    } catch (e) {
      const err = e as RpcError;
      this.message = t("msg-cmd-error", { title: this.commands.get(id)?.title ?? id, error: err.message });
      this.status();
      return undefined;
    }
  }

  /** Pobiera scenę; kolejne `doc.changed` w trakcie łączone w jedno odświeżenie. */
  refresh(): Promise<void> {
    if (this.refreshing) {
      this.refreshAgain = true;
      return this.refreshing;
    }
    this.refreshing = (async () => {
      do {
        this.refreshAgain = false;
        // scena przyrostowa: bez geometrii obiektów o znanym skrócie (REQ-16-006)
        const have: Record<string, string> = {};
        for (const [id, o] of this.sceneCache) have[id] = o.hash;
        const s = await this.rpc.call<Scene>("gui.scene", { tolerance: 0.02, have, have_tree: this.treeHash });
        this.sceneBytes = this.rpc.lastBytes;
        // drzewo obiektów przychodzi tylko po zmianie struktury
        const sx = s as Scene & { tree?: unknown; tree_hash?: string };
        if (sx.tree === null || sx.tree === undefined) sx.tree = this.treeCache;
        else this.treeCache = sx.tree;
        this.treeHash = sx.tree_hash ?? "";
        let changed = s.objects.length !== this.sceneCache.size;
        s.objects = s.objects.map((o) => {
          if (o.same) {
            const cached = this.sceneCache.get(o.id);
            if (cached) return cached;
          }
          changed = true;
          return o;
        });
        const order = s.objects.map((o) => o.id).join(",");
        const layersKey = JSON.stringify(s.layers.map((l) => [l.index, l.color, l.show]));
        if (changed || order !== this.sceneOrder || layersKey !== this.sceneLayers) {
          this.sceneCache = new Map(s.objects.map((o) => [o.id, o]));
          this.sceneOrder = order;
          this.sceneLayers = layersKey;
          this.renderer?.upload(s);
        }
        this.scene = s;
        void this.loadKerf();
        this.preview.refresh();
        this.layerSettings.refresh();
        this.panels.update(s);
        this.renderTabs(s);
        this.tool.onScene?.();
        // pasek numeryczny i pasek węzłów zależą od zaznaczenia
        this.toolbars?.render();
        if (!this.fitted && this.view.width > 1) {
          this.fitted = true;
          this.zoomPage();
        }
        this.redraw();
      } while (this.refreshAgain);
    })().finally(() => (this.refreshing = null));
    return this.refreshing;
  }

  redraw() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.paint();
    });
  }

  /** Rysuje klatkę natychmiast (zrzut ekranu, testy). */
  paint() {
    this.measure();
    const d = this.drag;
    this.renderer?.draw(this.view, this.scene, {
      dragOffset: d?.kind === "move" ? d.offset : undefined,
      marquee: d?.kind === "marquee" ? d.rect : null,
      preview: [
        ...this.tool.preview(),
        ...(this.snapMark?.guides ?? []),
        ...(this.guideDrag?.at === undefined ? [] : [this.guideLine(this.guideDrag)]),
      ],
      markers: this.tool.markers?.() ?? [],
      snapMark: this.snapMark && this.snapMark.kind !== "none" && this.snapMark.kind !== "grid" ? this.snapMark : null,
      kerf: this.kerfFlat,
      head: this.machine.head,
    });
    drawRulers($("ruler-top"), $("ruler-left"), this.view, this.cursor);
    this.status();
  }

  /** Aktualny rozmiar kanwy (ResizeObserver zgłasza zmianę dopiero w następnej klatce). */
  private measure() {
    const r = this.canvas.getBoundingClientRect();
    const w = Math.max(1, r.width);
    const hgt = Math.max(1, r.height);
    const v = this.view;
    if (v.width > 1 && v.height > 1 && (w !== v.width || hgt !== v.height)) {
      // lewy górny róg zostaje w miejscu: zmiana rozmiaru kanwy (np. zawinięcie
      // paska narzędzi) nie przesuwa rysunku pod kursorem
      const [x0, y1] = v.toWorld(0, 0);
      v.cx = x0 + w / 2 / v.scale;
      v.cy = y1 - hgt / 2 / v.scale;
    }
    v.width = w;
    v.height = hgt;
  }

  private resize() {
    const wasEmpty = this.view.width <= 1 || this.view.height <= 1;
    this.measure();
    // pierwszy rzeczywisty rozmiar kanwy: dopasuj stronę
    if (wasEmpty && this.view.width > 1 && this.scene) this.zoomPage();
    this.paint();
  }

  zoomPage() {
    if (!this.scene) return;
    this.fitted = true;
    this.measure();
    const [w, hgt] = this.scene.workspace;
    this.view.fit([0, 0, w, hgt]);
    this.redraw();
  }

  zoomSelection() {
    const b = this.scene && selectionBox(this.scene);
    this.measure();
    if (b) this.view.fit(b, 48);
    this.redraw();
  }

  /** Zrzut obszaru roboczego z linijkami (REQ-15-026) jako PNG base64. */
  screenshot(): string {
    this.paint();
    const top = $<HTMLCanvasElement>("ruler-top");
    const left = $<HTMLCanvasElement>("ruler-left");
    const out = document.createElement("canvas");
    out.width = left.width + this.canvas.width;
    out.height = top.height + this.canvas.height;
    const g = out.getContext("2d")!;
    g.fillStyle = THEME.ruler.bg;
    g.fillRect(0, 0, out.width, out.height);
    g.drawImage(top, left.width, 0);
    g.drawImage(left, 0, top.height);
    g.drawImage(this.canvas, left.width, top.height);
    return out.toDataURL("image/png").replace(/^data:image\/png;base64,/, "");
  }

  // ---------- narzędzia i przyciąganie ----------

  setTool(id: string) {
    const tool = this.tools[id];
    if (!tool) return;
    this.tool = tool;
    this.canvas.style.cursor = toolCursor(tool.id, tool.cursor);
    this.snapMark = null;
    // narzędzie startuje od bieżącego zaznaczenia (wypaczanie, edycja węzłów)
    if (this.scene) tool.onScene?.();
    this.renderToolRow();
    this.redraw();
  }

  pickLayer(index: number) {
    if (this.scene?.selection.length) this.run("object.assign_layer", { layer: index });
    else this.exec("layer.set_active", { layer: index });
  }

  async batch(steps: { id: string; params: unknown }[]): Promise<any> {
    for (const s of steps) this.commandLog.push(s.id);
    try {
      const r = await this.rpc.call("gui.batch", { steps });
      this.message = "";
      return r;
    } catch (e) {
      this.message = t("msg-batch-error", { error: (e as RpcError).message });
      this.status();
      return undefined;
    }
  }

  snapWorld(x: number, y: number, opt: SnapOptions = {}): SnapResult {
    return snap(this.scene, this.view, x, y, this.snapSettings, opt);
  }

  selectObjects(ids: number[]) {
    if (this.scene) this.scene.selection = ids;
    this.run("selection.set", { ids });
  }

  refreshTools() {
    this.renderToolRow();
  }

  setToolOpt(key: "eraserWidth" | "warpMode", v: number | string) {
    this.exec("tool.set_option", { key, value: v });
  }

  /** Stan wypaczania (testy, diagnostyka). */
  warpState() {
    const t = this.tools.warp as WarpTool;
    return { grid: t.grid.map((g) => [g[0], g[1]]), cols: t.mode.cols, rows: t.mode.rows, bounds: t.bounds };
  }

  // ---------- prowadnice ręczne (przeciągnięcie z linijki) ----------

  private guideDrag: { axis: "h" | "v"; at?: number } | null = null;

  private guideLine(g: { axis: "h" | "v"; at?: number }): number[] {
    const [w, hgt] = this.scene?.workspace ?? [0, 0];
    return g.axis === "h" ? [0, g.at!, w, g.at!] : [g.at!, 0, g.at!, hgt];
  }

  private bindRulers() {
    const onMove = (e: PointerEvent) => {
      const g = this.guideDrag;
      if (!g) return;
      const r = this.canvas.getBoundingClientRect();
      const [x, y] = this.view.toWorld(e.clientX - r.left, e.clientY - r.top);
      const s = this.snapWorld(x, y);
      g.at = g.axis === "h" ? s.y : s.x;
      this.redraw();
    };
    const onUp = (e: PointerEvent) => {
      const g = this.guideDrag;
      this.guideDrag = null;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      this.redraw();
      const r = this.canvas.getBoundingClientRect();
      const over = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
      if (!g || g.at === undefined || !over) return;
      const [p0x, p0y, p1x, p1y] = this.guideLine(g);
      this.run("shape.guide", { p0: [p0x, p0y], p1: [p1x, p1y] });
    };
    for (const [id, axis] of [["ruler-top", "h"], ["ruler-left", "v"]] as const) {
      const el = $(id);
      el.style.cursor = axis === "h" ? "ns-resize" : "ew-resize";
      el.title = t("ruler-guide-hint");
      el.addEventListener("pointerdown", (e) => {
        if (e.button !== 0 || !this.scene) return;
        e.preventDefault();
        this.guideDrag = { axis };
        window.addEventListener("pointermove", onMove);
        window.addEventListener("pointerup", onUp);
      });
    }
  }

  /** Stan narzędzia węzłów (testy, diagnostyka). */
  nodeEdit() {
    const t = this.tools.node as NodeTool;
    return { id: t.edited, selected: [...t.selected].sort((a, b) => a - b), contours: t.contours };
  }

  nodeSelect(nodes: number[]) {
    (this.tools.node as NodeTool).select(nodes);
  }

  // ---------- menu kontekstowe ----------

  private menu: HTMLElement | null = null;

  showMenu(clientX: number, clientY: number, items: { action: string; label: string; run: () => void; testid?: string }[]) {
    this.hideMenu();
    const m = h(
      "div",
      { id: "ctxmenu", role: "menu", "data-testid": "ctxmenu", style: `left:${clientX}px;top:${clientY}px` },
      ...items.map((it) =>
        h("button", { role: "menuitem", title: it.label, "data-testid": it.testid ?? `ctx-${it.action}`, onclick: () => {
          this.hideMenu();
          it.run();
        } }, it.label),
      ),
    );
    document.body.append(m);
    // nie wychodź poza okno
    const r = m.getBoundingClientRect();
    if (r.right > window.innerWidth) m.style.left = `${Math.max(0, window.innerWidth - r.width - 4)}px`;
    if (r.bottom > window.innerHeight) m.style.top = `${Math.max(0, window.innerHeight - r.height - 4)}px`;
    this.menu = m;
  }

  private hideMenu() {
    this.menu?.remove();
    this.menu = null;
  }

  pickTolerance(): number {
    return SNAP_PX / this.view.scale;
  }

  async create(id: string, params: Record<string, unknown>) {
    await this.run(id, params);
  }

  /** Przyciąganie w punkcie kanwy (piksele CSS względem kanwy). */
  private snapLocal(sx: number, sy: number, opt: SnapOptions = {}): SnapResult {
    const [x, y] = this.view.toWorld(sx, sy);
    return snap(this.scene, this.view, x, y, this.snapSettings, { extraNodes: this.tool.extraNodes(), ...opt });
  }

  /** Przyciąganie w punkcie strony (testy, diagnostyka). */
  snapAt(clientX: number, clientY: number): SnapResult {
    const r = this.canvas.getBoundingClientRect();
    return this.snapLocal(clientX - r.left, clientY - r.top);
  }

  private local(e: MouseEvent | WheelEvent): [number, number] {
    const r = this.canvas.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  }

  private bindCanvas() {
    const c = this.canvas;
    c.addEventListener("contextmenu", (e) => e.preventDefault());
    c.addEventListener("wheel", (e) => {
      e.preventDefault();
      const [sx, sy] = this.local(e);
      // gest trackpada „pinch” przychodzi jako wheel z ctrlKey
      const k = Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0015));
      this.exec("view.zoom", { factor: k, at: this.view.toWorld(sx, sy) });
    }, { passive: false });
    // gesty dotykowe (REQ-03-019): dwa palce — powiększanie i przesuwanie widoku;
    // jeden palec działa jak lewy przycisk myszy
    const touches = new Map<number, [number, number]>();
    let gesture: { mid: [number, number]; dist: number } | null = null;
    let afterGesture = false;
    const twoFingers = () => {
      const [a, b] = [...touches.values()];
      return { mid: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2] as [number, number], dist: Math.hypot(b[0] - a[0], b[1] - a[1]) };
    };
    const touchCapture = (e: PointerEvent): boolean => {
      if (e.pointerType !== "touch") return false;
      const p = this.local(e);
      if (e.type === "pointerdown") {
        touches.set(e.pointerId, p);
        if (touches.size === 2) {
          // drugi palec przerywa rozpoczęte przeciąganie lub rysowanie
          this.drag = null;
          this.snapMark = null;
          if (this.tool.id !== "select") this.tool.key(new KeyboardEvent("keydown", { key: "Escape" }));
          gesture = twoFingers();
          afterGesture = true;
          this.redraw();
        }
        return touches.size > 1;
      }
      if (e.type === "pointermove") {
        if (!touches.has(e.pointerId)) return afterGesture;
        touches.set(e.pointerId, p);
        if (gesture && touches.size >= 2) {
          const g = twoFingers();
          this.exec("view.pan", { dx: (g.mid[0] - gesture.mid[0]) / this.view.scale, dy: -(g.mid[1] - gesture.mid[1]) / this.view.scale });
          if (gesture.dist > 1 && g.dist > 1) {
            this.exec("view.zoom", { factor: g.dist / gesture.dist, at: this.view.toWorld(g.mid[0], g.mid[1]) });
          }
          gesture = g;
          this.redraw();
        }
        return afterGesture;
      }
      // pointerup / pointercancel: po geście pozostały palec nic nie robi, dopóki nie zostanie podniesiony
      touches.delete(e.pointerId);
      if (touches.size < 2) gesture = null;
      const swallow = afterGesture;
      if (touches.size === 0) afterGesture = false;
      return swallow;
    };
    for (const type of ["pointerdown", "pointermove", "pointerup", "pointercancel"]) {
      c.addEventListener(type, (e) => {
        if (touchCapture(e as PointerEvent)) e.stopImmediatePropagation();
      });
    }
    c.addEventListener("pointerdown", (e) => {
      c.focus({ preventScroll: true });
      try {
        c.setPointerCapture(e.pointerId);
      } catch {
        // wskaźnik bez aktywnego kontaktu (np. syntetyczne zdarzenia) — przeciąganie działa i bez przechwycenia
      }
      const p = this.local(e);
      if (e.button === 2 && this.tool.context) {
        const items = this.tool.context(e);
        if (items) {
          this.showMenu(e.clientX, e.clientY, items);
          return;
        }
      }
      if (e.button === 1 || e.button === 2 || (e.button === 0 && e.altKey)) {
        this.drag = { kind: "pan", last: p };
        return;
      }
      if (e.button !== 0 || !this.scene) return;
      if (this.tool.id !== "select") {
        this.snapMark = this.snapLocal(p[0], p[1]);
        this.tool.down(this.snapMark, e);
        this.redraw();
        return;
      }
      const w = this.view.toWorld(p[0], p[1]);
      const hit = pick(this.scene, w[0], w[1], 4 / this.view.scale);
      if (hit) {
        const sel = this.scene.selection;
        if (e.shiftKey) {
          const ids = sel.includes(hit.id) ? sel.filter((i) => i !== hit.id) : [...sel, hit.id];
          this.run("selection.set", { ids });
          return;
        }
        const ids = sel.includes(hit.id) ? [...sel] : [hit.id];
        if (!sel.includes(hit.id)) {
          this.scene.selection = ids; // natychmiastowa odpowiedź; potwierdza doc.changed
          this.run("selection.set", { ids });
        }
        // chwycony punkt przyciąga się do cech obiektów (bez siatki — swobodne przesuwanie)
        const grab = this.snapLocal(p[0], p[1], { grid: false });
        const start: [number, number] = grab.kind === "none" ? w : [grab.x, grab.y];
        this.drag = { kind: "move", ids, start, offset: [0, 0], moved: false };
      } else {
        this.drag = { kind: "marquee", start: w, rect: [w[0], w[1], w[0], w[1]], add: e.shiftKey };
      }
      this.redraw();
    });
    c.addEventListener("pointermove", (e) => {
      const p = this.local(e);
      const w = this.view.toWorld(p[0], p[1]);
      this.cursor = w;
      const d = this.drag;
      if (d?.kind === "pan") {
        this.exec("view.pan", { dx: (p[0] - d.last[0]) / this.view.scale, dy: -(p[1] - d.last[1]) / this.view.scale });
        d.last = p;
      } else if (this.tool.id !== "select") {
        this.snapMark = this.snapLocal(p[0], p[1]);
        this.tool.move(this.snapMark, e, (e.buttons & 1) === 1);
      } else if (d?.kind === "move") {
        const raw = Math.hypot(w[0] - d.start[0], w[1] - d.start[1]) * this.view.scale;
        const t = this.snapLocal(p[0], p[1], { grid: false, exclude: new Set(d.ids) });
        this.snapMark = t;
        const target = t.kind === "none" ? w : [t.x, t.y];
        d.offset = [target[0] - d.start[0], target[1] - d.start[1]];
        if (raw > 2) d.moved = true;
      } else if (d?.kind === "marquee") {
        d.rect = [d.start[0], d.start[1], w[0], w[1]];
      }
      this.redraw();
    });
    const end = async (e: PointerEvent) => {
      const d = this.drag;
      this.drag = null;
      if (c.hasPointerCapture(e.pointerId)) c.releasePointerCapture(e.pointerId);
      if (this.tool.id !== "select" && d?.kind !== "pan" && e.button === 0) {
        const p = this.local(e);
        this.tool.up(this.snapLocal(p[0], p[1]), e);
        return this.redraw();
      }
      this.snapMark = null;
      if (!d || !this.scene) return this.redraw();
      if (d.kind === "move" && d.moved) {
        const [dx, dy] = d.offset;
        // podgląd przesunięcia zostaje do nadejścia nowej sceny
        this.drag = { ...d };
        await this.run("object.transform", { ids: d.ids, translate: [dx, dy] });
        await this.refresh();
        this.drag = null;
      } else if (d.kind === "marquee") {
        const r = d.rect;
        const small = Math.abs(r[2] - r[0]) * this.view.scale < 3 && Math.abs(r[3] - r[1]) * this.view.scale < 3;
        if (small) {
          if (!d.add && this.scene.selection.length) this.run("selection.clear");
        } else {
          const ids = inRect(this.scene, r);
          this.run("selection.set", { ids: d.add ? [...new Set([...this.scene.selection, ...ids])] : ids });
        }
      }
      this.redraw();
    };
    c.addEventListener("pointerup", end);
    c.addEventListener("pointercancel", end);
    c.addEventListener("dblclick", (e) => this.tool.dblclick?.(e));
    // klik poza menu zamyka je
    window.addEventListener("pointerdown", (e) => {
      if (this.menu && !this.menu.contains(e.target as Node)) this.hideMenu();
    }, true);
    c.addEventListener("pointerleave", () => {
      this.cursor = null;
      this.redraw();
    });
  }

  /** Skróty z rejestru dla poleceń bez wymaganych parametrów (edytor skrótów — M1.4b). */
  private bindKeys() {
    window.addEventListener("keydown", (e) => {
      if (this.modal) return;
      const target = e.target as HTMLElement;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT")) return;
      if (this.menu && e.key === "Escape") {
        e.preventDefault();
        this.hideMenu();
        return;
      }
      // narzędzie ma pierwszeństwo (wpisywanie długości, Enter, Esc)
      if (this.tool.key(e)) {
        e.preventDefault();
        return;
      }
      if (e.key === "Escape" && this.tool.id !== "select") {
        e.preventDefault();
        this.exec("tool.select", { tool: "select" });
        return;
      }
      const combo = keyCombo(e);
      const nudge: Record<string, [number, number]> = {
        ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1],
      };
      if (nudge[e.key] && this.scene?.selection.length) {
        e.preventDefault();
        const k = e.shiftKey ? 10 : 1;
        this.run("object.transform", { translate: [nudge[e.key][0] * k, nudge[e.key][1] * k] });
        return;
      }
      // skróty obowiązujące (domyślne z rejestru albo ustawione w edytorze skrótów)
      for (const [id, sc] of this.shortcuts) {
        const c = this.commands.get(id);
        if (!sc || !c || (c.params_schema.required?.length ?? 0) > 0) continue;
        if (normalizeShortcut(sc) === combo) {
          e.preventDefault();
          this.run(id);
          return;
        }
      }
    });
  }

  /** Definicje pasków (REQ-03-020…024, 029, 030); układ i dokowanie — `Toolbars`. */
  private toolbarDefs(): ToolbarDef[] {
    const cmd = (id: string, labelKey: string, params: unknown = {}, testid?: string) => {
      const c = this.commands.get(id);
      const sc = this.shortcuts.get(id);
      const title = `${c?.title ?? id}${sc ? ` (${sc})` : ""}`;
      return h("button", { title, "aria-label": title, "data-command": id, "data-testid": testid, onclick: () => this.run(id, params) }, t(labelKey));
    };
    // polecenie okna (widok, okna dialogowe) — wykonuje je to okno (REQ-01-029)
    const win = (id: string, key: string, params: unknown, testid: string) => {
      const sc = this.shortcuts.get(id);
      const base = t(`${key}.title`) === `${key}.title` ? t(key) : t(`${key}.title`);
      const title = `${base}${sc ? ` (${sc})` : ""}`;
      return h("button", { title, "aria-label": title, "data-command": id, "data-testid": testid, onclick: () => this.exec(id, params) }, t(key));
    };
    const pathCmd = (id: string, key: string, ask: string) =>
      h("button", { title: this.commands.get(id)?.title ?? id, "data-command": id, "data-testid": `tb-${id}`, onclick: () => {
        const path = prompt(t(ask));
        if (path) this.run(id, { path });
      } }, t(key));
    const sep = () => h("span", { class: "sep" });
    const mainBar = (): HTMLElement[] => [
      h("span", { class: "brand" }, "BurnCraft"),
      cmd("document.new", "tb-new"),
      pathCmd("file.open", "tb-open", "ask-open"),
      pathCmd("file.import", "tb-import", "ask-import"),
      pathCmd("file.save", "tb-save", "ask-save"),
      pathCmd("gcode.export", "tb-export-gcode", "ask-export-gcode"),
      sep(),
      cmd("edit.undo", "tb-undo"),
      cmd("edit.redo", "tb-redo"),
      cmd("object.delete", "tb-delete"),
      sep(),
      win("view.zoom_page", "tb-zoom-page", {}, "zoom-page"),
      win("view.zoom_selection", "tb-zoom-selection", {}, "zoom-selection"),
      win("view.actual_size", "tb-zoom-100", {}, "zoom-100"),
      win("view.zoom", "tb-zoom-in", { factor: 1.25 }, "zoom-in"),
      win("view.zoom", "tb-zoom-out", { factor: 0.8 }, "zoom-out"),
      sep(),
      win("preview.open", "tb-preview", {}, "tb-preview"),
      sep(),
      win("dialog.open", "tb-settings", { dialog: "settings" }, "open-settings"),
      win("dialog.open", "tb-shortcuts", { dialog: "shortcuts" }, "open-shortcuts"),
    ];
    const createBar = (): HTMLElement[] =>
      TOOLS.filter((tl) => !(this.beginner && ADVANCED_TOOLS.has(tl.id))).map((tl) =>
        h("button", {
          class: "tool", title: t(`tool-${tl.id}`), "aria-label": t(`tool-${tl.id}`), "data-testid": `tool-${tl.id}`,
          "aria-pressed": String(this.tool.id === tl.id), "data-command": "tool.select", onclick: () => this.exec("tool.select", { tool: tl.id }),
        }, tl.label),
      );
    const modesBar = (): HTMLElement[] => {
      const opt = (label: string, key: keyof ToolOptions, testid: string, parse: (v: number) => number | null) => {
        const input = h("input", { type: "text", inputmode: "decimal", class: "cell", value: String(this.toolOpts[key]), "data-testid": testid, "data-command": "tool.set_option", "aria-label": label, title: label });
        const commit = () => {
          const v = parse(Number(input.value.replace(",", ".")));
          if (v === null) {
            input.value = String(this.toolOpts[key]);
            return;
          }
          this.exec("tool.set_option", { key, value: v });
          input.value = String(this.toolOpts[key]);
        };
        input.addEventListener("keydown", (e: KeyboardEvent) => {
          e.stopPropagation();
          if (e.key === "Enter") commit();
        });
        input.addEventListener("change", commit);
        return h("label", { class: "opt", title: label }, label, input);
      };
      const count = (min: number) => (v: number) => (Number.isInteger(v) && v >= min && v <= 1000 ? v : null);
      const options: HTMLElement[] = this.tool.options?.() ?? [];
      if (this.tool.id === "polygon") options.push(opt(t("opt-sides"), "sides", "opt-sides", count(3)));
      if (this.tool.id === "star") {
        options.push(opt(t("opt-points"), "points", "opt-points", count(2)));
        options.push(opt(t("opt-ratio"), "ratio", "opt-ratio", (v) => (v > 0 && v < 1 ? v : null)));
      }
      const snaps = SNAP_LABELS.map((k) => {
        const label = t(`snaplbl-${k}`);
        const title = t("snap-toggle", { what: label.toLowerCase() });
        return h("label", { class: "opt", title },
          h("input", { type: "checkbox", checked: this.snapSettings[k], "data-testid": `snap-${k}`, "data-command": "snap.set", "aria-label": label, title,
            onchange: (e: Event) => this.exec("snap.set", { key: k, enabled: (e.target as HTMLInputElement).checked }) }),
          label);
      });
      return [...options, ...(options.length ? [sep()] : []), h("span", { class: "muted" }, t("snap-prefix")), ...snaps];
    };
    const numericBar = (): HTMLElement[] => {
      const tf = this.scene && transformFields(this, this.scene, this.transform, "num");
      return tf ? [tf.anchor, ...tf.fields, tf.lock] : disabledTransformFields("num");
    };
    const modifiersBar = (): HTMLElement[] => [
      ...(["union", "difference", "intersection", "xor", "cut"] as const).map((op) => cmd("path.boolean", `mod-${op}`, { op }, `mod-${op}`)),
      sep(),
      cmd("object.group", "tb-group", {}, "mod-group"),
      cmd("object.ungroup", "tb-ungroup", {}, "mod-ungroup"),
      cmd("path.convert", "mod-convert", {}, "mod-convert"),
      cmd("path.break_apart", "mod-break", {}, "mod-break"),
      cmd("path.close", "mod-close", {}, "mod-close"),
      cmd("path.join", "mod-join", {}, "mod-join"),
    ];
    const alignBar = (): HTMLElement[] => [
      ...(["left", "hcenter", "right", "top", "vcenter", "bottom"] as const).map((mode) =>
        cmd("object.align", `align-${mode}`, { mode, to: "selection" }, `align-${mode}`)),
      sep(),
      cmd("object.distribute", "dist-h", { axis: "horizontal", by: "gaps" }, "dist-h"),
      cmd("object.distribute", "dist-v", { axis: "vertical", by: "gaps" }, "dist-v"),
      sep(),
      cmd("object.align_to_laser", "align-laser", {}, "align-laser"),
    ];
    const nodeBar = (): HTMLElement[] => {
      const acts = this.tool.id === "node" ? (this.tools.node as NodeTool).actions() : {};
      return ["delete", "smooth", "corner", "break", "join", "align-y", "align-x"].map((a) => {
        const f = (acts as Record<string, (() => void) | null>)[a] ?? null;
        const id = a.startsWith("align") ? "node.align" : `node.${a}`;
        return h("button", { title: t(`ctx-${a}`), "aria-label": t(`ctx-${a}`), "data-testid": `nodebar-${a}`, "data-command": id, disabled: !f, onclick: () => f?.() }, t(`nodebar-${a}`));
      });
    };
    return [
      { id: "main", build: mainBar },
      { id: "create", build: createBar },
      { id: "modes", fixed: true, build: modesBar },
      { id: "numeric", build: numericBar },
      { id: "modifiers", advanced: true, build: modifiersBar },
      { id: "align", build: alignBar },
      { id: "node", advanced: true, build: nodeBar },
    ];
  }

  /** Przebudowa pasków (zmiana narzędzia, zaznaczenia, preferencji). */
  private buildToolbar() {
    if (!this.commands.size) return;
    this.toolbars ??= new Toolbars(this, this.toolbarDefs());
    this.toolbars.render();
  }

  private renderToolRow() {
    this.toolbars?.render();
  }

  private onTransformState() {
    if (this.scene) this.panels.update(this.scene);
    this.toolbars?.render();
  }

  private renderTabs(s: Scene) {
    const tabs = s.docs.map((d, i) =>
      h(
        "div",
        { class: `tab${i === s.active ? " active" : ""}`, "data-testid": `tab-${i}` },
        h("button", { class: "tab-name", title: d.path ?? d.name, onclick: () => i !== s.active && this.run("doc.switch", { index: i }) },
          `${d.name}${d.dirty ? " •" : ""}`),
        h("button", { class: "tab-close", title: t("tab-close"), "aria-label": t("tab-close"), onclick: async () => {
          const r = await this.rpc.execute("doc.close", { index: i }).catch((e: RpcError) => e);
          if (r instanceof RpcError && r.data?.code === "unsaved_changes" && confirm(t("tab-discard", { name: d.name }))) {
            this.run("doc.close", { index: i, force: true });
          }
        } }, "×"),
      ),
    );
    $("tabs").replaceChildren(...tabs, h("button", { class: "tab-new", title: t("tab-new"), "aria-label": t("tab-new"), onclick: () => this.run("doc.new") }, "+"));
  }

  status() {
    const parts = [
      t(this.connected ? "status-connected" : "status-disconnected"),
      this.cursor ? `X ${fmtLen(this.cursor[0])}  Y ${fmtLen(this.cursor[1])} ${getUnit()}` : "",
      `${Math.round(this.view.zoomPercent)}%`,
      this.scene ? t("status-selected", { n: this.scene.selection.length }) : "",
      this.snapMark && this.snapMark.kind !== "none" ? t("status-snap", { kind: t(`snap-${this.snapMark.kind}`) }) : "",
    ].filter(Boolean);
    const input = this.tool.input();
    const el = $("status");
    const pr = this.progress;
    const progress: HTMLElement[] = [];
    if (pr) {
      // jeden trwały element: przycisk Anuluj nie jest podmieniany w trakcie kliknięcia
      this.progressUi ??= {
        root: h("span", { class: "progress", "data-testid": "progress" }),
        bar: h("progress", { max: "1" }) as HTMLProgressElement,
        label: h("span", {}),
        cancel: h("button", { "data-testid": "progress-cancel", "data-command": "job.cancel", onclick: () => this.exec("job.cancel") }),
      };
      const u = this.progressUi;
      const title = this.commands.get(pr.command)?.title ?? pr.command;
      u.root.dataset.command = pr.command;
      u.bar.value = pr.fraction;
      u.bar.setAttribute("aria-label", title);
      u.label.textContent = t("status-progress", { command: title, percent: Math.round(pr.fraction * 100) });
      u.cancel.textContent = t("progress-cancel");
      u.cancel.title = t("progress-cancel");
      if (u.root.childElementCount === 0) u.root.append(u.bar, u.label, u.cancel);
      progress.push(u.root);
    }
    el.replaceChildren(
      h("span", { "data-testid": "status-text" }, parts.join("   ")),
      ...progress,
      ...(input === null ? [] : [h("span", { class: "tool-input", title: t("status-input-hint"), "data-testid": "tool-input" }, input)]),
      h("span", { class: "error", "data-testid": "status-error" }, this.message),
    );
    el.dataset.connected = String(this.connected);
  }
}

function createRenderer(canvas: HTMLCanvasElement): Renderer | null {
  try {
    return new Renderer(canvas);
  } catch (e) {
    console.error(e);
    return null;
  }
}

/** Liczba skończona z parametru polecenia; inaczej wyjątek. */
function finite(v: unknown): number {
  const n = Number(v);
  if (typeof v !== "number" || !Number.isFinite(n)) throw new Error("wartość musi być liczbą");
  return n;
}

/** Kolor #rrggbb z preferencji, inaczej domyślny. */
function validColor(v: unknown, def: string): string {
  return typeof v === "string" && /^#[0-9a-f]{6}$/i.test(v) ? v.toLowerCase() : def;
}

/** Kalibracja ekranu z preferencji: liczba z zakresu 0,25…4, inaczej 1. */
function validScale(v: unknown): number {
  const n = Number(v);
  return Number.isFinite(n) && n >= 0.25 && n <= 4 ? n : 1;
}

function normalizeShortcut(s: string): string {
  const parts = s.split("+").map((p) => p.trim());
  const key = parts.pop() ?? "";
  const mods = new Set(parts.map((p) => (p === "Cmd" || p === "Meta" ? "Ctrl" : p)));
  return [mods.has("Ctrl") ? "Ctrl" : "", mods.has("Shift") ? "Shift" : "", mods.has("Alt") ? "Alt" : "",
    key.length === 1 ? key.toUpperCase() : key === "Del" ? "Delete" : key === "Esc" ? "Escape" : key]
    .filter(Boolean)
    .join("+");
}

const app = new App();
(window as any).burncraft = app;
app.start();
