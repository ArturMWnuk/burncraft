# Przegląd zgodności — etap M1 (edytor wektorowy, model dokumentu, import/eksport, GUI)

Data: 2026-11-03 · Środowisko: Windows 11, rustc 1.99.1 (MSRV 1.88), Python 3.12, Node 20, Edge (Playwright) · Procedura: spec. §21.3

## Podsumowanie

| | Liczba |
|---|---|
| Wymagania etapu M1 | **202** |
| Zweryfikowane (`verified`) | **202** (162 testem automatycznym, 40 procedurą ręczną ze zrzutami) |
| Częściowe (`in_progress`) | 0 |
| Brakujące (`todo`) | 0 |
| Przeniesione do innego etapu (zatwierdzone odstępstwa) | **2** — REQ-01-024 (DEV-003), REQ-02-072 (DEV-004), oba do M4 |
| Zmiana decyzji technicznej (zatwierdzona) | **1** — DEV-002 (Clipper2 jako port w czystym Rust) |

**Etap M1 jest ZAMKNIĘTY (2026-11-03)** (`milestones.yaml`: `M1: closed`). `req-check` przechodzi przy zamkniętym etapie.

## Odstępstwa

| DEV | REQ | Opis | Decyzja |
|---|---|---|---|
| DEV-002 | REQ-04-057…070, REQ-19-001, 002 | Booleany i offsety: `clipper2-rust` (port w czystym Rust, ten sam algorytm) zamiast Clipper2 w C++ (ADR 0006) | „Port w czystym Rust” — zleceniodawca, 2026-10-28 |
| DEV-003 | REQ-01-024 | 60 fps przy obrazach 50 MPix przeniesione do M4 razem z obiektem obrazu | „Przesuń do M4” — zleceniodawca, 2026-11-02 |
| DEV-004 | REQ-02-072 | Tekst z DXF przeniesiony do M4 razem z silnikiem tekstu `bc-text`; do tego czasu import pomija tekst z ostrzeżeniem | „Przenieś do M4” — zleceniodawca, 2026-11-03 |

## Decyzje zleceniodawcy podjęte przy zamykaniu (bez odstępstw)

- **REQ-01-029** („Dodaj do rejestru”, 2026-11-03): operacje widoku, narzędzi, przyciągania, aktywnej warstwy, punktu odniesienia i okien dialogowych są poleceniami rejestru wykonywanymi przez okno (`executor: "window"`). Agent MCP steruje nimi przez `--connect`. Test E2E sprawdza, że każda kontrolka pasków ma `data-command` z rejestru.
- **REQ-16-006** („Zrób teraz”, 2026-11-03): postęp `gui.progress` po 100 ms i anulowanie `gui.cancel` (`job.cancel`) z wycofaniem zmian; pasek postępu z przyciskiem Anuluj. Przy okazji usunięto kwadratowe sprawdzanie zakresu po tablicach: tablica 300 × 300 kopii zajmowała ponad 10 s, teraz ułamek sekundy.

## Zakres etapu (podsumowanie)

| Część | Zakres | Dowód |
|---|---|---|
| M1.1 | kształty parametryczne, booleany, offsety, tablice, wyrównanie, operacje na ścieżkach, pomiar, paleta, T1/T2, skróty MCP | `bc-geom`, `bc-commands` (`tests_m1.rs`), `bc-mcp` |
| M1.2 | wirtualne tablice, klony, pomocnicze, szablony, autozapis, kopie preferencji, wiele dokumentów, schowek, edycja w grupach | `tests_m12.rs` |
| M1.3 | import/eksport SVG, DXF, HPGL, `.lbrn2` (clean-room), ADR 0008 | `bc-io`, `tests_m13.rs`, fuzzing |
| M1.4 | GUI Tauri 2 + WebGL2, narzędzia, węzły, paski, panele, Fluent pl/en, motywy, kanał sterowania, wydajność | Playwright (`web/e2e`), `docs/procedures/M1-gui-checks.md` |
| Rdzeń (domknięcie) | okręgi styczne, nakładające się odcinki, wygładzanie, łuki, asystent booleanów, kopie wzdłuż ścieżki, wpusty/czopy, dokowanie, Quick Nest, przeglądarka kopii preferencji | `tests_m1core.rs`, `bc-geom::shaping`, `web/e2e/prefs.spec.ts` |

Pełna macierz z nazwami testów: [`docs/traceability.md`](../traceability.md).

## Kroki przeglądu (§21.3)

1. **Ponowne przeczytanie** §1.2–1.4, §2–4, §15, §16 i porównanie z rejestrem. Audyt GUI wykazał, że REQ-01-029 i REQ-16-006 były tylko częściowo spełnione. Oba domknięto po decyzji zleceniodawcy.
2. **Weryfikacja na działającym programie**: testy `auto` uruchamiają rzeczywisty kod. Testy hosta rozmawiają z `bc-host` po WebSocket, a testy E2E sterują zbudowanym frontendem z `burncraft-cli serve`. 40 procedur ręcznych ma zrzuty z działającej aplikacji (`docs/procedures/M1-gui-checks.md`); REQ-03-015 zmierzył zleceniodawca linijką.
3. **Testy lokalne** (2026-11-03): `cargo test --workspace` — 295 testów Rust (ok), `unittest` — 13 (ok), Playwright (Edge) — 60 ok, 8 pominiętych (generatory zrzutów do procedur ręcznych, uruchamiane z `BC_SCREENSHOTS=1`). `cargo clippy --workspace --all-targets -D warnings` i `cargo fmt --check` — czyste.
4. **req-check** z logami cargo, unittest i Playwright: 0 błędów. Kontrola bramki: przy `M1: closed` req-check nadal zgłasza 0 błędów.
5. **CI**: pełna macierz (`workflow_dispatch`) — 5 platform, aplikacja Tauri na 3 systemach, fuzzing 3 importerów. Pierwszy przebieg (41685337878, `bd40ff2` w archiwum `ArturMWnuk/burncraft-history`) wykrył niestabilny test wydajności na macOS Intel (warunek oczekiwania spełniony przed nadejściem sceny). Poprawka: `976e187`. Przebieg końcowy: zob. sekcja „CI”.

## CI

Prywatne repozytorium sprzed otwarcia projektu (dziś archiwum `ArturMWnuk/burncraft-history`; tam są wymienione commity i przebiegi), commit `aa09c1e` (kod końcowy etapu):
- push, run **41699058410**: Linux x64, Windows, aplikacja Tauri (Linux) — success.
- pełna macierz (`workflow_dispatch`), run **41699074436**: testy na Windows, macOS x64 (`macos-15-intel`), macOS arm64 (`macos-15`), Linux x64 i Linux arm64; aplikacja Tauri na Windows, macOS i Linux; fuzzing importerów SVG, DXF i `.lbrn2`. Wszystkie 11 zadań: success.

## Zadania otwarte poza wymaganiami M1

- REQ-01-024 i REQ-02-072 w M4 (DEV-003, DEV-004).
- `gui.progress` trafia do klientów kanału; powiadomienia MCP `notifications/progress` dla agentów stdio — razem z pełnym MCP (M6).
- Prawdziwy nesting kształtów (narzędzie `nest`, REQ-04-096…103) — M6.
