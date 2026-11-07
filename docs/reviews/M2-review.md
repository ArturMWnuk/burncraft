# Przegląd zgodności — etap M2 (planer, G-code, sterowniki GRBL, MCP urządzeń, GUI maszyny)

Data: 2026-11-07 · Środowisko: Windows 11, rustc 1.99.1 (MSRV 1.88), Python 3.12, Node 20, Edge (Playwright) · Procedura: spec. §21.3

## Podsumowanie

| | Liczba |
|---|---|
| Wymagania etapu M2 | **227** |
| Zweryfikowane (`verified`) | **227** (133 testem automatycznym, 80 na emulatorze GRBL, 13 procedurą ręczną ze zrzutami, 1 testem sprzętowym) |
| Częściowe (`in_progress`) | 0 |
| Brakujące (`todo`) | 0 |
| Odstępstwa (`deviation`) | 0 |

**Etap M2 jest ZAMKNIĘTY (2026-11-07)** (`milestones.yaml`: `M2: closed`). `req-check` przechodzi przy zamkniętym etapie.

## Odstępstwa

Brak odstępstw w M2.

## Decyzje zleceniodawcy w trakcie etapu

- **REQ-19-007** („Własny + grbl-sim w CI”, 2026-11-04): własny emulator GRBL w Rust we wszystkich testach i jako urządzenie fikcyjne; w CI na Linuksie test porównawczy z grbl-sim budowanym ze źródeł (kod GPL poza produktem i repozytorium).
- **REQ-19-017** (2026-11-04): test na laserze diodowym zleceniodawcy według checklisty na koniec M2. Wykonany 2026-11-05 (sekcja „Test sprzętowy”).
- **CI** (2026-11-04, „Odchudź CI”): push uruchamia tylko Linux x64; pełna macierz ręcznie (`workflow_dispatch`) raz na koniec etapu.
- **CI przy zamknięciu** („Close M2 without it”, 2026-11-07): GitHub Actions zablokowany przez limit wydatków; etap zamknięty bez zielonej pełnej macierzy na kodzie końcowym (szczegóły w sekcji „CI”).
- **REQ-03-032** (2026-11-07): zleceniodawca zgłosił, że w GUI nie da się ustawić parametrów cięcia z §7.2 poza podstawowymi. Wymaganie z M1 było spełnione tylko polami podstawowymi panelu Warstwy. Dodano okno ustawień warstwy ze wszystkimi parametrami (zatwierdzone razem ze wznowieniem M2.5).

## Zakres etapu (podsumowanie)

| Część | Zakres | Dowód |
|---|---|---|
| M2.1 | model zadania i planer: tryby Line/Fill/Offset Fill/Fill+Line/Flood Fill, parametry warstwy §7.2, mostki, lead-in/out, kolejność i optymalizacja, strefy zakazane, model czasu, ADR 0009 (80 wymagań) | `bc-plan` (testy `crates/bc-plan/tests`), `tests_m2.rs` |
| M2.2 | kompilator G-code `grbl` i `gcode`, raster, M73, dwa lasery, szablony, makra, import G-code, schemat dokumentu 5, ADR 0011 (18) | `crates/bc-drivers/tests/compile.rs`, golden |
| M2.3 | trait `Driver`, sterownik GRBL z 19 odmianami, transporty serial/TCP/WebSocket, emulator GRBL, urządzenia i profile, polecenia `machine.*`, `device.*`, `job.*`, `macro.*`, ADR 0010 (79) | `driver_grbl.rs`, `driver_jobs.rs`, grbl-sim w CI |
| M2.4 | MCP przez Streamable HTTP, subskrypcje, prompty, postęp, anulowanie, narzędzia urządzeń i zadań, polityka motion, ADR 0012 (26) | `crates/bc-mcp/tests` |
| M2.5 | GUI maszyny: okna Laser, Przesuń, Konsola, Makra, Podgląd z odtwarzaniem, znacznik głowicy, kerf, wyrównanie do lasera; okno ustawień warstwy; test sprzętowy (24) | Playwright `web/e2e/machine.spec.ts`, `layer-settings.spec.ts`, `docs/procedures/M2-gui-checks.md`, `docs/procedures/M2-hardware-grbl.md` |

Pełna macierz z nazwami testów: [`docs/traceability.md`](../traceability.md).

## Test sprzętowy (REQ-19-017)

Zleceniodawca, 2026-11-05: Two Trees TTS-20 PRO, płyta MKS DLC32 V4.00, GRBL 1.1h.2023112101, USB CH340 na COM3. Wyniki: [`docs/procedures/M2-hardware-grbl.md`](../procedures/M2-hardware-grbl.md), [`docs/hardware-matrix.md`](../hardware-matrix.md).

- Tak (14): połączenie, Konsola (`$I`, `$$`, `?`), bazowanie, alarm krańcówki i odblokowanie, jog XY, zapamiętana pozycja, znacznik głowicy, podgląd, ramka, cięcie wektorowe (kwadrat 30 × 30 mm, otwór Ø10 mm), grawer, pauza i wznowienie, Stop, start od wybranego miejsca.
- Pominięte (2): jog Z (laser bez osi Z), odłączenie USB w trakcie pracy (decyzja zleceniodawcy).
- Usterki znalezione w teście i poprawione przed zamknięciem (para `b22fb92` red / `dd05c1f` green): pole `alarm` zostawało po `$X` i `$H`; Two Trees nierozpoznany z `$I`; obszar profilu z kreatora 400 zamiast `$130/$131` = 419 mm; estymacja czasu bez kinematyki podłączonej maszyny (REQ-09-008); checklista zakładała kolejność „grawer najpierw”, niezgodną z domyślnym `cut_last = false`.
- Ograniczenie (nie odstępstwo): rozszerzone ustawienia płyty MKS `$40–$54` nie mają publicznie potwierdzonych znaczeń. Edytor pokazuje je jako „ustawienie rozszerzone producenta płyty” bez zakresów; nazwy osi A/B/C i `$28` są opisane.
- Nie potwierdzono na sprzęcie widoczności licznika „przejście N/M” i czasu po zakończeniu w oknie Laser (brak odpowiedzi na to pytanie). Stan zadania w `job.status` był poprawny; okno zweryfikowano na emulatorze (REQ-03-053/054).

