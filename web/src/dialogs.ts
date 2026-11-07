// Okna dialogowe: ustawienia (język, motyw, jednostki) i edytor skrótów
// klawiszowych oparty na rejestrze (REQ-01-039). Preferencje zapisuje host
// (`prefs.set`, `shortcuts.*` — PrefStore z kopiami zapasowymi).

import { h } from "./dom";
import { getLang, LANGS, t } from "./i18n";
import { RpcError } from "./rpc";

export interface DialogHost {
  call<T = any>(id: string, params?: unknown): Promise<T>;
  setPref(key: string, value: unknown): Promise<void>;
  prefs: { lang: string; theme: string; units: string; mode: string; screenScale: number; startColor: string; stopColor: string };
  /** Paski narzędzi (REQ-03-031): lista, widoczność, zapisane układy. */
  toolbarList(): { id: string; title: string; dock: string }[];
  setToolbarVisible(id: string, visible: boolean): void;
  layouts: Record<string, unknown>;
  saveNamedLayout(name: string): Promise<void>;
  /** Okno dialogowe otwarte — skróty aplikacji wyłączone. */
  modal: boolean;
  reloadShortcuts(): Promise<void>;
  /** Ponowne wczytanie i zastosowanie preferencji (po przywróceniu kopii). */
  loadPrefs(): Promise<void>;
}

/** Kombinacja klawiszy w postaci kanonicznej rdzenia (Ctrl+Shift+Alt+Klawisz). */
export function keyCombo(e: KeyboardEvent): string {
  const named: Record<string, string> = { " ": "Space", Del: "Delete", Esc: "Escape" };
  const k = named[e.key] ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key);
  return [e.ctrlKey || e.metaKey ? "Ctrl" : "", e.shiftKey ? "Shift" : "", e.altKey ? "Alt" : "", k].filter(Boolean).join("+");
}

function dialog(testid: string, title: string, onClose: () => void): { root: HTMLElement; box: HTMLElement; close: () => void } {
  const box = h("div", { class: "dialog", role: "dialog", "aria-modal": "true", "aria-label": title, "data-testid": testid });
  const root = h("div", { class: "dialog-backdrop" }, box);
  const close = () => {
    root.remove();
    onClose();
  };
  root.addEventListener("pointerdown", (e) => {
    if (e.target === root) close();
  });
  document.body.append(root);
  return { root, box, close };
}

