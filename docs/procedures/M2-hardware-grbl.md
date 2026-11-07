# REQ-19-017 — test sprzętowy: laser diodowy GRBL (checklista)

Test przeprowadza zleceniodawca na własnej maszynie. Wyniki trafiają do `docs/hardware-matrix.md` i do tabeli wyników na końcu tego pliku. REQ-19-017 dostaje status `verified` dopiero po otrzymaniu wyników. Każdy punkt z wynikiem „nie” zgłaszamy jako błąd albo odstępstwo (`DEVIATIONS.md`).

## Bezpieczeństwo (przed testem)

- Okulary ochronne właściwe dla długości fali lasera (zwykle 445–455 nm), także dla osób w pomieszczeniu.
- Pod laserem niepalny stół albo płyta; obok gaśnica lub koc gaśniczy. Nie zostawiaj pracującego lasera bez nadzoru.
- Wyciąg albo wentylacja: sklejka i MDF dymią.
- Materiał testowy: sklejka 3 mm albo tektura (do cięcia) i jasne drewno (do graweru).
- Ręka przy wyłączniku zasilania lasera. Przycisk „Stop” w BurnCraft zatrzymuje zadanie programowo (wstrzymanie + reset GRBL), ale nie zastępuje wyłącznika.
- Pierwsze próby z mocą 5–10 %, potem wartości z tabeli materiału producenta.

## Przygotowanie

1. Zbuduj i uruchom BurnCraft (Windows): okno `burncraft` albo `burncraft-cli serve` i przeglądarka `http://127.0.0.1:7984/`.
2. Podłącz laser przez USB. W Menedżerze urządzeń sprawdź port COM (CH340 / CP210x / FTDI).
3. Okno Laser → „Nowe urządzenie”: „Znajdź mój laser” (`machine.find`: porty z VID/PID i próba połączenia GRBL) → „Dodaj” przy znalezionym porcie. Gdy laser nie zostanie rozpoznany, wpisz port (np. `COM3`) i „Dodaj profil” (`machine.profile_save`, GRBL 115200 bd). Profil „Laser COM3” pojawi się na liście urządzeń. Obszar roboczy z dokumentacji lasera można ustawić przez `machine.profile_save` (MCP).
4. Zanotuj odpowiedź na `$I` z Konsoli (wersja firmware) i wynik `machine.laser_check` (`$32=1` tryb laserowy, `$30` = S-max).

## Checklista

Kolumna „Polecenie” to polecenie rejestru, które wykonuje dana kontrolka (to samo zrobi agent MCP albo CLI).

