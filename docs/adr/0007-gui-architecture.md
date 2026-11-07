# ADR 0007 — Architektura GUI i kanał sterowania okna

- Status: przyjęta (M1.4, 2026-10-29)
- Wymagania: REQ-01-017, REQ-01-021…026, REQ-01-036, REQ-01-042, REQ-01-043, REQ-15-002, REQ-15-026, REQ-19-013

## Decyzja

### Jeden frontend, jeden kanał

```
 ┌────────────── proces BurnCraft (bc-host) ───────────────┐
 │  Workbench (dokumenty) ◄── bc_mcp::Server (polityka, audyt)│
 │        ▲                         ▲                        │
 │  JSON-RPC „gui.*”          JSON-RPC „mcp”                 │
 │        │      WebSocket ws://127.0.0.1:7984/rpc           │
 └────────┼─────────────────────────┼────────────────────────┘
          │                         │
   frontend web/ (TS+WebGL2)   burncraft-cli mcp --connect 7984
   w oknie Tauri albo przeglądarce     (proxy stdio ↔ WebSocket)
```

- **`bc-host`** (Rust, axum + tokio): serwuje statyczny frontend (`web/dist`) i kanał **JSON-RPC 2.0 po WebSocket** `/rpc`. Domyślnie nasłuchuje na **127.0.0.1:7984**.
- Właścicielem stanu jest jedna instancja `bc_mcp::Server` z `Workbench`.
  - Wywołania GUI (`gui.execute`, `gui.batch`, `gui.scene`, …) działają na tym samym `Workbench` z uprawnieniami lokalnego użytkownika, ale przez **ten sam rejestr poleceń** (REQ-01-036 — zgodność 1:1 GUI–MCP).
  - Metoda `mcp` przekazuje surowe wiadomości MCP do `bc_mcp::Server`, więc agent podłączony do żywego okna przechodzi przez politykę bezpieczeństwa i dziennik audytu jak w trybie bezgłowym.
- Po każdej zmianie dokumentu serwer wysyła wszystkim klientom powiadomienie `doc.changed {revision, active}`. GUI widzi na bieżąco zmiany robione przez agenta, a agent widzi zmiany z GUI.
- **`burncraft-cli mcp --connect [7984]`**: gdy okno odpowiada, CLI jest przezroczystym pośrednikiem stdio ↔ WebSocket (metoda `mcp`). Gdy okna nie ma, uruchamia sesję bezgłową (bez zmian względem M0).
- Żądania jednego klienta host wykonuje **po kolei** (kolejka na połączenie), a różnych klientów — współbieżnie pod jednym zamkiem stanu. GUI może więc wysłać „zaznacz” i „przesuń” bez czekania na odpowiedź. Odpowiedzi okna na żądania hosta omijają kolejkę, dlatego zrzut ekranu nie blokuje kanału.
- **`screenshot`** (REQ-15-026): host prosi podłączone GUI o zrzut kanwy (`ui.screenshot` → PNG z WebGL). Bez GUI narzędzie zwraca błąd `no_window`, a `render_preview` dalej działa.

### Bezpieczeństwo kanału

- Nasłuch tylko na 127.0.0.1 (LAN z TLS i tokenem — M6).
- **Token sesji** (losowy, 256 bitów) jest wymagany przy połączeniu WebSocket (`?token=`). Host zapisuje go w pliku `control-<port>.token` w katalogu danych użytkownika, skąd czyta go CLI, i wstrzykuje do serwowanego `index.html`.
- **Kontrola `Origin`**: połączenia WebSocket są przyjmowane tylko z origin hosta (`http://127.0.0.1:<port>`, `http://localhost:<port>`), z `tauri://localhost` albo bez nagłówka `Origin` (klienci natywni). To blokuje przejęcie WebSocket przez obcą stronę w przeglądarce (CSWSH, DNS rebinding).

### Frontend (`web/`)