export function openSettings(host: DialogHost) {
  host.modal = true;
  const d = dialog("settings-dialog", t("settings-title"), () => (host.modal = false));
  const sel = (testid: string, label: string, value: string, opts: [string, string][], key: string) =>
    h("label", { class: "row" }, h("span", {}, label),
      h("select", { "data-testid": testid, "aria-label": label, title: label, onchange: async (e: Event) => {
        await host.setPref(key, (e.target as HTMLSelectElement).value);
        render();
      } }, ...opts.map(([v, l]) => h("option", { value: v, selected: v === value }, l))));
  const color = (testid: string, label: string, value: string, key: string) =>
    h("label", { class: "field" }, h("span", {}, label),
      h("input", { type: "color", value, "data-testid": testid, title: label, "aria-label": label, onchange: async (e: Event) => {
        await host.setPref(key, (e.target as HTMLInputElement).value);
      } }));
  /**
   * Kalibracja rzeczywistego rozmiaru (REQ-03-015): pasek ma 100 mm przy
   * bieżącej kalibracji; użytkownik mierzy go linijką i wpisuje wynik.
   */
  const calibration = () => {
    const scale = host.prefs.screenScale;
    const measured = h("input", { type: "text", inputmode: "decimal", "data-testid": "calibration-measured", placeholder: "100",
      title: t("calibration-measured"), "aria-label": t("calibration-measured") });
    measured.addEventListener("keydown", (e) => e.stopPropagation());
    const apply = async () => {
      const mm = Number(measured.value.replace(",", ".").replace(/\s*mm$/i, ""));
      if (!Number.isFinite(mm) || mm <= 0) return;
      const next = (scale * 100) / mm;
      if (next < 0.25 || next > 4) return;
      await host.setPref("ui.screen_scale", next);
      render();
    };
    return [
      h("h3", {}, t("calibration-title")),
      h("p", { class: "muted" }, t("calibration-hint")),
      h("div", { class: "calibration-bar", "data-testid": "calibration-bar", title: t("calibration-bar"),
        style: `width:${(100 * 96 * scale) / 25.4}px` }),
      h("div", { class: "row" },
        h("label", { class: "field" }, h("span", {}, t("calibration-measured")), measured),
        h("button", { "data-testid": "calibration-apply", title: t("calibration-apply"), onclick: apply }, t("calibration-apply")),
        h("button", { "data-testid": "calibration-reset", title: t("calibration-reset"), onclick: async () => {
          await host.setPref("ui.screen_scale", 1);
          render();
        } }, t("calibration-reset"))),
      h("p", { class: "muted", "data-testid": "calibration-scale" }, t("calibration-scale", { scale: scale.toFixed(3) })),
    ];
  };
  const render = () => {
    const bars = host.toolbarList().map((b) =>
      h("label", { class: "opt", title: b.title },
        h("input", { type: "checkbox", checked: b.dock !== "hidden", "data-testid": `toolbar-visible-${b.id}`, title: b.title, "aria-label": b.title,
          onchange: (e: Event) => {
            host.setToolbarVisible(b.id, (e.target as HTMLInputElement).checked);
            setTimeout(render, 0);
          } }),
        b.title),
    );
    const name = h("input", { type: "text", "data-testid": "layout-name", placeholder: t("layout-name"), title: t("layout-name"), "aria-label": t("layout-name") });
    name.addEventListener("keydown", (e) => e.stopPropagation());
    d.box.replaceChildren(
      h("h2", {}, t("settings-title")),
      sel("pref-lang", t("settings-lang"), host.prefs.lang, Object.entries(LANGS), "ui.lang"),
      sel("pref-theme", t("settings-theme"), host.prefs.theme, [["dark", t("theme-dark")], ["light", t("theme-light")], ["system", t("theme-system")]], "ui.theme"),
      sel("pref-units", t("settings-units"), host.prefs.units, [["mm", t("units-mm")], ["in", t("units-in")]], "ui.units"),
      sel("pref-mode", t("settings-mode"), host.prefs.mode, [["full", t("mode-full")], ["beginner", t("mode-beginner")]], "ui.mode"),
      // kolory przycisków Start / Stop okna Laser (REQ-03-052)
      h("div", { class: "row" },
        color("pref-start-color", t("settings-start-color"), host.prefs.startColor, "ui.start_color"),
        color("pref-stop-color", t("settings-stop-color"), host.prefs.stopColor, "ui.stop_color"),
        h("button", { "data-testid": "pref-colors-reset", title: t("settings-colors-reset"), onclick: async () => {
          await host.setPref("ui.start_color", null);
          await host.setPref("ui.stop_color", null);
          render();
        } }, t("settings-colors-reset"))),
      ...calibration(),
      h("h3", {}, t("settings-toolbars")),
      h("div", { class: "bars" }, ...bars),
      h("div", { class: "row" },
        name,
        h("button", { "data-testid": "layout-save", title: t("layout-save"), onclick: async () => {
          if (!name.value.trim()) return;
          await host.saveNamedLayout(name.value.trim());
          render();
        } }, t("layout-save")),
        h("select", { "data-testid": "layout-load", title: t("layout-load"), "aria-label": t("layout-load"), onchange: async (e: Event) => {
          const v = (e.target as HTMLSelectElement).value;
          if (v && host.layouts[v]) await host.setPref("ui.toolbars", host.layouts[v]);
          render();
        } },
          h("option", { value: "" }, t("layout-load")),
          ...Object.keys(host.layouts).map((k) => h("option", { value: k }, k))),
        h("button", { "data-testid": "toolbars-reset", title: t("toolbars-reset"), onclick: async () => {
          await host.setPref("ui.toolbars", null);
          render();
        } }, t("toolbars-reset")),
      ),
      h("div", { class: "row" },
        h("button", { "data-testid": "open-backups", title: t("backups-open.title"), onclick: () => {
          d.close();
          openBackups(host);
        } }, t("backups-open")),
        h("button", { "data-testid": "settings-close", title: t("dialog-close"), onclick: d.close }, t("dialog-close"))),
    );
  };
  render();
}

interface BackupInfo {
  name: string;
  created_ms: number;
  bytes: number;
}

interface PrefChange {
  key: string;
  backup: unknown;
  current: unknown;
}

/**
 * Przeglądarka kopii zapasowych preferencji (REQ-02-056): lista kopii od
 * najnowszej, różnice względem bieżących ustawień (`prefs.backup_diff`)
 * i przywracanie po potwierdzeniu (`prefs.restore`).
 */
