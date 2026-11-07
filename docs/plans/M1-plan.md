# Plan etapu M1 — edytor wektorowy, import/eksport, GUI

Wymaganie procesowe REQ-20-001. Zakres = 204 pozycje `ms: M1` w `requirements.yaml`. Etap powstaje w czterech przyrostach. W każdym najpierw piszę testy (osobny commit „testy”, który w CI jest czerwony), a potem implementację („green”). Historia git jest dowodem kolejności dla REQ-20-002 (po DEV-001 obowiązuje bez wyjątków).

| Przyrost | Zakres | Wymagania (główne) | Weryfikacja |
|---|---|---|---|
| **M1.1 Geometria i polecenia edycji** | kształty parametryczne (wielokąt, gwiazda, podwójna gwiazda, presety, łuki), booleany, offsety, tablice, wyrównanie/rozkład, kolejność rysowania, operacje na ścieżkach (konwersja, kierunek, punkt startowy, rozbijanie, zamykanie, łączenie, duplikaty, redukcja węzłów, otoczka wypukła, fazowanie/zaokrąglanie), pomiar, atrybuty obiektów, paleta warstw, T1/T2, narzędzia MCP ze skrótami | REQ-02-003, 008–011, 025–039, 045; REQ-04-003…093 (część bez GUI); REQ-15-020, 021, 031, 032, 037…043, 046, 047; REQ-19-001, 002 | testy jednostkowe + MCP |
| **M1.2 Model dokumentu** | wirtualne tablice, klony, obiekty pomocnicze, szablony, autozapis/odzyskiwanie, kopie preferencji, wiele dokumentów, kopiowanie między dokumentami, edycja w grupach, undo ze współdzieleniem struktury | REQ-02-019…021, 053…061; REQ-04-055, 073 | testy jednostkowe |
| **M1.3 Import/eksport** | SVG (DPI, warstwy wg koloru i nazwy, zagnieżdżenia), DXF R12–2018 (splajny, bloki, łuki, tekst, jednostki; eksport AC1009/AC1021), HPGL, `.lbrn`/`.lbrn2` (na pliku `plik_przykładowy.lbrn2`), fuzzing, CLI `convert`/`export`, MCP `open_file`/`save_file`/`import_file`/`export_file` | REQ-00-017; REQ-02-062…092; REQ-14-012, 015; REQ-15-027…030; REQ-19-003…005 | testy jednostkowe, golden, fuzz w CI (Linux, nightly) |
| **M1.4 GUI** | Tauri 2 + TypeScript, obszar roboczy WebGL2 (siatka, linijki, przyciąganie, prowadnice, zoom), paski narzędzi, panele (Warstwy, Obiekty, Właściwości kształtu, Historia, Notatki), narzędzia rysowania i edycji węzłów, tryb początkującego, motywy, HiDPI, gesty, edytor skrótów, tłumaczenia Fluent (en, pl), kanał `--control 7984` i `burncraft-cli mcp --connect`, `screenshot`, wydajność (200 tys. węzłów, 50 MPix), Playwright | REQ-01-017, 021…024, 029, 036, 039, 042, 043; REQ-03-*; REQ-04-001, 002, 020…022, 045…048 (interaktywne); REQ-15-002, 026; REQ-16-006; REQ-19-013 | Playwright, testy wydajności, procedury ręczne ze zrzutami |

### Podział M1.4

