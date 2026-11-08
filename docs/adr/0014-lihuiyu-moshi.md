# ADR 0014 — Sterowniki K40 Lihuiyu i Moshi (M3.3)

Status: przyjęty 2026-11-08 · Etap: M3.3 · Wymagania: REQ-13-030…049, REQ-13-051, 052, REQ-19-009

## Kontekst

K40 z płytą Lihuiyu (M2 Nano i pokrewne) oraz płyty Moshi MS10105 łączą się przez układ CH341 w trybie portu równoległego (USB `1a86:5512`). Producenci nie publikują protokołów. Zgodnie z ADR 0013 wiedza pochodzi z publicznych, otwartych źródeł czytanych wyłącznie jako opis bajtów na łączu. Głównym źródłem jest MeerK40t (licencja MIT): README i opis zachowania modułów `lihuiyu`, `moshi` i `ch341`. K40 Whisperer nie był używany. Kod BurnCraft powstał od zera; tablice (kody poleceń Moshi, CRC-8) są liczone z reguł, nie kopiowane.

Sterowniki muszą implementować trait `Driver` z ADR 0010 bez zmiany interfejsu, a każdy sterownik ma emulator (REQ-13-017).

## Decyzje

### 1. Dostęp do płyty: CH341 (`bc_drivers::ch341`)

Trait `Ch341` ma trzy operacje portu EPP: zapis danych, zapis adresu i odczyt stanu (6 bajtów, kod w bajcie 1). Są trzy implementacje:

- `Ch341Usb` — natywnie przez transport USB z M3.1 (`nusb`, REQ-13-039). Zapis danych to bajt 0xA6 przed każdymi 31 bajtami, a zapis adresu — bajt 0xA7. Port włącza żądanie sterujące 0xB1 (tryb EPP 1.9), a stan odczytuje żądanie 0x52. `UsbBulk` dostał żądania sterujące vendor IN/OUT z domyślną implementacją „nieobsługiwane”; to rozszerzenie transportu, nie traitu `Driver`.
- `Ch341Dll` — sterownik producenta CH341DLL na Windows (REQ-13-040). Biblioteka jest ładowana w czasie działania (`LoadLibraryW`, `GetProcAddress` z kernel32), więc nie ma nowej zależności ani wymogu budowania. Profil: `transport: {kind: "ch341_dll", index: 0}`.
- emulatory Lihuiyu i Moshi (§4). `ch341::EmuUsb` pokazuje emulator jako urządzenie USB `1a86:5512` i dekoduje ramki 0xA6/0xA7 oraz żądania sterujące. Testy sprawdzają więc ścieżkę natywną bez sprzętu.

Nazwy plików DLL (`CH341DLLA64.DLL`, `CH341DLL.DLL`), prototypy funkcji (wg nagłówka CH341DLL.H producenta) i bajt stanu są `UNVERIFIED` do testu K40 (M3.4).

### 2. Protokół EGV (Lihuiyu, `lihuiyu::egv`)

- **Pakiet**: 0x00, 30 bajtów danych, CRC-8 Dallas/Maxim 1-Wire (wielomian odbity 0x8C, start 0) z 30 bajtów (REQ-13-041). Strumień zadania jest dzielony na pakiety:
  - znak nowej linii kończy pakiet, a reszta jest dopełniana znakiem `F`;
  - `-` przed nową linią oznacza „czekaj na koniec zadania”;
  - pakiet nie rozcina pary `|`+litera ani liczby, więc pakiet czasu rzeczywistego wstawiony między pakiety zadania nie psuje odległości ani kodu prędkości.
- **Stany** (REQ-13-042): OK 206, BUSY 238, błąd CRC 207, zadanie zakończone 236, potwierdzenie numeru seryjnego 204.
  - Sterownik po każdym pakiecie odpytuje stan. 207 oznacza ponowienie pakietu (najwyżej 5 razy). BUSY i inne stany to dalsze czekanie (do 500 odczytów).
  - Czekanie na koniec zadania trwa, aż bit 0x02 stanu zgaśnie (236 albo 204).
- **Kierunki i odległości** (REQ-13-043):
  - `B` +X, `T` −X, `R` +Y (do przodu), `L` −Y, `M` przekątna, `D`/`U` wiązka;
  - odległość w krokach: `a`…`y` = 1…25, `|a`…`|z` = 26…51, trzy cyfry = 52…254, `z` = 255.
  - Linie ukośne są dzielone algorytmem Bresenhama na odcinki osiowe i przekątne.
