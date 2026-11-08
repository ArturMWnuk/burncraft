// Panele boczne: Warstwy, Obiekty (drzewo), Właściwości kształtu, Historia, Notatki.
// Każda zmiana idzie przez polecenie z rejestru (REQ-01-029, REQ-01-036).

import { h, num, parseNum } from "./dom";
import { t } from "./i18n";
import { DeviceInfo, Layer, Scene, noPwm, selectionBox } from "./scene";
import { fmtLen, parseLen } from "./units";

export interface Host {
  run(id: string, params?: unknown): Promise<unknown>;
  /** Polecenie z rejestru: okna wykonuje okno, pozostałe host (REQ-01-029). */
  exec(id: string, params?: unknown): unknown;
  scene: Scene | null;
  /** Aktywna warstwa dla nowych kształtów. */
  layer: number;
  /** Klik w paletę: przypisz zaznaczenie albo ustaw aktywną warstwę. */
  pickLayer(index: number): void;
}

/** Węzeł drzewa obiektów z `gui.scene` (REQ-02-040). */
export interface TreeNode {
  id: number;
  name: string | null;
  type: string;
  layer: number;
  visible: boolean;
  locked: boolean;
  helper: boolean;
  tags?: string[];
  note?: string | null;
  children: TreeNode[];
}

/** Wysokość wiersza drzewa obiektów (px) — wirtualizacja listy. */
const TREE_ROW = 22;

/** Parametry kształtów: liczby całkowite i kąty (reszta to długości w mm). */
const COUNT_PARAMS = new Set(["sides", "points"]);
const ANGLE_PARAMS = new Set(["rotation", "start", "sweep"]);
const POSITION_PARAMS = new Set(["x", "y", "cx", "cy"]);
/** Kolejność pól w panelu (host przysyła je alfabetycznie). */
const PARAM_ORDER = ["w", "h", "radius", "rx", "ry", "r", "r_outer", "r_middle", "r_inner", "sides", "points", "rotation", "start", "sweep"];

const MODES = ["line", "fill", "offset_fill", "image", "fill_line", "flood_fill"];

/** Kolumny warstw ukryte w trybie początkującego (REQ-03-051). */
const ADVANCED_COLS = new Set(["layers-col-passes"]);

/** Punkt odniesienia 3×3 (REQ-03-025): ułamki szerokości i wysokości ramki. */
const ANCHORS: Record<string, [number, number]> = {
  tl: [0, 1], tc: [0.5, 1], tr: [1, 1],
  cl: [0, 0.5], cc: [0.5, 0.5], cr: [1, 0.5],
  bl: [0, 0], bc: [0.5, 0], br: [1, 0],
};

function section(id: string, titleKey: string): { root: HTMLElement; body: HTMLElement; head: HTMLButtonElement } {
  const body = h("div", { class: "panel-body" });
  const head = h("button", { class: "panel-head", "aria-expanded": "true", title: t("objects-toggle") }, t(titleKey));
  const root = h("section", { class: "panel", id: `panel-${id}`, "data-testid": `panel-${id}` }, head, body);
  head.addEventListener("click", () => {
    const open = root.classList.toggle("collapsed");
    head.setAttribute("aria-expanded", String(!open));
  });
  return { root, body, head };
}

export function objLabel(o: { name: string | null; type: string; id: number }): string {
  const key = `obj-${o.type}`;
  const name = t(key);
  return o.name || `${name === key ? o.type : name} #${o.id}`;
}

/**
 * Pole liczbowe zatwierdzane Enterem lub opuszczeniem. `commit` zwraca tekst
 * do wyświetlenia (np. „25,4mm” → „1” w calach) albo `null` przy błędnej wartości.
 */
function numField(
  label: string,
  testid: string,
  value: string,
  commit: (text: string) => string | null,
  disabled = false,
) {
  const input = h("input", {
    type: "text", inputmode: "decimal", "data-testid": testid, "aria-label": label, title: label,
    value, disabled,
  });
  let last = input.value;
  const fire = () => {
    if (input.value === last) return;
    const shown = commit(input.value);
    input.value = shown ?? last;
    last = input.value;
  };
  input.addEventListener("keydown", (e: KeyboardEvent) => {
    if (e.key === "Enter") fire();
    if (e.key === "Escape") input.value = last;
    e.stopPropagation();
  });
  input.addEventListener("blur", fire);
  return h("label", { class: "field" }, h("span", {}, label), input);
}

