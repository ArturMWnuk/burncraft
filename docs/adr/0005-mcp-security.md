# ADR 0005 — Model bezpieczeństwa MCP

- Status: przyjęta (M0, 2026-10-28); część mechanizmów zaplanowana na M2/M6
- Wymagania: REQ-15-092…108, REQ-20-009

## Kontekst

Laser to ryzyko pożaru i obrażeń. Agent AI może się pomylić, a klient MCP może być zdalny. Wymagania §15.4: poziomy uprawnień, potwierdzenia, tokeny, limity, audyt, nadrzędny stop.

## Decyzja

### Poziomy (`SafetyTier`)

`read < design < motion < config < fire` — pole `safety_tier` każdego polecenia w rejestrze. Narzędzia MCP nie mają własnych uprawnień: `execute` i `batch` przyjmują poziom wywoływanego polecenia (dla wsadu — każdego kroku).

| Poziom | Przykłady | Domyślnie dla MCP |
|---|---|---|
| read | `document.inspect`, `selection.*`, `history.list` | bez pytań |
| design | `shape.*`, `object.*`, `layer.set`, `file.save` | bez pytań |
| motion | `device.jog`, `device.home`, ramka | potwierdzenie użytkownika (M2) |
| config | zapis ustawień maszyny | jawne potwierdzenie w GUI lub elicitation (M6) |
| fire | `start_job`, `device.test_fire` | jawne potwierdzenie w GUI lub elicitation (M6) |

### Zasady

1. **Decyzja w jednym miejscu**: `Policy::check(tier)` w `bc-mcp`, wywoływana przed wykonaniem każdego polecenia. Odmowa to wynik narzędzia z `isError: true` i `code: "confirmation_required"` (agent może poprosić użytkownika), a nie błąd protokołu.
2. **M0**: serwer nie ma jeszcze ścieżki potwierdzeń, więc odrzuca wszystko powyżej `design`. To zgodne z zasadą „bezpiecznie domyślnie”.
3. **Konfigurowalność**: `Policy.auto_allow` (Ustawienia, M6). Dla połączeń nielokalnych (`remote`) **fire zawsze wymaga potwierdzenia** — nie da się tego wyłączyć.
4. **Tokeny potwierdzenia (M6)**: jednorazowe, powiązane z `job_plan_id` i hashem planu (SHA-256 kanonicznego JSON planu + id urządzenia). Każda zmiana dokumentu zmienia hash, więc unieważnia token.
5. **Limity globalne (M6)**: maks. moc, maks. czas zadania, wymagany stan pokrywy/chłodzenia/wyciągu (gdy sterownik raportuje), „operator obecny” (aktywność w GUI w ostatnich N min).
6. **Stop nadrzędny (M2)**: `stop_job` jest zawsze dozwolony, bez potwierdzeń, z najwyższym priorytetem w kolejce sterownika. Utrata klienta MCP nie przerywa zadania; watchdog zatrzymuje jog.
7. **Dziennik audytu (od M0)**: każde wywołanie narzędzia → wiersz JSON Lines: `ts_ms`, `client` (z `clientInfo`), `tool`, `args` (duże ucinane), `outcome` (`ok`/`error`/`denied`), `error`. Domyślna lokalizacja: `%LOCALAPPDATA%\BurnCraft\mcp-audit.jsonl` (Windows), `~/Library/Logs/BurnCraft/` (macOS), `$XDG_STATE_HOME/burncraft/` (Linux); można ją zmienić przez `--audit-log`.
8. **Polecenia urządzenia** mają `requires_device = true`. Sesja bez urządzenia odrzuca je przed handlerem (`no_device`).

## Konsekwencje

- Testy: `bc-mcp/src/policy.rs` (kolejność poziomów, domyślna polityka, fire dla zdalnych) i `bc-mcp/src/tests.rs` (odmowa `device.jog`/`device.test_fire` przez execute i batch, zapis audytu).
- Scenariusz akceptacyjny 5 (§19): „start_job bez tokenu → odmowa” — test w M6 razem z narzędziem `start_job`.
- M2.4 (ADR 0012): potwierdzenie motion przez MCP elicitation albo `--allow-motion`, `stop_job` poza kolejką żądań, watchdog joga, utrata klienta nie przerywa zadania (testy `crates/bc-mcp/tests/mcp_safety.rs`).