- **Kod prędkości** (REQ-13-043): `CV` + dwie trzycyfrowe liczby (starszy i młodszy bajt) + poziom przyspieszenia. Wartość to `65536 − (B + M·T)`, gdzie T to okres kroku w ms przy 1000 dpi (`T = 25,4 / v`). Stałe zależą od płyty:
  - grupa domyślna (M, M1, M2, M3, 6C6879-LASER-M2/M3): M = 12120, B = 5120 / 5120 / 5632 / 6144 dla poziomów 1–4;
  - B2: M podwojone (24240), B jak w grupie domyślnej;
  - A, B, B1: M = 2000, B = 784 / 784 / 896 / 1024.
  - Płyty z członem przekątnej (wszystkie poza A, B, M) dopisują krok (3 cyfry) i wartość przekątnej (6 cyfr).
  - Poniżej 7 mm/s płyty M2, M3 i B2 używają kodu z sufiksem `C` (B = 8, M = 1010; dla B2 B jak A, M = 2020).
  - Poziomy przyspieszenia: wektor — progi 25,4 / 60 / 127 mm/s; raster poziomy — 25,4 / 127 / 320 mm/s.
  - Prędkość wektorowa jest ograniczona do 240 mm/s, a minimalna wynika z zakresu 16 bitów; przekroczenie daje ostrzeżenie `speed_clamped`.
  - Znany punkt kontrolny z README MeerK40t: `CV1410801` ≈ 12,7 mm/s na M2.
- **Program**:
  - wejście: `I` (albo `@NSE` przy zmianie trybu) + kod prędkości + przejazd szybki do startu + `N` + deklaracja kierunków + `S1E`;
  - koniec: `FNSE-`;
  - przejazdy dłuższe niż 256 kroków idą jako szybkie (wyjście `@NSE` i ponowne wejście);
  - przejazd szybki poza programem: `I` + ruchy + `S1P`;
  - bazowanie `IPP`, odblokowanie wózka `IS2P`, impuls testowy `IDS1P`/`IUS1P` (REQ-13-048).
- **Raster natywny „step mode”** (REQ-13-044): operacja rastra albo wypełnienia z liniami poziomymi, dwukierunkowymi i o stałym kroku Y dostaje kod `V` + prędkość + przyspieszenie + `G` + krok. Płyta przy każdej zmianie kierunku X sama przesuwa Y o krok `G`, więc między liniami nie ma przejazdów ani ponownych wejść w program. Pozostałe rastry są kompilowane ruchami wektorowymi.
- **Współrzędne**: K40 bazuje w lewym górnym rogu, Y rośnie do przodu. Dokument BurnCraft ma Y w górę (ADR 0003): `kroki = mm · dpi / 25,4 · skala`, `y = (wysokość stołu − y_dok)`. Domyślnie dpi = 1000 i skala 1 × 1, z korekcją X/Y w profilu (REQ-13-047).
- **Start zadania**: protokół jest względny, więc skompilowane zadanie zaczyna się w narożniku bazowania (0, 0). Sterownik śledzi pozycję z wysłanych pakietów (ten sam interpreter co emulator) i przed każdą rundą dojeżdża do narożnika.
- **Płyty i ID** (REQ-13-030…038, 046):
  - wariant płyty jest w profilu: `k40: {board, board_id, dpi, scale}`; listę dają `lihuiyu::BOARDS` i `machine.list.boards`;
  - ID płyty jest potwierdzane przy połączeniu pakietem `A` + MD5(ID wielkimi literami) i oczekiwaniem 0,5 s na stan 204; MD5 to własna implementacja wg RFC 1321.
- **Brak PWM** (REQ-13-045):
  - `Caps.pwm` (nowe pole struktury `Caps`, bez zmiany traitu) i `DeviceProfile::pwm()` mówią, czy moc da się ustawić z programu. PWM ma tylko M3 Nano (i 6C6879-LASER-M3) — polecenie `AT1` + dwa bajty mocy, `UNVERIFIED`, wymaga firmware 2024.01.18g+.
  - Bez PWM kompilator włącza wiązkę dla każdej mocy > 0 i zgłasza ostrzeżenie `power_ignored`. Zmienna moc w obrębie operacji (grafika) jest zamieniana na dither: dyfuzja błędu wzdłuż linii z rozdzielczością kroku.
  - GUI wyłącza pole mocy w panelu Warstwy i w oknie ustawień warstwy, z komunikatem o pokrętle mocy na laserze, ditheringu i prędkości. Scena (`gui.scene`) niesie pole `device` z informacją o PWM wybranego urządzenia.
- **Pauza i stop** (REQ-13-048): pauza wstrzymuje wysyłkę na bezpiecznej granicy pakietów i wysyła `PN`. Wznowienie wysyła ponownie `PN`, a stop czyści kolejkę i wysyła `I`. Wszystkie trzy są `UNVERIFIED`: zachowanie bufora płyty potwierdzi test K40.

### 3. Protokół Moshi (`moshi::proto`, REQ-13-051, 052)