/** Wspólny stan pól przekształcenia: panel Właściwości i pasek numeryczny. */
export interface TransformState {
  /** Punkt odniesienia: tl, tc, tr, cl, cc, cr, bl, bc, br. */
  anchor: string;
  lockAspect: boolean;
}

/**
 * Pola X/Y/szerokość/wysokość/obrót zaznaczenia w bieżących jednostkach
 * (REQ-03-024…028); `prefix` rozróżnia identyfikatory testów (prop, num).
 * `null`, gdy nic nie jest zaznaczone.
 */
export function transformFields(
  host: Host,
  s: Scene,
  st: TransformState,
  prefix: string,
): { fields: HTMLElement[]; lock: HTMLElement; anchor: HTMLElement } | null {
  const b = selectionBox(s);
  if (!b) return null;
  const [x0, y0, x1, y1] = b;
  const w = x1 - x0;
  const hgt = y1 - y0;
  const [fx, fy] = ANCHORS[st.anchor] ?? ANCHORS.bl;
  const ax = x0 + fx * w;
  const ay = y0 + fy * hgt;
  const tr = (p: Record<string, unknown>) => host.run("object.transform", p);
  const scaleTo = (nw: number, nh: number) => {
    const sx = w > 1e-9 ? nw / w : 1;
    const sy = hgt > 1e-9 ? nh / hgt : 1;
    tr({ scale: [sx, sy], anchor: [ax, ay] });
  };
  const len = (fn: (mm: number) => unknown) => (text: string) => {
    const v = parseLen(text);
    if (v === null) return null;
    fn(v);
    return fmtLen(v);
  };
  const lockInput = h("input", { type: "checkbox", checked: st.lockAspect, "data-testid": `${prefix}-lock`, title: t("props-lock"), "aria-label": t("props-lock"),
    "data-command": "transform.lock_aspect",
    onchange: (e: Event) => host.exec("transform.lock_aspect", { locked: (e.target as HTMLInputElement).checked }) });
  const anchor = h("div", { class: "anchor", role: "radiogroup", title: t("props-anchor"), "aria-label": t("props-anchor") },
    ...Object.keys(ANCHORS).map((k) =>
      h("button", { "data-testid": prefix === "prop" ? `anchor-${k}` : `${prefix}-anchor-${k}`, "aria-pressed": String(k === st.anchor), title: `${t("props-anchor")}: ${k}`,
        "data-command": "transform.set_anchor", onclick: () => host.exec("transform.set_anchor", { anchor: k }) }),
    ),
  );
  const tag = (el: HTMLElement) => {
    el.querySelectorAll<HTMLElement>("input").forEach((x) => (x.dataset.command = "object.transform"));
    return el;
  };
  return {
    fields: [
      numField(t("props-x"), `${prefix}-x`, fmtLen(ax), len((v) => tr({ translate: [v - ax, 0] }))),
      numField(t("props-y"), `${prefix}-y`, fmtLen(ay), len((v) => tr({ translate: [0, v - ay] }))),
      numField(t("props-w"), `${prefix}-w`, fmtLen(w), len((v) => scaleTo(v, st.lockAspect && w > 1e-9 ? (hgt * v) / w : hgt)), w < 1e-9),
      numField(t("props-h"), `${prefix}-h`, fmtLen(hgt), len((v) => scaleTo(st.lockAspect && hgt > 1e-9 ? (w * v) / hgt : w, v)), hgt < 1e-9),
      numField(t("props-rot"), `${prefix}-rot`, "0", (text) => {
        const v = parseNum(text);
        if (v === null) return null;
        if (v !== 0) tr({ rotate: v, anchor: [ax, ay] });
        return "0";
      }),
    ].map(tag),
    lock: h("label", { class: "field check", title: t("props-lock") }, lockInput, h("span", {}, t("props-lock"))),
    anchor,
  };
}

/**
 * Te same pola bez zaznaczenia — wyłączone. Pasek numeryczny ma stały układ,
 * więc zmiana zaznaczenia nie przesuwa kanwy pod kursorem.
 */