- **TypeScript + Vite**, bez frameworka UI (lekki, łatwy do osadzenia w Tauri i na tablecie). Panele są budowane z DOM.
- **Obszar roboczy: WebGL2**. Geometria przychodzi z hosta jako polilinie świata (`gui.scene`: spłaszczenie po stronie rdzenia, tolerancja zależna od powiększenia). Klient trzyma bufory GPU i rysuje linie w kolorach warstw. Kafelkowanie i LOD dla 200 tys. węzłów i 50 MPix powstaną w M1.4c wraz z testami wydajności.
- Każda akcja UI (menu, przycisk, skrót, przeciągnięcie uchwytu) wywołuje polecenie z rejestru przez `gui.execute`. Frontend nie modyfikuje dokumentu sam.
- **Playwright** (Chromium) testuje frontend serwowany przez `bc-host` — ten sam kod co w oknie Tauri.

### Okno desktopowe (`app/`, Tauri 2)

- Cienka powłoka: uruchamia `bc-host` w procesie i ładuje frontend z `http://127.0.0.1:<port>/`. `burncraft --control <port>` to binarka Tauri (M1.4c).
- Okno ładuje stronę z hosta jak przeglądarka (`WebviewUrl::External`), więc kanał, token i kontrola `Origin` działają bez zmian. Frontend nie korzysta z IPC Tauri, a okno nie potrzebuje uprawnień (capabilities).
- `app/` to **osobny workspace Cargo** (jak `fuzz/`). Tauri wnosi kilkaset zależności, a na Linuksie wymaga bibliotek WebKitGTK. Testy rdzenia budują się bez nich. Zadanie CI `app` przy każdym pushu buduje okno i uruchamia je na Linuksie (xvfb); codziennie i ręcznie robi to także na Windows i macOS. req-check w zadaniu testów rdzenia dostaje `--optional app/`.
- Test okna sprawdza, że frontend w oknie podłącza się do kanału jako okno (`gui.status`) i że agent dostaje prawdziwy zrzut kanwy (`screenshot`).
- Na Windows budowanie wymaga MSVC Build Tools z obciążeniem C++ (Tauri kompiluje kod C/C++ w skryptach budowania).
- `burncraft --serve [--port]`: ten sam host bez okna (przeglądarka, tablet — REQ-01-025).

### Wydajność (M1.4c2)

- **Rysowanie:** cała geometria sceny to jedna lista odcinków (`gl.LINES`) z kolorem przy każdym wierzchołku, więc jedno wywołanie rysowania niezależnie od liczby obiektów. Przeciągane obiekty mają w wierzchołkach flagę, a przesunięcie liczy shader.
- **Scena przyrostowa:** klient wysyła w `gui.scene` skróty znanych obiektów (`have`) i drzewa (`have_tree`). Host odsyła pełną geometrię tylko zmienionych obiektów, a drzewo tylko po zmianie struktury. Współrzędne są zaokrąglone do 0,1 µm; dokładne wartości do edycji daje `node.list`.
- **Drzewo obiektów:** wirtualizowane — w DOM tylko widoczne wiersze.
- **Pomiar** (Windows 11, GPU sprzętowy, host release, Edge, okno 1280×800):

  | Dokument | Klatka (mediana / p95) | Transfer po zmianie 1 obiektu | Najdłuższe zadanie UI |
  |---|---|---|---|
  | 2000 obiektów × 100 węzłów | 0,6 / 1,0 ms | 57,6 KB (wcześniej 22,7 MB) | < 50 ms |
  | 20 000 obiektów × 10 węzłów | 2,0 / 2,6 ms | 496 KB | < 50 ms |

- **Testy E2E** (`web/e2e/perf.spec.ts`): przy GPU sprzętowym próg klatki to 16,7 ms (60 fps), a długich zadań — 100 ms. Na rendererze programowym (SwiftShader w CI) progi są luźniejsze, żeby wychwytywać regresje, a nie mierzyć sprzęt.

## Konsekwencje

- Zależności sieciowe: `axum` i `tokio` tylko w `bc-host`; `bc-cli` ma synchroniczny klient `tungstenite` wyłącznie do `--connect` na 127.0.0.1. Rdzeń i `bc-mcp` ich nie mają. Żaden komponent nie wykonuje połączeń wychodzących poza localhost (REQ-16-009).
- Wydajność renderowania i warstwę Tauri weryfikują przyrosty M1.4b/c.
