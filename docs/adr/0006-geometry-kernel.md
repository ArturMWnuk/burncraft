# ADR 0006 — Jądro geometryczne (booleany, offsety, operacje na ścieżkach)

- Status: przyjęta (M1, 2026-10-28); zmieniona 2026-10-28 — **DEV-002** (port w czystym Rust zamiast wrappera C++)
- Wymagania: REQ-04-036…070, REQ-19-001, REQ-19-002

## Decyzja

1. **Booleany i offsety: Clipper2** (algorytm Angusa Johnsona) w porcie **`clipper2-rust` 1.2** (czysty Rust, BSL-1.0, z przeniesionymi testami biblioteki). Pierwotnie wybrany wrapper C++ (`clipper2` + `clipper2c-sys`) odrzucono w DEV-002, bo wymaga działającego kompilatora C++ i bibliotek CRT na każdej platformie. §1.2 wskazuje Clipper2, więc algorytm pozostaje zgodny ze specyfikacją.
2. **Skala całkowitoliczbowa**: 1 jednostka = 0,1 µm (`SCALE = 10 000`), czyli zakres ±9·10¹¹ mm na `i64`.
3. **Krzywe → polilinie przed operacją**: spłaszczanie z tolerancją **0,001 mm** (`BOOLEAN_TOLERANCE`). Wynik booleanów i offsetów to ścieżki liniowe. Odtwarzanie krzywych (redukcja węzłów, zamiana odcinków na łuki) to osobne polecenia.
4. **Wypełnienie**: każdy obiekt jest wypełniany regułą **even-odd** (otwór w otworze = materiał, jak przy laserze). Obiekty łączą się sumą: normalizacja even-odd, potem NonZero między obiektami. Ścieżki otwarte nie biorą udziału w booleanach ani offsetach (`GeomError::OpenPath`).
5. **Offset**: `Join::Miter` (limit 4× odległości), `Join::Round` (tolerancja łuku 0,0005 mm), `Join::Bevel` (ścięcie między końcami odsuniętych krawędzi). Dodatnie `delta` = na zewnątrz względem obszaru, niezależnie od kierunku ścieżki.
6. **Wynik** to lista kształtów: każdy obrys z otworami jako jedna ścieżka wielokonturowa.
7. **Dokładność łuków**: prostokąty z zaokrągleniami i elipsy są aproksymowane krzywymi Béziera z tolerancją `ARC_TOLERANCE` = 10 nm. Domyślne 4 krzywe na okrąg zawyżały promień o 0,027%, co wykryły testy pól.
8. **Kształty parametryczne** (wielokąt, gwiazda, podwójna gwiazda, łuk) zachowują parametry w modelu (REQ-02-033). Konwersja do ścieżki „wypieka” geometrię.

## Konsekwencje

- Brak zależności od kompilatora C++ w rdzeniu.
- `THIRD_PARTY.md`: Clipper2 (algorytm, BSL-1.0) i `clipper2-rust`.
- Zbiór trudnych przypadków (REQ-19-001/002) w `bc-geom/src/ops_tests.rs`: styczne krawędzie, wspólny wierzchołek, pierścienie z krzywych, obiekt z otworem, samoprzecięcie, kształt zdegenerowany, duże współrzędne, ścieżki otwarte, ostry kąt z limitem miter, offset większy niż kształt, zlewanie sąsiadów, niezależność od kierunku.