export function disabledTransformFields(prefix: string): HTMLElement[] {
  const field = (key: string, id: string) =>
    h("label", { class: "field" }, h("span", {}, t(key)),
      h("input", { type: "text", disabled: true, "data-testid": `${prefix}-${id}`, "data-command": "object.transform", title: t(key), "aria-label": t(key) }));
  return [
    h("div", { class: "anchor", title: t("props-anchor") },
      ...Object.keys(ANCHORS).map((k) => h("button", { disabled: true, "data-command": "transform.set_anchor", title: `${t("props-anchor")}: ${k}` }))),
    field("props-x", "x"), field("props-y", "y"), field("props-w", "w"), field("props-h", "h"), field("props-rot", "rot"),
    h("label", { class: "field check", title: t("props-lock") },
      h("input", { type: "checkbox", disabled: true, "data-command": "transform.lock_aspect", title: t("props-lock"), "aria-label": t("props-lock") }), h("span", {}, t("props-lock"))),
  ];
}

export class Panels {
  private layers = section("layers", "panel-layers");
  private objects = section("objects", "panel-objects");
  private props = section("properties", "panel-properties");
  private history = section("history", "panel-history");
  private notes = section("notes", "panel-notes");
  private notesArea = h("textarea", { "data-testid": "notes", rows: 5 });
  private notesTimer = 0;
  private imageInput = h("input", { type: "file", accept: "image/png,image/jpeg", multiple: true, class: "hidden-input", "data-testid": "notes-image-input" });
  private imageButton = h("button", { "data-testid": "notes-image-add", onclick: () => this.imageInput.click() });
  private imageBar = h("div", { class: "notes-bar" }, this.imageButton, this.imageInput);
  private gallery = h("div", { class: "note-images", "data-testid": "note-images" });
  /** Treść obrazów notatek: nazwa:rozmiar → data URL. */
  private imageCache = new Map<string, string>();
  private galleryKey = "";
  private notesFromHost = false;
  private collapsed = new Set<number>();
  private query = "";
  private search = h("input", { type: "search", class: "search", "data-testid": "objects-search" });
  private treeBox = h("div", {});
  /** Położenie węzłów drzewa: rodzic i indeks w kolejności rysowania. */
  private place = new Map<number, { parent: number | null; index: number; group: boolean }>();
  private dragged: number | null = null;

  constructor(
    root: HTMLElement,
    private host: Host,
    /** Punkt odniesienia i blokada proporcji wspólne z paskiem numerycznym. */
    private tf: TransformState,
  ) {
    root.append(this.layers.root, this.objects.root, this.props.root, this.history.root, this.notes.root);
    this.notes.body.append(this.notesArea, this.imageBar, this.gallery);
    this.notesArea.addEventListener("keydown", (e) => e.stopPropagation());
    this.notesArea.addEventListener("input", () => {
      this.notesFromHost = false;
      clearTimeout(this.notesTimer);
      this.notesTimer = window.setTimeout(() => this.flushNotes(), 400);
    });
    // obraz wklejony ze schowka albo upuszczony na notatki (REQ-03-049)
    this.notesArea.addEventListener("paste", (e: ClipboardEvent) => {
      const files = [...(e.clipboardData?.files ?? [])].filter((f) => f.type.startsWith("image/"));
      if (!files.length) return;
      e.preventDefault();
      void this.addImages(files);
    });
    this.notes.body.addEventListener("dragover", (e: DragEvent) => {
      if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
    });
    this.notes.body.addEventListener("drop", (e: DragEvent) => {
      const files = [...(e.dataTransfer?.files ?? [])].filter((f) => f.type.startsWith("image/"));
      if (!files.length) return;
      e.preventDefault();
      void this.addImages(files);
    });
    this.imageInput.addEventListener("change", () => {
      const files = [...(this.imageInput.files ?? [])];
      this.imageInput.value = "";
      void this.addImages(files);
    });
    this.search.addEventListener("keydown", (e) => e.stopPropagation());
    this.search.addEventListener("input", () => {
      this.query = this.search.value.trim().toLowerCase();
      if (this.host.scene) this.renderTree(this.host.scene);
    });
    const order = (to: string, key: string) =>
      h("button", { title: t(key), "aria-label": t(key), "data-testid": `order-${key.slice(6)}`, onclick: () => this.host.run("object.order", { to }) },
        { front: "⤒", forward: "↑", backward: "↓", back: "⤓" }[to]!);
    this.orderBar = h("div", { class: "order-bar" },
      order("front", "order-top"), order("forward", "order-up"), order("backward", "order-down"), order("back", "order-bottom"));
    this.objects.body.append(this.search, this.orderBar, this.treeBox);
    this.retranslate();
  }

  private orderBar: HTMLElement;

