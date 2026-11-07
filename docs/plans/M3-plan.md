# Plan etapu M3 — K40 Lihuiyu i Ruida (transporty USB/UDP, emulatory, sprzęt)

Wymaganie procesowe REQ-20-001. Zakres to 67 pozycji `ms: M3` w `requirements.yaml` (66 MUST, 1 SHOULD: REQ-02-087). Trzy pozycje przeszły do innych etapów decyzją zleceniodawcy: test sprzętowy Ruidy RDC6445G do M7 (DEV-005) i pakiety `.lbzip` do M4 (DEV-006).

Według sposobu weryfikacji:

| Weryfikacja | Liczba |
|---|---|
| na emulatorze | 53 |
| testem automatycznym | 5 |
| procedurą ręczną | 5 |
| na sprzęcie | 4 |

Specyfikacja §20 ustawia kolejność „emulatory, potem sprzęt”. Etap powstaje w czterech przyrostach. W każdym najpierw idzie commit „testy (red)”, potem „implementacja (green)”; oba są pushowane razem (REQ-20-002).

Przydział wymagań do przyrostów sprawdza skrypt: każda z 67 pozycji jest w dokładnie jednym przyroście.

| Przyrost | Zakres | Wymagania | Weryfikacja |
|---|---|---|---|
| **M3.1 Transporty USB i UDP, źródła wiedzy o protokołach** | ADR 0013: zasady korzystania z publicznych źródeł o protokołach (REQ-00-018); transport USB bulk (`bc-drivers::transport`, biblioteka do decyzji, zob. pytania), transport UDP; kreator „Znajdź mój laser” rozszerzony o urządzenia USB po VID/PID (CH341 `1A86:5512`, Ruida USB); reguły udev dla Linuksa (plik w repozytorium i polecenie instalacji); kreator sterownika USB na Windows (wykrycie braku sterownika WinUSB i prowadzenie przez instalację) | REQ-00-018; REQ-13-019, 020, 021, 023, 193 (6) | testy transportów na atrapach urządzeń USB i lokalnym gnieździe UDP; procedury ręczne ze zrzutami dla udev i kreatora Windows; przegląd źródeł w `THIRD_PARTY.md` |
| **M3.2 Ruida** | ADR 0014: sterownik Ruida. Kodowanie bajtów (swizzling, magic 0x88 domyślnie, konfigurowalny per model), pakiety UDP 50200→40200 z sumą kontrolną i ACK/NACK, wrapper zgodny z ZhiYuan, USB-serial; kompilator zadania do pliku `.rd` (wektory, raster natywny ze scan offset) i eksport na pendrive; wysyłka całego zadania do pamięci, start/pauza/stop, ramka, odczyt stanu, odczyt/zapis parametrów maszyny (zakres, przyspieszenia, offset lasera 2, autofokus, rotary), autofokus, lista/uruchamianie/usuwanie plików w pamięci (`Driver::list_files`/`run_file`/`delete_file`), mapowanie osi U/Z KT332N (wyłączalne); profile modeli (RDC6442G/S, RDC6445G/GZ, RDC6332G/M, RDLC-320A, RDC6422G, R5-DSP, RDV6442G, KT332N, RD8445, Hydra/GT5, Thunder, Omtech); kreator: wykrywanie Ruidy przez UDP broadcast; podgląd i dekodowanie plików `.rd`; własny emulator Ruida (serwer UDP z ACK, dekodowanie swizzlingu, tor ruchu → PNG porównany z podglądem planera, jak emulator GRBL) | REQ-13-119, 120, 122…146; REQ-13-184, 186, 194; REQ-19-008; REQ-02-087, 088; REQ-10-017; REQ-11-029; REQ-13-014 (36) | emulator Ruida we wszystkich testach (urządzenie fikcyjne „Symulator Ruida” także w GUI i CLI `emulator --ruida`); golden pliki `.rd`; profil RDC6445G powstaje tu, jego test sprzętowy (REQ-13-121) jest w M7 (DEV-005) |
| **M3.3 K40 Lihuiyu i Moshi** | ADR 0015: sterownik Lihuiyu. Protokół EGV: pakiety 30 bajtów + CRC-8 (Onewire), stany OK/BUSY/CRC error/task complete, komendy kierunku i kroku z prędkością kodowaną zależnie od wariantu płyty (M2 Nano, M3 Nano, M1, M, B1, B2, B, A, 6C6879-LASER-M2/M3), raster natywny „step mode”, home, rapid vs ruch powolny, pauza/wznowienie; konfiguracja wariantu i ID płyty, kroki/mm (1000 dpi), korekcja skali X/Y; USB CH341 natywnie (transport z M3.1) i przez sterownik CH341 DLL na Windows (ładowany w czasie działania); brak PWM: pole mocy nieaktywne z komunikatem w GUI, zamiast mocy dither i prędkość; Moshi MS10105 V4.x (protokół Moshi, zadanie wysyłane blokami); własny emulator Lihuiyu (pakiety z CRC, stany BUSY/OK, tor ruchu → PNG) | REQ-13-030…049; REQ-13-051, 052; REQ-19-009 (23) | emulator Lihuiyu i Moshi w testach i jako urządzenie fikcyjne; procedura ręczna dla REQ-13-045 (pole mocy w GUI); REQ-13-030, 040, 049 — sprzętowo w M3.4 |
| **M3.4 GUI plików i test sprzętowy K40** | okno „Lista plików na sterowniku” (lista, uruchom, usuń, wyślij; polecenia rejestru, `data-command`); checklista i test sprzętowy K40 M2 Nano u zleceniodawcy (jak REQ-19-017 w M2), z weryfikacją sprzętowych pozycji z M3.3; checklista Ruidy RDC6445G gotowa na test w M7 (DEV-005) | REQ-03-042; REQ-19-016 (2) | Playwright z emulatorem jako urządzeniem; procedura ręczna ze zrzutami; checklisty sprzętowe `docs/procedures/M3-hardware-*.md` |