| # | Krok | Polecenie | Oczekiwany wynik |
|---|---|---|---|
| 1 | **Połączenie**: okno Laser → urządzenie → „Połącz” | `machine.connect` | stan „gotowy” (albo „alarm”, gdy maszyna wymaga bazowania), rozpoznana odmiana i wersja; Konsola pokazuje wymianę |
| 2 | Konsola: `$I`, `$$`, `?` | `device.raw` | odpowiedzi z kolorowaniem; `$$` z wartościami $30, $32, $110…$131 |
| 3 | **Home**: „Bazuj” (jeśli maszyna ma krańcówki; inaczej pomiń i zanotuj) | `device.home` | głowica jedzie do krańcówek, stan wraca na „gotowy”, pozycja zgodna z `$H` |
| 4 | Alarm: odłącz/zablokuj ruch zgodnie z instrukcją producenta albo wywołaj alarm krańcówką; potem „Odblokuj” (`$X`) | `device.unlock` | alarm z opisem po polsku; po `$X` stan gotowy |
| 5 | **Jog XY**: okno Przesuń, krok 10 mm, X+, X−, Y+, Y− (tryb Szybko, potem Wolno) | `device.jog` | głowica przesuwa się o 10 mm w dobrą stronę; prędkość wolna wyraźnie mniejsza; pozycja w oknie zgodna |
| 6 | Jog Z (tylko z osią Z; inaczej pomiń) | `device.jog` | ruch Z o krok |
| 7 | Zapamiętana pozycja: zapisz „Narożnik”, odjedź, kliknij „Narożnik” | `machine.position_save`, `device.position_go` | powrót do zapamiętanej pozycji |
| 8 | Znacznik głowicy na obszarze roboczym | `machine.status` | czerwony krzyż w położeniu odpowiadającym głowicy (Punkt startowy „Współrzędne bezwzględne”) |
| 9 | Projekt testowy: kwadrat 30×30 mm z otworem Ø10 na C00 (Line, 3 mm sklejka: wg producenta, np. 6 mm/s, 100 %, 2–4 przejścia) i napis/prostokąt 20×10 na C01 (Fill, np. 80 mm/s, 30 %, interwał 0,1 mm) | — | podgląd (przycisk „Podgląd”) pokazuje kolejność wg warstw: C00 (otwór, potem kontur, każde przejście), potem C01; z „Cięcie na końcu” (`plan.settings {cut_last: true}`, domyślnie wyłączone): grawer, otwór, kontur; przejścia i czas |
| 10 | **Ramka**: „Ramka” (prostokąt) bez wiązki | `device.frame` | głowica obrysowuje zadanie, wiązka wyłączona (ewentualnie świeci tylko słaby wskaźnik, jeśli laser go ma) |
| 11 | **Cięcie wektorowe**: tylko warstwa C00 (Output C01 wyłączony), „Start” | `job.start` | otwór i kwadrat wycięte; licznik „przejście N/M”, procent; czas po zakończeniu; wymiary zmierz suwmiarką (podaj wynik) |
| 12 | **Grawer**: tylko C01, „Start” | `job.start` | równy grawer bez przesunięcia linii; brak przypalenia na końcach (M4 moc dynamiczna); zanotuj jakość |
| 13 | **Pauza / wznowienie**: w trakcie zadania „Pauza”, odczekaj 5 s, „Wznów” | `job.pause`, `job.resume` | ruch i wiązka zatrzymane w pauzie (stan „wstrzymany”), po wznowieniu zadanie kończy się poprawnie bez śladu w miejscu pauzy |
| 14 | **Stop**: uruchom zadanie i naciśnij „Stop” | `job.stop` | natychmiastowe zatrzymanie ruchu i wiązki; stan po resecie — zanotuj (alarm/gotowy) |
| 15 | „Rozpocznij od tego miejsca”: w Podglądzie ustaw suwak na drugiej części i uruchom | `job.start {from_segment}` | zadanie zaczyna od wybranej części (przejazd bez wiązki) |
| 16 | Rozłączenie USB w trakcie jogu albo zadania (opcjonalnie, tylko przy małej mocy) | — | program zgłasza utratę połączenia; maszyna zatrzymuje się (zachowanie GRBL) — zanotuj |

Punkty `// UNVERIFIED:` z M2.2–M2.4 do potwierdzenia przy tym teście (zanotuj, jeśli dotyczy):

- M73 (postęp) jest domyślnie wyłączone. Nie włączaj go dla GRBL.
- Wiązka w postoju przy M3 (tryb punktowy). Dotyczy tylko warstw z „dot mode”.
- Szczegóły odmiany producenta: zakres S (`$30`), dodatkowe pola raportu `?`.
- Stop: wstrzymanie + reset (0x18). Czy po resecie GRBL wymaga `$X`/bazowania?

## Co przesłać

1. Model lasera, płyta sterownika, odpowiedź `$I` i `machine.laser_check`.
2. Wynik każdego punktu 1–16: **tak / nie / pominięto** z krótką uwagą (wymiary z punktu 11, jakość z punktu 12).
3. Zdjęcia wyciętego i wygrawerowanego elementu (opcjonalnie) i ewentualne zrzuty Konsoli przy błędach.

## Wyniki

Test przeprowadził zleceniodawca 2026-11-05 (prowadzenie krok po kroku w sesji nadrzędnej).

- Laser: **Two Trees TTS-20 PRO** (`$I`: „Using machine:TTS-20 PRO_20230512”), płyta **MKS DLC32 V4.00** (`$I`: V4.00(8M.H35.20231121), `$53 = MKS_DLC`), USB CH340 (VID 1A86, PID 7523) na COM3, 115200 bd.
- Firmware: `[VER:1.1h.2023112101:]`. Odmiana rozpoznana w teście jako GRBL 1.1f/1.1h (usterka, poprawiona: teraz Two Trees).
- `machine.laser_check`: ok, bez problemów (`$32=1`, `$30=1000`, `$31=0`).
- Ustawienia: `$22=1` bazowanie, `$21=1` wyłączniki krańcowe, `$23=3`, `$130/$131` = 419 mm, `$132` = 50 mm, `$110/$111` = 30000 mm/min, `$120/$121` = 1700/1300 mm/s².

