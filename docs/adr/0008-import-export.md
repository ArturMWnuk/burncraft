# ADR 0008 — Import i eksport plików (SVG, DXF, HPGL, .lbrn2)

- Status: przyjęta (M1.3, 2026-10-29)
- Wymagania: REQ-00-017, REQ-02-062…092, REQ-14-012, REQ-14-015, REQ-15-027…030, REQ-19-003…005

## Decyzja

### Wspólny model

Każdy importer (`bc-io`) zwraca `Imported`: listę elementów (`Kind` + transformacja w mm, oś Y w górę), klucz warstwy (indeks, kolor, nazwa albo pióro HPGL), definicje warstw (gdy plik je niesie, np. `.lbrn2`) i ostrzeżenia. `bc_io::apply` wstawia elementy do dokumentu i dobiera warstwy:

- **kolor**: warstwa o identycznym kolorze, a gdy takiej nie ma — nowa warstwa z dokładnie tym kolorem (indeks ≥ 32). Kolory nie są „przyciągane” do palety, więc nic nie zmienia warstwy po cichu,
- **nazwa**: warstwa o tej samej nazwie (bez rozróżniania wielkości liter) albo nowa warstwa o tej nazwie,
- **pojedyncza**: wszystko na wskazaną warstwę.

Domyślnie import tworzy jedną grupę (jak w typowym CAD), a opcja `group: false` wstawia elementy luzem.

### SVG (`usvg` 0.49, bez obsługi tekstu)

- `usvg` normalizuje transformacje, style CSS, `<use>` i zagnieżdżone `<svg>`. Tekst jest pomijany z ostrzeżeniem (fonty od M4), a obrazy osadzone też z ostrzeżeniem (M4).
- **Jednostki / auto-DPI**: gdy `width`/`height` mają jednostki fizyczne (mm, cm, in, pt), skala wynika z `viewBox`. Gdy są w px albo bez jednostek, DPI zależy od generatora: Adobe Illustrator i Affinity (`xmlns:serif`) → 72, w pozostałych przypadkach (Inkscape, przeglądarki) → 96. Opcja `dpi` nadpisuje wykrycie.
- **Dokładność**: `usvg` liczy na `f32`, więc współrzędne SVG mają dokładność rzędu 10⁻⁵ mm (0,01 µm). To znacznie poniżej rozdzielczości lasera.
- Oś Y: `y_mm = wysokość_strony − y_svg`, więc rysunek zachowuje położenie na stronie (lewy dolny róg strony = (0, 0)).
- **Warstwy wg nazwy**: etykieta najbliższej grupy-przodka (`inkscape:label`, a w razie braku `id`). Etykiety są odczytywane z XML (`roxmltree`), bo `usvg` je pomija.
- **Eksport**: SVG 1.1 w mm (`width/height` w mm, `viewBox` w mm), grupa na warstwę (`id` i `inkscape:label` = nazwa warstwy), obrys w kolorze warstwy, bez wypełnienia. Obiekty pomocnicze są domyślnie pomijane.

### DXF (`dxf` 0.6, R12–2018)

- Encje: LINE, LWPOLYLINE (z wybrzuszeniami `bulge` → łuki), POLYLINE, CIRCLE, ARC, ELLIPSE, SPLINE i INSERT (bloki rekurencyjnie, z punktem bazowym, skalą i obrotem; głębokość ≤ 16). TEXT/MTEXT: przy `ignore_text` są pomijane po cichu, bez tej opcji — pomijane z ostrzeżeniem, dopóki nie ma fontów (M4).
- **Splajny** (NURBS) są próbkowane algorytmem de Boora i upraszczane do tolerancji 0,005 mm.
- **Jednostki**: `$INSUNITS` (1 in, 2 ft, 4 mm, 5 cm, 6 m, …). Bez jednostek → mm (opcja `units` nadpisuje).
- **Warstwy**: domyślnie wg nazwy warstwy DXF.
- **Eksport**: AC1009 (R12) — POLYLINE ze spłaszczonych krzywych (0,01 mm). AC1021 (R2007) — LWPOLYLINE dla odcinków i SPLINE stopnia 3 dla krzywych Béziera (dokładnie, bez spłaszczania). Warstwa DXF = warstwa BurnCraft, kolor ACI najbliższy kolorowi warstwy (R2007 dodatkowo z kolorem 24-bit), `$INSUNITS = 4` (mm).

### HPGL / PLT (własna implementacja)

