# BurnCraft — rejestr wymagań

Jedyne źródło prawdy o zakresie projektu (spec. §21.1). Dane: [`requirements.yaml`](requirements.yaml). Zestawienie i pełna lista poniżej są generowane przez `python tools/req-check/req_check.py --write`, więc nie edytuj ich ręcznie. Status etapów: [`milestones.yaml`](milestones.yaml). Odstępstwa: [`DEVIATIONS.md`](DEVIATIONS.md). Testy i procedury dla każdego wymagania: [`docs/traceability.md`](docs/traceability.md).

## Metoda rozbicia

- Specyfikacja `PROMPT_BurnCraft.md` (sekcje 0–21) została rozbita na **atomowe wymagania**: każdy punkt listy, wiersz tabeli (format, model sterownika), parametr warstwy, tryb dithera, narzędzie i zasób MCP, język lokalizacji i [POSTULAT] to osobna pozycja. Wyliczenia w jednym zdaniu („Threshold, Ordered, Atkinson…”) są rozbite na osobne pozycje.
- Identyfikator `REQ-<sekcja główna>-<nr>`; pole `sec` podaje podsekcję (np. `7.2`).
- **Priorytet**: oznaczenie z sekcji ([MUST]/[SHOULD]/[LATER]); wymagania bez oznaczenia dziedziczą oznaczenie sekcji.
- **Etap** M0–M7 przypisany wg §20. Kryterium weryfikacji: `auto` (test automatyczny), `emu` (emulator sterownika), `hw` (sprzęt), `manual` (procedura ręczna w `docs/procedures/`).
- `postulat` = liczba głosów z publicznej tablicy postulatów (§17; 0 = postulat bez podanej liczby).
- Teksty wymagań w YAML są zapisane bez polskich znaków diakrytycznych (identyfikatory, wyszukiwanie w terminalu). Wiążący jest tekst specyfikacji w podanej sekcji.

## Konflikty priorytetów (do wglądu)

Sekcja 17 przypisuje część funkcji do priorytetu B (1.x), a sekcje szczegółowe oznaczają je jako [MUST] (1.0). Albo odwrotnie: §4.5 oznacza generatory jako [SHOULD], a §17 umieszcza zawiasy żywe w priorytecie A. **Rozstrzygnięcie: obowiązuje wyższy priorytet**. Zakres się nie zmniejsza, więc według §21.4 nie jest to odstępstwo. 37 pozycji ma flagę `conflict: true`:

| Grupa | Wymagania | Sekcja szczegółowa | §17 | Przyjęto |
|---|---|---|---|---|
| Zakładki dokumentów | REQ-02-060, 061 | MUST (§2.3) | B | MUST, M1 |
| Gerber / Excellon | REQ-02-082, 083 | MUST (§2.4) | B | MUST, M6 |
| Gumka wektorowa, edycja w grupach | REQ-04-021, 055 | MUST (§4) | B | MUST, M1 |
| Zawiasy żywe | REQ-04-104…108 | SHOULD (§4.5) | A | MUST, M6 |
| Kategorie/ostatnie fonty, glify OpenType | REQ-05-010, 011, 022 | MUST (§5) | B | MUST, M4 |
| Usuwanie tła | REQ-06-031 | MUST (§6) | B | MUST, M4 |
| Zdjęcia w bibliotece materiałów, test kerfu | REQ-07-086, 103 | MUST (§7) | B | MUST, M4 |
| Strefy zakazane, moc zależna od XY | REQ-08-009, 010, 016, 017 | MUST (§8) | B | MUST, M2/M6 |
| Kalkulator kosztów, wznawianie po awarii | REQ-09-010…012, 016, 017 | MUST (§9) | B | MUST, M6 |
| Print & Cut 3/4 znaczniki | REQ-11-013 | MUST (§11) | B | MUST, M5 |
| LightObject X7/T9, Gweike | REQ-13-156…159 | MUST (tabela §13.3) | B | MUST, M6 |
| EZCAD3 | REQ-13-174…178 | „MUST w 1.x” (§13.3) | A („pełne EZCAD3”) | MUST, M6 |
| Wirtualna drukarka | REQ-13-189 (Windows), REQ-13-190 (wszystkie OS) | bez oznaczenia (§13.3) | C (wszystkie OS) | MUST dla Windows, LATER dla pozostałych |

Jeśli zleceniodawca woli obniżyć któryś z tych priorytetów (np. przenieść Gerber do 1.x), wystarczy decyzja. Obniżenie zostanie zapisane w `DEVIATIONS.md`.

## Inne niejasności rozstrzygnięte bez zmiany funkcji

- „Python + JS/Lua” (§1.1) vs „Python i JavaScript” (§14): Python i JS — MUST, Lua — SHOULD (REQ-14-003).
- Modele z dopiskiem „?” / „zbadać” (Trocen TL-3120, TopWisdom, Gweike, LaserPecker, upload `.ud`): wymagania istnieją. Weryfikacja zaczyna się od zbadania zgodności, a wynik trafi do `docs/hardware-matrix.md`. Brak publicznej wiedzy = zgłoszenie odstępstwa w momencie stwierdzenia.
- HEIC („HEIC*”) traktowany jako SHOULD — zależy od kodeka systemowego i patentów (REQ-02-103).
- Natywne MCP „Z na żywo” dla galvo (§17 A) pokrywa się z REQ-13-173 (sterowanie osią Z BJJCZ).

## Fragmenty specyfikacji świadomie nieuznane za osobne wymagania

| Fragment | Powód |
|---|---|
| §0 „Rola, cel” — zdania opisowe o roli architekta | opis kontekstu, nie funkcja |
| §1.2 lista bibliotek (`lyon`, Clipper2, `rstar`, `rustybuzz`, `usvg`, `nusb`, OpenCV…) | przykłady w ramach [SHOULD] wyboru stosu; decyzje zapisuje ADR 0002 i kolejne ADR-y |
| §13.1 blok `trait Driver` | rozbity na metody REQ-13-001…017 |
| §17 lista priorytetów A/B/C | streszczenie wymagań z sekcji 1–16; wpływa tylko na priorytet/etap (tabela konfliktów). Jako nowe wymagania dodano tylko pozycje, których nie ma gdzie indziej: funkcje biblioteki grafik, natywne aplikacje mobilne, współpraca wielu użytkowników (REQ-17-*) |
| Liczby głosów przy postulatach | metadane (`postulat`), nie wymagania |
| Nazwy linków do źródeł (MeerK40t, Balor, EduTech wiki…) w §0 i §13 | źródła wiedzy, ewidencjonowane w `THIRD_PARTY.md` |
| §20 opisy etapów M1–M7 | struktura planu; zakres etapów wynika z pola `ms` |
| §4.3 „zaokrąglenie/fillet” | duplikat REQ-04-054 (§4.2) |
| §4.4 „blokady” | duplikat REQ-02-025/026 |
| §12 „nesting na materiale z kamery” | duplikat REQ-04-103 |
| §3 „Biblioteka grafik” w §17 (druk katalogu itd.) | rozbita w REQ-17-001…005 |

## Zestawienie i pełna lista (generowane)

<!-- req-check:start -->

### Zestawienie: sekcja → liczba wymagań

| Sekcja | Wymagań | MUST | SHOULD | LATER | verified | done | in_progress | todo | deviation |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 20 | 20 | 0 | 0 | 7 | 0 | 0 | 13 | 0 |
| 1 | 44 | 41 | 3 | 0 | 36 | 0 | 0 | 8 | 0 |
| 2 | 111 | 101 | 10 | 0 | 67 | 0 | 0 | 44 | 0 |
| 3 | 83 | 83 | 0 | 0 | 52 | 0 | 0 | 31 | 0 |
| 4 | 117 | 108 | 9 | 0 | 82 | 0 | 0 | 35 | 0 |
| 5 | 58 | 58 | 0 | 0 | 0 | 0 | 0 | 58 | 0 |
| 6 | 42 | 42 | 0 | 0 | 0 | 0 | 0 | 42 | 0 |
| 7 | 104 | 104 | 0 | 0 | 51 | 0 | 0 | 53 | 0 |
| 8 | 19 | 19 | 0 | 0 | 17 | 0 | 0 | 2 | 0 |
| 9 | 17 | 17 | 0 | 0 | 8 | 0 | 0 | 9 | 0 |
| 10 | 24 | 24 | 0 | 0 | 17 | 0 | 0 | 7 | 0 |
| 11 | 38 | 31 | 7 | 0 | 1 | 0 | 0 | 37 | 0 |
| 12 | 26 | 26 | 0 | 0 | 0 | 0 | 0 | 26 | 0 |
| 13 | 199 | 195 | 2 | 2 | 73 | 0 | 0 | 126 | 0 |
| 14 | 32 | 27 | 5 | 0 | 8 | 0 | 0 | 24 | 0 |
| 15 | 108 | 108 | 0 | 0 | 74 | 0 | 0 | 34 | 0 |
| 16 | 13 | 13 | 0 | 0 | 7 | 0 | 0 | 6 | 0 |
| 17 | 7 | 0 | 5 | 2 | 0 | 0 | 0 | 7 | 0 |
| 18 | 19 | 19 | 0 | 0 | 6 | 0 | 0 | 13 | 0 |
| 19 | 36 | 36 | 0 | 0 | 15 | 0 | 0 | 21 | 0 |
| 20 | 12 | 12 | 0 | 0 | 11 | 0 | 0 | 0 | 1 |
| 21 | 15 | 15 | 0 | 0 | 14 | 0 | 0 | 1 | 0 |
| **Razem** | **1144** | 1099 | 41 | 4 | 546 | 0 | 0 | 597 | 1 |

### Etapy

| Etap | Wymagań | verified/deviation | Stan etapu |
|---|---|---|---|
| M0 | 112 | 112 | closed |
| M1 | 202 | 202 | closed |
| M2 | 227 | 227 | closed |
| M3 | 67 | 6 | planned |
| M4 | 170 | 0 | planned |
| M5 | 108 | 0 | planned |
| M6 | 167 | 0 | planned |
| M7 | 87 | 0 | planned |
| LATER | 4 | 0 | planned |

### Pełna lista wymagań


#### Sekcja 0

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-00-001 | 0 | MUST | M7 | manual | todo | Aplikacja desktopowa CAD/CAM 2D do przygotowania projektow i bezposredniego sterowania laserami diodowymi |
| REQ-00-002 | 0 | MUST | M7 | manual | todo | Obsluga laserow CO2 portalowych |
| REQ-00-003 | 0 | MUST | M7 | manual | todo | Obsluga laserow galvo: swiatlowodowych, UV i CO2 |
| REQ-00-004 | 0 | MUST | M7 | manual | todo | Pelny zakres funkcji profesjonalnego programu do laserow (edycja, ustawienia ciecia, podglad, sterowanie maszyna, kamery, tryby specjalne) — wymaganie nadrzedne, rozbite w sekcjach 2-13 |
| REQ-00-005 | 0 | MUST | M6 | auto | todo | Wbudowany serwer MCP umozliwiajacy agentowi AI projektowanie, przygotowanie i (po spelnieniu warunkow bezpieczenstwa) wykonanie zadan |
| REQ-00-006 | 0 | MUST | M7 | manual | todo | Obsluga wszystkich popularnych sterownikow, w tym K40 Lihuiyu M2/M3 Nano, DSP Ruida/Trocen/TopWisdom/Leetro/Moshi, rodzina G-code, galvo EZCAD2/EZCAD3/BSL — nadrzedne, rozbite w sekcji 13 |
| REQ-00-007 | 0 | MUST | M7 | auto | todo | Dzialanie na Windows 10+ |
| REQ-00-008 | 0 | MUST | M7 | auto | todo | Dzialanie na macOS 11+ x64 |
| REQ-00-009 | 0 | MUST | M7 | auto | todo | Dzialanie na macOS 11+ Apple Silicon |
| REQ-00-010 | 0 | MUST | M7 | auto | todo | Dzialanie na Linux x64 |
| REQ-00-011 | 0 | MUST | M7 | auto | todo | Dzialanie na Linux ARM64 |
| REQ-00-012 | 0 | MUST | M7 | hw | todo | Dzialanie na Raspberry Pi 4/5 |
| REQ-00-013 | 0 | MUST | M7 | manual | todo | Realizacja niezrealizowanych postulatow spolecznosci (sekcja 17) — nadrzedne |
| REQ-00-014 | 0 | MUST | M0 | manual | verified | Clean-room: brak dekompilacji, deasemblacji i kopiowania kodu, ikon, tekstow, dokumentacji i zasobow innych programow do laserow |
| REQ-00-015 | 0 | MUST | M0 | manual | verified | Wlasna nazwa, wlasna identyfikacja wizualna i wlasne ikony |
| REQ-00-016 | 0 | MUST | M0 | auto | verified | Wlasny natywny format pliku (nie format .lbrn/.lbrn2) |
| REQ-00-017 | 0 | MUST | M1 | manual | verified | Interoperacyjnosc (.lbrn/.lbrn2/.clb/.lbart/.lbzip) implementowana wylacznie na podstawie analizy plikow uzytkownika, nie kodu programow, ktore je zapisuja |
| REQ-00-018 | 0 | MUST | M3 | manual | verified | Wiedza o protokolach sterownikow wylacznie z publicznych, otwartych zrodel z poszanowaniem licencji |
| REQ-00-019 | 0 | MUST | M0 | manual | verified | Plik THIRD_PARTY.md dokumentuje kazde wykorzystane zrodlo wiedzy i biblioteke |
| REQ-00-020 | 0 | MUST | M0 | manual | verified | Licencja: GPL-3.0-or-later dla aplikacji, MIT/Apache-2.0 dla rdzenia i SDK wtyczek, uzasadnienie w docs/adr/0001-license.md |

#### Sekcja 1

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-01-001 | 1.1 | MUST | M0 | auto | verified | Rdzen bezglowy (headless) — dziala bez GUI |
| REQ-01-002 | 1.1 | MUST | M6 | auto | todo | GUI, CLI, serwer MCP i klient webowy sa cienkimi warstwami nad tym samym API polecen |
| REQ-01-003 | 1.1 | MUST | M0 | auto | verified | Crate bc-geom istnieje w workspace jako osobny modul; zakres: sciezki Beziera, booleany, offsety, splaszczanie, indeks przestrzenny |
| REQ-01-004 | 1.1 | MUST | M0 | auto | verified | Crate bc-doc istnieje w workspace jako osobny modul; zakres: model dokumentu, warstwy, obiekty, undo/redo (command pattern), serializacja .bcraft |
| REQ-01-005 | 1.1 | MUST | M0 | auto | verified | Crate bc-text istnieje w workspace jako osobny modul; zakres: fonty TTF/OTF/SHX/Hershey/jednokreskowe, shaping HarfBuzz, tekst zmienny |
| REQ-01-006 | 1.1 | MUST | M0 | auto | verified | Crate bc-image istnieje w workspace jako osobny modul; zakres: przetwarzanie rastrow, dithering, trasowanie, maski |
| REQ-01-007 | 1.1 | MUST | M0 | auto | verified | Crate bc-io istnieje w workspace jako osobny modul; zakres: import/eksport formatow plikow |
| REQ-01-008 | 1.1 | MUST | M0 | auto | verified | Crate bc-plan istnieje w workspace jako osobny modul; zakres: planer ciecia (kolejnosc, optymalizacja, kerf, mostki, overscan, wypelnienia, rastry) |
| REQ-01-009 | 1.1 | MUST | M0 | auto | verified | Crate bc-sim istnieje w workspace jako osobny modul; zakres: symulacja/podglad sciezek, estymacja czasu z modelem przyspieszen |
| REQ-01-010 | 1.1 | MUST | M0 | auto | verified | Crate bc-drivers istnieje w workspace jako osobny modul; zakres: sterowniki (trait Driver) i transporty |
| REQ-01-011 | 1.1 | MUST | M0 | auto | verified | Crate bc-camera istnieje w workspace jako osobny modul; zakres: kamery, kalibracja, wyrownanie, AprilTag/ArUco |
| REQ-01-012 | 1.1 | MUST | M0 | auto | verified | Crate bc-materials istnieje w workspace jako osobny modul; zakres: biblioteka materialow, testy materialowe |
| REQ-01-013 | 1.1 | MUST | M0 | auto | verified | Crate bc-commands istnieje w workspace jako osobny modul; zakres: rejestr polecen (id, schemat JSON parametrow, undo) |
| REQ-01-014 | 1.1 | MUST | M0 | auto | verified | Crate bc-mcp istnieje w workspace jako osobny modul; zakres: serwer MCP (stdio + Streamable HTTP), zasoby, prompty, polityka bezpieczenstwa |
| REQ-01-015 | 1.1 | MUST | M0 | auto | verified | Crate bc-script istnieje w workspace jako osobny modul; zakres: API skryptowe (Python + JS/Lua), system wtyczek |
| REQ-01-016 | 1.1 | MUST | M0 | auto | verified | Crate bc-cli istnieje w workspace jako osobny modul; zakres: burncraft-cli (operacje wsadowe, konwersja, wysylka zadan, mcp --connect) |
| REQ-01-017 | 1.1 | MUST | M1 | auto | verified | Katalog app/: GUI desktop |
| REQ-01-018 | 1.1 | MUST | M6 | auto | todo | Katalog web/: klient przegladarkowy/tabletowy wspoldzielacy frontend z GUI |
| REQ-01-019 | 1.1 | MUST | M6 | auto | todo | Katalog bridge/: BurnCraft Bridge — obraz Raspberry Pi laczacy laser z siecia |
| REQ-01-020 | 1.2 | SHOULD | M0 | manual | verified | Rdzen w Rust (lub zmiana uzasadniona w ADR) |
| REQ-01-021 | 1.2 | SHOULD | M1 | manual | verified | GUI: Tauri 2 + TypeScript (lub Qt 6/C++20 z uzasadnieniem w ADR) |
| REQ-01-022 | 1.2 | SHOULD | M1 | auto | verified | Obszar roboczy renderowany w WebGL2/WebGPU |
| REQ-01-023 | 1.2 | MUST | M1 | auto | verified | Plynne 60 fps przy 200 tys. wezlow (kafelkowanie/LOD) |
| REQ-01-024 | 1.2 | MUST | M4 | auto | todo | Plynne 60 fps przy obrazach 50 MPix (kafelkowanie/LOD) |
| REQ-01-025 | 1.2 | MUST | M6 | auto | todo | Frontend serwowany przez burncraft --serve jako aplikacja webowa [POSTULAT 325] |
| REQ-01-026 | 1.2 | MUST | M6 | auto | todo | Frontend serwowany przez Bridge |
| REQ-01-027 | 1.2 | MUST | M6 | manual | todo | Aplikacja webowa uzyteczna na tablecie/iPadzie (projektowanie z iPada) [POSTULAT 39] |
| REQ-01-028 | 1.2 | MUST | M6 | manual | todo | Mobilny „Designer” na telefonie [POSTULAT 112] |
| REQ-01-029 | 1.3 | MUST | M1 | auto | verified | Kazda operacja uzytkownika (menu, przycisk, skrot, przeciagniecie uchwytu, zmiana pola) jest poleceniem w rejestrze |
| REQ-01-030 | 1.3 | MUST | M0 | auto | verified | Polecenie ma id (np. shape.offset, layer.set, device.jog) |
| REQ-01-031 | 1.3 | MUST | M0 | auto | verified | Polecenie ma opis |
| REQ-01-032 | 1.3 | MUST | M0 | auto | verified | Polecenie ma schemat JSON parametrow |
| REQ-01-033 | 1.3 | MUST | M0 | auto | verified | Polecenie ma schemat JSON wyniku |
| REQ-01-034 | 1.3 | MUST | M0 | auto | verified | Polecenie ma flagi undoable, requires_device, safety_tier |
| REQ-01-035 | 1.3 | MUST | M0 | auto | verified | Polecenie ma domyslny skrot klawiszowy (opcjonalny) |
| REQ-01-036 | 1.3 | MUST | M1 | auto | verified | GUI wywoluje polecenia wylacznie przez rejestr (zgodnosc 1:1 GUI-MCP) |
| REQ-01-037 | 1.3 | MUST | M0 | auto | verified | Polecenia dzialaja na jawnych ids obiektow lub na biezacym zaznaczeniu |
| REQ-01-038 | 1.3 | MUST | M0 | auto | verified | Wsad (batch) polecen = jeden krok historii |
| REQ-01-039 | 1.3 | MUST | M1 | auto | verified | Edytor skrotow klawiszowych oparty na rejestrze |
| REQ-01-040 | 1.3 | MUST | M2 | auto | verified | Edytor makr: sekwencje polecen |
| REQ-01-041 | 1.3 | MUST | M2 | auto | verified | Edytor makr: surowe komendy do sterownika |
| REQ-01-042 | 1.4 | MUST | M1 | auto | verified | burncraft --control <port> (domyslnie 7984, 127.0.0.1) uruchamia GUI z kanalem sterowania JSON-RPC/WebSocket |
| REQ-01-043 | 1.4 | MUST | M1 | auto | verified | burncraft-cli mcp --connect 7984 wystawia MCP po stdio dzialajac na zywym oknie |
| REQ-01-044 | 1.4 | MUST | M0 | auto | verified | Bez dzialajacego okna CLI uruchamia sesje bezglowa |

