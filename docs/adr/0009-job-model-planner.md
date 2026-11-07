# ADR 0009 — Model zadania i planer (`bc-plan`)

- Status: przyjęta (M2.1, 2026-11-04)
- Wymagania: REQ-07-001…006, 008, 009, 010…028, 031…052; REQ-04-083…085; REQ-08-001…015, 018, 019; REQ-09-008; REQ-16-007; REQ-14-013, 014; REQ-15-023, 051…056

## Kontekst

Planer zamienia dokument (obiekty, warstwy z ustawieniami cięcia) na **zadanie**: uporządkowaną listę ruchów z mocą i prędkością. Zadanie jest jedynym wejściem kompilatorów maszynowych (G-code w M2.2, Ruida/Lihuiyu w M3, galvo w M5), podglądu ścieżek (`render_preview mode=toolpath`) i estymacji czasu. Musi być deterministyczne, bo testy golden porównują wynik kompilatora bajt w bajt (REQ-16-007).

## Decyzja

### 1. Jednostki i układ

- Długość w **mm**, prędkość w **mm/s**, przyspieszenie w **mm/s²**, czas w **s**, moc w **%** (0–100) mocy maksymalnej źródła. Układ współrzędnych dokumentu (ADR 0003: Y w górę, origin w lewym dolnym rogu obszaru). Przeliczenie na układ sterownika (narożnik bazowania, kierunki osi) robi kompilator (M2.2).
- Krzywe są spłaszczane z tolerancją **0,01 mm** (`bc_geom::DEFAULT_TOLERANCE`), więc zadanie składa się wyłącznie z odcinków. Odtwarzanie łuków G2/G3 to optymalizacja kompilatora, nie planera.

### 2. Model zadania

```
Job { ops: [Op], warnings: [Warning], stats: Stats, origin: [x, y], hash }
Op  { layer, sublayer, pass, kind: vector|raster, label, settings: OpSettings,
      pause: Option<komunikat>, delay_s, z, moves: [Move] }
Move = Travel { to, lift } | Cut { to, power, power2, speed } | Dwell { s, power }
```

- `Op` to jeden przebieg jednej warstwy (lub pod-warstwy) z jednym zestawem ustawień: prędkość, moc min/maks (dla 2 laserów osobno), M3/M4, PWM, air assist, Z. Każde przejście (`passes`) to osobne `Op` z własnym Z (`z_offset − pass · z_step`) i opóźnieniem przed przejściem.
- `Move::Cut` niesie moc odcinka. Rampy mocy, rampa na narożnikach, perforacja i tryb punktowy są rozwinięte już w planerze, więc kompilator nie zna tych funkcji — tylko odcinki z mocą.
- `Op.pause` = pauza z komunikatem przed warstwą (REQ-07-020). Kompilator G-code zamieni ją na M0 z komentarzem, a GUI pokaże komunikat.
- `Stats`: długość cięcia, długość przejazdów, liczba przebiegów, liczba operacji, czas (cięcie, przejazdy, opóźnienia), bbox zadania.

### 3. Ustawienia

- **Warstwa** (`bc_doc::Layer`) dostaje pełny zestaw parametrów z §7.2 w strukturze `LayerParams` (pole `params`, wszystkie z wartościami domyślnymi). Pod-warstwy (`sublayers`, REQ-07-050) to dodatkowe przebiegi z własnym trybem i parametrami, wykonywane po przebiegu głównym. Schemat dokumentu przechodzi na **wersję 4** (migracja 3 → 4 dodaje wartości domyślne).
- **Dokument** dostaje `plan: PlanSettings`: kolejność (grawer najpierw / cięcie na końcu, wewnętrzne przed zewnętrznymi, wg grup), optymalizacja (punkt startowy, kierunek, 2-opt, usuwanie nakładających się linii, redukcja zmian kierunku, objazdy wyciętych części), strefy zakazane, szerokość pasów rastra, kinematyka maszyny.
- **Mostki ręczne** są atrybutem obiektu (`tabs`: punkty na konturze we współrzędnych lokalnych). Mostki automatyczne wynikają z parametrów warstwy. Kreator (`plan.tabs_wizard`) zapisuje mostki ręczne na podstawie liczby, odstępu i rozmiaru.
- **Kinematyka** (`Kinematics`): prędkość maksymalna i przyspieszenie osobno dla X i Y, odchylenie na złączu (junction deviation), prędkość przejazdu. `Kinematics::from_grbl` czyta `$110/$111/$120/$121/$11`, `from_ruida` — przyspieszenie Ruida. Do czasu sterowników (M2.3) kinematyka jest w `PlanSettings` dokumentu, potem pochodzi z profilu urządzenia.