  /** Teksty po zmianie języka. */
  retranslate() {
    const heads: [ReturnType<typeof section>, string][] = [
      [this.layers, "panel-layers"], [this.objects, "panel-objects"], [this.props, "panel-properties"],
      [this.history, "panel-history"], [this.notes, "panel-notes"],
    ];
    for (const [s, k] of heads) {
      s.head.textContent = t(k);
      s.head.title = t("objects-toggle");
    }
    this.search.placeholder = t("objects-search");
    this.search.title = t("objects-search");
    this.notesArea.setAttribute("aria-label", t("notes-label"));
    this.notesArea.title = t("notes-label");
    this.imageButton.textContent = t("notes-image-add");
    this.imageButton.title = t("notes-image-add-hint");
    this.imageInput.title = t("notes-image-add");
    this.imageInput.setAttribute("aria-label", t("notes-image-add"));
    this.galleryKey = "";
    for (const b of this.orderBar.querySelectorAll("button")) {
      const key = `order-${b.dataset.testid!.slice(6)}`;
      b.title = t(key);
      b.setAttribute("aria-label", t(key));
    }
  }

  update(s: Scene) {
    this.renderLayers(s);
    this.renderTree(s);
    this.renderProps(s);
    this.renderHistory(s);
    // po operacji na obrazach tekst z hosta wygrywa także przy aktywnym polu (zmienione odwołania)
    if ((this.notesFromHost || document.activeElement !== this.notesArea) && this.notesArea.value !== s.notes) {
      this.notesArea.value = s.notes;
      this.notesFromHost = false;
    }
    void this.renderGallery(s);
  }

  // ---------- obrazy notatek (REQ-03-049) ----------

  /** Zapisuje oczekującą zmianę tekstu notatek (przed operacją na obrazach). */
  private async flushNotes() {
    clearTimeout(this.notesTimer);
    this.notesTimer = 0;
    if (this.host.scene && this.notesArea.value !== this.host.scene.notes) {
      await this.host.run("document.set_notes", { text: this.notesArea.value });
    }
  }

  private async addImages(files: File[]) {
    await this.flushNotes();
    for (const f of files) {
      const url = await new Promise<string>((ok, fail) => {
        const r = new FileReader();
        r.onload = () => ok(String(r.result));
        r.onerror = () => fail(r.error);
        r.readAsDataURL(f);
      });
      this.notesFromHost = true;
      await this.host.run("document.add_note_image", { data_base64: url.slice(url.indexOf(",") + 1) });
    }
  }

  private async renderGallery(s: Scene) {
    const list = s.note_images ?? [];
    const key = list.map((i) => `${i.name}:${i.size}`).join("|");
    if (key === this.galleryKey) return;
    this.galleryKey = key;
    const items = await Promise.all(list.map(async (img, i) => {
      const ck = `${img.name}:${img.size}`;
      let src = this.imageCache.get(ck);
      if (!src) {
        const r = (await this.host.run("document.note_image", { name: img.name })) as { mime: string; data_base64: string } | undefined;
        src = r ? `data:${r.mime};base64,${r.data_base64}` : "";
        if (src) this.imageCache.set(ck, src);
      }
      return h("figure", { class: "note-image", "data-testid": `note-image-${i}`, title: img.name },
        h("img", { src, alt: img.name }),
        h("figcaption", {}, img.name,
          h("button", { class: "icon", "data-testid": `note-image-${i}-remove`, title: t("notes-image-remove"), "aria-label": t("notes-image-remove"),
            onclick: async () => {
              await this.flushNotes();
              this.notesFromHost = true;
              await this.host.run("document.remove_note_image", { name: img.name });
            } }, "✕")));
    }));
    if (key === this.galleryKey) this.gallery.replaceChildren(...items);
  }

  // ---------- warstwy ----------