#### Sekcja 2

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-02-001 | 2.1 | MUST | M0 | auto | verified | Obiekt sciezka z dowolnymi podsciezkami |
| REQ-02-002 | 2.1 | MUST | M0 | auto | verified | Segmenty sciezki: linie |
| REQ-02-003 | 2.1 | MUST | M1 | auto | verified | Segmenty sciezki: luki |
| REQ-02-004 | 2.1 | MUST | M0 | auto | verified | Segmenty sciezki: kubiczne Beziery |
| REQ-02-005 | 2.1 | MUST | M0 | auto | verified | Segmenty sciezki: kwadratowe Beziery |
| REQ-02-006 | 2.1 | MUST | M0 | auto | verified | Prostokat z promieniem naroznikow |
| REQ-02-007 | 2.1 | MUST | M0 | auto | verified | Elipsa |
| REQ-02-008 | 2.1 | MUST | M1 | auto | verified | Wielokat foremny |
| REQ-02-009 | 2.1 | MUST | M1 | auto | verified | Gwiazda |
| REQ-02-010 | 2.1 | MUST | M1 | auto | verified | Podwojna gwiazda |
| REQ-02-011 | 2.1 | MUST | M1 | auto | verified | Trojkat, pieciokat, osmiokat (presety) |
| REQ-02-012 | 2.1 | MUST | M4 | auto | todo | Tekst edytowalny |
| REQ-02-013 | 2.1 | MUST | M4 | auto | todo | Tekst na sciezce |
| REQ-02-014 | 2.1 | MUST | M4 | auto | todo | Tekst w ramce |
| REQ-02-015 | 2.1 | MUST | M4 | auto | todo | Tekst zmienny |
| REQ-02-016 | 2.1 | MUST | M4 | auto | todo | Kod kreskowy / kod 2D |
| REQ-02-017 | 2.1 | MUST | M4 | auto | todo | Obraz rastrowy z maska, przycieciem i ustawieniami obrazu |
| REQ-02-018 | 2.1 | MUST | M0 | auto | verified | Grupa |
| REQ-02-019 | 2.1 | MUST | M1 | auto | verified | Obiekt wirtualnej tablicy (dynamiczna kopia) |
| REQ-02-020 | 2.1 | MUST | M1 | auto | verified | Obiekt-odniesienie (klon) |
| REQ-02-021 | 2.1 | MUST | M1 | auto | verified | Obiekt pomocniczy (linia prowadzaca, nie wycinana) |
| REQ-02-022 | 2.1 | MUST | M4 | auto | todo | Obiekt „model 3D / mapa glebi” dla grawerowania 3D |
| REQ-02-023 | 2.1 | MUST | M0 | auto | verified | Atrybut: przypisanie do warstwy (kolor) |
| REQ-02-024 | 2.1 | MUST | M0 | auto | verified | Atrybut: transformacja afiniczna niewypiekana do czasu polecenia |
| REQ-02-025 | 2.1 | MUST | M1 | auto | verified | Atrybut: blokada pozycji |
| REQ-02-026 | 2.1 | MUST | M1 | auto | verified | Atrybut: blokada tylko rozmiaru [POSTULAT] |
| REQ-02-027 | 2.1 | MUST | M0 | auto | verified | Atrybut: widocznosc |
| REQ-02-028 | 2.1 | MUST | M0 | auto | verified | Atrybut: nazwa |
| REQ-02-029 | 2.1 | MUST | M1 | auto | verified | Atrybut: notatka |
| REQ-02-030 | 2.1 | MUST | M1 | auto | verified | Atrybut: priorytet |
| REQ-02-031 | 2.1 | MUST | M1 | auto | verified | Atrybut: punkt startowy i kierunek sciezki |
| REQ-02-032 | 2.1 | MUST | M1 | auto | verified | Atrybut: tagi |
| REQ-02-033 | 2.1 | MUST | M1 | auto | verified | Ksztalty parametryczne zachowuja parametry (Okno Wlasciwosci Ksztaltu) do czasu konwersji na sciezke |
| REQ-02-034 | 2.2 | MUST | M0 | auto | verified | 30 domyslnych warstw kolorow |
| REQ-02-035 | 2.2 | MUST | M1 | auto | verified | Narzedzie/warstwa T1 (obszar „wytnij zaznaczone/ramka”) |
| REQ-02-036 | 2.2 | MUST | M1 | auto | verified | Narzedzie/warstwa T2 (pomocnicza, niewycinana) |
| REQ-02-037 | 2.2 | MUST | M1 | auto | verified | Zmiana koloru warstwy [POSTULAT 119] |
| REQ-02-038 | 2.2 | MUST | M1 | auto | verified | Zmiana nazwy warstwy |
| REQ-02-039 | 2.2 | MUST | M1 | auto | verified | Dodawanie wlasnych kolorow palety [POSTULAT 20] |
| REQ-02-040 | 2.2 | MUST | M1 | auto | verified | Panel listy obiektow (drzewo jak w programach graficznych) [POSTULAT 140] |
| REQ-02-041 | 2.2 | MUST | M1 | auto | verified | Panel obiektow: przeciaganie (zmiana rodzica/kolejnosci) |
| REQ-02-042 | 2.2 | MUST | M1 | auto | verified | Panel obiektow: ukrywanie i blokowanie |
| REQ-02-043 | 2.2 | MUST | M1 | auto | verified | Panel obiektow: zmiana kolejnosci rysowania |
| REQ-02-044 | 2.2 | MUST | M1 | auto | verified | Panel obiektow: wyszukiwanie |
| REQ-02-045 | 2.2 | MUST | M1 | auto | verified | Nazwy warstw zapisywane per plik [POSTULAT 27] |
| REQ-02-046 | 2.3 | MUST | M0 | auto | verified | .bcraft = ZIP z document.json |
| REQ-02-047 | 2.3 | MUST | M0 | auto | verified | document.json ma wersjonowany schemat JSON i jest walidowany przy odczycie |
| REQ-02-048 | 2.3 | MUST | M4 | auto | todo | Osadzone obrazy w .bcraft (PNG / 16-bit TIFF) |
| REQ-02-049 | 2.3 | MUST | M4 | auto | todo | Osadzone fonty w .bcraft (opcjonalnie) |
| REQ-02-050 | 2.3 | MUST | M0 | auto | verified | Miniatura w .bcraft |
| REQ-02-051 | 2.3 | MUST | M0 | auto | verified | Notatki w .bcraft |
| REQ-02-052 | 2.3 | MUST | M0 | auto | verified | Zgodnosc wstecz i migracje schematu |
| REQ-02-053 | 2.3 | MUST | M1 | auto | verified | Pliki szablonow .bcraft-template — otwarcie tworzy nowy dokument |
| REQ-02-054 | 2.3 | MUST | M1 | auto | verified | Autozapis co N minut |
| REQ-02-055 | 2.3 | MUST | M1 | auto | verified | Odzyskiwanie po awarii |
| REQ-02-056 | 2.3 | MUST | M1 | auto | verified | Kopie zapasowe preferencji z przegladarka kopii (BackupBrowser) |
| REQ-02-057 | 2.3 | MUST | M7 | auto | todo | Miniatury plikow w Eksploratorze Windows |
| REQ-02-058 | 2.3 | MUST | M7 | auto | todo | Miniatury w macOS Quick Look |
| REQ-02-059 | 2.3 | MUST | M7 | auto | todo | Miniatury w menedzerach plikow Linux |
| REQ-02-060 | 2.3 | MUST | M1 | auto | verified | Wiele dokumentow w zakladkach [POSTULAT 82] ⚠ konflikt priorytetu |
| REQ-02-061 | 2.3 | MUST | M1 | auto | verified | Kopiowanie obiektow miedzy zakladkami dokumentow ⚠ konflikt priorytetu |
| REQ-02-062 | 2.4 | MUST | M1 | auto | verified | Import SVG (Inkscape/Illustrator/Affinity) z automatycznym DPI |
| REQ-02-063 | 2.4 | MUST | M1 | auto | verified | Eksport SVG |
| REQ-02-064 | 2.4 | MUST | M1 | auto | verified | SVG: mapowanie na warstwy ciecia wg koloru obrysu/wypelnienia |
| REQ-02-065 | 2.4 | MUST | M1 | auto | verified | SVG: mapowanie na warstwy ciecia wg nazwy warstwy SVG [POSTULAT 29] |
| REQ-02-066 | 2.4 | MUST | M1 | auto | verified | SVG: zagniezdzone SVG |
| REQ-02-067 | 2.4 | MUST | M4 | auto | todo | SVG: obrazy osadzone |
| REQ-02-068 | 2.4 | MUST | M1 | auto | verified | Import DXF R12-2018 |
| REQ-02-069 | 2.4 | MUST | M1 | auto | verified | DXF: splajny |
| REQ-02-070 | 2.4 | MUST | M1 | auto | verified | DXF: bloki |
| REQ-02-071 | 2.4 | MUST | M1 | auto | verified | DXF: luki |
| REQ-02-072 | 2.4 | MUST | M4 | auto | todo | DXF: tekst z opcja „ignoruj tekst” |
| REQ-02-073 | 2.4 | MUST | M1 | auto | verified | DXF: jednostki z naglowka |
| REQ-02-074 | 2.4 | MUST | M1 | auto | verified | Eksport DXF AC1009 |
| REQ-02-075 | 2.4 | MUST | M1 | auto | verified | Eksport DXF AC1021 |
| REQ-02-076 | 2.4 | MUST | M4 | auto | todo | Import AI (PDF-based) z kanalem alfa |
| REQ-02-077 | 2.4 | MUST | M4 | auto | todo | Import PDF |
| REQ-02-078 | 2.4 | MUST | M4 | auto | todo | Eksport PDF |
| REQ-02-079 | 2.4 | MUST | M6 | auto | todo | Import EPS/PS przez Ghostscript (gdy dostepny) [POSTULAT 90] |
| REQ-02-080 | 2.4 | MUST | M1 | auto | verified | Import HPGL/PLT |
| REQ-02-081 | 2.4 | MUST | M1 | auto | verified | Eksport HPGL/PLT |
| REQ-02-082 | 2.4 | MUST | M6 | auto | todo | Import Gerber RS-274X [POSTULAT 36] ⚠ konflikt priorytetu |
| REQ-02-083 | 2.4 | MUST | M6 | auto | todo | Import Excellon ⚠ konflikt priorytetu |
| REQ-02-084 | 2.4 | SHOULD | M7 | auto | todo | Import DWG przez ODA/LibreDWG [POSTULAT 70] |
| REQ-02-085 | 2.4 | SHOULD | M7 | auto | todo | Import CDR przez libcdr [POSTULAT 62] |
| REQ-02-086 | 2.4 | SHOULD | M7 | auto | todo | Import RLD (RDWorks) [POSTULAT 44] |
| REQ-02-087 | 2.4 | SHOULD | M3 | auto | todo | Podglad/dekodowanie plikow .rd (Ruida) |
| REQ-02-088 | 2.4 | MUST | M3 | emu | todo | Eksport pliku .rd dla Ruidy na USB |
| REQ-02-089 | 2.4 | MUST | M2 | auto | verified | Import G-code (.nc, .gc, .gcode) — podglad i uruchamianie |
| REQ-02-090 | 2.4 | MUST | M2 | auto | verified | Eksport G-code |
| REQ-02-091 | 2.4 | MUST | M1 | auto | verified | Import .lbrn |
| REQ-02-092 | 2.4 | MUST | M1 | auto | verified | Import .lbrn2 |
| REQ-02-093 | 2.4 | MUST | M4 | auto | todo | Import bibliotek materialow .clb |
| REQ-02-094 | 2.4 | MUST | M4 | auto | todo | Import bibliotek grafik .lbart |
| REQ-02-095 | 2.4 | MUST | M4 | auto | todo | Import pakietow ustawien urzadzen .lbzip |
| REQ-02-096 | 2.4 | MUST | M4 | auto | todo | Import PNG |
| REQ-02-097 | 2.4 | MUST | M4 | auto | todo | Import JPG |
| REQ-02-098 | 2.4 | MUST | M4 | auto | todo | Import BMP |
| REQ-02-099 | 2.4 | MUST | M4 | auto | todo | Import GIF |
| REQ-02-100 | 2.4 | MUST | M4 | auto | todo | Import TIFF 8-bit |
| REQ-02-101 | 2.4 | MUST | M4 | auto | todo | Import TIFF 16-bit |
| REQ-02-102 | 2.4 | MUST | M4 | auto | todo | Import WebP |
| REQ-02-103 | 2.4 | SHOULD | M4 | auto | todo | Import HEIC (gdy dostepny kodek systemowy) |
| REQ-02-104 | 2.4 | MUST | M4 | auto | todo | Eksport przetworzonej bitmapy (PNG/JPG/BMP/TIFF) |
| REQ-02-105 | 2.4 | SHOULD | M5 | auto | todo | Import STL/OBJ jako mapa glebi |
| REQ-02-106 | 2.4 | MUST | M6 | auto | todo | Eksport projektu do PDF w skali 1:1 z legenda warstw [POSTULAT 17] |
| REQ-02-107 | 2.4 | MUST | M6 | auto | todo | Drukowanie „proof” projektu w skali 1:1 z legenda warstw [POSTULAT 72] |
| REQ-02-108 | 2.4 | SHOULD | M7 | auto | todo | Wtyczka/makro do CorelDRAW „Wyslij do BurnCraft” |
| REQ-02-109 | 2.4 | SHOULD | M6 | auto | todo | Rozszerzenie Inkscape „Wyslij do BurnCraft” |
| REQ-02-110 | 2.4 | SHOULD | M7 | auto | todo | Rozszerzenie Illustrator „Wyslij do BurnCraft” |
| REQ-02-111 | 2.4 | SHOULD | M6 | auto | todo | Integracje webowe: menu „Narzedzia web” i lokalny endpoint przyjmujacy projekty z generatorow |

