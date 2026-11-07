# Procedury ręczne — etap M0

Każda pozycja: wymaganie, procedura i wynik z 2026-10-28 (Windows 11, rustc 1.99.1). Powtarzalne polecenia podano w blokach.

## REQ-00-014 — clean-room
Procedura: przegląd repozytorium. Nie ma plików, kodu, ikon ani tekstów innych programów do laserów. W M0 nie analizowano też żadnych plików `.lbrn2`.
Wynik: **zgodne**. Repozytorium zawiera wyłącznie kod napisany od zera, schemat JSON i dokumentację.

## REQ-00-015 — własna nazwa i identyfikacja
Procedura: nazwa „BurnCraft”, binarka `burncraft-cli`, schemat URI `burncraft://`, MIME `application/x-burncraft`, własna paleta 30 kolorów (`bc-doc::DEFAULT_PALETTE`).
Wynik: **zgodne** (ikony i logo — przy GUI, M1).

## REQ-00-019 — THIRD_PARTY.md
Procedura: porównanie `THIRD_PARTY.md` z zależnościami bezpośrednimi w `Cargo.toml`.
Wynik: **zgodne**. Wszystkie zależności bezpośrednie i źródła wiedzy o protokołach są wymienione.

## REQ-00-020 — licencja
Procedura: `cargo metadata --no-deps` — 13 crate'ów `MIT OR Apache-2.0`, `bc-cli` `GPL-3.0-or-later`; ADR `docs/adr/0001-license.md`.
Wynik: **zgodne**. Uwaga: pełne teksty licencji nie są jeszcze dołączone (`LICENSES/README.md`).

## REQ-01-003 … REQ-01-016 — moduły workspace
Procedura:
```bash
cargo metadata --no-deps --format-version 1
```
Wynik: **zgodne**. Workspace zawiera pakiety bc-geom, bc-doc, bc-text, bc-image, bc-io, bc-plan, bc-sim, bc-drivers, bc-camera, bc-materials, bc-commands, bc-mcp, bc-script, bc-cli (14), a `cargo build --workspace` przechodzi. Funkcje modułów to osobne wymagania późniejszych etapów.
Identyfikatory: REQ-01-003, REQ-01-004, REQ-01-005, REQ-01-006, REQ-01-007, REQ-01-008, REQ-01-009, REQ-01-010, REQ-01-011, REQ-01-012, REQ-01-013, REQ-01-014, REQ-01-015, REQ-01-016.

## REQ-01-020 — rdzeń w Rust
Wynik: **zgodne** (ADR 0002).

## REQ-13-199 — macierz sprzętu
Wynik: **zgodne**. `docs/hardware-matrix.md` istnieje, ma poziomy sprzęt/emulator/eksperymentalne i stan na M0.

## REQ-15-012 — układ współrzędnych
Wynik: **zgodne**. ADR 0003; ten sam opis jest w `instructions` serwera i w polu `coordinate_system` raportu dokumentu.

## REQ-16-009 — praca offline
Procedura:
```bash
cargo tree --workspace -e normal --prefix none
```
Wyszukanie w drzewie `reqwest|hyper|ureq|curl|native-tls|rustls|tokio|isahc|surf` daje zero wyników. Jedyne połączenie sieciowe to sonda `--connect` do `127.0.0.1`.
Wynik: **zgodne**.

## REQ-16-010 — brak DRM
Wynik: **zgodne**. Kod nie zawiera mechanizmów licencjonowania ani aktywacji online.

## REQ-16-012 — logowanie z poziomami
Procedura: `BURNCRAFT_LOG=debug` i `BURNCRAFT_LOG=warn`, a następnie wysłanie `ping` do `burncraft-cli mcp`.
Wynik: **zgodne**. Na poziomie `debug` stderr zawiera wiersz INFO o starcie i wiersz DEBUG `żądanie MCP method="ping"`. Na poziomie `warn` stderr jest pusty. W obu przypadkach stdout zawiera tylko `{"id":1,"jsonrpc":"2.0","result":{}}`.

