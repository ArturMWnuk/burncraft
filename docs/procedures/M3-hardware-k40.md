# REQ-19-016 — test sprzętowy: K40 z płytą M2 Nano (checklista)

Test przeprowadza zleceniodawca na własnym K40 (płyta Lihuiyu M2 Nano, Windows). Wyniki trafiają do tabeli wyników na końcu tego pliku i do `docs/hardware-matrix.md`. Od wyników zależą wymagania:

- REQ-19-016 (test sprzętowy K40),
- REQ-13-030 (płyta M2 Nano),
- REQ-13-040 (połączenie przez oryginalny sterownik CH341, CH341DLL),
- REQ-13-049 (K40 bez modyfikacji działa „od pudełka”).

Do czasu wyników te wymagania mają status `in_progress`. Punkt z wynikiem „nie” zgłaszamy jako błąd (poprawka w kolejnej parze commitów red/green) albo jako odstępstwo (`DEVIATIONS.md`).

Checklista sprawdza szczegóły protokołu, których emulator nie potwierdza. ADR 0014 (sekcja „Konsekwencje”) oznacza je jako `UNVERIFIED`; tabela na końcu sekcji „Checklista” pokazuje, który punkt sprawdza który szczegół.

## Bezpieczeństwo (przed testem)

- K40 to laser CO₂ (10,6 µm), niewidoczna wiązka. Pokrywa zamknięta, gdy wiązka może być włączona; nie zaglądaj do komory w trakcie pracy.
- Chłodzenie wodne włączone i sprawdzone (przepływ) przed każdym włączeniem wiązki. Wyciąg włączony.
- Gaśnica albo koc gaśniczy obok. Nie zostawiaj pracującego lasera bez nadzoru.
- K40 M2 Nano nie ma PWM: moc ustawia pokrętło (albo panel cyfrowy) na laserze. Pierwsze próby z małą mocą (ok. 5 mA na amperomierzu albo 10–15 % na panelu).
- Punkty bez wiązki (ruch, prędkość, ramka) wykonuj z wyłączonym przyciskiem „Laser Switch” / „Laser Enable” na panelu lasera.
- Ręka przy wyłączniku zasilania. „Stop” w BurnCraft przerywa zadanie programowo (polecenie `I` płyty), ale nie zastępuje wyłącznika.
- Materiał testowy: tektura albo papier (cięcie, wymiary), jasna sklejka albo tektura (grawer).

## Przygotowanie

1. Zbuduj i uruchom BurnCraft (Windows): okno `burncraft` albo `burncraft-cli serve` i przeglądarka `http://127.0.0.1:7984/`.
2. Zanotuj z płyty sterownika (naklejka): model (np. `6C6879-LASER-M2`), numer seryjny / ID płyty.
3. Podłącz K40 przez USB i włącz zasilanie lasera. W Menedżerze urządzeń znajdź urządzenie CH341 (`USB-EPP/I2C… CH341A` albo podobne, USB ID `1A86:5512`) i zanotuj, jaki ma sterownik (sterownik producenta CH341 albo brak sterownika).
4. Jeśli na komputerze jest program producenta (LaserDRW, CorelLaser), zamknij go: urządzenie USB może być otwarte tylko przez jeden program.
5. Przygotuj stoper (telefon) i suwmiarkę albo linijkę z podziałką 0,5 mm.

Kolejność: część A używa oryginalnego sterownika CH341 producenta (stan „od pudełka”, bez zmian w systemie). Część B wymienia sterownik na WinUSB (Zadig) i powtarza najważniejsze punkty. Po części B program producenta nie połączy się z laserem, dopóki nie przywrócisz sterownika CH341 (punkt B6).

## Checklista

Kolumna „Polecenie” to polecenie rejestru, które wykonuje dana kontrolka (to samo zrobi agent MCP albo CLI).

### Część A — oryginalny sterownik CH341 (CH341DLL)