## Kroki przeglądu (§21.3)

1. **Ponowne przeczytanie** §7–10, §13, §15, §19 i porównanie z rejestrem. Na zgłoszenie zleceniodawcy uzupełniono REQ-03-032 (okno ustawień warstwy). Każda nowa kontrolka ma polecenie rejestru i `data-command` (REQ-01-029, test E2E).
2. **Weryfikacja na działającym programie**: testy `emu` sterują sterownikiem GRBL przez emulator (w CI także porównanie z grbl-sim). Testy E2E sterują zbudowanym frontendem z `burncraft-cli serve` i emulatorem jako urządzeniem. 13 procedur ręcznych ma zrzuty z działającej aplikacji (`docs/procedures/M2-gui-checks.md`), a REQ-19-017 sprawdził zleceniodawca na fizycznym laserze.
3. **Testy lokalne** (2026-11-07): `cargo test --workspace` — 514 testów Rust (ok), `unittest` — 13 (ok), Playwright (Edge) — 86 ok, 16 pominiętych (generatory zrzutów, `BC_SCREENSHOTS=1`). `cargo clippy --workspace --all-targets -D warnings` i `cargo fmt --check` — czyste.
4. **req-check** z logami cargo, unittest i Playwright: 0 błędów. Kontrola bramki: przy `M2: closed` req-check nadal zgłasza 0 błędów.
5. **CI**: pełna macierz (`workflow_dispatch`) — sekcja „CI”. Ostatni przebieg przed zablokowaniem Actions: 10 z 11 zadań zielonych; zamknięcie bez przebiegu na kodzie końcowym z decyzji zleceniodawcy.

## CI

Repozytorium prywatne `ArturMWnuk/burncraft`. **Etap zamknięto bez zielonego przebiegu pełnej macierzy na kodzie końcowym — decyzja zleceniodawcy** („Close M2 without it”, 2026-11-07), bo GitHub Actions przestał uruchamiać zadania (limit wydatków / płatność konta, run 42380358120 na `c2a8088`: „The job was not started because recent account payments have failed or your spending limit needs to be increased”).

Przebiegi pełnej macierzy (`workflow_dispatch`) — każdy znalazł inny problem, naprawiony przed następnym:
1. run 42367129224 (`dd05c1f`) — anulowany: push CI (42367119417, Linux x64) wykrył utratę szybkich zmian w oknie ustawień warstwy; poprawka `6270a83` (test) / `f7200ec`.
2. run 42375999789 (`f7200ec`) — Linux arm64: przeglądarki Playwright z cache x64 (wspólny klucz cache dla architektur); poprawka konfiguracji CI `ec4bf26`. Anulowany.
3. run 42376891140 (`ec4bf26`) — Linux x64: Konsola traciła fokus przy przebudowie okna; poprawka `afe7526` (test) / `a4d1b59`. Anulowany po wyniku.
4. run **42378563274** (`a4d1b59`): **10 z 11 zadań success** — testy na Linux x64, Linux arm64, macOS x64, macOS arm64; aplikacja Tauri na Windows, macOS i Linux; fuzzing SVG, DXF i `.lbrn2`. Windows: 2 niestabilne testy E2E (Podgląd ×2 kończył się przed klikiem Pauza; kliknięcie przy skali 2× liczone przed ustaleniem układu panelu bocznego), ustabilizowane w `c2a8088`.

Kod końcowy `c2a8088` różni się od `a4d1b59` tylko tymi dwoma testami E2E. Lokalnie (Windows, Edge): Playwright 86 ok, 16 pominiętych; oba testy 8/8 przy `--repeat-each 4`. Push CI na `a4d1b59` (42377909857, Linux x64): success. Do zrobienia po odblokowaniu Actions: jeden przebieg pełnej macierzy na `main` jako potwierdzenie.
## Zadania otwarte poza wymaganiami M2

- Przebieg pełnej macierzy CI na `main` po odblokowaniu GitHub Actions (potwierdzenie stabilizacji testów z `c2a8088`).
- Kalibracja estymacji czasu na historii zadań (REQ-09-008, część „samokalibracja”) — M6.
- Punkty `// UNVERIFIED:` niepotwierdzone w teście: wiązka w postoju przy M3 (tryb punktowy), M73 w odmianach GRBL, XMODEM FluidNC, polecenia czerwonego lasera.
- Znaczenia rozszerzonych ustawień płyt producentów (MKS DLC32 `$40–$54` i podobne) — po zdobyciu dokumentacji producenta albo przechwytu od użytkownika.
- Kerf planera dla osobnych, zagnieżdżonych obiektów (np. okrąg i prostokąt jako dwa obiekty) idzie na zewnątrz dla obu; do wewnątrz idą tylko otwory w tej samej ścieżce (zachowanie z M2.1, do decyzji w M3).
- Sterowniki Ruida i Lihuiyu — M3; galvo — M5.