- **M1.4a — host i szkielet GUI** (zrealizowany 2026-10-29; ręczne procedury ze zrzutami razem z M1.4c): `bc-host` (HTTP + JSON-RPC/WebSocket, token, kontrola Origin, powiadomienia `doc.changed`), `burncraft-cli serve` i `mcp --connect` (proxy do żywego okna), narzędzie `screenshot`. Frontend: kanwa WebGL2 (siatka, linijki, obszar maszyny, pan/zoom, zoom do zaznaczenia, strony i 100%), zaznaczanie i przesuwanie, panele Warstwy, Obiekty, Właściwości (edycja numeryczna), Historia, Notatki. Playwright.
- **M1.4b — narzędzia i edycja**: rysowanie (linie/Béziery z wpisywaniem długości i kątów, łuki, prostokąt, elipsa, wielokąt, gwiazda, pomiar, prowadnice), przyciąganie, edycja węzłów z menu kontekstowym, lasso, gumka, wypaczanie, paski narzędzi konfigurowalne i dokowalne, edytor skrótów, tłumaczenia (en, pl), motywy, tryb początkującego, HiDPI, gesty.
  - **b1** (zrealizowany 2026-10-31): narzędzia linia/Bézier (z wpisywaniem długości, „długość<kąt”, „@dx,dy”), prostokąt, elipsa, wielokąt, gwiazda; przyciąganie do siatki, węzłów, środków odcinków, środków obiektów i krawędzi; pasek tworzenia i opcji, kursor zależny od narzędzia.
  - **b2a** (zrealizowany 2026-10-31): model węzłów w `bc-geom`, polecenia `node.*` i `path.cut`, narzędzie edycji węzłów z menu kontekstowym i pozycją liczbową.
  - **b2b** (zrealizowany 2026-10-31): lasso, prowadnice ręczne i automatyczne, obrót i skala dwupunktowa, gumka, wypaczanie 4-punktowe, siatka, obwiednia, przycinanie do przecięć.
  - **b3a** (zrealizowany 2026-10-31): edytor skrótów oparty na rejestrze (`shortcuts.*`, PrefStore), drzewo obiektów (przeciąganie, kolejność, wyszukiwanie, ukrywanie i blokowanie; `object.arrange`), tłumaczenia Fluent pl/en, motywy ciemny/jasny/systemowy, mm/in z przeliczaniem w polach, 9-punktowy punkt odniesienia.
  - **b3b** (zrealizowany 2026-11-01): paski konfigurowalne i dokowalne z zapisem układów (główny, tworzenia, trybów, modyfikatorów, numeryczny, wyrównywania, węzłów), tryb początkującego, podpowiedzi do pól, HiDPI.
- **M1.4c — okno i wydajność**: powłoka Tauri 2 (`burncraft --control`), testy wydajności (200 tys. węzłów, 50 MPix), brak blokowania UI (REQ-16-006), procedury ręczne ze zrzutami.
  - **c1** (zrealizowany 2026-11-01): okno Tauri 2, zadanie CI `app`, ADR 0007.
  - **c2** (zrealizowany 2026-11-02): scena przyrostowa, rysowanie jednym wywołaniem, drzewo wirtualizowane; REQ-01-024 (50 MPix) przesunięte do M4 (DEV-003).
  - **c3** (zrealizowany 2026-11-02): procedury ręczne GUI ze zrzutami (`docs/procedures/M1-gui-checks.md`, zrzuty z `web/e2e/procedures.spec.ts` i z okna Tauri). Przegląd zrzutów ujawnił luki, które domknięto testami i kodem: parametry kształtów w oknie Właściwości (`shape.set_params`), podgląd stanów historii (`edit.history_preview`), obrazy w notatkach, kalibracja rzeczywistego rozmiaru (`ui.screen_scale`), gesty dwoma palcami, własne kursory narzędzi, widoczność ciemnych kolorów warstw na ciemnym tle. REQ-03-015: pomiar linijką zleceniodawcy — 68 mm bez kalibracji, 100 mm po kalibracji.

### Domknięcie rdzenia M1

