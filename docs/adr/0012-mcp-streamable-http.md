# ADR 0012 — MCP przez Streamable HTTP, subskrypcje, postęp, anulowanie i narzędzia urządzeń

- Status: przyjęta (M2.4, 2026-11-04)
- Wymagania: REQ-15-003, 006…009, 058…067, 070…073, 083, 084, 087, 094, 106…108
- Powiązane: ADR 0002 (własna warstwa MCP), ADR 0005 (bezpieczeństwo MCP), ADR 0010 (sterowniki)

## Kontekst

ADR 0002 wybrał w M0 własną implementację JSON-RPC po stdio zamiast `rmcp` i zapowiedział ponowną ocenę, gdy dojdą Streamable HTTP, subskrypcje, postęp, anulowanie i elicitation. M2.4 wprowadza te elementy oraz narzędzia urządzeń z §15.2 nad poleceniami rejestru z M2.3 (`machine.*`, `device.*`, `job.*`). Laser to ryzyko pożaru, więc stop musi działać zawsze, także gdy serwer jest zajęty.

## Opcje

| | `rmcp` (oficjalny SDK) | Rozbudowa własnej warstwy |
|---|---|---|
| Nowe zależności | `rmcp` + jego drzewo (m.in. własne makra, `tokio-util`, `futures`, `sse-stream`); pobranie wymaga zgody | brak nowych pobrań: `axum` 0.8 i `tokio` są już w workspace (`bc-host`), `futures-core` i `getrandom` są w `Cargo.lock` |
| Model wykonania | asynchroniczne handlery, stan w `Arc` | rejestr poleceń i `Workbench` są synchroniczne; jeden zamek na serwer, jak w `bc-host` |
| Ścieżka awaryjna stopu | trzeba obejść kolejkę SDK | pełna kontrola: `stop_job` omija kolejkę i zamek |
| Schematy narzędzi | generowane z typów Rust | już generowane z rejestru (`schemars`), 1:1 z `describe_command` |
| Zakres protokołu | pełny | potrzebny podzbiór: HTTP, sesje, SSE, subskrypcje, prompty, postęp, anulowanie, elicitation |

## Decyzja

**Rozbudowa własnej warstwy w `bc-mcp`** (bez `rmcp`). Uzasadnienie: brak nowych pobrań, ten sam model wykonania co host GUI (ADR 0007), pełna kontrola nad priorytetem stopu i watchdogiem. To wybór równoważnych bibliotek (§21.4), funkcje są bez zmian.

### 1. Rdzeń: klienci i kontekst żądania

- `Server` obsługuje wielu klientów (`PeerId`: stdio = 0, HTTP — kolejne sesje). Każdy klient ma nazwę z `clientInfo`, obsługę elicitation, subskrypcje i listę jogów (watchdog). Dokument i urządzenia są wspólne.
- `Server::handle_ctx(msg, &Ctx)`: `Ctx` niesie klienta, kanał do klienta (`Channel`: powiadomienia i żądania serwera) i flagę anulowania. `handle(msg)` bez kanału działa jak dotąd (host GUI, testy jednostkowe).

### 2. Transport stdio (`Server::run_io`)

- Wątek czytający rozdziela wiadomości: odpowiedzi klienta → oczekujące żądania serwera (elicitation), `notifications/cancelled` → flaga anulowania żądania w toku, `tools/call stop_job` → od razu ścieżką awaryjną. Reszta trafia po kolei do wątku głównego.
- Wątek główny co 100 ms wysyła `notifications/resources/updated` dla zmienionych subskrypcji i sprawdza watchdog.
- EOF (utrata klienta): jog anulowany, a proces czeka, aż zadania na urządzeniach się skończą — zadanie nie jest przerywane (REQ-15-107).

### 3. Transport Streamable HTTP (`bc_mcp::http::serve`, `burncraft-cli mcp --http [PORT]`)

- Nasłuch tylko na `127.0.0.1` (domyślny port 7985; host GUI ma 7984). Punkt końcowy `/mcp`: `POST` (jedna wiadomość JSON-RPC, bez wsadów), `GET` (strumień SSE powiadomień serwera), `DELETE` (koniec sesji).
- `initialize` → odpowiedź JSON z nagłówkiem `Mcp-Session-Id` (192 bity losowe). Kolejne żądania wymagają nagłówka (brak → 400, nieznana sesja → 404). Nieobsługiwany `MCP-Protocol-Version` → 400.
- `tools/call` odpowiada strumieniem SSE: powiadomienia o postępie, żądania `elicitation/create`, na końcu odpowiedź. Inne żądania — `application/json`. Powiadomienia i odpowiedzi klienta → 202.
- DNS rebinding: `Host` musi wskazywać pętlę zwrotną, a `Origin` (gdy jest) — `127.0.0.1`, `localhost` albo `[::1]`; inaczej 403.
- `initialize`, powiadomienia, odpowiedzi klienta i `stop_job` nie czekają na zamek serwera: drugi klient może się połączyć i zatrzymać maszynę, gdy pierwszy czeka na potwierdzenie.
- Sesja bez aktywności przez 30 min wygasa (utrata klienta). Wznawianie strumieni (`Last-Event-ID`) nie jest obsługiwane.
- LAN z tokenem i TLS (REQ-15-004) — M6.

