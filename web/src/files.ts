// Okno „Lista plików na sterowniku” (REQ-03-042, §3): pliki w pamięci wybranego
// urządzenia (karta SD FluidNC, REQ-13-014) — lista z rozmiarem, uruchom, usuń
// (z potwierdzeniem), wyślij projekt albo plik z dysku. Każda kontrolka wykonuje
// polecenie rejestru (REQ-01-029): machine.files, device.sd_run, device.sd_delete,
// device.sd_upload; okno otwiera i zamyka polecenie okna files.open / files.close.
// Urządzenie bez pamięci plików (K40, Moshi, GRBL bez karty) — komunikat zamiast listy.

import { h } from "./dom";
import { getLang, t } from "./i18n";
import { Rpc } from "./rpc";

export interface FilesHost {
  rpc: Rpc;
  modal: boolean;
  /** Id wykonanych poleceń (diagnostyka, testy REQ-01-029). */
  commandLog: string[];
  /** Polecenie okna albo hosta (REQ-01-029). */
  exec(id: string, params?: unknown): unknown;
  /** Wybrane urządzenie (null = brak połączenia). */
  readonly deviceName: string | null;
  /** Po uruchomieniu pliku: stan maszyny w panelu Laser. */
  afterRun(): Promise<void>;
}

export interface RemoteFile {
  name: string;
  size: number;
}

interface FilesList {
  supported: boolean;
  files: RemoteFile[];
  total_bytes: number;
  reason?: string;
}

/** Domyślna nazwa pliku projektu na karcie SD (FluidNC: katalog /sd). */
export const DEFAULT_FILE = "/sd/zadanie.gcode";

/** Rozmiar pliku: B, kB, MB (przecinek dziesiętny po polsku). */
export function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  const mb = n >= 1024 * 1024;
  const v = (n / (mb ? 1024 * 1024 : 1024)).toFixed(1);
  return `${getLang() === "pl" ? v.replace(".", ",") : v} ${mb ? "MB" : "kB"}`;
}

/** Katalog z pola nazwy (do ostatniego „/”) + nazwa pliku z dysku. */
export function targetName(field: string, file: string): string {
  const i = field.lastIndexOf("/");
  return `${i >= 0 ? field.slice(0, i + 1) : "/sd/"}${file}`;
}

export class DriverFiles {
  state = { open: false };
  /** Ostatni wynik `machine.files` (null = brak urządzenia albo jeszcze nie pobrano). */
  list: FilesList | null = null;
  private root: HTMLElement | null = null;
  private box: HTMLElement | null = null;
  private nameInput = h("input", { type: "text", "data-testid": "files-name", "data-command": "device.sd_upload", spellcheck: "false", autocomplete: "off" });
  private fileInput = h("input", { type: "file", accept: ".gcode,.gc,.nc,.ngc,.tap,.txt", class: "hidden-file", "data-testid": "files-file-input", "data-command": "device.sd_upload" });
  private status = "";
  private error = "";
  /** Plik czekający na potwierdzenie usunięcia. */
  private confirm: string | null = null;
  private busy = false;

  constructor(private host: FilesHost) {
    this.nameInput.value = DEFAULT_FILE;
    this.fileInput.addEventListener("change", () => {
      const f = this.fileInput.files?.[0];
      this.fileInput.value = "";
      if (f) void this.uploadFile(f);
    });
  }

  open() {
    this.state.open = true;
    this.host.modal = true;
    this.confirm = null;
    this.status = this.error = "";
    if (!this.root) {
      this.box = h("div", { class: "dialog driver-files", role: "dialog", tabindex: -1, "aria-label": t("files-title") });
      this.root = h("div", { class: "dialog-backdrop", "data-testid": "files-dialog" }, this.box);
      this.root.addEventListener("keydown", (e: KeyboardEvent) => {
        e.stopPropagation();
        if (e.key === "Escape") this.host.exec("files.close");
      });
      document.body.append(this.root);
      // fokus w oknie: Esc zamyka, skróty aplikacji nie działają pod oknem
      this.box.focus({ preventScroll: true });
    }
    this.render();
    void this.load();
    return {};
  }

  close() {
    this.state.open = false;
    this.root?.remove();
    this.root = this.box = null;
    this.host.modal = false;
    return {};
  }

  retranslate() {
    if (this.root) this.render();
  }

  // ---------- polecenia ----------

  private async call<T>(id: string, params: unknown = {}): Promise<T> {
    this.host.commandLog.push(id);
    return this.host.rpc.execute<T>(id, params);
  }

  /** Akcja okna: błąd w oknie, po akcji świeża lista. */
  private async act(f: () => Promise<void>, reload = true) {
    if (this.busy) return;
    this.busy = true;
    this.error = "";
    this.render();
    try {
      await f();
    } catch (e) {
      this.error = String((e as Error).message ?? e);
    } finally {
      this.busy = false;
    }
    if (reload) await this.load();
    else this.render();
  }

  /** Lista plików (`machine.files`). */
  async load() {
    if (!this.host.deviceName) {
      this.list = null;
      this.render();
      return;
    }
    try {
      this.list = await this.call<FilesList>("machine.files", {});
    } catch (e) {
      this.list = null;
      this.error = String((e as Error).message ?? e);
    }
    this.render();
  }