export function openBackups(host: DialogHost) {
  host.modal = true;
  let list: BackupInfo[] = [];
  let selected: string | null = null;
  let changes: PrefChange[] | null = null;
  let confirming = false;
  let status = "";
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.stopImmediatePropagation();
    if (confirming) {
      confirming = false;
      render();
    } else d.close();
  };
  window.addEventListener("keydown", onKey, true);
  const d = dialog("backups-dialog", t("backups-title"), () => {
    window.removeEventListener("keydown", onKey, true);
    host.modal = false;
  });
  d.box.classList.add("backups");
  const date = (ms: number) => new Date(ms).toLocaleString(getLang() === "pl" ? "pl-PL" : "en-GB");
  const show = (v: unknown) => (v === null || v === undefined ? t("backups-absent") : JSON.stringify(v));

  const load = async () => {
    try {
      list = (await host.call<{ backups: BackupInfo[] }>("prefs.backups")).backups;
    } catch (err) {
      list = [];
      status = (err as Error).message;
    }
    render();
  };

  const select = async (name: string) => {
    selected = name;
    confirming = false;
    changes = null;
    render();
    try {
      changes = (await host.call<{ changes: PrefChange[] }>("prefs.backup_diff", { name })).changes;
    } catch (err) {
      status = (err as Error).message;
    }
    render();
  };

  const restore = async () => {
    if (!selected) return;
    const info = list.find((b) => b.name === selected);
    try {
      await host.call("prefs.restore", { name: selected });
      await host.loadPrefs();
      status = t("backups-restored", { date: info ? date(info.created_ms) : selected });
    } catch (err) {
      status = (err as Error).message;
    }
    confirming = false;
    selected = null;
    changes = null;
    await load();
  };

  const diffView = (): HTMLElement => {
    if (!selected) return h("p", { class: "muted" }, t("backups-pick"));
    if (!changes) return h("p", { class: "muted" }, "…");
    if (!changes.length) return h("p", { class: "muted", "data-testid": "backup-no-diff" }, t("backups-no-diff"));
    return h("table", {},
      h("thead", {}, h("tr", {}, h("th", {}, t("backups-col-key")), h("th", {}, t("backups-col-backup")), h("th", {}, t("backups-col-current")))),
      h("tbody", {}, ...changes.map((c) =>
        h("tr", { "data-key": c.key },
          h("td", {}, h("code", {}, c.key)),
          h("td", { class: "diff-old", "data-testid": "diff-backup" }, show(c.backup)),
          h("td", { class: "diff-new", "data-testid": "diff-current" }, show(c.current))))));
  };

  const render = () => {
    const rows = list.map((b) =>
      h("li", {}, h("button", {
        class: "backup-row", "data-testid": "backup-row", "data-name": b.name, title: b.name,
        "aria-pressed": String(b.name === selected), onclick: () => select(b.name),
      }, h("span", {}, date(b.created_ms)), h("small", { class: "muted" }, t("backups-size", { kb: (b.bytes / 1024).toFixed(1) })))));
    const confirm = confirming
      ? h("div", { class: "conflict", role: "alertdialog", "data-testid": "backup-confirm" },
          h("span", {}, t("backups-confirm", { count: changes?.length ?? 0 })),
          h("button", { "data-testid": "backup-confirm-yes", title: t("backups-confirm-yes"), onclick: restore }, t("backups-confirm-yes")),
          h("button", { "data-testid": "backup-confirm-no", title: t("shortcuts-cancel"), onclick: () => {
            confirming = false;
            render();
          } }, t("shortcuts-cancel")))
      : "";
    d.box.replaceChildren(
      h("h2", {}, t("backups-title")),
      h("p", { class: "muted" }, t("backups-hint")),
      status ? h("p", { "data-testid": "backup-status", role: "status" }, status) : "",
      h("div", { class: "backups-body" },
        list.length
          ? h("ul", { class: "backup-list scroll", "data-testid": "backup-list" }, ...rows)
          : h("p", { class: "muted", "data-testid": "backup-empty" }, t("backups-empty")),
        h("div", { class: "backup-diff scroll", "data-testid": "backup-diff" }, diffView())),
      confirm,
      h("div", { class: "row" },
        h("button", { "data-testid": "backup-restore", title: t("backups-restore.title"), disabled: !selected || confirming,
          onclick: () => {
            confirming = true;
            render();
          } }, t("backups-restore")),
        h("button", { "data-testid": "backups-close", title: t("dialog-close"), onclick: () => d.close() }, t("dialog-close"))),
    );
  };
  render();
  void load();
}

interface ShortcutInfo {
  id: string;
  title: string;
  default: string | null;
  shortcut: string | null;
  custom: boolean;
}