| # | Wynik | Uwagi |
|---|---|---|
| 1 | tak | stan gotowy, wersja 1.1h.2023112101 |
| 2 | tak | `$I`, `$$`, `?` w Konsoli z kolorowaniem i historią |
| 3 | tak | bazowanie do krańcówek, stan gotowy, MPos 0, 0, 0; raport stanu zawiera komunikat Wi-Fi płyty (tryb AP) |
| 4 | tak | ALARM:1 po ręcznym wciśnięciu krańcówki, opis po polsku; „Odblokuj” → stan gotowy (pole `alarm` zostawało — usterka, poprawiona) |
| 5 | tak | jog 10 mm X±, Y± w dobrą stronę, Szybko i Wolno wyraźnie różne |
| 6 | pominięto | laser bez silnika osi Z |
| 7 | tak | pozycja „Narożnik” zapisana i odtworzona |
| 8 | tak | krzyżyk głowicy przy X 50, Y 130 zgodny z położeniem na stole i z jogiem |
| 9 | tak | 2 przejścia konturów, czas 00:01:36. Kolejność domyślnie wg warstw: C00 otwór → kwadrat (×2), potem C01; z `cut_last: true`: C01 → C00. Checklista zakładała „grawer najpierw” (poprawiona). G-code: M4, S0–S1000, grawer S300, M8/M9, koniec M5, G0 X0 Y0, M2 |
| 10 | tak | ramka objechała zadanie na materiale, wiązka wyłączona |
| 11 | tak | kwadrat 30 × 30 mm, otwór Ø10 mm (pomiar linijką); sklejka 3 mm nie przecięta całkiem na wylot — parametry materiału, nie program. 140/140 wierszy, 51,6 s, bez błędów i niedopełnień bufora |
| 12 | tak | grawer równy, ocena zleceniodawcy „OK” (80 mm/s, 30 %, interwał 0,1 mm, dwukierunkowo, M4). 408/408 wierszy, 38,9 s |
| 13 | tak | pauza zatrzymała ruch i wiązkę, po ok. 5 s „Wznów” dokończyło zadanie bez śladu. 74/74 wierszy, 31,0 s z pauzą |
| 14 | tak | Stop po 3,7 s: natychmiastowe zatrzymanie (wstrzymanie + 0x18); po resecie ALARM:3, praca wraca po „Odblokuj” i „Bazuj” |
| 15 | tak | zadanie od początku kwadratu (13 wierszy, otwór pominięty), dojazd bez wiązki; przerwane Stopem po 12,6 s (element już wycięty) |
| 16 | pominięto | decyzja zleceniodawcy („Pomijam 16”) |

Punkty `// UNVERIFIED:`:
- M73: nie włączano (domyślnie wyłączone).
- Wiązka w postoju przy M3 (tryb punktowy): nie testowano, projekt bez warstw w trybie punktowym.
- Odmiana producenta: `$30 = 1000`; raport `?` zawiera `Pn:` i komunikat `[MSG:Mode=AP…]` z Wi-Fi płyty.
- Stop (wstrzymanie + 0x18): po resecie ALARM:3, wymagane `$X` albo bazowanie (potwierdzone na MKS DLC32).

Nie potwierdzono na sprzęcie widoczności licznika „przejście N/M” i czasu po zakończeniu w oknie Laser (zleceniodawca nie odpowiedział na to pytanie). Stan zadania w `job.status` / `machine.status` był poprawny; okno sprawdzono na emulatorze (`docs/procedures/M2-gui-checks.md`, REQ-03-053/054).

Usterki z testu, poprawione w M2.5 (testy `crates/bc-drivers/tests/hardware_m25.rs`, `crates/bc-commands/tests/m25_commands.rs`):
1. Pole `alarm` w `machine.status` zostawało po `$X` i po bazowaniu: teraz znika, gdy raport stanu pokaże stan inny niż alarm.
2. „Znajdź mój laser” nie rozpoznawał Two Trees z „Using machine:TTS-…” w `$I`: rozpoznaje (także TS2, TS3).
3. Profil z kreatora miał obszar 400 × 400 mm: obszar z `$130/$131`.
4. `plan.estimate`, Podgląd i nagłówek G-code używały kinematyki dokumentu: przy podłączonej maszynie kinematyka z `$110/$111/$120/$121/$11` (`kinematics_source: device`), odświeżana po zapisie ustawień.
5. Krok 9 checklisty zakładał „grawer najpierw”: opis poprawiony (wartość domyślna `cut_last = false` bez zmian).
6. Rozszerzone ustawienia płyty MKS bez nazw: nazwy osi A/B/C (`$103–$105`, `$113–$115`, `$123–$125`, `$133–$135`) i `$28`; pozostałe (`$40–$54` itd.) opisane jako „ustawienie rozszerzone producenta płyty (opis w dokumentacji sterownika)”, bez zakresów. Ograniczenie, nie odstępstwo: REQ-13-198 dotyczy tabeli GRBL 1.1, a znaczeń rozszerzeń MKS DLC32 nie potwierdza publiczna dokumentacja (nazwy nie są zgadywane).