  private renderLayers(s: Scene) {
    const used = new Set(s.objects.map((o) => o.layer));
    for (const o of s.objects) for (const l of o.polyline_layers) used.add(l);
    const palette = h(
      "div",
      { class: "palette", role: "toolbar", "aria-label": t("panel-layers") },
      ...s.layers
        .filter((l) => l.index < 30)
        .map((l) =>
          h("button", {
            class: `swatch${l.index === this.host.layer ? " active" : ""}`, style: `background:${l.color}`,
            title: t("layers-palette", { name: l.name }),
            "data-testid": `palette-${l.index}`, "aria-pressed": String(l.index === this.host.layer),
            // z zaznaczeniem przypisuje warstwę obiektom, bez — ustawia warstwę nowych kształtów
            "data-command": s.selection.length ? "object.assign_layer" : "layer.set_active",
            onclick: () => this.host.pickLayer(l.index),
          }),
        ),
    );
    const dev = noPwm(s);
    const rows = s.layers.filter((l) => used.has(l.index)).map((l) => this.layerRow(l, dev));
    const cols = ["", "layers-col-name", "layers-col-mode", "layers-col-speed", "layers-col-power", "layers-col-passes", "layers-col-output", "layers-col-show"];
    const table = h(
      "table",
      { class: "layers" },
      h("thead", {}, h("tr", {}, ...cols.map((k) => h("th", { class: ADVANCED_COLS.has(k) ? "adv" : "" }, k ? t(k) : "")))),
      h("tbody", {}, ...rows),
    );
    // brak PWM (K40 M2 Nano, Moshi): moc ustawia pokrętło lasera (REQ-13-045)
    const note = dev ? h("p", { class: "note no-pwm", "data-testid": "layers-no-pwm", role: "note" }, t("power-no-pwm", { device: dev.name, board: dev.board ?? dev.driver })) : null;
    this.layers.body.replaceChildren(palette, ...(note ? [note] : []), rows.length ? table : h("p", { class: "empty" }, t("layers-empty")));
  }

  private layerRow(l: Layer, dev: DeviceInfo | null = null): HTMLElement {
    const set = (p: Record<string, unknown>) => this.host.run("layer.set", { index: l.index, ...p });
    const cell = (v: number, key: string, testid: string, label: string, adv = false) => {
      const i = h("input", { type: "text", inputmode: "decimal", class: "cell", value: num(v), "data-testid": testid, title: label, "aria-label": label });
      i.addEventListener("keydown", (e: KeyboardEvent) => {
        e.stopPropagation();
        if (e.key === "Enter") i.blur();
      });
      i.addEventListener("change", () => {
        const n = parseNum(i.value);
        if (n !== null) set({ [key]: key === "passes" ? Math.round(n) : n });
      });
      return h("td", { class: adv ? "adv" : "" }, i);
    };
    const mode = h(
      "select",
      { "data-testid": `layer-${l.index}-mode`, title: t("layers-col-mode"), "aria-label": t("layers-col-mode"),
        onchange: (e: Event) => set({ mode: (e.target as HTMLSelectElement).value }) },
      ...MODES.map((k) => h("option", { value: k, selected: k === l.mode }, t(`mode-${k}`))),
    );
    return h(
      "tr",
      {
        "data-testid": `layer-${l.index}`, "data-command": "layer.settings_open", title: t("layers-row-settings"),
        // dwuklik (poza polami edycji) otwiera okno ustawień warstwy (REQ-03-032)
        ondblclick: (e: MouseEvent) => {
          if (!(e.target as HTMLElement).closest("input, select")) this.host.exec("layer.settings_open", { index: l.index });
        },
      },
      h("td", {}, h("span", { class: "swatch small", style: `background:${l.color}` })),
      h("td", {}, l.name),
      h("td", {}, mode),
      cell(l.speed, "speed", `layer-${l.index}-speed`, t("layers-col-speed")),
      dev ? h("td", {}, h("input", { type: "text", class: "cell", value: num(l.power_max), disabled: true, "data-testid": `layer-${l.index}-power`,
        title: t("power-no-pwm-short"), "aria-label": `${t("layers-col-power")}: ${t("power-no-pwm-short")}` }))
        : cell(l.power_max, "power_max", `layer-${l.index}-power`, t("layers-col-power")),
      cell(l.passes, "passes", `layer-${l.index}-passes`, t("layers-col-passes"), true),
      h("td", {}, h("input", { type: "checkbox", checked: l.output, "aria-label": t("layers-output"), title: t("layers-output"), "data-testid": `layer-${l.index}-output`,
        onchange: (e: Event) => set({ output: (e.target as HTMLInputElement).checked }) })),
      h("td", {}, h("input", { type: "checkbox", checked: l.show, "aria-label": t("layers-show"), title: t("layers-show"), "data-testid": `layer-${l.index}-show`,
        onchange: (e: Event) => set({ show: (e.target as HTMLInputElement).checked }) })),
    );
  }

  // ---------- drzewo obiektów (REQ-02-040…044) ----------