### 4. Subskrypcje, prompty, postęp, anulowanie

- `resources/subscribe|unsubscribe` dla każdego zasobu. Serwer porównuje skrót treści co 100 ms (dla `burncraft://document` — skrót stanu: rewizja, zaznaczenie, aktywna zakładka) i wysyła `notifications/resources/updated {uri}` klientowi, który subskrybował.
- Nowe zasoby: `burncraft://device/{id}/status` (= `machine.status`, id = nazwa profilu w kodowaniu procentowym) i `burncraft://job/{id}` (= `job.status` + `machine.console`: stan, postęp 0–1, dziennik wymiany).
- Prompty: `prompts/list`, `prompts/get`. `prepare_cut_file` zawiera listę kontrolną (otwarte kształty w wypełnieniu, duplikaty, kerf, kolejność, obszar roboczy) i bieżące ostrzeżenia `plan.job`.
- Postęp: `_meta.progressToken` w `tools/call` → `notifications/progress {progressToken, progress, total: 1}` z `Session::tick` (co najmniej co 50 ms, rosnąco).
- Anulowanie: `notifications/cancelled {requestId}` ustawia flagę; polecenie kończy się błędem `cancelled` bez zmian w dokumencie (polecenia są atomowe), a serwer nie wysyła odpowiedzi.

### 5. Narzędzia urządzeń i polityka (ADR 0005)

- Narzędzia `list_devices`, `select_device`, `connect`, `disconnect`, `device_status`, `home`, `jog`, `move_to`, `set_origin`, `frame`, `pause_job`, `resume_job`, `stop_job`, `read_machine_settings` to nazwy poleceń rejestru (tabela w `bc-mcp/src/devices.rs`); schematy wejścia i wyjścia są z rejestru, poziom — z polecenia.
- **motion po potwierdzeniu użytkownika (REQ-15-094)**: gdy klient zgłosił `capabilities.elicitation`, serwer wysyła `elicitation/create` z opisem polecenia, urządzenia i parametrów oraz schematem `{confirm: boolean}`; tylko `accept` z `confirm: true` wykonuje polecenie. Bez elicitation — odmowa `confirmation_required`. Użytkownik może z góry dopuścić ruch przy uruchomieniu serwera (`--allow-motion`). config i fire pozostają odrzucane przez MCP (potwierdzenie w GUI i elicitation dla nich — M6, REQ-15-095/096).
- **stop nadrzędny (REQ-15-072, 106)**: `stop_job` wywołuje `job.stop` (poziom read) przez `Emergency` — tymczasową sesję na wspólnych urządzeniach, poza kolejką żądań i zamkiem serwera. Fizyczny E-stop (wejście reset sterownika) kończy zadanie alarmem, a `resume_job` w alarmie jest odrzucane przez sterownik.
- **watchdog (REQ-15-108)**: jog zlecony przez klienta jest anulowany (`device.jog_cancel`), gdy klient milczy dłużej niż limit (domyślnie 5 s, `--jog-watchdog`) albo zostanie utracony (EOF, `DELETE`, wygaśnięcie sesji). Zwykłe ruchy i zadania nie podlegają watchdogowi.

## Konsekwencje

- Testy: `crates/bc-mcp/tests/` — klient stdio (`Server::run_io` na rurach) i HTTP (surowe HTTP/1.1 z dekodowaniem chunked/SSE) na emulatorze GRBL; `crates/bc-cli/tests/mcp_http.rs` — proces `burncraft-cli mcp --http 0`.
- `bc-mcp` zależy od `bc-drivers` (stan urządzeń dla watchdoga i stopu), `axum`, `tokio`, `futures-core`, `getrandom`.
- Host GUI (`bc-host`) nadal przekazuje MCP z `--connect` metodą `mcp` bez kanału — tam bez elicitation i bez subskrypcji; okno potwierdzeń w GUI — M6. Punkt `/mcp` w hoście — do rozważenia razem z M2.5.
- `// UNVERIFIED:` fizyczny E-stop odwzorowany w testach jako reset sterownika (0x18, jak wejście RESET w GRBL); okablowanie E-stop w maszynach bywa różne (przerwanie zasilania, wejście drzwi).