  private uploadProject() {
    const name = this.nameInput.value.trim();
    return this.act(async () => {
      const r = await this.call<{ bytes: number }>("device.sd_upload", { name });
      this.status = t("files-sent", { name, size: formatSize(r.bytes) });
    });
  }

  private async uploadFile(f: File) {
    const name = targetName(this.nameInput.value.trim(), f.name);
    const text = await f.text();
    return this.act(async () => {
      const r = await this.call<{ bytes: number }>("device.sd_upload", { name, text });
      this.status = t("files-sent", { name, size: formatSize(r.bytes) });
    });
  }

  private run(name: string) {
    return this.act(async () => {
      await this.call("device.sd_run", { name });
      this.status = t("files-started", { name });
      await this.host.afterRun();
    });
  }

  private remove(name: string) {
    this.confirm = null;
    return this.act(async () => {
      await this.call("device.sd_delete", { name });
      this.status = t("files-deleted", { name });
    });
  }

  // ---------- widok ----------

  private btn(testid: string, command: string, label: string, onclick: () => void, cls = "") {
    return h("button", { class: cls, "data-testid": testid, "data-command": command, title: t(label), "aria-label": t(label), disabled: this.busy, onclick }, t(label));
  }

  private render() {
    if (!this.box) return;
    const keep = document.activeElement === this.nameInput;
    this.nameInput.title = t("files-name.title");
    this.nameInput.setAttribute("aria-label", t("files-name"));
    const dev = this.host.deviceName;
    const parts: (HTMLElement | null)[] = [
      h("div", { class: "row" }, h("h2", {}, t("files-title")), h("span", { class: "muted", "data-testid": "files-device" }, dev ? t("files-device", { device: dev }) : "")),
    ];
    if (!dev) {
      parts.push(h("p", { class: "muted", "data-testid": "files-no-device" }, t("files-no-device")));
    } else if (this.list && !this.list.supported) {
      parts.push(h("p", { class: "note", "data-testid": "files-unsupported" }, t("files-unsupported", { device: dev })));
    } else if (this.list) {
      parts.push(this.table(this.list), this.uploadRow());
    }
    parts.push(
      h("p", { class: "muted files-status", "data-testid": "files-status", role: "status" }, this.status),
      h("p", { class: "error", "data-testid": "files-error", role: "alert" }, this.error),
      h("div", { class: "row" },
        dev && this.list?.supported !== false ? this.btn("files-refresh", "machine.files", "files-refresh", () => void this.act(async () => undefined)) : h("span", {}),
        this.btn("files-close", "files.close", "dialog-close", () => this.host.exec("files.close"))),
    );
    this.box.replaceChildren(...parts.filter((x): x is HTMLElement => x !== null));
    this.box.setAttribute("aria-label", t("files-title"));
    if (keep && this.nameInput.isConnected) this.nameInput.focus({ preventScroll: true });
  }

  private table(l: FilesList): HTMLElement {
    if (!l.files.length) return h("p", { class: "muted", "data-testid": "files-empty" }, t("files-empty"));
    const rows = l.files.map((f, i) => {
      const actions = this.confirm === f.name
        ? [h("span", { class: "muted" }, t("files-delete-ask")),
          this.btn(`files-delete-confirm-${i}`, "device.sd_delete", "files-delete-yes", () => void this.remove(f.name), "stop-btn"),
          this.btn(`files-delete-cancel-${i}`, "machine.files", "files-delete-no", () => {
            this.confirm = null;
            void this.load();
          })]
        : [this.btn(`files-run-${i}`, "device.sd_run", "files-run", () => void this.run(f.name), "start-btn"),
          this.btn(`files-delete-${i}`, "device.sd_delete", "files-delete", () => {
            this.confirm = f.name;
            this.render();
          })];
      return h("tr", { "data-testid": `files-row-${i}` },
        h("td", { class: "files-file", "data-testid": `files-name-${i}` }, f.name),
        h("td", { class: "num", "data-testid": `files-size-${i}` }, formatSize(f.size)),
        h("td", { class: "files-actions" }, h("div", { class: "files-acts" }, ...actions)));
    });
    return h("div", { class: "scroll" },
      h("table", { class: "files-table" },
        h("thead", {}, h("tr", {}, h("th", {}, t("files-col-name")), h("th", { class: "num" }, t("files-col-size")), h("th", {}, ""))),
        h("tbody", {}, ...rows)),
      h("p", { class: "muted", "data-testid": "files-total" }, t("files-total", { count: l.files.length, size: formatSize(l.total_bytes) })));
  }

  private uploadRow(): HTMLElement {
    return h("div", { class: "files-upload" },
      h("label", { class: "field wide" }, h("span", {}, t("files-name")), this.nameInput),
      h("div", { class: "row" },
        this.btn("files-upload-project", "device.sd_upload", "files-upload-project", () => void this.uploadProject()),
        this.btn("files-upload-file", "device.sd_upload", "files-upload-file", () => this.fileInput.click()),
        this.fileInput));
  }
}