| # | Krok | Polecenie | Oczekiwany wynik |
|---|---|---|---|
| A1 | **Wykrycie USB**: okno Laser → „Znajdź mój laser” | `machine.find` | w sekcji „Urządzenia USB” wiersz K40 (CH341, `1A86:5512`) ze stanem „sterownik producenta” i przyciskiem „Dodaj K40 (Lihuiyu)”. Gdy stan to „brak sterownika”: zanotuj i przejdź do części B |
| A2 | **Profil**: „Dodaj K40 (Lihuiyu)” | `machine.profile_save` | na liście urządzeń profil „K40 (USB)”; sekcja „Płyta K40” pokazuje wariant „M2 Nano — bez PWM”, korekcję skali 1 / 1 |
| A3 | **Połączenie**: wybierz „K40 (USB)” → „Połącz” | `machine.connect` | stan „gotowy”; w Konsoli wiersz stanu płyty (np. `< OK (M2 Nano)`). Przy błędzie zanotuj pełny komunikat (np. brak biblioteki `CH341DLLA64.DLL` / `CH341DLL.DLL`) i miejsce instalacji biblioteki producenta |
| A4 | **Panel Warstwy** po połączeniu | — | pole „Moc %” nieaktywne, komunikat „Brak PWM: moc ustawia pokrętło na laserze” (jak w `M3-gui-checks.md`, REQ-13-045) |
| A5 | **Bazowanie**: „Bazuj” | `device.home` (`IPP`) | głowica jedzie do narożnika bazowania (zwykle lewy tylny) i zatrzymuje się na krańcówkach; pozycja w oknie Laser X 0 Y 0; krzyżyk głowicy na kanwie w odpowiadającym narożniku stołu. Zanotuj narożnik |
| A6 | **Kierunki osi i jog**: okno Przesuń, krok 10 mm: X+, X−, Y+, Y− | `device.jog` | głowica jedzie w stronę strzałki, krzyżyk na kanwie przesuwa się w tę samą stronę co głowica na stole; odległość 10 mm (pomiar linijką, ±0,5 mm). Jog K40 to przejazd szybki płyty: „Szybko” i „Wolno” mogą się nie różnić — zanotuj |
| A7 | **Zero użytkownika**: odjedź jogiem, „Ustaw zero”, odjedź, „Idź do zera” | `device.set_origin`, `device.go_origin` | powrót do miejsca ustawienia zera |
| A8 | **Ramka**: prostokąt 50 × 30 mm w X 20, Y 20 (C00), „Ramka” | `device.frame` | głowica objeżdża prostokąt bez wiązki; położenie na stole zgodne z położeniem na kanwie względem narożnika bazowania |
| A9 | **Impuls testowy**: okno Makra, nowe makro „Impuls” z wierszem `@device.test_fire {"power": 100, "ms": 200}`, „Laser Switch” włączony, mała moc na pokrętle, kawałek papieru pod głowicą, „Uruchom” | `macro.run` → `device.test_fire` (`IDS1P` … `IUS1P`) | jeden krótki impuls (kropka na papierze), wiązka gaśnie po ok. 0,2 s |
| A10 | **Kod prędkości M2** (bez wiązki): linia pozioma (C00, Line) i „Start”; stoperem zmierz czas ruchu wzdłuż linii (od ruszenia na linii do zatrzymania; najlepiej nagranie telefonem) | `job.start` | czasy: 100 mm przy 10 mm/s → 10 s; 100 mm przy 20 mm/s → 5 s; 200 mm przy 50 mm/s → 4 s; 50 mm przy 5 mm/s (kod z sufiksem C) → 10 s. Tolerancja ±5 %. Zanotuj zmierzone czasy (MeerK40t opisuje płyty o ok. 8 % wolniejsze od zadanej prędkości) |
| A11 | **Cięcie i wymiary**: prostokąt 50 × 30 mm (C00, Line, np. 10 mm/s, moc z pokrętła wg materiału), tektura albo papier, „Laser Switch” włączony, „Start” | `job.start` | prostokąt wycięty, zamknięty (początek i koniec się schodzą); wymiary suwmiarką: 50,0 × 30,0 mm (±0,3 mm). Zanotuj pomiar X i Y |
| A12 | **Korekcja skali** (tylko gdy A11 poza tolerancją): „Korekcja skali X / Y” = wymiar zadany / zmierzony (np. 50 / 49,4 = 1,012), powtórz A11 | `machine.profile_save` | wymiary w tolerancji ±0,2 mm. Zanotuj wpisane współczynniki |
| A13 | **Raster (krok G)**: prostokąt 30 × 15 mm i napis (C01, Fill, 150 mm/s, interwał 0,1 mm, dwukierunkowo), jasna sklejka albo tektura, „Start” | `job.start` (raster natywny płyty) | równy grawer, linie bez przesunięcia między kierunkami (krawędzie proste, nie „ząbkowane”), bez przerw i zatrzymań w trakcie (bufor płyty). Zanotuj czas zadania i czas z `plan.estimate` (pole „Czas” w oknie Laser / Podgląd) |
| A14 | **Pauza i wznowienie**: w trakcie zadania z A11 albo A13 „Pauza”, odczekaj 5 s, „Wznów” | `job.pause`, `job.resume` (`PN`) | ruch i wiązka zatrzymane (stan „wstrzymany”), po wznowieniu zadanie kończy się bez śladu w miejscu pauzy. Zanotuj, czy zatrzymanie było natychmiastowe, czy po dokończeniu bieżącego fragmentu |
| A15 | **Stop**: uruchom zadanie, „Stop” | `job.stop` (`I`) | ruch i wiązka zatrzymane natychmiast; stan „gotowy” albo „zatrzymane”; potem „Bazuj” działa. Zanotuj stan po stopie |
| A16 | **ID płyty**: w „ID płyty” wpisz numer z naklejki, „Rozłącz”, „Połącz”; potem zmień jeden znak ID i połącz ponownie; na koniec wyczyść pole | `machine.profile_save`, `machine.connect` | z poprawnym ID: połączenie i wiersz `< SERIAL OK (…)` w Konsoli; z błędnym: komunikat „ID płyty … nie zostało potwierdzone”. Jeśli płyta nie potwierdza nawet poprawnego ID — zanotuj (ID jest opcjonalne) |
| A17 | **Pliki na sterowniku**: okno Laser → „Pliki na sterowniku” | `files.open`, `machine.files` | komunikat, że urządzenie nie ma pamięci plików (K40 nie przechowuje zadań) |
| A18 | **Odłączenie kabla USB** w trakcie przejazdu (bez wiązki: np. A10 przy 10 mm/s) | — | BurnCraft zgłasza utratę połączenia (stan „rozłączony”, błąd w pasku stanu), okno nie zawiesza się; po podłączeniu kabla „Połącz” działa. Zanotuj zachowanie płyty (dokończyła bufor czy stanęła) |

