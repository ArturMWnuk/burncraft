# Procedury ręczne — etap M1

Procedury interfejsu (REQ-01-021, REQ-03-*) ze zrzutami ekranu są w [M1-gui-checks.md](M1-gui-checks.md).

## REQ-00-017 — interoperacyjność wyłącznie z plików użytkownika
Procedura: przegląd `crates/bc-io/src/lbrn.rs`, ADR 0008 i `THIRD_PARTY.md`.
Wynik (2026-10-29): **zgodne**. Importer `.lbrn2` obsługuje wyłącznie elementy widoczne w pliku dostarczonym przez użytkownika (`CutSetting`, `Shape Type="Rect"`, `XForm`). Inne typy kształtów są zgłaszane jako ostrzeżenie i nie są zgadywane. Nie użyto kodu, dokumentacji ani zasobów innych programów do laserów. 2026-10-29: dodano próbki użytkownika `elipsa`, `hex`, `krzywe`, `text` i `grupa` (`.lbrn` i `.lbrn2`). Obsługa rozszerzona wyłącznie o elementy z tych plików. Render `grupa.lbrn2` porównany wzrokowo z miniaturą zapisaną w pliku: zgodny. Formaty `.clb`, `.lbart` i `.lbzip` czekają na pliki przykładowe (M3/M4). 2026-11-07, przed otwarciem repozytorium: pliki użytkownika zastąpiono w testach własnymi odpowiednikami z `tools/fixtures/gen_lbrn_samples.py` (te same kształty i wymiary, bez miniatur i metadanych); testy przechodzą bez zmian asercji.

## REQ-19-003, REQ-19-004, REQ-19-005 — fuzzing importerów (cargo-fuzz)
Procedura: zadanie CI `fuzz` (Linux, nightly, libFuzzer + AddressSanitizer), po 120 s na każdy cel, z korpusem startowym w `fuzz/corpus/`. Wymienione commity i przebiegi są w archiwum historii sprzed otwarcia projektu, `ArturMWnuk/burncraft-history`.
Wynik — run **40821330127** (commit `acf841e`, 2026-10-29), bez awarii:
- REQ-19-003 `svg_import`: 1 223 587 przebiegów, pokrycie 6067 krawędzi,
- REQ-19-004 `dxf_import`: 696 715 przebiegów, pokrycie 3546 krawędzi,
- REQ-19-005 `lbrn2_import`: 2 292 824 przebiegów, pokrycie 2105 krawędzi.
Uzupełnienie na wszystkich platformach: deterministyczny test mutacyjny `req_19_003_004_005_importers_never_panic_on_mutated_input` w `cargo test`.
Pierwszy przebieg (40818693227) nie zbudował celów: `cargo-fuzz` z `install-action` domyślnie wybiera target musl. Poprawione jawnym `--target x86_64-unknown-linux-gnu`.

Kolejne przebiegi:
- run 40824017757 (commit `3f1cf80`): fuzzer **znalazł** brak pamięci (>2 GB) w `svg_import` (łuk z punktem ~10⁵⁸) i `dxf_import` (okrąg o promieniu ~10⁶⁰). Dane wejściowe są testami regresyjnymi w `crates/bc-io/tests/fixtures/fuzz/`. Naprawa: limit współrzędnych 10⁷ mm, tolerancja łuków względna do rozmiaru, limity DXF, atomowe polecenia (ADR 0008).
- run **40825939733** (commit `739337f`, 2026-10-29), po naprawie, bez awarii: `svg_import` 1 753 594, `dxf_import` 1 266 262, `lbrn2_import` 2 197 854 przebiegów.
