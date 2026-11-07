# Plan etapu M3 — K40 Lihuiyu i Ruida (transporty USB/UDP, emulatory, sprzęt)

Wymaganie procesowe REQ-20-001. Zakres to 70 pozycji `ms: M3` w `requirements.yaml` (69 MUST, 1 SHOULD: REQ-02-087).

Według sposobu weryfikacji:

| Weryfikacja | Liczba |
|---|---|
| na emulatorze | 53 |
| testem automatycznym | 6 |
| procedurą ręczną | 5 |
| na sprzęcie | 6 |

Specyfikacja §20 ustawia kolejność „emulatory, potem sprzęt”. Etap powstaje w czterech przyrostach. W każdym najpierw idzie commit „testy (red)”, potem „implementacja (green)”; oba są pushowane razem (REQ-20-002).

Przydział wymagań do przyrostów sprawdza skrypt: każda z 70 pozycji jest w dokładnie jednym przyroście.

| Przyrost | Zakres | Wymagania | Weryfikacja |
|---|---|---|---|
| **M3.1 Transporty USB i UDP, źródła wiedzy o protokołach** | ADR 0013: zasady korzystania z publicznych źródeł o protokołach (REQ-00-018); transport USB bulk (`bc-drivers::transport`, biblioteka do decyzji, zob. pytania), transport UDP; kreator „Znajdź mój laser” rozszerzony o urządzenia USB po VID/PID (CH341 `1A86:5512`, Ruida USB); reguły udev dla Linuksa (plik w repozytorium i polecenie instalacji); kreator sterownika USB na Windows (wykrycie braku sterownika WinUSB i prowadzenie przez instalację) | REQ-00-018; REQ-13-019, 020, 021, 023, 193 (6) | testy transportów na atrapach urządzeń USB i lokalnym gnieździe UDP; procedury ręczne ze zrzutami dla udev i kreatora Windows; przegląd źródeł w `THIRD_PARTY.md` |
| **M3.2 Ruida** | ADR 0014: sterownik Ruida. Kodowanie bajtów (swizzling, magic 0x88 domyślnie, konfigurowalny per model), pakiety UDP 50200→40200 z sumą kontrolną i ACK/NACK, wrapper zgodny z ZhiYuan, USB-serial; kompilator zadania do pliku `.rd` (wektory, raster natywny ze scan offset) i eksport na pendrive; wysyłka całego zadania do pamięci, start/pauza/stop, ramka, odczyt stanu, odczyt/zapis parametrów maszyny (zakres, przyspieszenia, offset lasera 2, autofokus, rotary), autofokus, lista/uruchamianie/usuwanie plików w pamięci (`Driver::list_files`/`run_file`/`delete_file`), mapowanie osi U/Z KT332N (wyłączalne); profile modeli (RDC6442G/S, RDC6445G/GZ, RDC6332G/M, RDLC-320A, RDC6422G, R5-DSP, RDV6442G, KT332N, RD8445, Hydra/GT5, Thunder, Omtech); kreator: wykrywanie Ruidy przez UDP broadcast; podgląd i dekodowanie plików `.rd`; własny emulator Ruida (serwer UDP z ACK, dekodowanie swizzlingu, tor ruchu → PNG porównany z podglądem planera, jak emulator GRBL) | REQ-13-119…146; REQ-13-184, 186, 194; REQ-19-008; REQ-02-087, 088; REQ-10-017; REQ-11-029; REQ-13-014 (37) | emulator Ruida we wszystkich testach (urządzenie fikcyjne „Symulator Ruida” także w GUI i CLI `emulator --ruida`); golden pliki `.rd`; REQ-13-121 (RDC6445G) — sprzętowo w M3.4 |
| **M3.3 K40 Lihuiyu i Moshi** | ADR 0015: sterownik Lihuiyu. Protokół EGV: pakiety 30 bajtów + CRC-8 (Onewire), stany OK/BUSY/CRC error/task complete, komendy kierunku i kroku z prędkością kodowaną zależnie od wariantu płyty (M2 Nano, M3 Nano, M1, M, B1, B2, B, A, 6C6879-LASER-M2/M3), raster natywny „step mode”, home, rapid vs ruch powolny, pauza/wznowienie; konfiguracja wariantu i ID płyty, kroki/mm (1000 dpi), korekcja skali X/Y; USB CH341 natywnie (transport z M3.1) i przez sterownik CH341 DLL na Windows (ładowany w czasie działania); brak PWM: pole mocy nieaktywne z komunikatem w GUI, zamiast mocy dither i prędkość; Moshi MS10105 V4.x (protokół Moshi, zadanie wysyłane blokami); własny emulator Lihuiyu (pakiety z CRC, stany BUSY/OK, tor ruchu → PNG) | REQ-13-030…049; REQ-13-051, 052; REQ-19-009 (23) | emulator Lihuiyu i Moshi w testach i jako urządzenie fikcyjne; procedura ręczna dla REQ-13-045 (pole mocy w GUI); REQ-13-030, 040, 049 — sprzętowo w M3.4 |
| **M3.4 GUI plików, pakiety ustawień, test sprzętowy** | okno „Lista plików na sterowniku” (lista, uruchom, usuń, wyślij; polecenia rejestru, `data-command`); import pakietów ustawień urządzeń `.lbzip` (na podstawie plików przykładowych od zleceniodawcy); checklisty i testy sprzętowe K40 M2 Nano i Ruida RDC6445 (jak REQ-19-017 w M2), z weryfikacją sprzętowych pozycji z M3.2 i M3.3 | REQ-03-042; REQ-02-095; REQ-19-016, 018 (4) | Playwright z emulatorem jako urządzeniem; procedura ręczna ze zrzutami; checklisty sprzętowe `docs/procedures/M3-hardware-*.md` |

