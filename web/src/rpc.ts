// Klient kanału sterowania bc-host: JSON-RPC 2.0 po WebSocket (ADR 0007).

export class RpcError extends Error {
  constructor(
    message: string,
    public code: number,
    public data?: { code?: string; step?: number },
  ) {
    super(message);
  }
}

type Pending = { resolve: (v: unknown) => void; reject: (e: unknown) => void };
type RequestHandler = (params: unknown) => Promise<unknown> | unknown;

export class Rpc {
  private ws?: WebSocket;
  private seq = 0;
  private pending = new Map<number, Pending>();
  private handlers = new Map<string, RequestHandler>();
  private listeners = new Map<string, ((params: any) => void)[]>();
  onState: (connected: boolean) => void = () => {};
  /** Rozmiar ostatniej odpowiedzi (znaki JSON) — pomiar transferu sceny. */
  lastBytes = 0;

  constructor(private url: string) {}

  static fromPage(): Rpc {
    const fromQuery = new URLSearchParams(location.search).get("token");
    const injected = (window as any).BURNCRAFT_TOKEN as string | undefined;
    const token = fromQuery ?? (injected && !injected.startsWith("%") ? injected : "");
    const proto = location.protocol === "https:" ? "wss" : "ws";
    return new Rpc(`${proto}://${location.host}/rpc?token=${encodeURIComponent(token)}`);
  }

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url);
      this.ws = ws;
      ws.onopen = () => {
        this.onState(true);
        resolve();
      };
      ws.onerror = () => reject(new Error("brak połączenia z hostem BurnCraft"));
      ws.onclose = () => {
        this.onState(false);
        for (const p of this.pending.values()) p.reject(new Error("połączenie zamknięte"));
        this.pending.clear();
      };
      ws.onmessage = (ev) => {
        const text = ev.data as string;
        const msg = JSON.parse(text);
        if (msg.id !== undefined && msg.method === undefined) this.lastBytes = text.length;
        this.dispatch(msg);
      };
    });
  }

  /** Obsługa żądań hosta skierowanych do okna (np. ui.screenshot). */
  handle(method: string, h: RequestHandler) {
    this.handlers.set(method, h);
  }

  on(method: string, f: (params: any) => void) {
    const l = this.listeners.get(method) ?? [];
    l.push(f);
    this.listeners.set(method, l);
  }

  call<T = any>(method: string, params: unknown = {}): Promise<T> {
    const id = ++this.seq;
    return new Promise<T>((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        reject(new Error("brak połączenia z hostem BurnCraft"));
        return;
      }
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
    });
  }

  /** Polecenie z rejestru (REQ-01-036: GUI działa wyłącznie przez rejestr). */
  execute<T = any>(id: string, params: unknown = {}): Promise<T> {
    return this.call<T>("gui.execute", { id, params });
  }

  private async dispatch(msg: any) {
    if (msg.method !== undefined) {
      if (msg.id === undefined) {
        for (const f of this.listeners.get(msg.method) ?? []) f(msg.params);
        return;
      }
      const h = this.handlers.get(msg.method);
      let reply: object;
      try {
        if (!h) throw new Error(`nieobsługiwane żądanie ${msg.method}`);
        reply = { result: await h(msg.params) };
      } catch (e) {
        reply = { error: { code: -32000, message: String((e as Error).message ?? e) } };
      }
      this.ws?.send(JSON.stringify({ jsonrpc: "2.0", id: msg.id, ...reply }));
      return;
    }
    const p = this.pending.get(msg.id);
    if (!p) return;
    this.pending.delete(msg.id);
    if (msg.error) p.reject(new RpcError(msg.error.message, msg.error.code, msg.error.data));
    else p.resolve(msg.result);
  }
}