#### Sekcja 3

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-03-001 | 3 | MUST | M1 | manual | verified | Obszar roboczy: siatka |
| REQ-03-002 | 3 | MUST | M1 | manual | verified | Obszar roboczy: linijki |
| REQ-03-003 | 3 | MUST | M1 | manual | verified | Obszar roboczy: obszar maszyny |
| REQ-03-004 | 3 | MUST | M2 | manual | verified | Obszar roboczy: widoczne polozenie glowicy |
| REQ-03-005 | 3 | MUST | M5 | manual | todo | Obszar roboczy: nakladka obrazu z kamery |
| REQ-03-006 | 3 | MUST | M1 | manual | verified | Przyciaganie do siatki |
| REQ-03-007 | 3 | MUST | M1 | manual | verified | Przyciaganie do obiektow |
| REQ-03-008 | 3 | MUST | M1 | manual | verified | Przyciaganie do wezlow |
| REQ-03-009 | 3 | MUST | M1 | manual | verified | Przyciaganie do srodkow |
| REQ-03-010 | 3 | MUST | M1 | manual | verified | Przyciaganie do punktow srodkowych linii [POSTULAT 24] |
| REQ-03-011 | 3 | MUST | M1 | manual | verified | Prowadnice automatyczne |
| REQ-03-012 | 3 | MUST | M1 | manual | verified | Prowadnice reczne |
| REQ-03-013 | 3 | MUST | M1 | manual | verified | Zoom do zaznaczenia |
| REQ-03-014 | 3 | MUST | M1 | manual | verified | Zoom do strony |
| REQ-03-015 | 3 | MUST | M1 | manual | verified | Zoom 100% rzeczywistego rozmiaru [POSTULAT 23] |
| REQ-03-016 | 3 | MUST | M1 | manual | verified | Motyw ciemny / jasny / systemowy |
| REQ-03-017 | 3 | MUST | M1 | manual | verified | Ciemne tlo obszaru roboczego |
| REQ-03-018 | 3 | MUST | M1 | manual | verified | Skalowanie High-DPI / 4K [POSTULAT 18] |
| REQ-03-019 | 3 | MUST | M1 | manual | verified | Gesty dotykowe i trackpada (pinch-zoom) [POSTULAT 25] |
| REQ-03-020 | 3 | MUST | M1 | manual | verified | Pasek narzedzi glowny |
| REQ-03-021 | 3 | MUST | M1 | manual | verified | Pasek narzedzi tworzenia |
| REQ-03-022 | 3 | MUST | M1 | manual | verified | Pasek trybow |
| REQ-03-023 | 3 | MUST | M1 | manual | verified | Pasek modyfikatorow |
| REQ-03-024 | 3 | MUST | M1 | manual | verified | Pasek edycji numerycznej: X/Y/W/H/obrot |
| REQ-03-025 | 3 | MUST | M1 | manual | verified | Edycja numeryczna: 9-punktowy punkt odniesienia |
| REQ-03-026 | 3 | MUST | M1 | manual | verified | Edycja numeryczna: blokada proporcji |
| REQ-03-027 | 3 | MUST | M1 | manual | verified | Edycja numeryczna: przelacznik mm/in |
| REQ-03-028 | 3 | MUST | M1 | manual | verified | Szybkie przeliczanie wartosci mm<->in w polach [POSTULAT 57] |
| REQ-03-029 | 3 | MUST | M1 | manual | verified | Pasek wyrownywania |
| REQ-03-030 | 3 | MUST | M1 | manual | verified | Pasek wezlow |
| REQ-03-031 | 3 | MUST | M1 | manual | verified | Paski konfigurowalne, dokowalne, z zapisem ukladow |
| REQ-03-032 | 3 | MUST | M1 | manual | verified | Okno Warstwy/Ciecia |
| REQ-03-033 | 3 | MUST | M2 | manual | verified | Okno Laser: start/pauza/stop, ramka, punkt startowy, pozycja zadania |
| REQ-03-034 | 3 | MUST | M2 | manual | verified | Okno Przesun: jog XY |
| REQ-03-035 | 3 | MUST | M2 | manual | verified | Okno Przesun: jog Z |
| REQ-03-036 | 3 | MUST | M5 | manual | todo | Okno Przesun: jog osi obrotowej |
| REQ-03-037 | 3 | MUST | M2 | manual | verified | Okno Przesun: kroki i predkosci jogu |
| REQ-03-038 | 3 | MUST | M2 | manual | verified | Okno Przesun: predkosc wolna/szybka [POSTULAT 21] |
| REQ-03-039 | 3 | MUST | M2 | manual | verified | Okno Przesun: zapamietane pozycje XY z nazwami [POSTULAT 21] |
| REQ-03-040 | 3 | MUST | M2 | manual | verified | Okno Konsola z kolorowaniem skladni i historia |
| REQ-03-041 | 3 | MUST | M2 | manual | verified | Okno Makra |
| REQ-03-042 | 3 | MUST | M3 | manual | todo | Okno Lista plikow na sterowniku |
| REQ-03-043 | 3 | MUST | M4 | manual | todo | Okno Biblioteka materialow |
| REQ-03-044 | 3 | MUST | M4 | manual | todo | Okno Biblioteka grafik |
| REQ-03-045 | 3 | MUST | M5 | manual | todo | Okno Kamera |
| REQ-03-046 | 3 | MUST | M2 | manual | verified | Okno Podglad |
| REQ-03-047 | 3 | MUST | M1 | manual | verified | Okno Wlasciwosci ksztaltu |
| REQ-03-048 | 3 | MUST | M1 | manual | verified | Okno Historia cofania z podgladem stanow i skokiem |
| REQ-03-049 | 3 | MUST | M1 | manual | verified | Okno Notatki z obrazami [POSTULAT 21] |
| REQ-03-050 | 3 | MUST | M4 | manual | todo | Okno Menedzer fontow |
| REQ-03-051 | 3 | MUST | M1 | manual | verified | Tryb poczatkujacego (ukryte zaawansowane opcje) i tryb pelny |
| REQ-03-052 | 3 | MUST | M2 | manual | verified | Przyciski Start zielony / Stop czerwony z konfigurowalnymi kolorami [POSTULAT 60] |
| REQ-03-053 | 3 | MUST | M2 | manual | verified | Widoczny czas zadania po jego zakonczeniu [POSTULAT 22] |
| REQ-03-054 | 3 | MUST | M2 | manual | verified | Licznik przejsc „przejscie 3/5” obok procentu [POSTULAT 17] |
| REQ-03-055 | 3 | MUST | M1 | manual | verified | Kursor zalezny od narzedzia [POSTULAT 17] |
| REQ-03-056 | 3 | MUST | M1 | manual | verified | Menu kontekstowe w edycji wezlow [POSTULAT 39] |
| REQ-03-057 | 3 | MUST | M1 | manual | verified | Podpowiedzi kontekstowe do kazdego pola |
| REQ-03-058 | 3 | MUST | M7 | manual | todo | Pomoc F1 do kazdego pola |
| REQ-03-059 | 3 | MUST | M1 | auto | verified | Pliki tlumaczen Fluent/ICU, latwe do uzupelniania |
| REQ-03-060 | 3 | MUST | M7 | manual | todo | Lokalizacja: polski |
| REQ-03-061 | 3 | MUST | M1 | manual | verified | Lokalizacja: angielski |
| REQ-03-062 | 3 | MUST | M7 | manual | todo | Lokalizacja: niemiecki |
| REQ-03-063 | 3 | MUST | M7 | manual | todo | Lokalizacja: francuski |
| REQ-03-064 | 3 | MUST | M7 | manual | todo | Lokalizacja: hiszpanski |
| REQ-03-065 | 3 | MUST | M7 | manual | todo | Lokalizacja: wloski |
| REQ-03-066 | 3 | MUST | M7 | manual | todo | Lokalizacja: portugalski (Brazylia) |
| REQ-03-067 | 3 | MUST | M7 | manual | todo | Lokalizacja: niderlandzki |
| REQ-03-068 | 3 | MUST | M7 | manual | todo | Lokalizacja: czeski |
| REQ-03-069 | 3 | MUST | M7 | manual | todo | Lokalizacja: ukrainski |
| REQ-03-070 | 3 | MUST | M7 | manual | todo | Lokalizacja: rosyjski |
| REQ-03-071 | 3 | MUST | M7 | manual | todo | Lokalizacja: japonski |
| REQ-03-072 | 3 | MUST | M7 | manual | todo | Lokalizacja: chinski uproszczony |
| REQ-03-073 | 3 | MUST | M7 | manual | todo | Lokalizacja: chinski tradycyjny |
| REQ-03-074 | 3 | MUST | M7 | manual | todo | Lokalizacja: koreanski |
| REQ-03-075 | 3 | MUST | M7 | manual | todo | Lokalizacja: turecki |
| REQ-03-076 | 3 | MUST | M7 | manual | todo | Dostepnosc: pelna obsluga klawiatura |
| REQ-03-077 | 3 | MUST | M7 | manual | todo | Dostepnosc: czytniki ekranu dla paneli |
| REQ-03-078 | 3 | MUST | M7 | manual | todo | Dostepnosc: tryb wysokiego kontrastu |
| REQ-03-079 | 3 | MUST | M7 | manual | todo | Dostepnosc: palety przyjazne daltonistom |
| REQ-03-080 | 3 | MUST | M6 | manual | todo | Gamepad: jogowanie |
| REQ-03-081 | 3 | MUST | M6 | manual | todo | Gamepad: ramka |
| REQ-03-082 | 3 | MUST | M6 | manual | todo | Gamepad: start/pauza |
| REQ-03-083 | 3 | MUST | M6 | manual | todo | Gamepad: edytor mapowania przyciskow |

#### Sekcja 4

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-04-001 | 4.1 | MUST | M1 | auto | verified | Rysowanie linii/polilinii z Bezierami (klik / przeciagniecie) |
| REQ-04-002 | 4.1 | MUST | M1 | auto | verified | Wprowadzanie dokladnych dlugosci i katow podczas rysowania [POSTULAT 43] |
| REQ-04-003 | 4.1 | MUST | M1 | auto | verified | Narzedzie luku: 3 punkty [POSTULAT 37] |
| REQ-04-004 | 4.1 | MUST | M1 | auto | verified | Narzedzie luku: srodek + promien + katy |
| REQ-04-005 | 4.1 | MUST | M1 | auto | verified | Narzedzie prostokata |
| REQ-04-006 | 4.1 | MUST | M1 | auto | verified | Narzedzie elipsy |
| REQ-04-007 | 4.1 | MUST | M1 | auto | verified | Narzedzie wielokatow |
| REQ-04-008 | 4.1 | MUST | M1 | auto | verified | Narzedzie gwiazd |
| REQ-04-009 | 4.1 | MUST | M4 | auto | todo | Narzedzie tekstu |
| REQ-04-010 | 4.1 | MUST | M4 | auto | todo | Narzedzie kodow |
| REQ-04-011 | 4.1 | MUST | M1 | auto | verified | Pomiar odleglosci |
| REQ-04-012 | 4.1 | MUST | M1 | auto | verified | Pomiar kata |
| REQ-04-013 | 4.1 | MUST | M6 | auto | todo | Wymiarowanie: linia wymiarowa pozioma [POSTULAT 92] |
| REQ-04-014 | 4.1 | MUST | M6 | auto | todo | Wymiarowanie: linia wymiarowa pionowa [POSTULAT 50] |
| REQ-04-015 | 4.1 | MUST | M6 | auto | todo | Wymiarowanie: linia wymiarowa wyrownana [POSTULAT 46] |
| REQ-04-016 | 4.1 | MUST | M6 | auto | todo | Wymiarowanie: wymiar katowy |
| REQ-04-017 | 4.1 | MUST | M6 | auto | todo | Wymiarowanie: wymiar promienia |
| REQ-04-018 | 4.1 | MUST | M6 | auto | todo | Wymiary jako obiekty pomocnicze niewycinane, z opcja wyciecia/grawerowania |
| REQ-04-019 | 4.1 | MUST | M1 | auto | verified | Generator okregow stycznych do 3 okregow |
| REQ-04-020 | 4.1 | MUST | M1 | auto | verified | Prowadnice (tworzenie) |
| REQ-04-021 | 4.1 | MUST | M1 | auto | verified | Gumka / narzedzie wymazywania dla wektorow [POSTULAT 25] ⚠ konflikt priorytetu |
| REQ-04-022 | 4.1 | MUST | M1 | auto | verified | Zaznaczanie lasso / odreczne [POSTULAT 21] |
| REQ-04-023 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: dodaj wezel |
| REQ-04-024 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: usun wezel |
| REQ-04-025 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: przelam |
| REQ-04-026 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: polacz |
| REQ-04-027 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: wygladz |
| REQ-04-028 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: narozniki |
| REQ-04-029 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: linia <-> krzywa |
| REQ-04-030 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: ustaw punkt startowy |
| REQ-04-031 | 4.2 | MUST | M1 | auto | verified | Edycja wezlow: odwroc kierunek |
| REQ-04-032 | 4.2 | MUST | M1 | auto | verified | Dokladna pozycja wezla numerycznie [POSTULAT 21] |
| REQ-04-033 | 4.2 | MUST | M1 | auto | verified | Wyrownywanie wezlow [POSTULAT 23] |
| REQ-04-034 | 4.2 | MUST | M1 | auto | verified | Przycinanie (trim) do przeciec |
| REQ-04-035 | 4.2 | MUST | M1 | auto | verified | Nozyczki (przeciecie sciezki w punkcie) |
| REQ-04-036 | 4.2 | MUST | M1 | auto | verified | Konwersja do sciezki |
| REQ-04-037 | 4.2 | MUST | M1 | auto | verified | Auto-laczenie z tolerancja |
| REQ-04-038 | 4.2 | MUST | M1 | auto | verified | Zamykanie sciezek z tolerancja |
| REQ-04-039 | 4.2 | MUST | M1 | auto | verified | Usuwanie duplikatow |
| REQ-04-040 | 4.2 | MUST | M1 | auto | verified | Usuwanie nakladajacych sie odcinkow |
| REQ-04-041 | 4.2 | MUST | M1 | auto | verified | Rozbijanie (break apart) |
| REQ-04-042 | 4.2 | MUST | M1 | auto | verified | Optymalizacja: redukcja wezlow |
| REQ-04-043 | 4.2 | MUST | M1 | auto | verified | Optymalizacja: wygladzanie |
| REQ-04-044 | 4.2 | MUST | M1 | auto | verified | Optymalizacja: zamiana odcinkow na luki |
| REQ-04-045 | 4.2 | MUST | M1 | auto | verified | Wypaczanie 4-punktowe |
| REQ-04-046 | 4.2 | MUST | M1 | auto | verified | Deformacja siatka (mesh) |
| REQ-04-047 | 4.2 | MUST | M1 | auto | verified | Deformacja obwiednia (envelope) |
| REQ-04-048 | 4.2 | MUST | M1 | auto | verified | Tymczasowe undo w trybie wypaczania |
| REQ-04-049 | 4.2 | MUST | M4 | auto | todo | Wypaczanie tekstu do dowolnego ksztaltu („word art”) [POSTULAT 92] |
| REQ-04-050 | 4.2 | MUST | M1 | auto | verified | Obrot dwupunktowy |
| REQ-04-051 | 4.2 | MUST | M1 | auto | verified | Skala dwupunktowa |
| REQ-04-052 | 4.2 | MUST | M1 | auto | verified | Obrys „gumka” (convex hull) |
| REQ-04-053 | 4.2 | MUST | M1 | auto | verified | Fazowanie 45° (chamfer) per wezel [POSTULAT 38] |
| REQ-04-054 | 4.2 | MUST | M1 | auto | verified | Zaokraglanie naroznikow (fillet) per wezel |
| REQ-04-055 | 4.2 | MUST | M1 | auto | verified | Edycja wewnatrz grup bez rozgrupowania [POSTULAT 34] ⚠ konflikt priorytetu |
| REQ-04-056 | 4.3 | MUST | M1 | auto | verified | Odbicie lustrzane |
| REQ-04-057 | 4.3 | MUST | M1 | auto | verified | Offset wewnetrzny |
| REQ-04-058 | 4.3 | MUST | M1 | auto | verified | Offset zewnetrzny |
| REQ-04-059 | 4.3 | MUST | M1 | auto | verified | Offset obustronny |
| REQ-04-060 | 4.3 | MUST | M1 | auto | verified | Offset: narozniki ostre |
| REQ-04-061 | 4.3 | MUST | M1 | auto | verified | Offset: narozniki zaokraglone |
| REQ-04-062 | 4.3 | MUST | M1 | auto | verified | Offset: narozniki sciete |
| REQ-04-063 | 4.3 | MUST | M1 | auto | verified | Offset: opcja usuniecia oryginalu |
| REQ-04-064 | 4.3 | MUST | M1 | auto | verified | Boolean: suma |
| REQ-04-065 | 4.3 | MUST | M1 | auto | verified | Boolean: roznica |
| REQ-04-066 | 4.3 | MUST | M1 | auto | verified | Boolean: iloczyn |
| REQ-04-067 | 4.3 | MUST | M1 | auto | verified | Boolean: XOR |
| REQ-04-068 | 4.3 | MUST | M1 | auto | verified | Boolean: wyciecie ksztaltow (cut shapes) |
| REQ-04-069 | 4.3 | MUST | M1 | auto | verified | Asystent booleanow |
| REQ-04-070 | 4.3 | MUST | M1 | auto | verified | Booleany „stosowe” na wielu obiektach |
| REQ-04-071 | 4.3 | MUST | M1 | auto | verified | Tablica prostokatna |
| REQ-04-072 | 4.3 | MUST | M1 | auto | verified | Tablica prostokatna: przesuniecie wierszy |
| REQ-04-073 | 4.3 | MUST | M1 | auto | verified | Tablica wirtualna/dynamiczna |
| REQ-04-074 | 4.3 | MUST | M6 | auto | todo | Tablica z losowymi rozmiarami [POSTULAT 73] |
| REQ-04-075 | 4.3 | MUST | M6 | auto | todo | Tablica z losowymi obrotami |
| REQ-04-076 | 4.3 | MUST | M6 | auto | todo | Tablica z losowymi przesunieciami |
| REQ-04-077 | 4.3 | MUST | M1 | auto | verified | Tablica kolowa |
| REQ-04-078 | 4.3 | MUST | M1 | auto | verified | Kopiowanie wzdluz sciezki |
| REQ-04-079 | 4.3 | MUST | M4 | auto | todo | Polecenie: tekst po sciezce |
| REQ-04-080 | 4.3 | MUST | M1 | auto | verified | Zrownanie szerokosci |
| REQ-04-081 | 4.3 | MUST | M1 | auto | verified | Zrownanie wysokosci |
| REQ-04-082 | 4.3 | MUST | M1 | auto | verified | Zmiana rozmiaru wpustow i czopow (slot/tab resizer) z tolerancja materialu |
| REQ-04-083 | 4.3 | MUST | M2 | auto | verified | Mostki/tabulatory reczne |
| REQ-04-084 | 4.3 | MUST | M2 | auto | verified | Mostki/tabulatory automatyczne |
| REQ-04-085 | 4.3 | MUST | M2 | auto | verified | Kreator mostkow |
| REQ-04-086 | 4.4 | MUST | M1 | auto | verified | Wyrownanie do zaznaczenia |
| REQ-04-087 | 4.4 | MUST | M1 | auto | verified | Wyrownanie do pierwszego/ostatniego zaznaczonego |
| REQ-04-088 | 4.4 | MUST | M1 | auto | verified | Wyrownanie do strony |
| REQ-04-089 | 4.4 | MUST | M2 | auto | verified | Wyrownanie do pozycji lasera |
| REQ-04-090 | 4.4 | MUST | M1 | auto | verified | Rozklad rownomierny |
| REQ-04-091 | 4.4 | MUST | M1 | auto | verified | Dokowanie |
| REQ-04-092 | 4.4 | MUST | M1 | auto | verified | Przesuwanie o wartosc |
| REQ-04-093 | 4.4 | MUST | M1 | auto | verified | Zmiana kolejnosci rysowania |
| REQ-04-094 | 4.4 | MUST | M0 | auto | verified | Grupowanie / rozgrupowanie |
| REQ-04-095 | 4.4 | MUST | M1 | auto | verified | Quick Nest (prostokaty) |
| REQ-04-096 | 4.4 | MUST | M6 | auto | todo | Nesting ksztaltow (NFP / genetyczny jak SVGnest/Deepnest) [POSTULAT 113] |
| REQ-04-097 | 4.4 | MUST | M6 | auto | todo | Nesting: obroty co N° |
| REQ-04-098 | 4.4 | MUST | M6 | auto | todo | Nesting: odbicia |
| REQ-04-099 | 4.4 | MUST | M6 | auto | todo | Nesting: odstep |
| REQ-04-100 | 4.4 | MUST | M6 | auto | todo | Nesting: czesci wewnatrz otworow |
| REQ-04-101 | 4.4 | MUST | M6 | auto | todo | Nesting: wiele arkuszy |
| REQ-04-102 | 4.4 | MUST | M6 | auto | todo | Nesting: wykorzystanie resztek |
| REQ-04-103 | 4.4 | MUST | M6 | auto | todo | Nesting na obrazie z kamery wokol nieregularnego materialu |
| REQ-04-104 | 4.5 | MUST | M6 | auto | todo | Generator zawiasow zywych: wzor prosty [POSTULAT 84] ⚠ konflikt priorytetu |
| REQ-04-105 | 4.5 | MUST | M6 | auto | todo | Generator zawiasow zywych: wzor falowy ⚠ konflikt priorytetu |
| REQ-04-106 | 4.5 | MUST | M6 | auto | todo | Generator zawiasow zywych: plaster miodu ⚠ konflikt priorytetu |
| REQ-04-107 | 4.5 | MUST | M6 | auto | todo | Generator zawiasow zywych: spiralny ⚠ konflikt priorytetu |
| REQ-04-108 | 4.5 | MUST | M6 | auto | todo | Zawiasy: parametry grubosc, szerokosc, giecie ⚠ konflikt priorytetu |
| REQ-04-109 | 4.5 | SHOULD | M6 | auto | todo | Generator pudelek finger joint z kerfem |
| REQ-04-110 | 4.5 | SHOULD | M6 | auto | todo | Generator pudelek: pokrywka i przegrody |
| REQ-04-111 | 4.5 | SHOULD | M6 | auto | todo | Generator puzzli |
| REQ-04-112 | 4.5 | SHOULD | M6 | auto | todo | Generator kol zebatych |
| REQ-04-113 | 4.5 | SHOULD | M6 | auto | todo | Generator napisow pierscieniowych (ring text) [POSTULAT 19] |
| REQ-04-114 | 4.5 | SHOULD | M6 | auto | todo | Generator slojow drewna [POSTULAT 30] |
| REQ-04-115 | 4.5 | SHOULD | M6 | auto | todo | Wzory kreskowania: fale, kropki, tekstury, kafle [POSTULAT 34] |
| REQ-04-116 | 4.5 | SHOULD | M6 | auto | todo | Generator gradientow / ramp mocy [POSTULAT 19] |
| REQ-04-117 | 4.5 | SHOULD | M6 | auto | todo | Kazdy generator jest poleceniem z parametrami dostepnym przez MCP |