- 1 jednostka plotera = 0,025 mm (40/mm). Obsługiwane polecenia: IN, SP (pióro n → warstwa n−1), PU, PD, PA, PR. Pozostałe są ignorowane z ostrzeżeniem.
- Eksport: `IN;`, pióro na warstwę, PU/PD ze spłaszczonych ścieżek.

### Format `.lbrn2` / `.lbrn` (clean-room)

- Implementacja wyłącznie na podstawie plików dostarczonych przez użytkownika: `plik_przykladowy.lbrn2` oraz `elipsa`, `hex`, `krzywe`, `text`, `grupa` w obu formatach. Od otwarcia repozytorium (2026-11-07) w `tests/fixtures/` są własne odpowiedniki tych plików z `tools/fixtures/gen_lbrn_samples.py`: te same kształty i wymiary, tylko elementy czytane przez importer, bez miniatur i metadanych, a napis ma kontury własnego kroju blokowego. Oryginalne pliki zostały prywatne.
- `.lbrn2` (`FormatVersion="1"`) i `.lbrn` (`FormatVersion="0"`) mają tę samą strukturę. Różni się tylko zapis ścieżek: zwięzły `VertList`/`PrimList` w `.lbrn2`, elementy `<V>`/`<P>` w `.lbrn`. Test sprawdza, że oba dają identyczną geometrię.
- Obsługiwane (zweryfikowane na plikach):
  - `CutSetting` (index, name, maxPower, minPower, speed),
  - `Rect` (W, H, Cr), `Ellipse` (Rx, Ry) — wyśrodkowane, zostają parametryczne,
  - `Polygon` (Rx, Ry, N; wierzchołki na kątach 2πk/N od 0°; przy Rx = Ry kształt parametryczny, w przeciwnym razie ścieżka),
  - `Path` (uchwyty c0 = wychodzący, c1 = wchodzący; `L`/`B`; zamknięcie, gdy prymityw wraca do początku podścieżki),
  - `Group` (dzieci z `XForm` względnym wobec grupy),
  - `Text` → kontury z `BackupPath` jako ścieżka, z ostrzeżeniem (edycja tekstu od M4). `XForm` w `BackupPath` jest **bezwzględny**, co wynika z porównania `grupa` i `text`.
- **Oś Y w górę** — potwierdzone konturem litery „L” i miniaturami (`krzywe`, `hex`, `grupa`). Render BurnCraft pliku `grupa.lbrn2` od użytkownika pokrył się z miniaturą zapisaną w tym pliku (2026-10-29).
- Inne typy (np. obrazy `Bitmap`) są pomijane z ostrzeżeniem, bez zgadywania formatu. Kolejne typy dochodzą wraz z próbkami.
- Projekt otwiera się jako nowy dokument bez ścieżki, więc zapis nie nadpisze oryginału. Można go też importować do bieżącego dokumentu.

### Odporność (REQ-19-003…005)

- `cargo-fuzz` (libFuzzer, nightly) — cele `svg_import`, `dxf_import`, `lbrn2_import` w `fuzz/`. W CI działa osobne zadanie na Linuksie z limitem czasu na cel.
- **Limity (wynik fuzzingu, run 40824017757)**: fuzzer znalazł brak pamięci (>2 GB) dla łuku SVG z punktem ~10⁵⁸ i okręgu DXF o promieniu ~10⁶⁰. Aproksymacja łuków z tolerancją bezwzględną generowała astronomiczną liczbę krzywych. Wprowadzono:
  - `bc_geom::MAX_COORD` = 10⁷ mm (10 km). Importery pomijają elementy poza zakresem z ostrzeżeniem, polecenia odrzucają takie wartości (`invalid_params`), a walidacja dokumentu odrzuca plik.
  - tolerancję łuków względną do rozmiaru (`arc_tolerance` = max(10 nm, 10⁻⁹·r)), więc liczba segmentów jest ograniczona przy dowolnej skali,
  - wstępną kontrolę liczb w atrybutach geometrii SVG przed `usvg` (|x| > 10⁸ → błąd),
  - w DXF: kontrolę zakresu encji, limit tablic INSERT (10 000 kopii) i budżet 2 mln prymitywów z bloków,
  - **atomowość poleceń**: błąd w trakcie polecenia przywraca stan dokumentu sprzed jego wykonania.
- Dane wejściowe z fuzzera są testami regresyjnymi (`tests/fixtures/fuzz/`).
- Dodatkowo deterministyczny test mutacyjny w zwykłym `cargo test` (wszystkie platformy): tysiące zmutowanych plików wzorcowych — importer nie może spanikować.