## REQ-18-014 … REQ-18-017 — dokumentacja MCP
Wynik: **zgodne**. `docs/mcp.md` zawiera listę narzędzi z przykładami JSON (REQ-18-014), konfigurację `claude_desktop_config.json` (REQ-18-015) i `.mcp.json` (REQ-18-016) z plikami w `examples/` oraz politykę bezpieczeństwa (REQ-18-017, także ADR 0005).

## REQ-18-018 — ADR-y
Wynik: **zgodne**. `docs/adr/0001…0005`.

## REQ-18-019 — CONTRIBUTING.md
Wynik: **zgodne**. Zawiera sekcję „Dodawanie sterownika lasera”.

## REQ-20-001 — plan etapu
Wynik: **zgodne**. `docs/plans/M0-plan.md`.

## REQ-20-004 — znaczniki UNVERIFIED
Wynik: **zgodne dla M0**. W M0 nie ma kodu protokołów, więc nie było niepewnych szczegółów. Zasada jest opisana w `CONTRIBUTING.md` i `THIRD_PARTY.md`.

## REQ-20-005 … REQ-20-009 — ADR 0001–0005
Wynik: **zgodne**. Identyfikatory: REQ-20-005, REQ-20-006, REQ-20-007, REQ-20-008, REQ-20-009.

## REQ-20-010 — szkielet repozytorium
Wynik: **zgodne**. Układ katalogów wg §1.1: `crates/` (14), `app/`, `web/`, `bridge/`.

## REQ-21-001 — rejestr przed kodem
Wynik: **zgodne**. `requirements.yaml` (1144 pozycje) powstał przed pierwszym crate'em. `REQUIREMENTS.md` zawiera metodę, konflikty i zestawienie.

## REQ-21-003, REQ-21-004 — zestawienie i lista pominiętych fragmentów
Wynik: **zgodne**. `REQUIREMENTS.md`: tabela „sekcja → liczba wymagań” (REQ-21-003) i tabela „Fragmenty świadomie nieuznane za wymagania” (REQ-21-004).

## REQ-21-013 — przegląd etapu
Wynik: **zgodne**. `docs/reviews/M0-review.md`.

## REQ-21-014 — format DEVIATIONS.md
Wynik: **zgodne**. Kolumny `ID | REQ | Opis | Powód | Wybrany wariant | Data | Decyzja zleceniodawcy` są sprawdzane przez req-check (`test_req_21_011_deviation_needs_approved_entry`).

## REQ-19-020 … REQ-19-024, REQ-20-011, REQ-21-012 — CI
Procedura: przebieg GitHub Actions workflow `CI` na `main` (repozytorium prywatne ArturMWnuk/burncraft), run **40670642608**, commit `12cfd2d`, 2026-10-28.
Każde zadanie wykonuje fmt --check, clippy -D warnings, cargo test, testy req-check i req-check z logami testów.
Wynik: **zgodne**, wszystkie zadania success:
- REQ-19-020 Windows (`windows-latest`) — 3 min 49 s
- REQ-19-021 macOS x64 (`macos-15-intel`) — 3 min 6 s
- REQ-19-022 macOS arm64 (`macos-14`) — 37 s
- REQ-19-023 Linux x64 (`ubuntu-24.04`) — 1 min 50 s
- REQ-19-024 Linux arm64 (`ubuntu-24.04-arm`) — 1 min 22 s
- REQ-20-011 CI etapu M0, REQ-21-012 req-check w CI — krok `req_check.py --test-log …` zakończony kodem 0 na wszystkich platformach.
Uwaga: pierwszy przebieg (40666725918) wykrył błąd kodowania konsoli Windows w req-check i wycofany runner `macos-13`; oba naprawione w `12cfd2d`.