#### Sekcja 5

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-05-001 | 5 | MUST | M4 | auto | todo | Fonty systemowe |
| REQ-05-002 | 5 | MUST | M4 | auto | todo | Fonty z folderu uzytkownika |
| REQ-05-003 | 5 | MUST | M4 | auto | todo | Fonty jednokreskowe Hershey |
| REQ-05-004 | 5 | MUST | M4 | auto | todo | Fonty SHX |
| REQ-05-005 | 5 | MUST | M4 | auto | todo | SVG-fonts |
| REQ-05-006 | 5 | MUST | M4 | auto | todo | Stick fonts |
| REQ-05-007 | 5 | MUST | M4 | auto | todo | Menedzer fontow: wyszukiwanie |
| REQ-05-008 | 5 | MUST | M4 | auto | todo | Menedzer fontow: ulubione |
| REQ-05-009 | 5 | MUST | M4 | auto | todo | Menedzer fontow: ukrywanie |
| REQ-05-010 | 5 | MUST | M4 | auto | todo | Menedzer fontow: kategorie [POSTULAT 25] ⚠ konflikt priorytetu |
| REQ-05-011 | 5 | MUST | M4 | auto | todo | Menedzer fontow: ostatnio uzywane [POSTULAT 24] ⚠ konflikt priorytetu |
| REQ-05-012 | 5 | MUST | M4 | auto | todo | Edycja tekstu bezposrednio w obszarze roboczym |
| REQ-05-013 | 5 | MUST | M4 | auto | todo | Edycja tekstu w oknie |
| REQ-05-014 | 5 | MUST | M4 | auto | todo | Tekst wielowierszowy |
| REQ-05-015 | 5 | MUST | M4 | auto | todo | Rich text: rozne fonty/rozmiary/style w jednym tekscie [POSTULAT 97] |
| REQ-05-016 | 5 | MUST | M4 | auto | todo | Wyrownanie tekstu |
| REQ-05-017 | 5 | MUST | M4 | auto | todo | Odstepy (litery, wiersze) |
| REQ-05-018 | 5 | MUST | M4 | auto | todo | Kerning |
| REQ-05-019 | 5 | MUST | M4 | auto | todo | Tekst pionowy [POSTULAT 149] |
| REQ-05-020 | 5 | MUST | M4 | auto | todo | Zawijanie tekstu w ramce |
| REQ-05-021 | 5 | MUST | M4 | auto | todo | Rozbijanie bloku tekstu na wiersze [POSTULAT 19] |
| REQ-05-022 | 5 | MUST | M4 | auto | todo | Alternatywne glify OpenType [POSTULAT 33] ⚠ konflikt priorytetu |
| REQ-05-023 | 5 | MUST | M4 | auto | todo | Ligatury OpenType |
| REQ-05-024 | 5 | MUST | M4 | auto | todo | Spawanie liter (weld) |
| REQ-05-025 | 5 | MUST | M4 | auto | todo | Sprawdzanie pisowni (Hunspell, slowniki wielojezyczne) [POSTULAT 87] |
| REQ-05-026 | 5 | MUST | M4 | auto | todo | Tekst zmienny: numeracja seryjna (start/krok/zakres/format) |
| REQ-05-027 | 5 | MUST | M4 | auto | todo | Tekst zmienny: data/czas |
| REQ-05-028 | 5 | MUST | M4 | auto | todo | Tekst zmienny: pola z CSV |
| REQ-05-029 | 5 | MUST | M4 | auto | todo | Tekst zmienny: pola z XLSX |
| REQ-05-030 | 5 | MUST | M4 | auto | todo | Tekst zmienny: laczenie korespondencji (merge) |
| REQ-05-031 | 5 | MUST | M4 | auto | todo | Tekst zmienny: podglad rekordu |
| REQ-05-032 | 5 | MUST | M4 | auto | todo | Tekst zmienny: automatyczny przyrost po kazdym zadaniu |
| REQ-05-033 | 5 | MUST | M4 | auto | todo | Tekst zmienny: pomijanie pustych linii |
| REQ-05-034 | 5 | MUST | M4 | auto | todo | Wiele plikow CSV w projekcie [POSTULAT 270] |
| REQ-05-035 | 5 | MUST | M4 | auto | todo | Zmienne obrazy/logo z CSV (kolumna ze sciezka obrazu/SVG) [POSTULAT 143] |
| REQ-05-036 | 5 | MUST | M4 | auto | todo | Zmienne w kodach kreskowych |
| REQ-05-037 | 5 | MUST | M4 | auto | todo | Kod 1D Code128 |
| REQ-05-038 | 5 | MUST | M4 | auto | todo | Kod 1D Code39 |
| REQ-05-039 | 5 | MUST | M4 | auto | todo | Kod 1D Code93 |
| REQ-05-040 | 5 | MUST | M4 | auto | todo | Kod 1D EAN-8 |
| REQ-05-041 | 5 | MUST | M4 | auto | todo | Kod 1D EAN-13 |
| REQ-05-042 | 5 | MUST | M4 | auto | todo | Kod 1D UPC-A |
| REQ-05-043 | 5 | MUST | M4 | auto | todo | Kod 1D UPC-E |
| REQ-05-044 | 5 | MUST | M4 | auto | todo | Kod 1D ITF |
| REQ-05-045 | 5 | MUST | M4 | auto | todo | Kod 1D Codabar |
| REQ-05-046 | 5 | MUST | M4 | auto | todo | Kod 1D GS1-128 |
| REQ-05-047 | 5 | MUST | M4 | auto | todo | Kod 2D QR z wyborem korekcji bledow |
| REQ-05-048 | 5 | MUST | M4 | auto | todo | Kod 2D Data Matrix |
| REQ-05-049 | 5 | MUST | M4 | auto | todo | Kod 2D GS1 DataMatrix |
| REQ-05-050 | 5 | MUST | M4 | auto | todo | Kod 2D PDF417 |
| REQ-05-051 | 5 | MUST | M4 | auto | todo | Kod 2D Aztec |
| REQ-05-052 | 5 | MUST | M4 | auto | todo | Kod 2D MaxiCode |
| REQ-05-053 | 5 | MUST | M4 | auto | todo | Znaczniki AprilTag |
| REQ-05-054 | 5 | MUST | M4 | auto | todo | Znaczniki ArUco |
| REQ-05-055 | 5 | MUST | M4 | auto | todo | Kody: parametr rozmiaru modulu |
| REQ-05-056 | 5 | MUST | M4 | auto | todo | Kody: cicha strefa |
| REQ-05-057 | 5 | MUST | M4 | auto | todo | Kody: inwersja (anodowane aluminium) |
| REQ-05-058 | 5 | MUST | M4 | auto | todo | Kody: kompensacja wypalenia |

#### Sekcja 6

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-06-001 | 6 | MUST | M4 | auto | todo | Tryb obrazu Threshold |
| REQ-06-002 | 6 | MUST | M4 | auto | todo | Tryb obrazu Ordered |
| REQ-06-003 | 6 | MUST | M4 | auto | todo | Tryb obrazu Atkinson |
| REQ-06-004 | 6 | MUST | M4 | auto | todo | Tryb obrazu Floyd-Steinberg („Dither”) |
| REQ-06-005 | 6 | MUST | M4 | auto | todo | Tryb obrazu Stucki |
| REQ-06-006 | 6 | MUST | M4 | auto | todo | Tryb obrazu Jarvis |
| REQ-06-007 | 6 | MUST | M4 | auto | todo | Tryb obrazu Sierra |
| REQ-06-008 | 6 | MUST | M4 | auto | todo | Tryb obrazu Burkes |
| REQ-06-009 | 6 | MUST | M4 | auto | todo | Tryb obrazu Newsprint |
| REQ-06-010 | 6 | MUST | M4 | auto | todo | Tryb obrazu Halftone (kat, LPI, ksztalt kropki) |
| REQ-06-011 | 6 | MUST | M4 | auto | todo | Tryb obrazu Sketch |
| REQ-06-012 | 6 | MUST | M4 | auto | todo | Tryb obrazu Grayscale (modulacja mocy) |
| REQ-06-013 | 6 | MUST | M4 | auto | todo | Tryb obrazu 3D Slice (ciecie warstwami glebi) |
| REQ-06-014 | 6 | MUST | M4 | auto | todo | Tryb obrazu Depth map 8-bit |
| REQ-06-015 | 6 | MUST | M4 | auto | todo | Tryb obrazu Depth map 16-bit (glebokie grawerowanie galvo) |
| REQ-06-016 | 6 | MUST | M4 | auto | todo | Tryb cieniowania CO2 (dither o zmiennej dlugosci impulsu) |
| REQ-06-017 | 6 | MUST | M4 | auto | todo | Dostosowanie: jasnosc |
| REQ-06-018 | 6 | MUST | M4 | auto | todo | Dostosowanie: kontrast |
| REQ-06-019 | 6 | MUST | M4 | auto | todo | Dostosowanie: gamma |
| REQ-06-020 | 6 | MUST | M4 | auto | todo | Dostosowanie: ostrzenie (radius/strength) |
| REQ-06-021 | 6 | MUST | M4 | auto | todo | Dostosowanie: wyrownanie histogramu |
| REQ-06-022 | 6 | MUST | M4 | auto | todo | Krzywa mocy w skali szarosci |
| REQ-06-023 | 6 | MUST | M4 | auto | todo | Zaawansowana krzywa tonalna [POSTULAT 17] |
| REQ-06-024 | 6 | MUST | M4 | auto | todo | Progi bieli i czerni [POSTULAT 18] |
| REQ-06-025 | 6 | MUST | M4 | auto | todo | Negatyw |
| REQ-06-026 | 6 | MUST | M4 | auto | todo | Lustro obrazu |
| REQ-06-027 | 6 | MUST | M4 | auto | todo | Przepuszczanie przez ustawiona rozdzielczosc DPI |
| REQ-06-028 | 6 | MUST | M4 | auto | todo | Tryb pass-through (bez przetwarzania) |
| REQ-06-029 | 6 | MUST | M4 | auto | todo | Przycinanie obrazu |
| REQ-06-030 | 6 | MUST | M4 | auto | todo | Maska wektorowa obrazu |
| REQ-06-031 | 6 | MUST | M4 | auto | todo | Usuwanie tla lokalnym modelem segmentacji offline [POSTULAT 30] ⚠ konflikt priorytetu |
| REQ-06-032 | 6 | MUST | M4 | auto | todo | Kalibracja rozmiaru obrazu po dwoch punktach [POSTULAT 35] |
| REQ-06-033 | 6 | MUST | M4 | auto | todo | Trasowanie: prog |
| REQ-06-034 | 6 | MUST | M4 | auto | todo | Trasowanie: odciecie |
| REQ-06-035 | 6 | MUST | M4 | auto | todo | Trasowanie: wygladzanie |
| REQ-06-036 | 6 | MUST | M4 | auto | todo | Trasowanie: optymalizacja |
| REQ-06-037 | 6 | MUST | M4 | auto | todo | Trasowanie: ignorowanie malych obiektow |
| REQ-06-038 | 6 | MUST | M4 | auto | todo | Trasowanie szkicu / linii centralnej (centerline) |
| REQ-06-039 | 6 | MUST | M4 | auto | todo | Trasowanie wielu plikow wsadowo |
| REQ-06-040 | 6 | MUST | M4 | auto | todo | Konwersja zaznaczenia do bitmapy |
| REQ-06-041 | 6 | MUST | M4 | auto | todo | Zapis przetworzonej bitmapy |
| REQ-06-042 | 6 | MUST | M4 | auto | todo | Podglad dithera 1:1 z uwzglednieniem DPI i srednicy plamki |

#### Sekcja 7

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-07-001 | 7.1 | MUST | M2 | auto | verified | Tryb warstwy Line (kontur) |
| REQ-07-002 | 7.1 | MUST | M2 | auto | verified | Tryb warstwy Fill (skanowanie) |
| REQ-07-003 | 7.1 | MUST | M2 | auto | verified | Fill: wszystkie ksztalty razem |
| REQ-07-004 | 7.1 | MUST | M2 | auto | verified | Fill: grupy razem |
| REQ-07-005 | 7.1 | MUST | M2 | auto | verified | Fill: kazdy ksztalt osobno |
| REQ-07-006 | 7.1 | MUST | M2 | auto | verified | Tryb warstwy Offset Fill (koncentryczny) |
| REQ-07-007 | 7.1 | MUST | M4 | auto | todo | Tryb warstwy Image |
| REQ-07-008 | 7.1 | MUST | M2 | auto | verified | Tryb warstwy Fill+Line |
| REQ-07-009 | 7.1 | MUST | M2 | auto | verified | Tryb warstwy Flood Fill (obszarami) |
| REQ-07-010 | 7.2 | MUST | M2 | auto | verified | Parametr warstwy: nazwa |
| REQ-07-011 | 7.2 | MUST | M2 | auto | verified | Parametr warstwy: Output wl./wyl. |
| REQ-07-012 | 7.2 | MUST | M2 | auto | verified | Parametr warstwy: Show |
| REQ-07-013 | 7.2 | MUST | M2 | auto | verified | Parametr warstwy: Air assist |
| REQ-07-014 | 7.2 | MUST | M2 | auto | verified | Parametr warstwy: predkosc |
| REQ-07-015 | 7.2 | MUST | M2 | auto | verified | Parametr warstwy: moc maksymalna |
| REQ-07-016 | 7.2 | MUST | M2 | auto | verified | Parametr warstwy: moc minimalna |
| REQ-07-017 | 7.2 | MUST | M2 | auto | verified | Osobne moce dla 2 laserow |
| REQ-07-018 | 7.2 | MUST | M2 | auto | verified | Liczba przejsc |
| REQ-07-019 | 7.2 | MUST | M2 | auto | verified | Opoznienie miedzy przejsciami [POSTULAT 77] |
| REQ-07-020 | 7.2 | MUST | M2 | auto | verified | Pauza miedzy warstwami z komunikatem (np. „zmien material”) [POSTULAT 22] |
| REQ-07-021 | 7.2 | MUST | M2 | auto | verified | Z offset |
| REQ-07-022 | 7.2 | MUST | M2 | auto | verified | Z step na przejscie |
| REQ-07-023 | 7.2 | MUST | M2 | auto | verified | Priorytet warstwy |
| REQ-07-024 | 7.2 | MUST | M2 | auto | verified | Czestotliwosc PWM (gdzie wspierane) |
| REQ-07-025 | 7.2 | MUST | M2 | auto | verified | Tryb stalej mocy (GRBL M3) vs dynamicznej (M4) |
| REQ-07-026 | 7.2 | MUST | M2 | auto | verified | Kerf offset |
| REQ-07-027 | 7.2 | MUST | M2 | auto | verified | Osobny kerf X i Y [POSTULAT 80] |
| REQ-07-028 | 7.2 | MUST | M2 | auto | verified | Kerf dla Fill [POSTULAT 28] |
| REQ-07-029 | 7.2 | MUST | M2 | auto | verified | Wizualizacja kerfu w edytorze [POSTULAT 22] |
| REQ-07-030 | 7.2 | MUST | M2 | auto | verified | Wizualizacja kerfu w podgladzie [POSTULAT 20] |
| REQ-07-031 | 7.2 | MUST | M2 | auto | verified | Mostki: rozmiar |
| REQ-07-032 | 7.2 | MUST | M2 | auto | verified | Mostki: liczba |
| REQ-07-033 | 7.2 | MUST | M2 | auto | verified | Mostki: odstep |
| REQ-07-034 | 7.2 | MUST | M2 | auto | verified | Mostki: pomijanie ksztaltow wewnetrznych |
| REQ-07-035 | 7.2 | MUST | M2 | auto | verified | Lead-in/lead-out: linia |
| REQ-07-036 | 7.2 | MUST | M2 | auto | verified | Lead-in/lead-out: luk |
| REQ-07-037 | 7.2 | MUST | M2 | auto | verified | Lead-in/lead-out: dlugosc i kat |
| REQ-07-038 | 7.2 | MUST | M2 | auto | verified | Overcut zamknietych ksztaltow |
| REQ-07-039 | 7.2 | MUST | M2 | auto | verified | Tryb perforacji (ciecie/przerwa) |
| REQ-07-040 | 7.2 | MUST | M2 | auto | verified | Dot mode (czas, odstep) |
| REQ-07-041 | 7.2 | MUST | M2 | auto | verified | Ramp: dlugosc rampy mocy, opcja tylko na zewnatrz |
| REQ-07-042 | 7.2 | MUST | M2 | auto | verified | Rampa mocy dla naroznikow (min moc w zwolnieniach) |
| REQ-07-043 | 7.2 | MUST | M2 | auto | verified | Skanowanie dwukierunkowe |
| REQ-07-044 | 7.2 | MUST | M2 | auto | verified | Kat skanowania |
| REQ-07-045 | 7.2 | MUST | M2 | auto | verified | Cross-hatch |
| REQ-07-046 | 7.2 | MUST | M2 | auto | verified | Interwal / LPI / DPI |
| REQ-07-047 | 7.2 | MUST | M2 | auto | verified | Overscanning automatyczny |
| REQ-07-048 | 7.2 | MUST | M2 | auto | verified | Overscanning reczny % |
| REQ-07-049 | 7.2 | MUST | M2 | auto | verified | Kompensacja scan offset zalezna od predkosci (tabela) |
| REQ-07-050 | 7.2 | MUST | M2 | auto | verified | Sub-layers (wiele przebiegow o roznych ustawieniach w jednej warstwie) |
| REQ-07-051 | 7.2 | MUST | M2 | auto | verified | Opcja globalna: grawer najpierw, wewnetrzne przed zewnetrznymi, ciecie zawsze na koncu [POSTULAT 81] |
| REQ-07-052 | 7.2 | MUST | M2 | auto | verified | Ostrzezenie, gdy warstwa Line/Cut wypada przed Fill [POSTULAT 24] |
| REQ-07-053 | 7.3 | MUST | M5 | emu | todo | Galvo: czestotliwosc Q |
| REQ-07-054 | 7.3 | MUST | M5 | emu | todo | Galvo: szerokosc impulsu (MOPA) |
| REQ-07-055 | 7.3 | MUST | M5 | emu | todo | Galvo: moc |
| REQ-07-056 | 7.3 | MUST | M5 | emu | todo | Galvo: predkosc |
| REQ-07-057 | 7.3 | MUST | M5 | emu | todo | Galvo: liczba petli |
| REQ-07-058 | 7.3 | MUST | M5 | emu | todo | Galvo: do 8 hatchy na warstwe |
| REQ-07-059 | 7.3 | MUST | M5 | emu | todo | Hatch: kat |
| REQ-07-060 | 7.3 | MUST | M5 | emu | todo | Hatch: interwal |
| REQ-07-061 | 7.3 | MUST | M5 | emu | todo | Hatch: przyrost kata per przejscie |
| REQ-07-062 | 7.3 | MUST | M5 | emu | todo | Hatch: tryb jednokierunkowy |
| REQ-07-063 | 7.3 | MUST | M5 | emu | todo | Hatch: tryb dwukierunkowy |
| REQ-07-064 | 7.3 | MUST | M5 | emu | todo | Hatch: tryb offsetowy |
| REQ-07-065 | 7.3 | MUST | M5 | emu | todo | Hatch: tryb ciagly „bow” |
| REQ-07-066 | 7.3 | MUST | M5 | emu | todo | Hatch: auto-obrot |
| REQ-07-067 | 7.3 | MUST | M5 | emu | todo | Hatch: kontur przed/po |
| REQ-07-068 | 7.3 | MUST | M5 | emu | todo | Wobble: srednica |
| REQ-07-069 | 7.3 | MUST | M5 | emu | todo | Wobble: odstep |
| REQ-07-070 | 7.3 | MUST | M5 | emu | todo | Wobble: ksztalt |
| REQ-07-071 | 7.3 | MUST | M5 | emu | todo | Opoznienie start |
| REQ-07-072 | 7.3 | MUST | M5 | emu | todo | Opoznienie stop |
| REQ-07-073 | 7.3 | MUST | M5 | emu | todo | Opoznienie polygon |
| REQ-07-074 | 7.3 | MUST | M5 | emu | todo | Opoznienie jump |
| REQ-07-075 | 7.3 | MUST | M5 | emu | todo | Opoznienie mark |
| REQ-07-076 | 7.3 | MUST | M5 | emu | todo | TC laser on/off |
| REQ-07-077 | 7.3 | MUST | M5 | emu | todo | Kompensacja konca |
| REQ-07-078 | 7.3 | MUST | M5 | emu | todo | Przyspieszenie |
| REQ-07-079 | 7.3 | MUST | M5 | emu | todo | Predkosc skoku |
| REQ-07-080 | 7.3 | MUST | M5 | emu | todo | Tryb Split |
| REQ-07-081 | 7.3 | MUST | M5 | emu | todo | Skok Z |
| REQ-07-082 | 7.3 | MUST | M5 | emu | todo | Glebokie grawerowanie z mapy glebi z automatycznym krokiem Z |
| REQ-07-083 | 7.4 | MUST | M4 | auto | todo | Biblioteka materialow: hierarchia material -> grubosc -> operacja |
| REQ-07-084 | 7.4 | MUST | M4 | auto | todo | Przypisanie wpisu do urzadzenia/typu zrodla |
| REQ-07-085 | 7.4 | MUST | M4 | auto | todo | Notatki przy wpisie [POSTULAT 33] |
| REQ-07-086 | 7.4 | MUST | M4 | auto | todo | Zdjecia przy wpisie [POSTULAT 30] ⚠ konflikt priorytetu |
| REQ-07-087 | 7.4 | MUST | M4 | auto | todo | Ogniskowa obiektywu przy wpisie [POSTULAT 19] |
| REQ-07-088 | 7.4 | MUST | M4 | auto | todo | Wybor materialu bezposrednio w edytorze ustawien warstwy [POSTULAT 27] |
| REQ-07-089 | 7.4 | MUST | M4 | auto | todo | Laczenie bibliotek |
| REQ-07-090 | 7.4 | MUST | M4 | auto | todo | Eksport/import CSV |
| REQ-07-091 | 7.4 | MUST | M4 | auto | todo | Eksport/import JSON |
| REQ-07-092 | 7.4 | MUST | M4 | auto | todo | Import .clb |
| REQ-07-093 | 7.4 | MUST | M4 | auto | todo | Biblioteka zdalna/wspolna (Git/HTTP/folder sieciowy) dla wielu stanowisk |
| REQ-07-094 | 7.5 | MUST | M4 | auto | todo | Generator testu materialu: siatka moc x predkosc |
| REQ-07-095 | 7.5 | MUST | M4 | auto | todo | Generator testu: dowolne 2 parametry (interwal, czestotliwosc, przejscia, Q-pulse) |
| REQ-07-096 | 7.5 | MUST | M4 | auto | todo | Generator testu: etykiety |
| REQ-07-097 | 7.5 | MUST | M4 | auto | todo | Generator testu: rozmiar do 400 mm |
| REQ-07-098 | 7.5 | MUST | M4 | auto | todo | Generator testu: wlasne ksztalty/tekst/obraz jako probka [POSTULAT 35] |
| REQ-07-099 | 7.5 | MUST | M4 | emu | todo | Generator testu: wysylka bezposrednio do lasera |
| REQ-07-100 | 7.5 | MUST | M4 | auto | todo | Test ogniskowej (rampa Z lub pochylona linia) |
| REQ-07-101 | 7.5 | MUST | M4 | auto | todo | Test interwalu |
| REQ-07-102 | 7.5 | MUST | M4 | auto | todo | Test scan offset |
| REQ-07-103 | 7.5 | MUST | M4 | auto | todo | Test kerfu z kreatorem obliczania kerfu [POSTULAT 17] ⚠ konflikt priorytetu |
| REQ-07-104 | 7.5 | MUST | M5 | auto | todo | Test korekcji pola galvo |