export function openShortcuts(host: DialogHost) {
  host.modal = true;
  let list: ShortcutInfo[] = [];
  let query = "";
  let capturing: string | null = null;
  let conflict: { id: string; shortcut: string; message: string } | null = null;
  const tbody = h("tbody", {});
  const conflictBox = h("div", {});
  const search = h("input", { type: "search", "data-testid": "shortcuts-search", placeholder: t("shortcuts-search"), "aria-label": t("shortcuts-search"), title: t("shortcuts-search") });

  const onKey = async (e: KeyboardEvent) => {
    if (!capturing) {
      if (e.key === "Escape") d.close();
      return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;
    const id = capturing;
    capturing = null;
    if (e.key === "Escape") return renderRows();
    await assign(id, keyCombo(e), false);
  };
  window.addEventListener("keydown", onKey, true);
  const d = dialog("shortcuts-dialog", t("shortcuts-title"), async () => {
    window.removeEventListener("keydown", onKey, true);
    host.modal = false;
    await host.reloadShortcuts();
  });

  const load = async () => {
    list = (await host.call<{ commands: ShortcutInfo[] }>("shortcuts.list")).commands;
    renderRows();
  };

  const assign = async (id: string, shortcut: string | null, force: boolean) => {
    try {
      await host.call("shortcuts.set", { id, shortcut, force });
      conflict = null;
    } catch (err) {
      const r = err as RpcError;
      if (r.data?.code === "shortcut_conflict" && shortcut) conflict = { id, shortcut, message: r.message };
      else conflict = { id, shortcut: "", message: r.message };
    }
    await load();
  };

  const renderRows = () => {
    const q = query.toLowerCase();
    const rows = list
      .filter((c) => !q || c.title.toLowerCase().includes(q) || c.id.toLowerCase().includes(q))
      .map((c) =>
        h("tr", {},
          h("td", {}, h("div", {}, c.title), h("small", { class: "muted" }, c.id)),
          h("td", {}, h("kbd", { "data-testid": `shortcut-${c.id}` }, capturing === c.id ? t("shortcuts-press") : c.shortcut ?? t("shortcuts-none"))),
          h("td", {},
            h("button", { "data-testid": `shortcut-edit-${c.id}`, title: t("shortcuts-edit"), onclick: () => {
              capturing = c.id;
              conflict = null;
              renderRows();
            } }, t("shortcuts-edit")),
            " ",
            h("button", { "data-testid": `shortcut-remove-${c.id}`, title: t("shortcuts-remove"), onclick: () => assign(c.id, null, false) }, "✕"),
            " ",
            c.custom
              ? h("button", { "data-testid": `shortcut-reset-${c.id}`, title: t("shortcuts-reset"), onclick: async () => {
                  await host.call("shortcuts.reset", { id: c.id });
                  await load();
                } }, t("shortcuts-reset"))
              : "",
          ),
        ),
      );
    tbody.replaceChildren(...rows);
    conflictBox.replaceChildren(
      ...(conflict
        ? [h("div", { class: "conflict", "data-testid": "shortcut-conflict" },
            h("span", {}, conflict.message),
            ...(conflict.shortcut
              ? [h("button", { "data-testid": "shortcut-force", title: t("shortcuts-force"), onclick: () => assign(conflict!.id, conflict!.shortcut, true) }, t("shortcuts-force"))]
              : []),
            h("button", { "data-testid": "shortcut-cancel-conflict", title: t("shortcuts-cancel"), onclick: () => {
              conflict = null;
              renderRows();
            } }, t("shortcuts-cancel")))]
        : []),
    );
  };

  search.addEventListener("input", () => {
    query = search.value.trim();
    renderRows();
  });
  d.box.append(
    h("div", { class: "row" }, h("h2", {}, t("shortcuts-title")), search),
    conflictBox,
    h("div", { class: "scroll" },
      h("table", {},
        h("thead", {}, h("tr", {}, h("th", {}, t("shortcuts-col-command")), h("th", {}, t("shortcuts-col-shortcut")), h("th", {}, ""))),
        tbody)),
    h("div", { class: "row" },
      h("button", { "data-testid": "shortcuts-reset-all", title: t("shortcuts-reset-all"), onclick: async () => {
        await host.call("shortcuts.reset", {});
        conflict = null;
        await load();
      } }, t("shortcuts-reset-all")),
      h("button", { "data-testid": "shortcuts-close", title: t("dialog-close"), onclick: () => d.close() }, t("dialog-close"))),
  );
  void load();
  search.focus();
}