- Bajt polecenia to permutacja bitów numeru polecenia i kolumny: bajty nieparzyste (b7…b0) ← (7, 6, 2, 4, 3, 5, 1, 0), parzyste ← (5, 1, 7, 2, 4, 3, 6, 0). Test sprawdza wiersze tablicy z README MeerK40t.
- Program składa się z:
  - nagłówka: prędkość wektorowa (polecenie 5: cięcie i przejazd, mm/s − 1) albo rastra (polecenie 4: cm/s − 1);
  - przesunięcia (polecenie 0: z, x, y);
  - ruchów i cięć w krokach 1000 dpi: do punktu (7 / 15), poziomych (6 / 14), pionowych (3 / 11); liczby int16 little-endian;
  - 7 poleceń zakończenia (2).
  Kompilator `moshi` tworzy jeden program na operację (plik `.mos`).
- Wysyłka blokami: dla każdego programu sterownik czeka na stan READY (205), zapisuje adresem prolog (polecenie 6), potem dane w pakietach po 32 bajty, epilog adresem (polecenie 2) i czeka na READY. PROCESSING to 207, ERROR — 237.
- Bazowanie Moshi nie ma publicznego opisu. `home` to przejazd do narożnika (0, 0), a stop to polecenie 1 adresem. Oba są `UNVERIFIED`.

### 4. Emulatory (REQ-19-009, REQ-13-017, REQ-19-012)

- `lihuiyu::emu::LihuiyuEmulator` (za traitem `Ch341`):
  - sprawdza ramkę i CRC: zła suma daje 207, a test może zepsuć kolejne sumy;
  - ma bufor wejściowy o zadanej pojemności: pakiet czeka jako BUSY, gdy bufor jest pełny;
  - wykonuje ruchy w czasie wirtualnym z mnożnikiem;
  - interpreter EGV obsługuje kierunki, odległości, kody prędkości z dekodowaniem wg płyty, program, raster z krokiem `G`, bazowanie `IPP`, pauzę `PN`, zakończenie `FNSE`, potwierdzenie numeru seryjnego i moc `AT`;
  - stan 236 oznacza „zadanie zakończone”;
  - tor ruchu (`EgvMove`) zamienia `trace_job` na zadanie w mm dokumentu, a testy porównują jego PNG z podglądem planera.
- `moshi::emu::MoshiEmulator`: prolog i epilog adresem, program z pakietów danych, stany READY/PROCESSING/ERROR, wykonanie i ten sam tor ruchu.
- Urządzenia fikcyjne w GUI: „Symulator K40 (Lihuiyu)” (M2 Nano, stół 310 × 210 mm) i „Symulator Moshi” (330 × 210 mm) obok „Symulatora” GRBL. `TransportSpec::Emulator { variant: "lihuiyu" | "moshi" }` działa w czasie rzeczywistym.

### 5. Profil, polecenia i GUI

- `DeviceProfile.driver`: `grbl` | `lihuiyu` | `moshi`. Sterowniki K40 wymagają transportu `usb`, `ch341_dll` albo emulatora tego samego sterownika.
- `context_of` przekazuje płytę, kroki i stół wybranego urządzenia K40 do kompilatora (`gcode.compile {driver: "lihuiyu"|"moshi"}`, `job.start`).
- Okno Laser:
  - sekcja „Płyta K40” z wariantem płyty, ID płyty i korekcją skali X/Y (polecenie `machine.profile_save`);
  - przy K40 z kreatora USB przyciski „Dodaj K40 (Lihuiyu)” i „Dodaj Moshi”. Przy sterowniku producenta tworzą profil z transportem `ch341_dll`, w pozostałych przypadkach — z transportem `usb`.

## Konsekwencje

- Zgodność kodowania z konkretną płytą potwierdza dopiero sprzęt. Emulator sprawdza spójność kompilatora, sterownika i dekodera, a nie zachowanie firmware. Do checklisty testu K40 (M3.4, REQ-13-030, 040, 049, REQ-19-016) trafiają:
  - punkty końcowe CH341, żądania sterujące i bajt stanu;
  - funkcje CH341DLL;
  - kod prędkości M2 przy kilku prędkościach (pomiar czasu przejazdu odcinka);
  - kierunki osi i narożnik bazowania;
  - `IPP`, `IS2P`, `PN` (pauza i wznowienie), `I` (stop);
  - rozmiar bufora płyty (BUSY);
  - raster `G`;
  - potwierdzenie ID płyty.
- Prędkości Lihuiyu bywają o ok. 8 % niższe od zadanych (MeerK40t: korekcja `fix_speeds`). BurnCraft nie stosuje tej korekcji, dopóki test K40 nie pokaże, że jest potrzebna.
- Moshi: brak PWM, impulsu testowego i surowych poleceń. Pauza działa między blokami.