#### Sekcja 8

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-08-001 | 8 | MUST | M2 | auto | verified | Kolejnosc ciecia wg warstwy |
| REQ-08-002 | 8 | MUST | M2 | auto | verified | Kolejnosc wg priorytetu |
| REQ-08-003 | 8 | MUST | M2 | auto | verified | Kolejnosc wg grupy |
| REQ-08-004 | 8 | MUST | M2 | auto | verified | Wewnetrzne przed zewnetrznymi |
| REQ-08-005 | 8 | MUST | M2 | auto | verified | Wybor najlepszego punktu startowego |
| REQ-08-006 | 8 | MUST | M2 | auto | verified | Wybor kierunku ciecia |
| REQ-08-007 | 8 | MUST | M2 | auto | verified | Minimalizacja przejazdow (heurystyka + 2-opt) |
| REQ-08-008 | 8 | MUST | M2 | auto | verified | Unikanie przejazdow nad wycietymi czesciami (podnoszenie trajektorii lub objazd) |
| REQ-08-009 | 8 | MUST | M2 | auto | verified | Strefy zakazane dla przejazdow [POSTULAT 56] ⚠ konflikt priorytetu |
| REQ-08-010 | 8 | MUST | M2 | auto | verified | Strefy zakazane dla wiazki ⚠ konflikt priorytetu |
| REQ-08-011 | 8 | MUST | M2 | auto | verified | Usuwanie nakladajacych sie linii przy planowaniu |
| REQ-08-012 | 8 | MUST | M2 | auto | verified | Redukcja zmian kierunku |
| REQ-08-013 | 8 | MUST | M2 | auto | verified | Ciecie tylko zaznaczonych |
| REQ-08-014 | 8 | MUST | M2 | auto | verified | Uzycie poczatku zaznaczenia jako origin |
| REQ-08-015 | 8 | MUST | M2 | auto | verified | Rastry: dzielenie na pasy (kolejnosc rzedu skanowania) |
| REQ-08-016 | 8 | MUST | M6 | auto | todo | Moc zalezna od polozenia X/Y — mapa kompensacji [POSTULAT 37] ⚠ konflikt priorytetu |
| REQ-08-017 | 8 | MUST | M6 | auto | todo | Moc zalezna od X/Y jako efekt artystyczny ⚠ konflikt priorytetu |
| REQ-08-018 | 8 | MUST | M2 | auto | verified | Rozne maksymalne predkosci/przyspieszenia osi X i Y w modelu czasu i rastrowaniu [POSTULAT 32] |
| REQ-08-019 | 8 | MUST | M2 | auto | verified | Wielowatkowy, przyrostowy planer: przeliczenie podgladu < 1 s dla typowych projektow |

#### Sekcja 9

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-09-001 | 9 | MUST | M2 | auto | verified | Podglad z odtwarzaniem (suwak, predkosc) |
| REQ-09-002 | 9 | MUST | M2 | auto | verified | Podglad: przejazdy |
| REQ-09-003 | 9 | MUST | M2 | auto | verified | Podglad: kolejnosc |
| REQ-09-004 | 9 | MUST | M2 | auto | verified | Podglad: kerf |
| REQ-09-005 | 9 | MUST | M2 | auto | verified | Podglad: kierunek |
| REQ-09-006 | 9 | MUST | M2 | auto | verified | Podglad: punkty startowe |
| REQ-09-007 | 9 | MUST | M4 | auto | todo | Podglad rastrow z przyblizona symulacja wypalenia (model plamki i ciepla) |
| REQ-09-008 | 9 | MUST | M2 | auto | verified | Estymacja czasu z modelem przyspieszen/jerku z ustawien sterownika (GRBL $120/$121, Ruida accel) |
| REQ-09-009 | 9 | MUST | M6 | auto | todo | Samokalibracja estymacji czasu na podstawie historii zadan |
| REQ-09-010 | 9 | MUST | M6 | auto | todo | Kalkulator kosztow: material (m2/arkusz), czas maszyny, energia, marza -> cena [POSTULAT 57] ⚠ konflikt priorytetu |
| REQ-09-011 | 9 | MUST | M6 | auto | todo | Koszt widoczny w podgladzie ⚠ konflikt priorytetu |
| REQ-09-012 | 9 | MUST | M6 | auto | todo | Eksport oferty PDF z kosztem ⚠ konflikt priorytetu |
| REQ-09-013 | 9 | MUST | M2 | auto | verified | „Rozpocznij od tego miejsca” z podgladu |
| REQ-09-014 | 9 | MUST | M6 | auto | todo | Reczne zaznaczanie obszarow do ponownego wyciecia (recut) w podgladzie [POSTULAT 258] |
| REQ-09-015 | 9 | MUST | M6 | auto | todo | Recut: zaznaczanie na obrazie z kamery |
| REQ-09-016 | 9 | MUST | M6 | emu | todo | Wznawianie zadania po awarii od ostatniego potwierdzonego bloku [POSTULAT 17] ⚠ konflikt priorytetu |
| REQ-09-017 | 9 | MUST | M6 | emu | todo | Wznawianie G-code od numeru linii z bezpiecznym dojazdem ⚠ konflikt priorytetu |

#### Sekcja 10

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-10-001 | 10 | MUST | M2 | emu | verified | Start From: Absolute Coords |
| REQ-10-002 | 10 | MUST | M2 | emu | verified | Start From: Current Position |
| REQ-10-003 | 10 | MUST | M2 | emu | verified | Start From: User Origin |
| REQ-10-004 | 10 | MUST | M2 | emu | verified | 9-punktowy Job Origin |
| REQ-10-005 | 10 | MUST | M5 | emu | todo | Job Origin srodek-srodek dla galvo [POSTULAT 271] |
| REQ-10-006 | 10 | MUST | M2 | auto | verified | Zapamietanie ustawien startu w pliku |
| REQ-10-007 | 10 | MUST | M2 | emu | verified | Ramka prostokatna |
| REQ-10-008 | 10 | MUST | M2 | emu | verified | Ramka konturowa (rubber band) |
| REQ-10-009 | 10 | MUST | M5 | emu | todo | Ramka ciagla galvo (powtarzana do zatrzymania) |
| REQ-10-010 | 10 | MUST | M2 | emu | verified | Ramka z czerwonym laserem z korekta offsetu |
| REQ-10-011 | 10 | MUST | M5 | emu | todo | Naprzemienne powtarzanie obu trybow ramki [POSTULAT 26] |
| REQ-10-012 | 10 | MUST | M2 | emu | verified | Ustawienie punktu zerowego uzytkownika |
| REQ-10-013 | 10 | MUST | M2 | emu | verified | Idz do zera |
| REQ-10-014 | 10 | MUST | M2 | emu | verified | Home |
| REQ-10-015 | 10 | MUST | M2 | emu | verified | Przesun laser do zaznaczenia |
| REQ-10-016 | 10 | MUST | M2 | emu | verified | Ustaw pozycje lasera klikajac w obszarze |
| REQ-10-017 | 10 | MUST | M3 | emu | todo | Wyslij do pamieci sterownika (Ruida/Trocen) |
| REQ-10-018 | 10 | MUST | M2 | emu | verified | Zapisz na karte SD |
| REQ-10-019 | 10 | MUST | M2 | auto | verified | Zapisz plik G-code |
| REQ-10-020 | 10 | MUST | M2 | emu | verified | Kolejka zadan (wiele zadan w sekwencji) |
| REQ-10-021 | 10 | MUST | M2 | emu | verified | Powtarzanie zadania N razy z opoznieniem |
| REQ-10-022 | 10 | MUST | M6 | auto | todo | Historia zadan (czas, ustawienia, material, miniatura, zuzycie, wynik) z ponownym uruchomieniem [POSTULAT 71] |
| REQ-10-023 | 10 | MUST | M6 | auto | todo | Liczniki eksploatacji (godziny tuby/diody) [POSTULAT 56] |
| REQ-10-024 | 10 | MUST | M6 | auto | todo | Przypomnienia serwisowe (czyszczenie optyki, smarowanie) |

#### Sekcja 11

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-11-001 | 11 | MUST | M5 | emu | todo | Rotary: walki (roller) |
| REQ-11-002 | 11 | MUST | M5 | emu | todo | Rotary: uchwyt (chuck) |
| REQ-11-003 | 11 | MUST | M5 | emu | todo | Rotary: konfiguracja osi (Y lub A) |
| REQ-11-004 | 11 | MUST | M5 | emu | todo | Rotary: mm na obrot |
| REQ-11-005 | 11 | MUST | M5 | emu | todo | Rotary: srednica obiektu/walka |
| REQ-11-006 | 11 | MUST | M5 | auto | todo | Rotary: zapisane konfiguracje dla produktow [POSTULAT 24] |
| REQ-11-007 | 11 | MUST | M5 | emu | todo | Rotary: dzielenie na sekcje |
| REQ-11-008 | 11 | MUST | M5 | emu | todo | Rotary: rotacja tylko na przejazdach |
| REQ-11-009 | 11 | MUST | M5 | emu | todo | Galvo rotary marking z podzialem na plastry |
| REQ-11-010 | 11 | MUST | M5 | emu | todo | Galvo rotary: wizualnie edytowalny srodek wyjsciowy |
| REQ-11-011 | 11 | MUST | M5 | auto | todo | Zmienna srednicy w custom G-code |
| REQ-11-012 | 11 | MUST | M5 | emu | todo | Print & Cut: rejestracja dwoma znacznikami |
| REQ-11-013 | 11 | MUST | M5 | emu | todo | Print & Cut: 3/4 znaczniki z korekcja skosu i skali [POSTULAT 17] ⚠ konflikt priorytetu |
| REQ-11-014 | 11 | MUST | M5 | emu | todo | Print & Cut: rejestracja kamera |
| REQ-11-015 | 11 | MUST | M5 | emu | todo | Print & Cut: rejestracja czerwonym punktem |
| REQ-11-016 | 11 | MUST | M5 | emu | todo | Repeat marking (galvo) |
| REQ-11-017 | 11 | MUST | M5 | emu | todo | Split marking z przesuwem liniowym/obrotowym (galvo) |
| REQ-11-018 | 11 | MUST | M5 | emu | todo | Feeder (podajnik/przenosnik — sekcje z przesunieciem materialu) |
| REQ-11-019 | 11 | MUST | M5 | emu | todo | Center finder |
| REQ-11-020 | 11 | MUST | M5 | emu | todo | Cylinder correction (galvo na walcu bez rotary) |
| REQ-11-021 | 11 | MUST | M5 | emu | todo | Taper warp (stozki) |
| REQ-11-022 | 11 | MUST | M5 | emu | todo | Galvo lens calibration: korekcja pola 9/25/49-punktowa |
| REQ-11-023 | 11 | MUST | M5 | auto | todo | Import plikow korekcji .cor |
| REQ-11-024 | 11 | MUST | M5 | emu | todo | Dual laser: dwie glowice/zrodla z offsetem i nazwami zrodel |
| REQ-11-025 | 11 | MUST | M6 | manual | todo | Dual laser dostepny we wszystkich edycjach (brak edycji platnych) [POSTULAT 45] |
| REQ-11-026 | 11 | MUST | M6 | manual | todo | Projekcja (projektor nad stolem jako podglad) |
| REQ-11-027 | 11 | MUST | M6 | auto | todo | Edytowalna checklista bezpieczenstwa przed startem |
| REQ-11-028 | 11 | MUST | M2 | emu | verified | Sonda Z: GRBL G38 |
| REQ-11-029 | 11 | MUST | M3 | emu | todo | Autofokus Ruida |
| REQ-11-030 | 11 | MUST | M6 | emu | todo | Czujniki wysokosci xTool/Thunder |
| REQ-11-031 | 11 | MUST | M5 | emu | todo | Mapowanie wysokosci powierzchni (mesh) z korekta Z w zadaniu |
| REQ-11-032 | 11 | SHOULD | M7 | auto | todo | Abstrakcja „Tool” w urzadzeniu |
| REQ-11-033 | 11 | SHOULD | M7 | emu | todo | Offset noza ciagnionego (drag knife) [POSTULAT 49] |
| REQ-11-034 | 11 | SHOULD | M7 | emu | todo | Narzedzie styczne |
| REQ-11-035 | 11 | SHOULD | M7 | emu | todo | Podnoszenie Z / M-kody narzedzia (tool release) [POSTULAT 30] |
| REQ-11-036 | 11 | SHOULD | M7 | emu | todo | Wyjscie HPGL dla ploterow winylowych [POSTULAT 38] |
| REQ-11-037 | 11 | SHOULD | M7 | emu | todo | Tryb plazmy [POSTULAT 18] |
| REQ-11-038 | 11 | SHOULD | M7 | emu | todo | Tryb lekkiego frezu |