- **Polecenia rdzenia** (zrealizowane 2026-11-03): moduł `bc-geom::shaping` i polecenia rejestru `shape.tangent_circles` (REQ-04-019), `path.remove_overlaps` (REQ-04-040), `path.smooth` (REQ-04-043), `path.lines_to_arcs` (REQ-04-044), `path.boolean_preview` — asystent booleanów (REQ-04-069), `array.along_path` (REQ-04-078), `path.resize_slots` (REQ-04-082), `object.dock` (REQ-04-091), `object.quick_nest` (REQ-04-095). Opis w `docs/mcp.md`. Testy: `crates/bc-commands/src/tests_m1core.rs`.
- **REQ-02-072 (tekst z DXF)** przeniesione do M4 razem z silnikiem tekstu `bc-text` (DEV-004, decyzja 2026-11-03). W M1 importer pomija TEXT/MTEXT i tekst w blokach z jednym ostrzeżeniem, a `ignore_text` pomija je bez ostrzeżenia; testy `req_02_072_*` w `crates/bc-io/tests/import_export.rs`.
- **REQ-02-056 (przeglądarka kopii preferencji)** zrealizowane 2026-11-03: okno Ustawienia → „Kopie preferencji…” (`web/src/dialogs.ts::openBackups`) z listą kopii od najnowszej (data, rozmiar), podglądem różnic względem bieżących ustawień (nowe polecenie `prefs.backup_diff`) i przywracaniem po potwierdzeniu (`prefs.restore`, potem ponowne wczytanie preferencji); tłumaczenia pl/en. Testy: `tests_m12.rs::req_02_056_*`, `web/e2e/prefs.spec.ts::req_02_056_*`.
- **REQ-01-029** zrealizowane 2026-11-03 (decyzja „Dodaj do rejestru”): widok, narzędzia, przyciąganie, aktywna warstwa, punkt odniesienia i okna dialogowe jako polecenia rejestru wykonywane przez okno (`bc-commands/src/window.rs`, `executor: "window"`, przekazywanie przez host `ui.execute`). Testy: `tests_m1close.rs`, `host.rs::req_01_029_*`, `web/e2e/commands.spec.ts`.
- **REQ-16-006** zrealizowane 2026-11-03 (decyzja „Zrób teraz”): postęp `gui.progress` i anulowanie `gui.cancel` / `job.cancel` z wycofaniem zmian, pasek postępu w oknie; liniowe sprawdzanie zakresu po tablicach. Testy: `host.rs::req_16_006_*`, `web/e2e/progress.spec.ts`.
- **Zamknięcie etapu** (2026-11-03): pełna macierz CI run 41699074436 na `aa09c1e` — wszystkie zadania zielone; przegląd `docs/reviews/M1-review.md`; `milestones.yaml`: `M1: closed`.

## ADR-y etapu

- **ADR 0006** — jądro geometryczne: Clipper2 do booleanów i offsetów, spłaszczanie krzywych, tolerancje, skala całkowitoliczbowa.
- **ADR 0007** — architektura GUI: Tauri 2, kanał sterowania JSON-RPC/WebSocket, renderowanie WebGL2, współdzielony frontend z `web/`.
- **ADR 0008** (w M1.3) — import/eksport: mapowanie warstw, jednostki i DPI, odwrócenie osi Y, analiza `.lbrn2` (clean-room).

## Ryzyka i pytania do zleceniodawcy (zgłaszane, gdy się pojawią)

- 2026-10-31: wyczerpany limit minut GitHub Actions (repozytorium prywatne, macOS liczony ×10), a GitHub nie uruchomił przebiegu 41234900922. Decyzja zleceniodawcy („Odchudź CI”): przy każdym pushu testy na Linux x64 i Windows; pełna macierz pięciu platform (REQ-19-020…024) i fuzzing (REQ-19-003…005) codziennie oraz ręcznie (`workflow_dispatch`) przed zamknięciem etapu. Dowodem wymagań platformowych jest ostatni zielony przebieg pełnej macierzy.

- Fuzzing `cargo-fuzz` wymaga nightly i libFuzzer — w CI tylko na Linuksie (na Windows nieobsługiwane przez libFuzzer w tym trybie).
- Wymagania GUI weryfikowane `manual` dostaną procedury ze zrzutami ekranu z działającej aplikacji.
- Próg wydajności 60 fps zależy od sprzętu. Procedura pomiaru i maszyna referencyjna zostaną zapisane w ADR 0007.