### 4. Kolejność przekształceń (potok jednej warstwy)

1. **Zbiór geometrii**: widoczne liście warstwy z `output = true` (bez obiektów pomocniczych i warstw T1/T2), opcjonalnie tylko zaznaczone (REQ-08-013), z rozwinięciem grup, tablic i klonów. Obiekt zachowuje grupę najwyższego poziomu i priorytet.
2. **Usuwanie nakładających się linii** (REQ-08-011), gdy włączone.
3. **Kerf** (REQ-07-026…028): offset obrysów zamkniętych o `kerf` — na zewnątrz dla konturów zewnętrznych, do wewnątrz dla otworów. Osobny kerf X/Y: skalowanie Y o `kx/ky`, offset o `kx`, skalowanie odwrotne (dokładne dla krawędzi poziomych i pionowych, interpolowane dla skośnych). `kerf_x` i `kerf_y` muszą mieć ten sam znak. Dla Fill — offset obszaru przed skanowaniem (`fill_kerf`).
4. **Wypełnienia** (Fill, Offset Fill, Flood Fill, część Fill w Fill+Line): linie skanowania z kątem, interwałem, cross-hatch, overscanem, kompensacją scan offset; grupowanie wg trybu (razem / grupy / osobno / obszarami).
5. **Mostki** (REQ-07-031…034, REQ-04-083…085): przerwy w konturach (ręczne + automatyczne).
6. **Lead-in / lead-out i overcut** (REQ-07-035…038): dopiero po mostkach, żeby wejście nie trafiło w przerwę; tylko na zamkniętych konturach bez mostków, od strony odpadu, w punkcie startowym wybranym przez optymalizację (krok 8).
7. **Kolejność** (REQ-08-001…004, REQ-07-051): warstwa → priorytet → grupa → wewnętrzne przed zewnętrznymi. Wewnętrzne przed zewnętrznymi jest ograniczeniem kolejności (kontur zawierający musi iść po zawartym), a nie tylko sortowaniem.
8. **Optymalizacja** (REQ-08-005…007, 012): najbliższy sąsiad z ograniczeniami kolejności, wybór punktu startowego na konturze zamkniętym i kierunku, potem 2-opt na blokach bez ograniczeń między sobą; scalanie współliniowych odcinków.
9. **Rozwinięcie mocy** (REQ-07-039…042): perforacja, tryb punktowy, rampa na początku/końcu, rampa narożnikowa (wymaga profilu prędkości, więc po optymalizacji).
10. **Przejazdy** (REQ-08-008…010): przejazd przecinający strefę zakazaną albo wycięty już kontur zamknięty omija przeszkodę najkrótszą drogą po grafie widoczności narożników poszerzonych prostokątów. Gdy objazd nie istnieje, przejazd dostaje `lift = true` (podniesienie Z) i ostrzeżenie. Cięcie w strefie zakazanej dla wiązki jest wycinane z zadania z ostrzeżeniem.
11. **Origin** (REQ-08-014): przesunięcie całego zadania tak, by wybrany narożnik zaznaczenia był w (0, 0).

Rastry Fill można dzielić na pionowe **pasy** o zadanej szerokości (REQ-08-015). Linie skanowania idą wtedy w kolejności rzędu w obrębie pasa, a pasy od lewej do prawej, więc głowica nie przejeżdża przez puste obszary.

### 5. Determinizm (REQ-16-007)