#### Sekcja 12

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-12-001 | 12 | MUST | M5 | manual | todo | Kamery USB (UVC) |
| REQ-12-002 | 12 | MUST | M5 | manual | todo | Kamery sieciowe RTSP |
| REQ-12-003 | 12 | MUST | M5 | manual | todo | Kamery sieciowe MJPEG |
| REQ-12-004 | 12 | MUST | M5 | manual | todo | Kamery sieciowe HTTP snapshot |
| REQ-12-005 | 12 | MUST | M5 | manual | todo | Kamera wbudowana xTool (jesli API publiczne/dokumentowalne) |
| REQ-12-006 | 12 | MUST | M5 | manual | todo | Kamera wbudowana Thunder |
| REQ-12-007 | 12 | MUST | M5 | manual | todo | Kamera wbudowana WeCreat |
| REQ-12-008 | 12 | MUST | M5 | manual | todo | Kamera wbudowana Gweike |
| REQ-12-009 | 12 | MUST | M5 | manual | todo | Kamera wbudowana Omtech |
| REQ-12-010 | 12 | MUST | M6 | manual | todo | Kamery USB podlaczone do Bridge [POSTULAT 19] |
| REQ-12-011 | 12 | MUST | M5 | manual | todo | Dwie i wiecej kamer: widok mozaiki i zszywanie |
| REQ-12-012 | 12 | MUST | M5 | auto | todo | Kalibracja obiektywu: szachownica |
| REQ-12-013 | 12 | MUST | M5 | auto | todo | Kalibracja obiektywu: kropki |
| REQ-12-014 | 12 | MUST | M5 | auto | todo | Kalibracja obiektywu: AprilTag z wynikiem jakosci |
| REQ-12-015 | 12 | MUST | M5 | manual | todo | Wyrownanie kamery do obszaru lasera: wzorzec wypalony + klikniecia |
| REQ-12-016 | 12 | MUST | M5 | auto | todo | Wyrownanie kamery automatycznie przez znaczniki |
| REQ-12-017 | 12 | MUST | M5 | manual | todo | Kamera na glowicy: skanowanie i skladanie obrazu calego stolu |
| REQ-12-018 | 12 | MUST | M5 | manual | todo | Ustawienia ekspozycji, balansu bieli i ostrosci |
| REQ-12-019 | 12 | MUST | M5 | manual | todo | Presety kamery |
| REQ-12-020 | 12 | MUST | M5 | manual | todo | Zapis tla |
| REQ-12-021 | 12 | MUST | M5 | manual | todo | Rozmycie tla |
| REQ-12-022 | 12 | MUST | M5 | manual | todo | Przechwycenie surowe i zrektyfikowane |
| REQ-12-023 | 12 | MUST | M5 | manual | todo | Aktualizacja nakladki |
| REQ-12-024 | 12 | MUST | M5 | manual | todo | Sledzenie pozycji materialu (krawedzie, obrot) z automatycznym dopasowaniem projektu |
| REQ-12-025 | 12 | MUST | M5 | auto | todo | Rozpoznawanie znacznikow do print&cut |
| REQ-12-026 | 12 | MUST | M5 | manual | todo | Wykrywanie wczesniej wycietych obszarow (resztki) |

#### Sekcja 13

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-13-001 | 13.1 | MUST | M2 | emu | verified | Trait Driver: capabilities() (osie, rotary, Z, raster natywny, pamiec plikow, air, 2 lasery, kamera, sonda, czujnik pokrywy) |
| REQ-13-002 | 13.1 | MUST | M2 | emu | verified | Driver: connect(transport) / disconnect() |
| REQ-13-003 | 13.1 | MUST | M2 | emu | verified | Driver: status() — pozycja, stan idle/run/hold/alarm, pokrywa, chlodzenie, temperatura, bledy |
| REQ-13-004 | 13.1 | MUST | M2 | emu | verified | Driver: home() |
| REQ-13-005 | 13.1 | MUST | M2 | emu | verified | Driver: jog(axis, dist, speed) |
| REQ-13-006 | 13.1 | MUST | M2 | emu | verified | Driver: move_to() |
| REQ-13-007 | 13.1 | MUST | M2 | emu | verified | Driver: set_origin() |
| REQ-13-008 | 13.1 | MUST | M2 | emu | verified | Driver: frame(kind) |
| REQ-13-009 | 13.1 | MUST | M2 | emu | verified | Driver: test_fire(power, ms) z safety tier „fire” |
| REQ-13-010 | 13.1 | MUST | M2 | emu | verified | Driver: send_job(Job) — streaming lub upload pliku |
| REQ-13-011 | 13.1 | MUST | M2 | emu | verified | Driver: pause() / resume() |
| REQ-13-012 | 13.1 | MUST | M2 | emu | verified | Driver: stop() — zawsze dozwolony, najwyzszy priorytet |
| REQ-13-013 | 13.1 | MUST | M2 | emu | verified | Driver: read_settings() / write_settings() z backupem przed zapisem |
| REQ-13-014 | 13.1 | MUST | M3 | emu | todo | Driver: list_files() / run_file() / delete_file() gdy wspierane |
| REQ-13-015 | 13.1 | MUST | M2 | auto | verified | Driver: compile(Job) — offline eksport pliku dla sterownika |
| REQ-13-016 | 13.1 | MUST | M2 | auto | verified | Kompilator zadan niezalezny od transportu |
| REQ-13-017 | 13.1 | MUST | M2 | emu | verified | Kazdy sterownik ma emulator/symulator do testow |
| REQ-13-018 | 13.2 | MUST | M2 | auto | verified | Transport Serial/USB-CDC (FTDI, CH340, CP210x) |
| REQ-13-019 | 13.2 | MUST | M3 | auto | verified | Transport USB bulk przez libusb/WinUSB |
| REQ-13-020 | 13.2 | MUST | M3 | manual | verified | Kreator instalacji sterownika USB na Windows |
| REQ-13-021 | 13.2 | MUST | M3 | manual | verified | Reguly udev na Linuksie |
| REQ-13-022 | 13.2 | MUST | M2 | auto | verified | Transport TCP |
| REQ-13-023 | 13.2 | MUST | M3 | auto | verified | Transport UDP |
| REQ-13-024 | 13.2 | MUST | M2 | auto | verified | Transport WebSocket |
| REQ-13-025 | 13.2 | MUST | M6 | auto | todo | Transport HTTP/REST |
| REQ-13-026 | 13.2 | MUST | M6 | auto | todo | Wykrywanie Wi-Fi (mDNS/UDP discovery) |
| REQ-13-027 | 13.2 | MUST | M6 | hw | todo | BurnCraft Bridge: RPi laczacy USB/serial/UDP z LAN-em, z obsluga kamer |
| REQ-13-028 | 13.2 | MUST | M2 | auto | verified | Wieloinstancyjnosc: kilka laserow naraz z jednego komputera |
| REQ-13-029 | 13.2 | MUST | M2 | auto | verified | Wybor urzadzenia z CLI --device „Nazwa” |
| REQ-13-030 | 13.3 | MUST | M3 | hw | todo | K40 Lihuiyu: plyta M2 Nano |
| REQ-13-031 | 13.3 | MUST | M3 | emu | todo | K40 Lihuiyu: plyta M3 Nano |
| REQ-13-032 | 13.3 | MUST | M3 | emu | todo | K40 Lihuiyu: plyta M1 |
| REQ-13-033 | 13.3 | MUST | M3 | emu | todo | K40 Lihuiyu: plyta M |
| REQ-13-034 | 13.3 | MUST | M3 | emu | todo | K40 Lihuiyu: plyta B1 |
| REQ-13-035 | 13.3 | MUST | M3 | emu | todo | K40 Lihuiyu: plyta B2 |
| REQ-13-036 | 13.3 | MUST | M3 | emu | todo | K40 Lihuiyu: plyta B |
| REQ-13-037 | 13.3 | MUST | M3 | emu | todo | K40 Lihuiyu: plyta A |
| REQ-13-038 | 13.3 | MUST | M3 | emu | todo | K40 Lihuiyu: plyta 6C6879-LASER-M2/M3 |
| REQ-13-039 | 13.3 | MUST | M3 | emu | todo | Lihuiyu: USB CH341 (1A86:5512) natywnie przez libusb |
| REQ-13-040 | 13.3 | MUST | M3 | hw | todo | Lihuiyu: alternatywnie przez oryginalny sterownik CH341 DLL (Windows) |
| REQ-13-041 | 13.3 | MUST | M3 | emu | todo | Lihuiyu: protokol EGV — pakiety 30 bajtow + CRC-8 (Onewire) |
| REQ-13-042 | 13.3 | MUST | M3 | emu | todo | Lihuiyu: obsluga potwierdzen stanu OK/BUSY/CRC error/task complete |
| REQ-13-043 | 13.3 | MUST | M3 | emu | todo | Lihuiyu: komendy kierunku/kroku z predkoscia kodowana zaleznie od wariantu plyty |
| REQ-13-044 | 13.3 | MUST | M3 | emu | todo | Lihuiyu: raster natywny w trybie step mode |
| REQ-13-045 | 13.3 | MUST | M3 | manual | todo | Lihuiyu: brak PWM — pole mocy nieaktywne, jasny komunikat w UI, zamiast mocy dither/predkosc |
| REQ-13-046 | 13.3 | MUST | M3 | emu | todo | Lihuiyu: konfiguracja wariantu plyty i ID plyty (serial) |
| REQ-13-047 | 13.3 | MUST | M3 | emu | todo | Lihuiyu: kroki/mm (1000 dpi) i korekcja skali X/Y |
| REQ-13-048 | 13.3 | MUST | M3 | emu | todo | Lihuiyu: home, rapid vs ruch powolny, pauza/wznowienie |
| REQ-13-049 | 13.3 | MUST | M3 | hw | todo | K40 niezmodyfikowany dziala „od pudelka” |
| REQ-13-050 | 13.3 | MUST | M2 | emu | verified | K40 po modernizacji (Cohesion3D Mini/LaserBoard, Awesome.tech, SKR + GRBL-LPC/Smoothie/grblHAL) przez rodzine G-code |
| REQ-13-051 | 13.3 | MUST | M3 | emu | todo | Moshi: Moshidraw MS10105 (V4.x) przez USB CH341 |
| REQ-13-052 | 13.3 | MUST | M3 | emu | todo | Moshi: protokol Moshi, cale zadanie wysylane blokami |
| REQ-13-053 | 13.3 | MUST | M2 | emu | verified | GRBL 0.9 |
| REQ-13-054 | 13.3 | MUST | M2 | emu | verified | GRBL 1.1f/1.1h |
| REQ-13-055 | 13.3 | MUST | M2 | emu | verified | grblHAL |
| REQ-13-056 | 13.3 | MUST | M2 | emu | verified | FluidNC |
| REQ-13-057 | 13.3 | MUST | M2 | emu | verified | GRBL-LPC |
| REQ-13-058 | 13.3 | MUST | M2 | emu | verified | GRBL-ESP32 |
| REQ-13-059 | 13.3 | MUST | M2 | emu | verified | GRBL-M3 (diody) |
| REQ-13-060 | 13.3 | MUST | M2 | emu | verified | Odmiany producentow GRBL: Ortur |
| REQ-13-061 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Atomstack |
| REQ-13-062 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Sculpfun |
| REQ-13-063 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Creality Falcon |
| REQ-13-064 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Two Trees |
| REQ-13-065 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: NEJE (G-code) |
| REQ-13-066 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Comgrow |
| REQ-13-067 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Longer |
| REQ-13-068 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Snapmaker GRBL |
| REQ-13-069 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Algolaser |
| REQ-13-070 | 13.3 | MUST | M2 | emu | verified | Odmiany GRBL: Wainlux |
| REQ-13-071 | 13.3 | MUST | M6 | emu | todo | Odmiany GRBL: LaserPecker (gdy G-code — do zbadania) |
| REQ-13-072 | 13.3 | MUST | M2 | emu | verified | GRBL: transport serial |
| REQ-13-073 | 13.3 | MUST | M2 | emu | verified | GRBL: TCP Telnet port 23 |
| REQ-13-074 | 13.3 | MUST | M2 | emu | verified | GRBL: WebSocket (FluidNC/ESP3D) |
| REQ-13-075 | 13.3 | MUST | M2 | emu | verified | GRBL: strumieniowanie character counting (bufor 127/RX) |
| REQ-13-076 | 13.3 | MUST | M2 | emu | verified | GRBL: tryb send-response |
| REQ-13-077 | 13.3 | MUST | M2 | emu | verified | GRBL: zapytania ? w tle |
| REQ-13-078 | 13.3 | MUST | M2 | emu | verified | GRBL: nadpisania czasu rzeczywistego (feed/power override) |
| REQ-13-079 | 13.3 | MUST | M2 | emu | verified | GRBL: jog $J= |
| REQ-13-080 | 13.3 | MUST | M2 | emu | verified | GRBL: $H |
| REQ-13-081 | 13.3 | MUST | M2 | emu | verified | GRBL: $X |
| REQ-13-082 | 13.3 | MUST | M2 | emu | verified | GRBL: $$ odczyt/zapis z kopia zapasowa |
| REQ-13-083 | 13.3 | MUST | M2 | emu | verified | GRBL: kody bledow/alarmow z opisami |
| REQ-13-084 | 13.3 | MUST | M2 | emu | verified | GRBL: kreator sprawdzajacy $32=1 (laser mode) i $30 (S-max) |
| REQ-13-085 | 13.3 | MUST | M2 | emu | verified | GRBL: M3/M4 |
| REQ-13-086 | 13.3 | MUST | M2 | auto | verified | GRBL: kompresja G-code (pomijanie powtorzen, skrocone wspolrzedne) |
| REQ-13-087 | 13.3 | MUST | M2 | emu | verified | GRBL: rastry z S per piksel |
| REQ-13-088 | 13.3 | MUST | M2 | emu | verified | GRBL: postep M73 |
| REQ-13-089 | 13.3 | MUST | M2 | emu | verified | GRBL: obsluga 2 laserow (M3/M4 + wybor narzedzia) |
| REQ-13-090 | 13.3 | MUST | M2 | emu | verified | GRBL: air assist M7/M8 |
| REQ-13-091 | 13.3 | MUST | M5 | emu | todo | GRBL: rotary os A lub Y |
| REQ-13-092 | 13.3 | MUST | M6 | emu | todo | Smoothieware: Smoothieboard, C3D przez Serial/USB i TCP |
| REQ-13-093 | 13.3 | MUST | M6 | emu | todo | Smoothieware: komenda fire, raster przez S |
| REQ-13-094 | 13.3 | MUST | M6 | emu | todo | Marlin 2.x z LASER_FEATURE / Inline laser |
| REQ-13-095 | 13.3 | MUST | M6 | emu | todo | Marlin: M3 I (inline), synchronizacja M400 |
| REQ-13-096 | 13.3 | MUST | M6 | emu | todo | RepRapFirmware/Duet 2/3 (M452) przez HTTP |
| REQ-13-097 | 13.3 | MUST | M6 | emu | todo | RepRapFirmware/Duet przez serial |
| REQ-13-098 | 13.3 | MUST | M6 | emu | todo | CNC G-code ogolny: Mach3 |
| REQ-13-099 | 13.3 | MUST | M6 | emu | todo | CNC G-code: Mach4 |
| REQ-13-100 | 13.3 | MUST | M6 | emu | todo | CNC G-code: UCCNC |
| REQ-13-101 | 13.3 | MUST | M6 | emu | todo | CNC G-code: Masso |
| REQ-13-102 | 13.3 | MUST | M6 | emu | todo | CNC G-code: LinuxCNC |
| REQ-13-103 | 13.3 | MUST | M6 | emu | todo | CNC G-code: Snapmaker |
| REQ-13-104 | 13.3 | MUST | M6 | emu | todo | CNC G-code: plik / serial |
| REQ-13-105 | 13.3 | MUST | M2 | auto | verified | Szablony naglowka/stopki G-code |
| REQ-13-106 | 13.3 | MUST | M2 | auto | verified | Edytowalny blok G20/G21 |
| REQ-13-107 | 13.3 | MUST | M2 | auto | verified | Makra pre/post per warstwa |
| REQ-13-108 | 13.3 | MUST | M2 | auto | verified | Makra pre/post per zadanie |
| REQ-13-109 | 13.3 | MUST | M6 | emu | todo | xTool D1 |
| REQ-13-110 | 13.3 | MUST | M6 | emu | todo | xTool D1 Pro |
| REQ-13-111 | 13.3 | MUST | M6 | emu | todo | xTool F1 |
| REQ-13-112 | 13.3 | MUST | M6 | emu | todo | xTool S1 |
| REQ-13-113 | 13.3 | MUST | M6 | emu | todo | xTool M1 |
| REQ-13-114 | 13.3 | MUST | M6 | emu | todo | xTool P2 |
| REQ-13-115 | 13.3 | MUST | M6 | emu | todo | xTool F2 |
| REQ-13-116 | 13.3 | MUST | M6 | emu | todo | xTool: USB (G-code) |
| REQ-13-117 | 13.3 | MUST | M6 | emu | todo | xTool: Wi-Fi (HTTP API) [POSTULAT 124] |
| REQ-13-118 | 13.3 | MUST | M6 | emu | todo | xTool P2: kamera i czujnik wysokosci [POSTULAT 26] |
| REQ-13-119 | 13.3 | MUST | M3 | emu | todo | Ruida RDC6442G |
| REQ-13-120 | 13.3 | MUST | M3 | emu | todo | Ruida RDC6442S |
| REQ-13-121 | 13.3 | MUST | M7 | hw | todo | Ruida RDC6445G |
| REQ-13-122 | 13.3 | MUST | M3 | emu | todo | Ruida RDC6445GZ |
| REQ-13-123 | 13.3 | MUST | M3 | emu | todo | Ruida RDC6332G |
| REQ-13-124 | 13.3 | MUST | M3 | emu | todo | Ruida RDC6332M |
| REQ-13-125 | 13.3 | MUST | M3 | emu | todo | Ruida RDLC-320A |
| REQ-13-126 | 13.3 | MUST | M3 | emu | todo | Ruida RDC6422G |
| REQ-13-127 | 13.3 | MUST | M3 | emu | todo | Ruida R5-DSP |
| REQ-13-128 | 13.3 | MUST | M3 | emu | todo | Ruida RDV6442G |
| REQ-13-129 | 13.3 | MUST | M3 | emu | todo | Ruida KT332N (ARM) |
| REQ-13-130 | 13.3 | MUST | M3 | emu | todo | Ruida RD8445 |
| REQ-13-131 | 13.3 | MUST | M3 | emu | todo | Ruida Hydra/GT5 |
| REQ-13-132 | 13.3 | MUST | M3 | emu | todo | Ruida: UDP 50200->40200 (Ethernet) |
| REQ-13-133 | 13.3 | MUST | M3 | emu | todo | Ruida: USB-serial |
| REQ-13-134 | 13.3 | MUST | M3 | emu | todo | Ruida: plik .rd na pendrive |
| REQ-13-135 | 13.3 | MUST | M3 | emu | todo | Ruida: swizzling bajtow (magic 0x88 domyslnie, konfigurowalny per model) |
| REQ-13-136 | 13.3 | MUST | M3 | emu | todo | Ruida: suma kontrolna pakietow UDP |
| REQ-13-137 | 13.3 | MUST | M3 | emu | todo | Ruida: potwierdzenia ACK/NACK |
| REQ-13-138 | 13.3 | MUST | M3 | emu | todo | Ruida: wysylka calego zadania do pamieci |
| REQ-13-139 | 13.3 | MUST | M3 | emu | todo | Ruida: start/pauza/stop |
| REQ-13-140 | 13.3 | MUST | M3 | emu | todo | Ruida: ramka |
| REQ-13-141 | 13.3 | MUST | M3 | emu | todo | Ruida: odczyt/zapis parametrow maszyny (zakres, przyspieszenia, offset lasera 2, autofokus, rotary) |
| REQ-13-142 | 13.3 | MUST | M3 | emu | todo | Ruida: lista/uruchamianie/usuwanie plikow w pamieci |
| REQ-13-143 | 13.3 | MUST | M3 | emu | todo | Ruida: odczyt stanu |
| REQ-13-144 | 13.3 | MUST | M3 | emu | todo | Ruida: raster natywny ze scan offset |
| REQ-13-145 | 13.3 | MUST | M3 | emu | todo | Ruida KT332N: mapowanie osi U/Z (wylaczalne) |
| REQ-13-146 | 13.3 | MUST | M3 | emu | todo | Ruida: wrapper UDP zgodny z ZhiYuan |
| REQ-13-147 | 13.3 | MUST | M6 | emu | todo | Trocen AWC708C |
| REQ-13-148 | 13.3 | MUST | M6 | emu | todo | Trocen AWC708 LITE |
| REQ-13-149 | 13.3 | MUST | M6 | emu | todo | Trocen AWC608 |
| REQ-13-150 | 13.3 | MUST | M6 | emu | todo | Trocen AWC7813 |
| REQ-13-151 | 13.3 | MUST | M6 | emu | todo | Trocen TL-3120 (zgodnosc do zbadania) |
| REQ-13-152 | 13.3 | MUST | M6 | emu | todo | Trocen: Ethernet/USB, protokol i upload pliku (.ud? — do zbadania) |
| REQ-13-153 | 13.3 | MUST | M6 | emu | todo | TopWisdom TL-403 (do zbadania) |
| REQ-13-154 | 13.3 | MUST | M6 | emu | todo | TopWisdom TL-410 (do zbadania) |
| REQ-13-155 | 13.3 | SHOULD | M7 | emu | todo | Leetro MPC6515/6525/6535 (RE zgodnie z art. 75 ust. 2 pkt 3 pr. aut. / dyr. 2009/24/WE) [POSTULAT 46] |
| REQ-13-156 | 13.3 | MUST | M6 | emu | todo | LightObject X7 DSP ⚠ konflikt priorytetu |
| REQ-13-157 | 13.3 | MUST | M6 | emu | todo | LightObject T9 DSP [POSTULAT 20] ⚠ konflikt priorytetu |
| REQ-13-158 | 13.3 | MUST | M6 | emu | todo | Gweike G2 (do zbadania) [POSTULAT 163] ⚠ konflikt priorytetu |
| REQ-13-159 | 13.3 | MUST | M6 | emu | todo | Gweike M3 Ultra (do zbadania) ⚠ konflikt priorytetu |
| REQ-13-160 | 13.3 | MUST | M5 | hw | todo | BJJCZ LMCV4-FIBER-M |
| REQ-13-161 | 13.3 | MUST | M5 | emu | todo | BJJCZ LMCV4-DIGIT |
| REQ-13-162 | 13.3 | MUST | M5 | emu | todo | BJJCZ LMC-CO2 |
| REQ-13-163 | 13.3 | MUST | M5 | emu | todo | BJJCZ UV |
| REQ-13-164 | 13.3 | MUST | M5 | emu | todo | BJJCZ MOPA (JPT/Raycus/MAX) |
| REQ-13-165 | 13.3 | MUST | M5 | emu | todo | BJJCZ: USB 9588:9899 |
| REQ-13-166 | 13.3 | MUST | M5 | emu | todo | BJJCZ: protokol listowy — bloki 0x0C00 polecen 12-bajtowych |
| REQ-13-167 | 13.3 | MUST | M5 | emu | todo | BJJCZ: ustawienia lasera Q, moc, MO, PRF, FPK |
| REQ-13-168 | 13.3 | MUST | M5 | emu | todo | BJJCZ: opoznienia |
| REQ-13-169 | 13.3 | MUST | M5 | emu | todo | BJJCZ: port I/O (pedal, czujniki) |
| REQ-13-170 | 13.3 | MUST | M5 | emu | todo | BJJCZ: os rotary/Z przez porty krokow |
| REQ-13-171 | 13.3 | MUST | M5 | emu | todo | BJJCZ: czerwona ramka |
| REQ-13-172 | 13.3 | MUST | M5 | emu | todo | BJJCZ: korekcja pola (.cor EZCAD) |
| REQ-13-173 | 13.3 | MUST | M5 | emu | todo | BJJCZ: sterowanie osia Z i auto-krok Z w zadaniu |
| REQ-13-174 | 13.3 | MUST | M6 | emu | todo | EZCAD3 DLC2-M4-2D: markowanie ⚠ konflikt priorytetu |
| REQ-13-175 | 13.3 | MUST | M6 | emu | todo | EZCAD3 DLC2-M4-3D: markowanie ⚠ konflikt priorytetu |
| REQ-13-176 | 13.3 | MUST | M6 | emu | todo | EZCAD3: rotary ⚠ konflikt priorytetu |
| REQ-13-177 | 13.3 | MUST | M6 | emu | todo | EZCAD3: split ⚠ konflikt priorytetu |
| REQ-13-178 | 13.3 | MUST | M6 | emu | todo | EZCAD3: Z ⚠ konflikt priorytetu |
| REQ-13-179 | 13.3 | SHOULD | M7 | emu | todo | Galvo BSL z kalibracja |
| REQ-13-180 | 13.3 | MUST | M6 | emu | todo | Emblaser 2 |
| REQ-13-181 | 13.3 | MUST | M6 | emu | todo | Emblaser 3 |
| REQ-13-182 | 13.3 | MUST | M6 | emu | todo | Emblaser Pro |
| REQ-13-183 | 13.3 | MUST | M6 | emu | todo | Cubiio 2 |
| REQ-13-184 | 13.3 | MUST | M3 | emu | todo | Thunder (Ruida) |
| REQ-13-185 | 13.3 | MUST | M6 | emu | todo | WeCreat (G-code/HTTP) |
| REQ-13-186 | 13.3 | MUST | M3 | emu | todo | Omtech (Ruida) |
| REQ-13-187 | 13.3 | MUST | M7 | manual | todo | Glowforge nieobslugiwany — wyjasnienie w FAQ |
| REQ-13-188 | 13.3 | LATER | LATER | emu | todo | Trotec/Epilog/ULS przez drukarke wirtualna/sterownik producenta [POSTULAT 22] |
| REQ-13-189 | 13.3 | MUST | M6 | emu | todo | Wirtualna drukarka BurnCraft: „drukuj” z dowolnego programu -> dokument (Windows) [POSTULAT 36] ⚠ konflikt priorytetu |
| REQ-13-190 | 13.3 | LATER | LATER | emu | todo | Wirtualna drukarka na wszystkich OS |
| REQ-13-191 | 13.3 | MUST | M2 | auto | verified | Urzadzenie fikcyjne/symulator: praca bez maszyny, pelny podglad i eksport |
| REQ-13-192 | 13.3 | MUST | M2 | emu | verified | Kreator „Znajdz moj laser”: skanowanie portow |
| REQ-13-193 | 13.3 | MUST | M3 | emu | verified | Kreator: USB VID/PID |
| REQ-13-194 | 13.3 | MUST | M3 | emu | todo | Kreator: UDP broadcast Ruida |
| REQ-13-195 | 13.3 | MUST | M6 | emu | todo | Kreator: mDNS |
| REQ-13-196 | 13.3 | MUST | M2 | auto | verified | Reczne tworzenie profilu urzadzenia |
| REQ-13-197 | 13.3 | MUST | M6 | auto | todo | Pakiety producentow (profile, biblioteki materialow, menu linkow) importowane przeciagnieciem |
| REQ-13-198 | 13.3 | MUST | M2 | emu | verified | Edytor ustawien maszyny: opisy, walidacja zakresow, kopia przed zapisem, diff |
| REQ-13-199 | 13.3 | MUST | M0 | manual | verified | docs/hardware-matrix.md z poziomem: sprzet / emulator / eksperymentalne |