  private matches(n: TreeNode): boolean {
    if (!this.query) return true;
    const hay = [objLabel(n), n.type, t(`obj-${n.type}`), ...(n.tags ?? []), n.note ?? ""].join(" ").toLowerCase();
    return hay.includes(this.query) || n.children.some((c) => this.matches(c));
  }

  /**
   * Drzewo jako płaska lista widocznych wierszy (zwinięcia i wyszukiwanie),
   * a w DOM tylko wiersze w obszarze przewijania (REQ-16-006: tysiące obiektów
   * nie blokują UI).
   */
  private renderTree(s: Scene) {
    const tree = (s as unknown as { tree?: TreeNode[] }).tree ?? [];
    this.place.clear();
    const rows: { n: TreeNode; depth: number; open: boolean }[] = [];
    const walk = (nodes: TreeNode[], parent: number | null, depth: number) => {
      nodes.forEach((n, index) => this.place.set(n.id, { parent, index, group: n.type === "group" }));
      // na górze listy obiekt rysowany najpóźniej (jak w programach graficznych)
      for (let i = nodes.length - 1; i >= 0; i--) {
        const n = nodes[i];
        if (!this.matches(n)) continue;
        const open = !this.collapsed.has(n.id) || !!this.query;
        rows.push({ n, depth, open });
        if (n.children.length && open) walk(n.children, n.id, depth + 1);
        else if (n.children.length) n.children.forEach((c, ci) => this.place.set(c.id, { parent: n.id, index: ci, group: c.type === "group" }));
      }
    };
    walk(tree, null, 0);
    this.treeRows = rows;
    this.treeScene = s;
    if (!rows.length) {
      this.treeBox.replaceChildren(h("p", { class: "empty" }, t("objects-empty")));
      return;
    }
    if (!this.treeBox.querySelector(".tree-scroll")) {
      const inner = h("div", { class: "tree-inner", role: "tree" });
      const scroll = h("div", { class: "tree-scroll" }, inner);
      scroll.addEventListener("scroll", () => {
        if (this.treeFrame) return;
        this.treeFrame = requestAnimationFrame(() => {
          this.treeFrame = 0;
          this.renderTreeRows();
        });
      });
      this.treeBox.replaceChildren(scroll);
    }
    this.renderTreeRows();
  }

  private treeRows: { n: TreeNode; depth: number; open: boolean }[] = [];
  private treeScene: Scene | null = null;
  private treeFrame = 0;

  /** Wiersze w obszarze przewijania (z zapasem). */
  private renderTreeRows() {
    const s = this.treeScene;
    const scroll = this.treeBox.querySelector<HTMLElement>(".tree-scroll");
    const inner = scroll?.querySelector<HTMLElement>(".tree-inner");
    if (!s || !scroll || !inner) return;
    const total = this.treeRows.length;
    inner.style.height = `${total * TREE_ROW}px`;
    const first = Math.max(0, Math.floor(scroll.scrollTop / TREE_ROW) - 10);
    const last = Math.min(total, Math.ceil((scroll.scrollTop + (scroll.clientHeight || 240)) / TREE_ROW) + 10);
    const sel = new Set(s.selection);
    const color = new Map(s.layers.map((l) => [l.index, l.color]));
    const els = this.treeRows.slice(first, last).map(({ n, depth, open }, k) => {
      const row = h(
        "div",
        {
          class: `row${sel.has(n.id) ? " selected" : ""}`, "data-testid": `object-${n.id}`, "aria-selected": String(sel.has(n.id)),
          "aria-level": depth + 1, draggable: "true", role: "treeitem",
          style: `top:${(first + k) * TREE_ROW}px;padding-left:${4 + depth * 14}px`,
          onclick: (e: MouseEvent) => {
            const ids = e.shiftKey || e.ctrlKey
              ? (sel.has(n.id) ? s.selection.filter((i) => i !== n.id) : [...s.selection, n.id])
              : [n.id];
            this.host.run("selection.set", { ids });
          },
        },
        n.children.length
          ? h("button", { class: "twisty", title: t("objects-toggle"), "aria-label": t("objects-toggle"), "data-testid": `object-${n.id}-toggle`,
              "aria-expanded": String(open),
              onclick: (e: Event) => {
                e.stopPropagation();
                if (this.collapsed.has(n.id)) this.collapsed.delete(n.id);
                else this.collapsed.add(n.id);
                this.renderTree(s);
              } }, open ? "▾" : "▸")
          : h("span", { class: "twisty" }),
        h("span", { class: "swatch small", style: `background:${color.get(n.layer) ?? "#fff"}` }),
        h("span", { class: "label" }, objLabel(n)),
        h("button", { class: "icon", title: t(n.visible ? "obj-hide" : "obj-show"), "aria-label": t(n.visible ? "obj-hide" : "obj-show"), "data-testid": `object-${n.id}-visible`,
          onclick: (e: Event) => { e.stopPropagation(); this.host.run("object.set", { ids: [n.id], visible: !n.visible }); } },
          n.visible ? "👁" : "—"),
        h("button", { class: "icon", title: t(n.locked ? "obj-unlock" : "obj-lock"), "aria-label": t(n.locked ? "obj-unlock" : "obj-lock"), "data-testid": `object-${n.id}-locked`,
          onclick: (e: Event) => { e.stopPropagation(); this.host.run("object.set", { ids: [n.id], locked: !n.locked }); } },
          n.locked ? "🔒" : "🔓"),
      );
      this.bindDrop(row, n.id);
      return row;
    });
    inner.replaceChildren(...els);
  }

