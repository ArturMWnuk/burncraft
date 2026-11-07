# Przegląd zgodności — etap M0 (fundamenty)

Data: 2026-10-28 · Środowisko: Windows 11, rustc 1.99.1, Python 3.12 · Procedura: spec. §21.3

## Podsumowanie

| | Liczba |
|---|---|
| Wymagania etapu M0 | **112** |
| Zweryfikowane (`verified`) | **111** (78 testem automatycznym, 33 procedurą ręczną, w tym 7 przebiegiem CI) |
| Częściowe (`in_progress`) | 0 |
| Brakujące (`todo`) | 0 |
| Odstępstwa (zatwierdzone) | **1** — DEV-001 |

**Etap M0 jest ZAMKNIĘTY (2026-10-28)** (`milestones.yaml`: `M0: closed`). `req-check` przechodzi przy zamkniętym etapie.

## Odstępstwa

| DEV | REQ | Opis | Decyzja |
|---|---|---|---|
| DEV-001 | REQ-20-002 | W M0 testy powstawały razem z kodem, nie przed nim (dotyczy tylko M0). Od M1 testy przed implementacją | „Zaakceptuj odstępstwo” — zleceniodawca, 2026-10-28 (`DEVIATIONS.md`) |

## CI (zweryfikowane 2026-10-28)

Repozytorium prywatne `ArturMWnuk/burncraft`, run **40670642608** (commit `12cfd2d`): Windows, macOS x64 (`macos-15-intel`), macOS arm64, Linux x64 i Linux arm64 — wszystkie zadania success (od 37 s do 3 min 49 s). Pierwszy przebieg (40666725918) wykrył dwa błędy, oba naprawione w `12cfd2d`:
- `req-check` padał na konsoli Windows (cp1252 nie koduje polskich znaków),
- runner `macos-13` jest wycofany, więc zadanie stało w kolejce bez końca.

## Zweryfikowane

| REQ | Opis | Dowód |
|---|---|---|
| REQ-00-014, 015, 019, 020 | clean-room, własna identyfikacja, THIRD_PARTY, licencja | `docs/procedures/M0-manual-checks.md` |
| REQ-00-016 | własny format pliku | `bc-doc::bcraft::req_00_016_own_native_format` |
| REQ-01-001 | rdzeń bezgłowy | `bc-cli/tests/mcp_stdio.rs::req_15_001_…` |
| REQ-01-003…016 | 14 modułów workspace | procedura (`cargo metadata`) |
| REQ-01-020 | Rust | ADR 0002 |
| REQ-01-030…035 | spec polecenia: id, opis, schematy, flagi, skrót | `bc-commands::req_01_030_to_035_every_command_has_full_spec` |
| REQ-01-037 | ids albo zaznaczenie | `req_01_037_explicit_ids_or_selection` |
| REQ-01-038 | wsad = jeden krok historii, atomowo | `req_01_038_batch_is_one_history_step_and_atomic` |
| REQ-01-044 | `--connect` bez okna → sesja bezgłowa | `mcp_stdio.rs::req_01_044_…` |
| REQ-02-001, 002, 004, 005 | ścieżki: podścieżki, linie, Béziery 3. i 2. stopnia | `bc-geom` tests |
| REQ-02-006, 007 | prostokąt z promieniem, elipsa | `bc-geom` tests |
| REQ-02-018, 023, 024, 027, 028 | grupa, warstwa, transformacja niewypiekana, widoczność, nazwa | `bc-doc`, `bc-commands` tests |
| REQ-02-034 | 30 warstw | `req_02_034_thirty_default_layers` |
| REQ-02-046, 047, 050, 051, 052 | `.bcraft`: ZIP+JSON, schemat i walidacja, miniatura, notatki, migracje | `bc-doc::bcraft` tests |
| REQ-04-094 | grupowanie/rozgrupowanie | `req_04_094_group_ungroup` |
| REQ-13-199 | macierz sprzętu | `docs/hardware-matrix.md` |
| REQ-14-011, 018 | `burncraft-cli open`, `mcp` | `mcp_stdio.rs` |
| REQ-15-001, 005, 011 | stdio, structuredContent/outputSchema, instructions | `bc-mcp` tests, `mcp_stdio.rs` |
| REQ-15-012 | układ współrzędnych | ADR 0003 |
| REQ-15-013…019 | list_commands, describe_command, execute, batch, undo, redo, inspect_document | `bc-mcp` tests |
| REQ-15-022, 025 | render_preview design → PNG, bbox, max_size | `bc-sim`, `bc-mcp` tests |
| REQ-15-078…082 | zasoby document/object/layer/commands/command | `req_15_078_to_082_resources` |
| REQ-15-092, 093 | poziomy bezpieczeństwa, domyślna polityka (motion/fire odrzucone) | `policy.rs`, `tests.rs` |
| REQ-15-105 | dziennik audytu | `req_15_105_audit_written_to_file`, `req_15_093_…_and_req_15_105_audits` |
| REQ-16-009, 010, 012 | offline, brak DRM, poziomy logów | procedury |
| REQ-18-014…019 | dokumentacja MCP, konfiguracje klientów, polityka, ADR-y, CONTRIBUTING | procedury |
| REQ-20-001, 003, 004, 005…010, 012 | plan etapu, „bez testu nie gotowe” (req-check), UNVERIFIED, ADR 0001–0005, szkielet, CLI MCP | procedury / testy |
| REQ-21-001…011, 013, 014 | rejestr, pola, zestawienie, pominięcia, oznaczanie testów, `requirements` w poleceniach, req-check (4 reguły), przegląd, DEVIATIONS | `tools/req-check/test_req_check.py`, procedury |