#### Sekcja 14

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-14-001 | 14 | MUST | M6 | auto | todo | API skryptowe Python (osadzony) [POSTULAT 114] |
| REQ-14-002 | 14 | MUST | M6 | auto | todo | API skryptowe JavaScript [POSTULAT 79] |
| REQ-14-003 | 14 | SHOULD | M6 | auto | todo | API skryptowe Lua (wymienione w 1.1) |
| REQ-14-004 | 14 | MUST | M6 | auto | todo | Skrypty: dostep do calego rejestru polecen |
| REQ-14-005 | 14 | MUST | M6 | auto | todo | Skrypty: zdarzenia (otwarcie pliku, start/koniec zadania, blad, zmiana stanu maszyny) |
| REQ-14-006 | 14 | SHOULD | M6 | auto | todo | Wtyczki: wlasne panele UI (HTML) [POSTULAT 40] |
| REQ-14-007 | 14 | MUST | M6 | auto | todo | Wtyczki: wlasne generatory |
| REQ-14-008 | 14 | MUST | M6 | auto | todo | Wtyczki: wlasne importery |
| REQ-14-009 | 14 | MUST | M6 | auto | todo | Wtyczki: wlasne sterowniki (trait Rust + ABI dynamiczne lub WASM) [POSTULAT 38] |
| REQ-14-010 | 14 | MUST | M6 | auto | todo | Menedzer wtyczek z podpisem i uprawnieniami |
| REQ-14-011 | 14 | MUST | M0 | auto | verified | CLI: burncraft-cli open |
| REQ-14-012 | 14 | MUST | M1 | auto | verified | CLI: convert |
| REQ-14-013 | 14 | MUST | M2 | auto | verified | CLI: plan |
| REQ-14-014 | 14 | MUST | M2 | auto | verified | CLI: estimate |
| REQ-14-015 | 14 | MUST | M1 | auto | verified | CLI: export |
| REQ-14-016 | 14 | MUST | M2 | auto | verified | CLI: send |
| REQ-14-017 | 14 | MUST | M2 | auto | verified | CLI: status |
| REQ-14-018 | 14 | MUST | M0 | auto | verified | CLI: mcp |
| REQ-14-019 | 14 | MUST | M6 | auto | todo | Automatyzacja UDP: LOADFILE:path |
| REQ-14-020 | 14 | MUST | M6 | auto | todo | UDP: START |
| REQ-14-021 | 14 | MUST | M6 | auto | todo | UDP: STOP |
| REQ-14-022 | 14 | MUST | M6 | auto | todo | UDP: PAUSE |
| REQ-14-023 | 14 | MUST | M6 | auto | todo | UDP: STATUS |
| REQ-14-024 | 14 | MUST | M6 | auto | todo | UDP: FORCECLOSE |
| REQ-14-025 | 14 | MUST | M6 | auto | todo | UDP: LASER:Nazwa |
| REQ-14-026 | 14 | MUST | M6 | auto | todo | Webhooki (stan, postep, alarmy) [POSTULAT 19] |
| REQ-14-027 | 14 | SHOULD | M6 | auto | todo | MQTT (stan, postep, alarmy) |
| REQ-14-028 | 14 | SHOULD | M6 | auto | todo | Integracja Home Assistant |
| REQ-14-029 | 14 | SHOULD | M6 | auto | todo | Sterowanie wyciagiem, sprezarka, chlodziarka przez integracje |
| REQ-14-030 | 14 | MUST | M6 | auto | todo | Wspolne ustawienia na folderze sieciowym |
| REQ-14-031 | 14 | MUST | M6 | auto | todo | Profile uzytkownikow |
| REQ-14-032 | 14 | MUST | M6 | auto | todo | Tryb tylko-do-odczytu dla operatorow |

#### Sekcja 15

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-15-001 | 15.1 | MUST | M0 | auto | verified | MCP stdio przez burncraft-cli mcp w trybie bezglowym |
| REQ-15-002 | 15.1 | MUST | M1 | auto | verified | MCP stdio z --connect <port> do zywego okna |
| REQ-15-003 | 15.1 | MUST | M2 | auto | verified | MCP Streamable HTTP na 127.0.0.1 |
| REQ-15-004 | 15.1 | MUST | M6 | auto | todo | MCP HTTP w LAN z tokenem i TLS (opcjonalnie) |
| REQ-15-005 | 15.1 | MUST | M0 | auto | verified | Zgodnosc z aktualna specyfikacja MCP: narzedzia ze structuredContent i outputSchema |
| REQ-15-006 | 15.1 | MUST | M2 | auto | verified | Zasoby z subskrypcjami |
| REQ-15-007 | 15.1 | MUST | M2 | auto | verified | Prompty |
| REQ-15-008 | 15.1 | MUST | M2 | auto | verified | Powiadomienia o postepie |
| REQ-15-009 | 15.1 | MUST | M2 | auto | verified | Anulowanie zadan MCP |
| REQ-15-010 | 15.1 | MUST | M6 | auto | todo | Elicitation do potwierdzen |
| REQ-15-011 | 15.1 | MUST | M0 | auto | verified | Pole instructions opisuje jednostki, uklad wspolrzednych, model polecen, typowy przeplyw i zasady bezpieczenstwa |
| REQ-15-012 | 15.1 | MUST | M0 | manual | verified | Decyzja i opis ukladu wspolrzednych/jednostek (ADR 0003) |
| REQ-15-013 | 15.2 | MUST | M0 | auto | verified | Narzedzie MCP list_commands {filter?} |
| REQ-15-014 | 15.2 | MUST | M0 | auto | verified | Narzedzie describe_command {id} |
| REQ-15-015 | 15.2 | MUST | M0 | auto | verified | Narzedzie execute {id, params} |
| REQ-15-016 | 15.2 | MUST | M0 | auto | verified | Narzedzie batch {steps[]} jako jeden krok historii |
| REQ-15-017 | 15.2 | MUST | M0 | auto | verified | Narzedzie undo |
| REQ-15-018 | 15.2 | MUST | M0 | auto | verified | Narzedzie redo |
| REQ-15-019 | 15.2 | MUST | M0 | auto | verified | Narzedzie inspect_document {detail?}: warstwy, obiekty (id, typ, warstwa, bbox, transformacja, dlugosc, zamkniecie), zaznaczenie, statystyki |
| REQ-15-020 | 15.2 | MUST | M1 | auto | verified | Narzedzie query_objects {layer?, type?, bbox?, text?} |
| REQ-15-021 | 15.2 | MUST | M1 | auto | verified | Narzedzie get_object {id} |
| REQ-15-022 | 15.2 | MUST | M0 | auto | verified | render_preview tryb design -> PNG |
| REQ-15-023 | 15.2 | MUST | M2 | auto | verified | render_preview tryb toolpath |
| REQ-15-024 | 15.2 | MUST | M4 | auto | todo | render_preview tryb burn_sim |
| REQ-15-025 | 15.2 | MUST | M0 | auto | verified | render_preview: bbox i max_size |
| REQ-15-026 | 15.2 | MUST | M1 | auto | verified | Narzedzie screenshot (okno) |
| REQ-15-027 | 15.2 | MUST | M1 | auto | verified | Narzedzie open_file |
| REQ-15-028 | 15.2 | MUST | M1 | auto | verified | Narzedzie save_file |
| REQ-15-029 | 15.2 | MUST | M1 | auto | verified | Narzedzie import_file {path, options} |
| REQ-15-030 | 15.2 | MUST | M1 | auto | verified | Narzedzie export_file {path, format} |
| REQ-15-031 | 15.2 | MUST | M1 | auto | verified | Narzedzie draw_shape {type, params, layer} |
| REQ-15-032 | 15.2 | MUST | M1 | auto | verified | Narzedzie draw_path {d\|points, closed, layer} |
| REQ-15-033 | 15.2 | MUST | M4 | auto | todo | Narzedzie add_text |
| REQ-15-034 | 15.2 | MUST | M4 | auto | todo | Narzedzie add_barcode |
| REQ-15-035 | 15.2 | MUST | M4 | auto | todo | Narzedzie place_image |
| REQ-15-036 | 15.2 | MUST | M4 | auto | todo | Narzedzie trace_image |
| REQ-15-037 | 15.2 | MUST | M1 | auto | verified | Narzedzie transform {ids, translate/rotate/scale/mirror, anchor} |
| REQ-15-038 | 15.2 | MUST | M1 | auto | verified | Narzedzie boolean |
| REQ-15-039 | 15.2 | MUST | M1 | auto | verified | Narzedzie offset |
| REQ-15-040 | 15.2 | MUST | M1 | auto | verified | Narzedzie array_grid |
| REQ-15-041 | 15.2 | MUST | M1 | auto | verified | Narzedzie array_circular |
| REQ-15-042 | 15.2 | MUST | M1 | auto | verified | Narzedzie align |
| REQ-15-043 | 15.2 | MUST | M1 | auto | verified | Narzedzie distribute |
| REQ-15-044 | 15.2 | MUST | M6 | auto | todo | Narzedzie nest |
| REQ-15-045 | 15.2 | MUST | M6 | auto | todo | Narzedzie generate {generator, params} |
| REQ-15-046 | 15.2 | MUST | M1 | auto | verified | Narzedzie set_layer {color, settings} |
| REQ-15-047 | 15.2 | MUST | M1 | auto | verified | Narzedzie assign_layer {ids, color} |
| REQ-15-048 | 15.2 | MUST | M4 | auto | todo | Narzedzie material_library_search |
| REQ-15-049 | 15.2 | MUST | M4 | auto | todo | Narzedzie apply_material |
| REQ-15-050 | 15.2 | MUST | M4 | auto | todo | Narzedzie create_material_test |
| REQ-15-051 | 15.2 | MUST | M2 | auto | verified | Narzedzie plan_job: czas, dlugosc ciecia/przejazdow, przejscia, ostrzezenia, koszt |
| REQ-15-052 | 15.2 | MUST | M2 | auto | verified | plan_job: ostrzezenie o otwartych ksztaltach w Fill |
| REQ-15-053 | 15.2 | MUST | M2 | auto | verified | plan_job: ostrzezenie o obiektach poza obszarem |
| REQ-15-054 | 15.2 | MUST | M2 | auto | verified | plan_job: ostrzezenie o duplikatach |
| REQ-15-055 | 15.2 | MUST | M2 | auto | verified | plan_job: ostrzezenie o cieciu przed grawerem |
| REQ-15-056 | 15.2 | MUST | M2 | auto | verified | plan_job: ostrzezenie o mocy powyzej limitu |
| REQ-15-057 | 15.2 | MUST | M2 | auto | verified | Narzedzie export_machine_file {driver, path} |
| REQ-15-058 | 15.2 | MUST | M2 | auto | verified | Narzedzie list_devices |
| REQ-15-059 | 15.2 | MUST | M2 | auto | verified | Narzedzie select_device |
| REQ-15-060 | 15.2 | MUST | M2 | auto | verified | Narzedzie connect |
| REQ-15-061 | 15.2 | MUST | M2 | auto | verified | Narzedzie disconnect |
| REQ-15-062 | 15.2 | MUST | M2 | auto | verified | Narzedzie device_status (takze jako zasob z subskrypcja) |
| REQ-15-063 | 15.2 | MUST | M2 | emu | verified | Narzedzie home (tier motion) |
| REQ-15-064 | 15.2 | MUST | M2 | emu | verified | Narzedzie jog (tier motion) |
| REQ-15-065 | 15.2 | MUST | M2 | emu | verified | Narzedzie move_to (tier motion) |
| REQ-15-066 | 15.2 | MUST | M2 | emu | verified | Narzedzie set_origin (tier motion) |
| REQ-15-067 | 15.2 | MUST | M2 | emu | verified | Narzedzie frame (tier motion) |
| REQ-15-068 | 15.2 | MUST | M6 | emu | todo | Narzedzie start_job {job_plan_id, confirm_token} (tier fire) |
| REQ-15-069 | 15.2 | MUST | M6 | emu | todo | Narzedzie test_fire (tier fire) |
| REQ-15-070 | 15.2 | MUST | M2 | emu | verified | Narzedzie pause_job |
| REQ-15-071 | 15.2 | MUST | M2 | emu | verified | Narzedzie resume_job |
| REQ-15-072 | 15.2 | MUST | M2 | emu | verified | Narzedzie stop_job — zawsze dozwolone, bez potwierdzen |
| REQ-15-073 | 15.2 | MUST | M2 | auto | verified | Narzedzie read_machine_settings |
| REQ-15-074 | 15.2 | MUST | M6 | auto | todo | Narzedzie write_machine_settings {changes, confirm_token} (tier config, kopia + diff) |
| REQ-15-075 | 15.2 | MUST | M5 | auto | todo | Narzedzie camera_capture {camera?, rectified} -> PNG |
| REQ-15-076 | 15.2 | MUST | M5 | auto | todo | Narzedzie camera_update_overlay |
| REQ-15-077 | 15.2 | MUST | M5 | auto | todo | Narzedzie detect_material_outline |
| REQ-15-078 | 15.3 | MUST | M0 | auto | verified | Zasob burncraft://document |
| REQ-15-079 | 15.3 | MUST | M0 | auto | verified | Zasob burncraft://object/{id} |
| REQ-15-080 | 15.3 | MUST | M0 | auto | verified | Zasob burncraft://layer/{color} |
| REQ-15-081 | 15.3 | MUST | M0 | auto | verified | Zasob burncraft://commands |
| REQ-15-082 | 15.3 | MUST | M0 | auto | verified | Zasob burncraft://command/{id} |
| REQ-15-083 | 15.3 | MUST | M2 | auto | verified | Zasob burncraft://device/{id}/status (subskrypcja) |
| REQ-15-084 | 15.3 | MUST | M2 | auto | verified | Zasob burncraft://job/{id} (postep, log) |
| REQ-15-085 | 15.3 | MUST | M4 | auto | todo | Zasob burncraft://materials |
| REQ-15-086 | 15.3 | MUST | M6 | auto | todo | Zasob burncraft://history/jobs |
| REQ-15-087 | 15.3 | MUST | M2 | auto | verified | Prompt prepare_cut_file |
| REQ-15-088 | 15.3 | MUST | M4 | auto | todo | Prompt engrave_photo |
| REQ-15-089 | 15.3 | MUST | M6 | auto | todo | Prompt make_box |
| REQ-15-090 | 15.3 | MUST | M4 | auto | todo | Prompt run_material_test |
| REQ-15-091 | 15.3 | MUST | M4 | auto | todo | Prompt batch_serial_numbers |
| REQ-15-092 | 15.4 | MUST | M0 | auto | verified | Poziomy bezpieczenstwa read < design < motion < config < fire |
| REQ-15-093 | 15.4 | MUST | M0 | auto | verified | Domyslna polityka: read + design bez pytan |
| REQ-15-094 | 15.4 | MUST | M2 | auto | verified | Domyslna polityka: motion po potwierdzeniu uzytkownika |
| REQ-15-095 | 15.4 | MUST | M6 | auto | todo | config i fire wylacznie po jawnym potwierdzeniu w GUI (dialog z miniatura, czasem, maks. moca, materialem, urzadzeniem) |
| REQ-15-096 | 15.4 | MUST | M6 | auto | todo | config i fire przez MCP elicitation, gdy GUI nieobecne |
| REQ-15-097 | 15.4 | MUST | M6 | auto | todo | Polityka konfigurowalna w Ustawieniach |
| REQ-15-098 | 15.4 | MUST | M6 | auto | todo | Potwierdzen fire nie mozna wylaczyc dla polaczen nielokalnych |
| REQ-15-099 | 15.4 | MUST | M6 | auto | todo | Tokeny potwierdzenia jednorazowe, powiazane z job_plan_id i hashem planu |
| REQ-15-100 | 15.4 | MUST | M6 | auto | todo | Zmiana dokumentu uniewaznia token |
| REQ-15-101 | 15.4 | MUST | M6 | auto | todo | Limit globalny: maks. moc |
| REQ-15-102 | 15.4 | MUST | M6 | auto | todo | Limit globalny: maks. czas zadania |
| REQ-15-103 | 15.4 | MUST | M6 | auto | todo | Wymagany stan pokrywy/chlodzenia/wyciagu (gdy raportowany) |
| REQ-15-104 | 15.4 | MUST | M6 | auto | todo | Wymagany „operator obecny” (przycisk w GUI w ostatnich N minutach) |
| REQ-15-105 | 15.4 | MUST | M0 | auto | verified | Pelny dziennik audytu akcji MCP (kto/kiedy/co/parametry/wynik) |
| REQ-15-106 | 15.4 | MUST | M2 | emu | verified | stop_job i fizyczny E-stop zawsze nadrzedne |
| REQ-15-107 | 15.4 | MUST | M2 | emu | verified | Utrata polaczenia klienta MCP nie przerywa zadania |
| REQ-15-108 | 15.4 | MUST | M2 | emu | verified | Watchdog zatrzymuje ruchy jog po utracie klienta |