- Żadnej iteracji po `HashMap`/`HashSet` w ścieżce wyniku. Sortowania są stabilne, liczby porównuje się przez `f64::total_cmp`, remisy rozstrzyga kolejność obiektu w dokumencie i id.
- Brak losowości: 2-opt przegląda pary w stałej kolejności.
- Równoległość: warstwy (i pod-warstwy) planują się niezależnie w `std::thread::scope`, a wynik każdej trafia do tablicy pod swój indeks. Scalanie idzie w kolejności indeksów, więc wynik nie zależy od liczby wątków (`PlanOptions.threads`, testy dla 1 i N wątków).
- Kanoniczna serializacja zadania (`Job::to_bytes`: współrzędne zaokrąglone do 1 µm, moc do 0,001 %) i jej SHA-256 (`Job.hash`) — ten sam dokument daje ten sam hash. Hash posłuży też tokenom potwierdzenia `start_job` (§15.4).

### 6. Przyrostowość (REQ-08-019)

`Planner` przechowuje pamięć podręczną planów warstw pod kluczem SHA-256 z (geometria warstwy, parametry warstwy, ustawienia planu, opcje). Zmiana jednej warstwy przelicza tylko ją; kolejność i przejazdy między warstwami są liczone zawsze (tanie). Cel: przeliczenie podglądu < 1 s dla typowych projektów (pomiar w testach na projektach referencyjnych).

### 7. Model czasu (REQ-09-008, REQ-08-018)

- Profil prędkości **trapezowy** z planowaniem wyprzedzającym (look-ahead) jak w GRBL: dla odcinka o kierunku `u` prędkość maksymalna `v = min(F, vx/|ux|, vy/|uy|)`, przyspieszenie `a = min(ax/|ux|, ay/|uy|)`. Prędkość na złączu wynika z odchylenia (`$11`): `v_j² = a · δ · sin(θ/2) / (1 − sin(θ/2))`. Przejście wstecz i w przód ogranicza prędkości do osiągalnych z przyspieszenia. Czas odcinka = przyspieszanie + jazda + hamowanie.
- Jerk: GRBL nie ma ograniczenia jerku, więc model jest czysto trapezowy. Dla sterowników z jerkiem (Ruida) jerk wchodzi jako współczynnik wydłużenia faz przyspieszania (`jerk_factor`, domyślnie 1). Samokalibracja z historii zadań to M6.
- Overscan automatyczny = droga rozpędzania `v² / (2a)` w osi skanowania (osobno X i Y). Overscan ręczny w % = `procent · prędkość` (mm na każde 1 mm/s, czyli 2,5 % przy 100 mm/s daje 2,5 mm).
- Pauzy z komunikatem nie wliczają się do czasu (czekają na człowieka); opóźnienia między przejściami tak.

### 8. API i polecenia

- `bc_plan::plan(doc, &PlanOptions, &mut dyn FnMut(done, total) -> Result<(), Cancelled>) -> Result<Job, PlanError>` i `bc_plan::estimate(&Job, &Kinematics) -> Estimate`.
- Polecenia rejestru: `layer.params` (parametry §7.2 warstwy), `plan.settings` (ustawienia planu dokumentu), `object.tabs` (mostki ręczne), `plan.tabs_wizard` (kreator mostków), `plan.job` (statystyki i ostrzeżenia), `plan.estimate` (czas z rozbiciem). Długie planowanie wywołuje `Session::tick` (postęp i anulowanie, REQ-16-006).
- CLI: `burncraft-cli plan <plik>` i `burncraft-cli estimate <plik>` (przez te same polecenia). MCP: `plan_job` i `render_preview` w trybie `toolpath`.

## Konsekwencje

- Kompilatory M2.2+ nie implementują rampy, perforacji ani kerfu — dostają gotowe odcinki z mocą.
- Schemat dokumentu 4: starsze wersje programu odmówią otwarcia pliku zapisanego nowszą wersją (jak dotąd).
- Kinematyka w dokumencie to stan przejściowy do M2.3 (profil urządzenia ma pierwszeństwo, gdy urządzenie jest wybrane).
- Graf widoczności dla objazdów jest kwadratowy względem liczby przeszkód. Przeszkodami są tylko te przecięte przez dany przejazd (zwykle kilka), więc koszt pozostaje mały.