Każde nowe sterowanie w GUI to polecenie rejestru (REQ-01-029, `executor`). Sterowniki Ruida i Lihuiyu implementują trait `Driver` z M2 bez zmiany interfejsu (ADR 0010); nowe możliwości (pamięć plików, autofokus, brak PWM) opisują `Caps`.

## ADR-y etapu

- **ADR 0013** — źródła wiedzy o protokołach (REQ-00-018): wolno korzystać z publicznej dokumentacji, opisów protokołów i z otwartych implementacji czytanych jako źródło wiedzy (np. MeerK40t, K40 Whisperer — GPL), bez kopiowania kodu, struktur ani tekstów; każdy fakt o protokole ma w kodzie odnośnik do źródła, a lista źródeł jest w `THIRD_PARTY.md`; szczegóły niepotwierdzone na sprzęcie dostają `// UNVERIFIED:` jak w M2.
- **ADR 0014** — sterownik Ruida: warstwy (kodowanie bajtów → pakiet UDP/serial → polecenia → plik `.rd`), kompilator `.rd` jako czysta funkcja zadania (jak G-code w ADR 0011), model pamięci plików, emulator.
- **ADR 0015** — sterownik Lihuiyu i Moshi: ramka EGV, CRC-8, maszyna stanów potwierdzeń, tablice kodowania prędkości per wariant płyty, raster „step mode”, dither zamiast mocy, emulator.
- Transport USB (biblioteka, sterownik na Windows) — w ADR 0014/0015 albo osobny ADR, po decyzji zleceniodawcy.

## Nowe zależności (do zgody zleceniodawcy przed pobraniem)

- Biblioteka USB — propozycja: **`nusb`** (czysty Rust, MIT/Apache-2.0, WinUSB na Windows, usbfs na Linuksie, IOKit na macOS). Alternatywa: `rusb` (wiązanie do libusb, LGPL-2.1, wymaga dołączenia libusb).
- UDP — biblioteka standardowa (bez nowych zależności).
- CH341 DLL (REQ-13-040) — nie jest zależnością budowania: program ładuje sterownik producenta zainstalowany przez użytkownika (`libloading`, MIT, jeśli potrzebne).

## Pytania do zleceniodawcy (przed startem)

1. **Sprzęt do testów (6 pozycji `hw`)**: REQ-13-030, 040, 049 i REQ-19-016 wymagają K40 z płytą M2 Nano; REQ-13-121 i REQ-19-018 — Ruidy RDC6445G. Czy masz dostęp do tych maszyn? Bez nich te pozycje wymagają odstępstwa (np. przeniesienia testu sprzętowego), a zamknięcie etapu — Twojej decyzji.
2. **Biblioteka USB**: `nusb` (proponowana) czy `rusb`/libusb? Każda wymaga pobrania z crates.io.
3. **Kreator sterownika USB na Windows (REQ-13-020)**: WinUSB dla CH341 trzeba zainstalować w systemie. Propozycja: kreator wykrywa brak sterownika i prowadzi krok po kroku przez instalację zewnętrznym narzędziem (np. Zadig), bez dołączania go do BurnCraft. Alternatywa: własny instalator INF (wymaga podpisu sterownika).
4. **Źródła wiedzy (REQ-00-018, ADR 0013)**: czy akceptujesz korzystanie z otwartych projektów (MeerK40t, K40 Whisperer — GPL) wyłącznie jako źródła wiedzy o protokole, bez kopiowania kodu? Specyfikacja wymienia je w §13.3.
5. **Pakiety `.lbzip` (REQ-02-095)**: potrzebne pliki przykładowe (jak przy `.lbrn2` w M1). Bez nich import nie powstanie — przeniesienie do M4 wymagałoby odstępstwa.

## Ryzyka

- Kodowanie prędkości Lihuiyu i szczegóły swizzlingu Ruidy różnią się między płytami i firmware. Emulatory sprawdzają spójność kodowania, ale nie zgodność z konkretną płytą — to potwierdza dopiero sprzęt. Każdy niepotwierdzony szczegół dostaje `// UNVERIFIED:` ze źródłem.
- USB na macOS i Linuksie wymaga uprawnień (udev, odłączenie sterownika jądra CH341). Testy CI używają atrap; prawdziwy port tylko w testach sprzętowych.
- Limit minut GitHub Actions nie dotyczy już repozytorium publicznego (standardowe runnery są bezpłatne); zasada „jeden push na przyrost” i pełna weryfikacja lokalna obowiązują dalej.