  /** Przeciągnij i upuść (REQ-02-041): na grupę — do środka, na obiekt — nad nim. */
  private bindDrop(row: HTMLElement, id: number) {
    row.addEventListener("dragstart", (e: DragEvent) => {
      this.dragged = id;
      e.dataTransfer?.setData("text/plain", String(id));
      if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
    });
    row.addEventListener("dragover", (e: DragEvent) => {
      if (this.dragged === null || this.dragged === id) return;
      e.preventDefault();
      row.classList.add("drop");
    });
    row.addEventListener("dragleave", () => row.classList.remove("drop"));
    row.addEventListener("drop", (e: DragEvent) => {
      e.preventDefault();
      row.classList.remove("drop");
      const src = this.dragged ?? Number(e.dataTransfer?.getData("text/plain"));
      this.dragged = null;
      const target = this.place.get(id);
      if (!src || src === id || !target) return;
      if (target.group) this.host.run("object.arrange", { ids: [src], parent: id });
      else {
        const from = this.place.get(src);
        // w tym samym rodzicu wyjęcie obiektu spod celu przesuwa indeks celu
        const shift = from && from.parent === target.parent && from.index < target.index ? 0 : 1;
        this.host.run("object.arrange", { ids: [src], parent: target.parent, index: target.index + shift });
      }
    });
    row.addEventListener("dragend", () => (this.dragged = null));
  }

  // ---------- właściwości ----------

  private renderProps(s: Scene) {
    const tf = transformFields(this.host, s, this.tf, "prop");
    if (!tf) {
      this.props.body.replaceChildren(h("p", { class: "empty" }, t("props-none")));
      return;
    }
    const parts: HTMLElement[] = [
      h("div", { class: "row-between" }, h("p", { class: "muted" }, t("props-selected", { n: s.selection.length })), tf.anchor),
      h("div", { class: "grid2" }, ...tf.fields, tf.lock),
    ];
    if (s.selection.length === 1) {
      const o = s.objects.find((x) => x.id === s.selection[0]);
      if (o) {
        const name = h("input", { type: "text", value: o.name ?? "", "data-testid": "prop-name", "aria-label": t("props-name"), title: t("props-name") });
        name.addEventListener("keydown", (e: KeyboardEvent) => {
          e.stopPropagation();
          if (e.key === "Enter") name.blur();
        });
        name.addEventListener("change", () => this.host.run("object.set", { name: name.value }));
        const note = h("textarea", { rows: 2, "data-testid": "prop-note", "aria-label": t("props-note"), title: t("props-note") });
        note.value = o.note ?? "";
        note.addEventListener("keydown", (e) => e.stopPropagation());
        note.addEventListener("change", () => this.host.run("object.set", { note: note.value }));
        if (o.params) parts.push(this.shapeParams(o.id, o.params));
        parts.push(h("label", { class: "field wide" }, h("span", {}, t("props-name")), name));
        parts.push(h("label", { class: "field wide" }, h("span", {}, t("props-note")), note));
        if (o.error) parts.push(h("p", { class: "error" }, o.error));
      }
    }
    this.props.body.replaceChildren(...parts);
  }