Każde nowe sterowanie w GUI to polecenie rejestru (REQ-01-029, `executor`). Sterowniki Ruida i Lihuiyu implementują trait `Driver` z M2 bez zmiany interfejsu (ADR 0010); nowe możliwości (pamięć plików, autofokus, brak PWM) opisują `Caps`.

## ADR-y etapu

- **ADR 0013** — źródła wiedzy o protokołach (REQ-00-018): wolno korzystać z publicznej dokumentacji, opisów protokołów i z otwartych implementacji czytanych jako źródło wiedzy (np. MeerK40t, K40 Whisperer — GPL), bez kopiowania kodu, struktur ani tekstów; każdy fakt o protokole ma w kodzie odnośnik do źródła, a lista źródeł jest w `THIRD_PARTY.md`; szczegóły niepotwierdzone na sprzęcie dostają `// UNVERIFIED:` jak w M2.
- **ADR 0014** — sterownik Ruida: warstwy (kodowanie bajtów → pakiet UDP/serial → polecenia → plik `.rd`), kompilator `.rd` jako czysta funkcja zadania (jak G-code w ADR 0011), model pamięci plików, emulator.
- **ADR 0015** — sterownik Lihuiyu i Moshi: ramka EGV, CRC-8, maszyna stanów potwierdzeń, tablice kodowania prędkości per wariant płyty, raster „step mode”, dither zamiast mocy, emulator.
- Transport USB (`nusb`, kreator sterownika na Windows, reguły udev) — część ADR 0013 albo osobny ADR przy M3.1.

## Nowe zależności

- **`nusb`** — transport USB bulk (czysty Rust, MIT/Apache-2.0, WinUSB na Windows, usbfs na Linuksie, IOKit na macOS); decyzja zleceniodawcy 2026-11-07.
- UDP — biblioteka standardowa (bez nowych zależności).
- CH341 DLL (REQ-13-040) — nie jest zależnością budowania: program ładuje sterownik producenta zainstalowany przez użytkownika (`libloading`, MIT, jeśli potrzebne).

## Decyzje zleceniodawcy (2026-11-07)

- **Sprzęt**: zleceniodawca ma K40 z płytą M2 Nano — test sprzętowy K40 w M3.4 (REQ-13-030, 040, 049, REQ-19-016). Ruidy RDC6445G nie ma — test przechodzi do M7 („Przenieś test do M7”, DEV-005); sterownik i profil modelu powstają w M3.2 na emulatorze.
- **Biblioteka USB**: `nusb` (czysty Rust, MIT/Apache-2.0) — zgoda na pobranie z crates.io przy M3.1.
- **Kreator sterownika na Windows (REQ-13-020)**: wykrywa brak sterownika WinUSB i prowadzi krok po kroku przez instalację zewnętrznym narzędziem (np. Zadig), którego BurnCraft nie dołącza; alternatywą jest oryginalny sterownik CH341 DLL (REQ-13-040).
- **Źródła wiedzy (REQ-00-018, ADR 0013)**: otwarte projekty (MeerK40t, K40 Whisperer, GPL) tylko jako źródło wiedzy o protokołach, bez kopiowania kodu; odnośniki do źródeł w kodzie i `THIRD_PARTY.md`.
- **Pakiety `.lbzip` (REQ-02-095)**: bez plików przykładowych — przeniesione do M4 („Nie, przenieś do M4”, DEV-006).
## Ryzyka

- Kodowanie prędkości Lihuiyu i szczegóły swizzlingu Ruidy różnią się między płytami i firmware. Emulatory sprawdzają spójność kodowania, ale nie zgodność z konkretną płytą — to potwierdza dopiero sprzęt. Każdy niepotwierdzony szczegół dostaje `// UNVERIFIED:` ze źródłem.
- USB na macOS i Linuksie wymaga uprawnień (udev, odłączenie sterownika jądra CH341). Testy CI używają atrap; prawdziwy port tylko w testach sprzętowych.
- Limit minut GitHub Actions nie dotyczy już repozytorium publicznego (standardowe runnery są bezpłatne); zasada „jeden push na przyrost” i pełna weryfikacja lokalna obowiązują dalej.
