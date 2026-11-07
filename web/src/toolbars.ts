// Paski narzędzi konfigurowalne i dokowalne z zapisem układów (REQ-03-031).
// Układ (dok i kolejność każdego paska) leży w preferencjach hosta
// (`ui.toolbars`), nazwane układy w `ui.toolbar_layouts`.

import { h } from "./dom";
import { t } from "./i18n";

export type Dock = "top" | "left" | "right" | "bottom" | "hidden";

export const DOCKS: Exclude<Dock, "hidden">[] = ["top", "left", "right", "bottom"];

export interface ToolbarDef {
  id: string;
  /** Ukryty w trybie początkującego (REQ-03-051). */
  advanced?: boolean;
  /**
   * Stała szerokość w doku poziomym (zawartość zależy od narzędzia): zmiana
   * narzędzia nie zmienia zawijania doku, więc kanwa nie przesuwa się pod kursorem.
   */
  fixed?: boolean;
  build(): HTMLElement[];
}

export type Layout = Record<string, { dock: Dock; order: number }>;

/** Domyślnie: u góry pasek główny, tryby i wyrównywanie; na dole edycja numeryczna, węzły i modyfikatory. */
export const DEFAULT_LAYOUT: Layout = {
  main: { dock: "top", order: 0 },
  modes: { dock: "top", order: 1 },
  align: { dock: "top", order: 2 },
  numeric: { dock: "bottom", order: 0 },
  node: { dock: "bottom", order: 1 },
  modifiers: { dock: "bottom", order: 2 },
  create: { dock: "left", order: 0 },
};

export interface ToolbarHost {
  layout: Layout;
  beginner: boolean;
  saveLayout(l: Layout): void;
  showMenu(x: number, y: number, items: { action: string; label: string; run: () => void; testid?: string }[]): void;
}

export class Toolbars {
  private dragged: string | null = null;

  constructor(private host: ToolbarHost, private defs: ToolbarDef[]) {
    for (const d of DOCKS) this.bindZone(document.getElementById(`dock-${d}`)!, d);
  }

  get ids(): string[] {
    return this.defs.map((d) => d.id);
  }

  private place(id: string) {
    return this.host.layout[id] ?? DEFAULT_LAYOUT[id] ?? { dock: "top" as Dock, order: 99 };
  }

  /** Przenosi pasek do doku (na koniec) albo ukrywa go. */
  moveTo(id: string, dock: Dock) {
    const order = Math.max(-1, ...Object.values(this.host.layout).filter((p) => p.dock === dock).map((p) => p.order)) + 1;
    this.host.saveLayout({ ...this.host.layout, [id]: { dock, order } });
  }

  render() {
    for (const d of DOCKS) document.getElementById(`dock-${d}`)!.replaceChildren();
    const visible = this.defs
      .filter((d) => !(d.advanced && this.host.beginner))
      .map((d) => ({ d, p: this.place(d.id) }))
      .filter(({ p }) => p.dock !== "hidden")
      .sort((a, b) => a.p.order - b.p.order);
    for (const { d, p } of visible) {
      const dock = document.getElementById(`dock-${p.dock}`)!;
      const title = t(`toolbar-${d.id}`);
      const grip = h("button", {
        class: "grip", draggable: "true", title: t("toolbar-grip", { name: title }), "aria-label": t("toolbar-grip", { name: title }),
        "data-testid": `tb-grip-${d.id}`,
      }, "⠿");
      grip.addEventListener("dragstart", (e: DragEvent) => {
        this.dragged = d.id;
        e.dataTransfer?.setData("text/plain", `toolbar:${d.id}`);
        // wszystkie doki widoczne jako cele upuszczenia
        document.documentElement.classList.add("dragging-toolbar");
      });
      grip.addEventListener("dragend", () => {
        this.dragged = null;
        document.documentElement.classList.remove("dragging-toolbar");
      });
      const menu = h("button", {
        class: "tb-menu", title: t("toolbar-menu", { name: title }), "aria-label": t("toolbar-menu", { name: title }),
        "data-testid": `tb-menu-${d.id}`,
        onclick: (e: MouseEvent) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          this.host.showMenu(r.left, r.bottom, [...DOCKS, "hidden" as const].map((dk) => ({
            action: `dock-${dk}`, testid: `dock-${dk}`, label: t(`dock-${dk}`), run: () => this.moveTo(d.id, dk),
          })));
        },
      }, "⋮");
      dock.append(h("div", { class: d.fixed ? "toolbar fixed" : "toolbar", role: "toolbar", "aria-label": title, "data-testid": `toolbar-${d.id}` }, grip, ...d.build(), menu));
    }
  }

  private bindZone(zone: HTMLElement, dock: Exclude<Dock, "hidden">) {
    zone.addEventListener("dragover", (e: DragEvent) => {
      if (!this.dragged) return;
      e.preventDefault();
      zone.classList.add("drop");
    });
    zone.addEventListener("dragleave", () => zone.classList.remove("drop"));
    zone.addEventListener("drop", (e: DragEvent) => {
      zone.classList.remove("drop");
      document.documentElement.classList.remove("dragging-toolbar");
      const data = e.dataTransfer?.getData("text/plain") ?? "";
      const id = this.dragged ?? (data.startsWith("toolbar:") ? data.slice(8) : null);
      this.dragged = null;
      if (!id) return;
      e.preventDefault();
      this.moveTo(id, dock);
    });
  }
}