#### Sekcja 16

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-16-001 | 16 | MUST | M7 | auto | todo | 100 000 obiektow / 2 mln wezlow: plynne przewijanie i zaznaczanie |
| REQ-16-002 | 16 | MUST | M4 | auto | todo | Obraz 20 000 x 20 000 px: podglad dithera kafelkowo < 3 s |
| REQ-16-003 | 16 | MUST | M7 | hw | todo | G-code: >= 1000 linii/s na RPi 4 |
| REQ-16-004 | 16 | MUST | M2 | auto | verified | Bufor wstepny strumieniowania |
| REQ-16-005 | 16 | MUST | M2 | auto | verified | Monitor niedopelnienia bufora z ostrzezeniem |
| REQ-16-006 | 16 | MUST | M1 | auto | verified | Brak blokowania UI przy operacjach > 100 ms (watki robocze, anulowanie, paski postepu) |
| REQ-16-007 | 16 | MUST | M2 | auto | verified | Deterministyczne wyniki planera (te same wejscia -> ten sam strumien bajtow) |
| REQ-16-008 | 16 | MUST | M7 | manual | todo | Telemetria wylacznie opt-in |
| REQ-16-009 | 16 | MUST | M0 | auto | verified | Pelne dzialanie offline |
| REQ-16-010 | 16 | MUST | M0 | manual | verified | Brak DRM/licencji online |
| REQ-16-011 | 16 | MUST | M7 | auto | todo | Aktualizacje z podpisem |
| REQ-16-012 | 16 | MUST | M0 | auto | verified | Logowanie z poziomami |
| REQ-16-013 | 16 | MUST | M7 | auto | todo | „Zbierz informacje do pomocy” — paczka bez danych osobowych |

#### Sekcja 17

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-17-001 | 17 | SHOULD | M7 | manual | todo | Biblioteka grafik: podkategorie [POSTULAT 63] |
| REQ-17-002 | 17 | SHOULD | M7 | manual | todo | Biblioteka grafik: tagi [POSTULAT 31] |
| REQ-17-003 | 17 | SHOULD | M7 | manual | todo | Biblioteka grafik: foldery [POSTULAT 23] |
| REQ-17-004 | 17 | SHOULD | M7 | manual | todo | Biblioteka grafik: druk katalogu [POSTULAT 22] |
| REQ-17-005 | 17 | SHOULD | M7 | manual | todo | Notatki ze zdjeciami w bibliotece grafik [POSTULAT 21] |
| REQ-17-006 | 17 | LATER | LATER | manual | todo | Natywne aplikacje iOS/Android |
| REQ-17-007 | 17 | LATER | LATER | manual | todo | Wspolpraca wielu uzytkownikow na jednym dokumencie |

#### Sekcja 18

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-18-001 | 18 | MUST | M7 | manual | todo | Podrecznik uzytkownika po polsku |
| REQ-18-002 | 18 | MUST | M7 | manual | todo | Podrecznik uzytkownika po angielsku |
| REQ-18-003 | 18 | MUST | M7 | auto | todo | Podrecznik generowany ze zrodel Markdown |
| REQ-18-004 | 18 | MUST | M7 | auto | todo | Kontekstowa pomoc F1 dla kazdego polecenia (z rejestru) |
| REQ-18-005 | 18 | MUST | M7 | manual | todo | Samouczek „pierwszy projekt” |
| REQ-18-006 | 18 | MUST | M7 | manual | todo | Samouczek „pierwszy test materialu” |
| REQ-18-007 | 18 | MUST | M7 | manual | todo | Przewodniki per sterownik |
| REQ-18-008 | 18 | MUST | M7 | manual | todo | Przewodnik K40 krok po kroku: instalacja sterownika USB, wybor plyty, kalibracja krokow |
| REQ-18-009 | 18 | MUST | M7 | manual | todo | Rozwiazywanie problemow: jakosc |
| REQ-18-010 | 18 | MUST | M7 | manual | todo | Rozwiazywanie problemow: polaczenie |
| REQ-18-011 | 18 | MUST | M7 | manual | todo | Rozwiazywanie problemow: bledy GRBL |
| REQ-18-012 | 18 | MUST | M7 | manual | todo | Rozwiazywanie problemow: siec Ruida |
| REQ-18-013 | 18 | MUST | M7 | manual | todo | Rozwiazywanie problemow: sterowniki galvo |
| REQ-18-014 | 18 | MUST | M0 | manual | verified | Dokumentacja MCP: lista narzedzi z przykladami JSON |
| REQ-18-015 | 18 | MUST | M0 | manual | verified | Przyklad konfiguracji claude_desktop_config.json |
| REQ-18-016 | 18 | MUST | M0 | manual | verified | Przyklad konfiguracji Claude Code .mcp.json |
| REQ-18-017 | 18 | MUST | M0 | manual | verified | Dokumentacja polityki bezpieczenstwa MCP |
| REQ-18-018 | 18 | MUST | M0 | manual | verified | ADR-y dla kluczowych decyzji |
| REQ-18-019 | 18 | MUST | M0 | manual | verified | CONTRIBUTING.md z instrukcja dodawania sterownika |

#### Sekcja 19

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-19-001 | 19 | MUST | M1 | auto | verified | Testy jednostkowe geometrii: booleany na zbiorze trudnych przypadkow |
| REQ-19-002 | 19 | MUST | M1 | auto | verified | Testy jednostkowe geometrii: offsety na zbiorze trudnych przypadkow |
| REQ-19-003 | 19 | MUST | M1 | auto | verified | Fuzzing cargo-fuzz importera SVG |
| REQ-19-004 | 19 | MUST | M1 | auto | verified | Fuzzing importera DXF |
| REQ-19-005 | 19 | MUST | M1 | auto | verified | Fuzzing importera .lbrn2 |
| REQ-19-006 | 19 | MUST | M2 | auto | verified | Testy golden kompilatorow sterownikow (projekt referencyjny -> bajty/G-code) |
| REQ-19-007 | 19 | MUST | M2 | auto | verified | Emulator GRBL (grbl-sim + symulator bufora) |
| REQ-19-008 | 19 | MUST | M3 | auto | todo | Emulator Ruida (serwer UDP z ACK, dekodowanie swizzlingu, wizualizacja ruchu) |
| REQ-19-009 | 19 | MUST | M3 | auto | todo | Emulator Lihuiyu (pakiety z CRC, stany BUSY/OK) |
| REQ-19-010 | 19 | MUST | M5 | auto | todo | Emulator BJJCZ (listy polecen) |
| REQ-19-011 | 19 | MUST | M6 | auto | todo | Mock xTool HTTP |
| REQ-19-012 | 19 | MUST | M2 | auto | verified | Emulatory renderuja wynik do PNG porownywanego z podgladem planera |
| REQ-19-013 | 19 | MUST | M1 | auto | verified | Testy E2E GUI (Playwright) |
| REQ-19-014 | 19 | MUST | M6 | auto | todo | Testy MCP: scenariusz pudelko 100x60x40 ze sklejki 3 mm -> plan -> eksport G-code |
| REQ-19-015 | 19 | MUST | M6 | auto | todo | Testy MCP: start_job bez tokenu -> odmowa |
| REQ-19-016 | 19 | MUST | M3 | hw | todo | Testy sprzetowe z checklista: K40 M2 Nano |
| REQ-19-017 | 19 | MUST | M2 | hw | verified | Testy sprzetowe: GRBL diodowy |
| REQ-19-018 | 19 | MUST | M7 | hw | todo | Testy sprzetowe: Ruida RDC6445 |
| REQ-19-019 | 19 | MUST | M5 | hw | todo | Testy sprzetowe: galvo LMCV4 |
| REQ-19-020 | 19 | MUST | M0 | auto | verified | CI: Windows |
| REQ-19-021 | 19 | MUST | M0 | auto | verified | CI: macOS x64 |
| REQ-19-022 | 19 | MUST | M0 | auto | verified | CI: macOS arm64 |
| REQ-19-023 | 19 | MUST | M0 | auto | verified | CI: Linux x64 |
| REQ-19-024 | 19 | MUST | M0 | auto | verified | CI: Linux arm64 |
| REQ-19-025 | 19 | MUST | M7 | auto | todo | Paczki MSI |
| REQ-19-026 | 19 | MUST | M7 | auto | todo | Paczki DMG |
| REQ-19-027 | 19 | MUST | M7 | auto | todo | Paczki AppImage |
| REQ-19-028 | 19 | MUST | M7 | auto | todo | Paczki deb |
| REQ-19-029 | 19 | MUST | M7 | auto | todo | Paczki Flatpak |
| REQ-19-030 | 19 | MUST | M6 | auto | todo | Obraz Bridge |
| REQ-19-031 | 19 | MUST | M7 | auto | todo | Akceptacja 1: .lbrn2 z 10 warstwami -> identyczna geometria (+-0,01 mm) |
| REQ-19-032 | 19 | MUST | M7 | hw | todo | Akceptacja 2: K40 M2 Nano — jog, home, ramka, ciecie wektorowe, grawer rastrowy na sprzecie |
| REQ-19-033 | 19 | MUST | M7 | hw | todo | Akceptacja 3: GRBL i Ruida — pelny cykl zadania z pauza/wznowieniem/stopem |
| REQ-19-034 | 19 | MUST | M7 | hw | todo | Akceptacja 4: galvo LMCV4 — markowanie z hatchami i rotary |
| REQ-19-035 | 19 | MUST | M7 | auto | todo | Akceptacja 5: agent MCP bez GUI wykonuje scenariusz E2E do eksportu pliku, start lasera wymaga potwierdzenia |
| REQ-19-036 | 19 | MUST | M7 | auto | todo | Akceptacja 6: wszystkie [MUST] verified lub zatwierdzone odstepstwo, req-check przechodzi w CI |

#### Sekcja 20

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-20-001 | 20 | MUST | M0 | manual | verified | Przed kazdym etapem: plan etapu z lista REQ i ADR |
| REQ-20-002 | 20 | MUST | M0 | manual | deviation | Kolejnosc pracy: testy, potem implementacja |
| REQ-20-003 | 20 | MUST | M0 | manual | verified | Nie oznaczac funkcji jako gotowej bez testu |
| REQ-20-004 | 20 | MUST | M0 | manual | verified | Niepewne szczegoly protokolow oznaczane // UNVERIFIED: z odnosnikiem do zrodla |
| REQ-20-005 | 20 | MUST | M0 | manual | verified | M0: ADR 0001 licencja |
| REQ-20-006 | 20 | MUST | M0 | manual | verified | M0: ADR 0002 stos |
| REQ-20-007 | 20 | MUST | M0 | manual | verified | M0: ADR 0003 uklad wspolrzednych i jednostki |
| REQ-20-008 | 20 | MUST | M0 | manual | verified | M0: ADR 0004 format pliku |
| REQ-20-009 | 20 | MUST | M0 | manual | verified | M0: ADR 0005 model bezpieczenstwa MCP |
| REQ-20-010 | 20 | MUST | M0 | auto | verified | M0: szkielet repozytorium wg 1.1 |
| REQ-20-011 | 20 | MUST | M0 | auto | verified | M0: CI |
| REQ-20-012 | 20 | MUST | M0 | auto | verified | M0: pierwsza dzialajaca wersja burncraft-cli mcp z list_commands, execute, inspect_document, render_preview |

#### Sekcja 21

| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |
|---|---|---|---|---|---|---|
| REQ-21-001 | 21 | MUST | M0 | auto | verified | Rejestr REQUIREMENTS.md + requirements.yaml przed kodem |
| REQ-21-002 | 21 | MUST | M0 | auto | verified | Kazde wymaganie ma id, cytat/parafraze z sekcja, priorytet, etap, kryterium weryfikacji, status, odnosniki |
| REQ-21-003 | 21 | MUST | M0 | manual | verified | Zestawienie „sekcja -> liczba wymagan” |
| REQ-21-004 | 21 | MUST | M0 | manual | verified | Lista fragmentow swiadomie nieuznanych za wymaganie z uzasadnieniem |
| REQ-21-005 | 21 | MUST | M0 | auto | verified | Testy oznaczane identyfikatorami wymagan |
| REQ-21-006 | 21 | MUST | M0 | auto | verified | Polecenia w rejestrze maja pole requirements: [REQ-...] |
| REQ-21-007 | 21 | MUST | M0 | auto | verified | tools/req-check generuje docs/traceability.md |
| REQ-21-008 | 21 | MUST | M0 | auto | verified | req-check: blad, gdy REQ zakonczonego etapu nie jest verified/deviation |
| REQ-21-009 | 21 | MUST | M0 | auto | verified | req-check: blad, gdy REQ done/verified nie ma testu ani procedury recznej |
| REQ-21-010 | 21 | MUST | M0 | auto | verified | req-check: blad, gdy test odwoluje sie do nieistniejacego REQ |
| REQ-21-011 | 21 | MUST | M0 | auto | verified | req-check: blad, gdy deviation nie ma odnosnika do DEVIATIONS.md |
| REQ-21-012 | 21 | MUST | M0 | auto | verified | req-check uruchamiany w CI |
| REQ-21-013 | 21 | MUST | M0 | manual | verified | Przeglad zgodnosci po kazdym etapie w docs/reviews/M<n>-review.md |
| REQ-21-014 | 21 | MUST | M0 | manual | verified | DEVIATIONS.md w formacie DEV-<nr> \| REQ \| opis \| powod \| wariant \| data \| decyzja |
| REQ-21-015 | 21 | MUST | M7 | manual | todo | Odbior koncowy docs/reviews/release-1.0.md |

<!-- req-check:end -->