### Część B — sterownik WinUSB (Zadig), połączenie natywne przez USB

| # | Krok | Polecenie | Oczekiwany wynik |
|---|---|---|---|
| B1 | **Przewodnik**: „Znajdź mój laser” → przy K40 „Jak podłączyć…” | `machine.usb_guide` | przewodnik WinUSB z USB ID `1A86 5512` i informacją o alternatywie CH341DLL (zrzut w `M3-gui-checks.md`, REQ-13-020) |
| B2 | **Zadig**: Options → List All Devices → urządzenie `1A86 5512` → WinUSB → Replace Driver; potem „Znajdź mój laser” | `machine.find` | K40 ze stanem „gotowe” (WinUSB) |
| B3 | **Profil USB**: „Dodaj K40 (Lihuiyu)”, „Połącz” | `machine.profile_save`, `machine.connect` | profil „K40 (USB)” z transportem USB `1A86:5512`; stan „gotowy” |
| B4 | **Powtórz** A5 (bazowanie), A6 (jog), A10 tylko 100 mm przy 20 mm/s, A11 (cięcie), A15 (stop), A18 (odłączenie kabla) | jak w części A | wyniki jak w części A |
| B5 | Wersja układu CH341 w Konsoli po połączeniu (jeśli widoczna) | — | zanotuj |
| B6 | (opcjonalnie) **Przywrócenie sterownika producenta**: Menedżer urządzeń → urządzenie → Aktualizuj sterownik / Odinstaluj urządzenie (z usunięciem WinUSB), potem instalator CH341PAR producenta | — | program producenta i część A znów działają |

Szczegóły `UNVERIFIED` z ADR 0014 i punkty, które je sprawdzają:

| Szczegół | Punkty |
|---|---|
| punkty końcowe CH341, inicjalizacja EPP (żądanie 0xB1), bajt stanu (żądanie 0x52) | B3, B4 |
| funkcje i bajt stanu CH341DLL (nazwy plików, prototypy) | A3, A5–A18 |
| kod prędkości M2 (w tym sufiks C poniżej 7 mm/s), ewentualna korekcja o ok. 8 % | A10 |
| kierunki osi i narożnik bazowania | A5, A6, A8 |
| `IPP` (bazowanie), `IS2P` (odblokowanie), `PN` (pauza i wznowienie), `I` (stop) | A5, A14, A15 |
| rozmiar bufora płyty (stan BUSY) | A13, A18 |
| raster natywny `G` | A13 |
| potwierdzenie ID płyty (MD5, stan 204) | A16 |
| skala 1000 dpi i korekcja skali | A6, A11, A12 |

## Co przesłać

1. Model płyty i jej ID z naklejki, sterownik CH341 w Menedżerze urządzeń przed testem.
2. Wynik każdego punktu: **tak / nie / pominięto** z krótką uwagą. Liczby: zmierzone czasy z A10, wymiary z A11 (i A12), czasy z A13, narożnik bazowania z A5.
3. Przy błędach: tekst komunikatu i zawartość Konsoli (zrzut ekranu okna Laser i Konsoli).
4. Zdjęcia wyciętego prostokąta i graweru (opcjonalnie).

## Wyniki

Do uzupełnienia po teście.

- Data testu:
- Płyta (naklejka), ID:
- Sterownik CH341 przed testem:
- Wersja BurnCraft (commit):

| # | Wynik | Uwagi |
|---|---|---|
| A1 | | |
| A2 | | |
| A3 | | |
| A4 | | |
| A5 | | narożnik bazowania: |
| A6 | | |
| A7 | | |
| A8 | | |
| A9 | | |
| A10 | | 10 mm/s: … s; 20 mm/s: … s; 50 mm/s: … s; 5 mm/s: … s |
| A11 | | X: … mm, Y: … mm |
| A12 | | współczynniki X / Y: |
| A13 | | czas zadania: …; plan.estimate: … |
| A14 | | |
| A15 | | |
| A16 | | |
| A17 | | |
| A18 | | |
| B1 | | |
| B2 | | |
| B3 | | |
| B4 | | |
| B5 | | |
| B6 | | |