  /**
   * Parametry kształtu (REQ-03-047): wymiary w bieżących jednostkach, liczby
   * boków i ramion, kąty w stopniach — `shape.set_params`, jeden krok historii.
   */
  private shapeParams(id: number, params: Record<string, number | null>): HTMLElement {
    const set = (key: string, v: number) => this.host.run("shape.set_params", { id, params: { [key]: v } });
    // położenie zmienia się polami X/Y przekształcenia — tu tylko wymiary i kształt
    const rank = (k: string) => (PARAM_ORDER.indexOf(k) + PARAM_ORDER.length + 1) % (PARAM_ORDER.length + 1);
    const entries = Object.entries(params).filter(([k, v]) => v !== null && !POSITION_PARAMS.has(k)).sort(([a], [b]) => rank(a) - rank(b));
    const fields = entries.map(([key, v]) => {
      const label = t(`param-${key}`);
      if (COUNT_PARAMS.has(key)) {
        return numField(label, `param-${key}`, String(v), (text) => {
          const n = parseNum(text);
          if (n === null || !Number.isInteger(n)) return null;
          set(key, n);
          return String(n);
        });
      }
      if (ANGLE_PARAMS.has(key)) {
        return numField(label, `param-${key}`, num(v as number), (text) => {
          const n = parseNum(text);
          if (n === null) return null;
          set(key, n);
          return num(n);
        });
      }
      return numField(label, `param-${key}`, fmtLen(v as number), (text) => {
        const mm = parseLen(text);
        if (mm === null) return null;
        set(key, mm);
        return fmtLen(mm);
      });
    });
    return h("div", { class: "shape-params", "data-testid": "shape-params" },
      h("p", { class: "muted" }, t("props-shape-params")), h("div", { class: "grid2" }, ...fields));
  }

  // ---------- historia ----------

  private renderHistory(s: Scene) {
    const { undo, redo } = s.history;
    const steps = async (id: string, n: number) => {
      for (let i = 0; i < n; i++) await this.host.run(id);
    };
    // offset stanu względem bieżącego: −k = po k cofnięciach, +k = po k ponowieniach
    const item = (cls: string, testid: string, offset: number, label: string, onclick: () => void) => {
      const li = h("li", { class: cls, "data-testid": testid, title: t("history-preview-hint"), onclick }, label);
      li.addEventListener("mouseenter", () => this.showPreview(li, s.revision, offset));
      li.addEventListener("mouseleave", () => this.hidePreview());
      return li;
    };
    const items = [
      item(undo.length === 0 ? "current" : "", "history-start", -undo.length, t("history-start"), () => steps("edit.undo", undo.length)),
      ...undo.map((label, i) =>
        item(i === undo.length - 1 ? "current" : "", `history-${i}`, -(undo.length - 1 - i), label, () => steps("edit.undo", undo.length - 1 - i)),
      ),
      ...redo.map((label, j) => item("redo", `history-redo-${j}`, j + 1, label, () => steps("edit.redo", j + 1))),
    ];
    this.history.body.replaceChildren(h("ol", { class: "history" }, ...items));
    this.hidePreview();
  }

  // ---------- podgląd stanów historii (REQ-03-048) ----------

  private preview: HTMLElement | null = null;
  private previewToken = 0;
  /** Miniatury według rewizji dokumentu i offsetu (rewizja zmienia się z każdym krokiem). */
  private previewCache = new Map<string, { png: string; objects: number }>();

  private async showPreview(li: HTMLElement, revision: number, offset: number) {
    const token = ++this.previewToken;
    const key = `${revision}:${offset}`;
    let p = this.previewCache.get(key);
    if (!p) {
      const r = (await this.host.run("edit.history_preview", { offset, max_size: 192 })) as { png_base64: string; objects: number } | undefined;
      if (!r || token !== this.previewToken) return;
      p = { png: r.png_base64, objects: r.objects };
      if (this.previewCache.size > 64) this.previewCache.clear();
      this.previewCache.set(key, p);
    }
    if (token !== this.previewToken || !li.isConnected) return;
    const box = this.preview ?? h("div", { class: "history-preview", "data-testid": "history-preview", role: "tooltip" });
    box.replaceChildren(
      h("img", { src: `data:image/png;base64,${p.png}`, alt: t("history-preview-alt") }),
      h("span", {}, t("history-preview-objects", { n: p.objects })),
    );
    const r = li.getBoundingClientRect();
    box.style.top = `${Math.max(4, r.top)}px`;
    box.style.right = `${Math.max(4, window.innerWidth - r.left + 8)}px`;
    if (!this.preview) document.body.append(box);
    this.preview = box;
  }

  private hidePreview() {
    this.previewToken++;
    this.preview?.remove();
    this.preview = null;
  }
}