Pełna macierz z nazwami testów: [`docs/traceability.md`](../traceability.md).

## Kroki przeglądu (§21.3)

1. **Ponowne przeczytanie** §0, §1, §2.1–2.3, §15, §18–21 i porównanie z rejestrem. Dodano brakujące pozycje dla MCP (`describe_command`, `batch`, `undo`, `redo`, osobne tryby `render_preview`, zasoby).
2. **Weryfikacja na działającym programie**: wszystkie testy `auto` uruchamiają rzeczywiste kody. Test `mcp_stdio.rs` uruchamia proces `burncraft-cli mcp` i rozmawia z nim po stdio jak klient MCP. Ręcznie sprawdzono też poziomy logów i czystość stdout.
3. **Testy**: `cargo test --workspace` — 55 testów Rust (ok), `unittest` req-check — 8 testów (ok). `cargo clippy --workspace --all-targets -D warnings` i `cargo fmt --check` — czyste.
4. **req-check**: `python tools/req-check/req_check.py --test-log cargo-test.log --test-log py-test.log` → 0 błędów. Kontrola bramki: przestawienie `M0: closed` daje dokładnie 8 błędów (wymagania z tabeli „Częściowe”) i kod wyjścia 1.
5. **Zamknięcie**: M0 zamknięty 2026-10-28 po zatwierdzeniu DEV-001.

## Rozstrzygnięcia bez zmiany funkcji (do wglądu)

- Własny serwer JSON-RPC zamiast `rmcp` (ADR 0002). Rewizja przy Streamable HTTP (M2).
- Undo przez memento. Współdzielenie struktury dla dużych dokumentów w M1 (ADR 0002).
- Konflikty priorytetów §17 vs sekcje szczegółowe: obowiązuje wyższy priorytet (37 pozycji, `REQUIREMENTS.md`).
- REQ-00-005 (pełny MCP z wykonywaniem zadań) i REQ-01-002 (GUI/web jako cienkie warstwy) przypisano do M6. M0 dostarcza ich fundament.
- Wejście stdio toleruje BOM UTF-8 (Windows PowerShell 5.1 dodaje go do potoku).

## Zadania otwarte poza wymaganiami M0

- Pełne teksty licencji (`LICENSES/README.md`).
- Następny etap M1: GUI Tauri, import/eksport SVG/DXF/.lbrn2, edytor wektorowy. Import `.lbrn2` wymaga przykładowych plików od zleceniodawcy (clean-room: analiza plików, które użytkownik posiada).
