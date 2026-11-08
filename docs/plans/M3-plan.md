# Plan etapu M3 — K40 Lihuiyu i Moshi (transporty USB/UDP, emulatory, sprzęt)

Wymaganie procesowe REQ-20-001. W `requirements.yaml` jest 66 pozycji `ms: M3`. Aktywnych jest 32 (wszystkie MUST); 34 to odstępstwo DEV-007.

**Zmiana zakresu 2026-11-07: rezygnacja z Ruidy (DEV-007, „Rezygnujemy z Ruidy.”).** Sterownik Ruida, pliki `.rd`, emulator Ruidy, autofokus, wykrywanie w sieci, profile Thunder/Omtech i test sprzętowy Ruidy wypadają z projektu. Wcześniej z M3 wyszły też:
- wysyłka do pamięci sterownika (REQ-10-017) — do M6, tylko dla Trocen;
- pakiety `.lbzip` (REQ-02-095) — do M4 (DEV-006).

DEV-007 zastępuje DEV-005.

Według sposobu weryfikacji:

| Weryfikacja | Liczba |
|---|---|
| na emulatorze | 20 |
| testem automatycznym | 3 |
| procedurą ręczną | 5 |
| na sprzęcie | 4 |

Specyfikacja §20 ustawia kolejność „emulatory, potem sprzęt”. Etap powstaje w trzech przyrostach (M3.1, M3.3, M3.4). Przyrost M3.2 (Ruida) wypadł z zakresu, a numeracja pozostałych zostaje bez zmian. W każdym najpierw idzie commit „testy (red)”, potem „implementacja (green)”; oba są pushowane razem (REQ-20-002).

Przydział wymagań do przyrostów sprawdza skrypt: każda z 32 aktywnych pozycji jest w dokładnie jednym przyroście.

| Przyrost | Zakres | Wymagania | Weryfikacja |
|---|---|---|---|
| **M3.1 Transporty USB i UDP, źródła wiedzy o protokołach** | ADR 0013: zasady korzystania z publicznych źródeł o protokołach (REQ-00-018); transport USB bulk (`bc-drivers::transport`, biblioteka do decyzji, zob. pytania), transport UDP; kreator „Znajdź mój laser” rozszerzony o urządzenia USB po VID/PID (CH341 `1A86:5512`); reguły udev dla Linuksa (plik w repozytorium i polecenie instalacji); kreator sterownika USB na Windows (wykrycie braku sterownika WinUSB i prowadzenie przez instalację) | REQ-00-018; REQ-13-019, 020, 021, 023, 193 (6) | testy transportów na atrapach urządzeń USB i lokalnym gnieździe UDP; procedury ręczne ze zrzutami dla udev i kreatora Windows; przegląd źródeł w `THIRD_PARTY.md` |
| ~~M3.2 Ruida~~ | **wyłączone z projektu (DEV-007)** | — | — |
| **M3.3 K40 Lihuiyu i Moshi** | ADR 0014: sterownik Lihuiyu i Moshi. Protokół EGV: pakiety 30 bajtów + CRC-8 (Onewire), stany OK/BUSY/CRC error/task complete, komendy kierunku i kroku z prędkością kodowaną zależnie od wariantu płyty (M2 Nano, M3 Nano, M1, M, B1, B2, B, A, 6C6879-LASER-M2/M3), raster natywny „step mode”, home, rapid vs ruch powolny, pauza/wznowienie; konfiguracja wariantu i ID płyty, kroki/mm (1000 dpi), korekcja skali X/Y; USB CH341 natywnie (transport z M3.1) i przez sterownik CH341 DLL na Windows (ładowany w czasie działania); brak PWM: pole mocy nieaktywne z komunikatem w GUI, zamiast mocy dither i prędkość; Moshi MS10105 V4.x (protokół Moshi, zadanie wysyłane blokami); własny emulator Lihuiyu (pakiety z CRC, stany BUSY/OK, tor ruchu → PNG) | REQ-13-030…049; REQ-13-051, 052; REQ-19-009 (23) | emulator Lihuiyu i Moshi w testach i jako urządzenie fikcyjne; procedura ręczna dla REQ-13-045 (pole mocy w GUI); REQ-13-030, 040, 049 — sprzętowo w M3.4 |
| **M3.4 Pliki na sterowniku i test sprzętowy K40** | API plików sterownika `Driver::list_files`/`run_file`/`delete_file` (gdy wspierane — karta SD FluidNC z M2) i okno „Lista plików na sterowniku” (lista, uruchom, usuń, wyślij; polecenia rejestru, `data-command`); checklista i test sprzętowy K40 M2 Nano u zleceniodawcy (jak REQ-19-017 w M2), z weryfikacją sprzętowych pozycji z M3.3 | REQ-03-042; REQ-13-014; REQ-19-016 (3) | Playwright z emulatorem jako urządzeniem; procedura ręczna ze zrzutami; checklisty sprzętowe `docs/procedures/M3-hardware-*.md` |

Każde nowe sterowanie w GUI to polecenie rejestru (REQ-01-029, `executor`). Sterowniki Lihuiyu i Moshi implementują trait `Driver` z M2 bez zmiany interfejsu (ADR 0010); nowe możliwości (brak PWM, natywny raster) opisują `Caps`.

## Stan przyrostów

- **M3.4 — część programowa zakończona 2026-11-08; test sprzętowy K40 czeka na zleceniodawcę.** REQ-03-042 i REQ-13-014 `verified`. REQ-19-016, REQ-13-030, 040, 049 są `in_progress` do wyników checklisty `docs/procedures/M3-hardware-k40.md`. Odstępstw nie ma. Para commitów red/green:
  - API plików sterownika (`Driver::list_files`/`upload_file`/`run_file`/`delete_file`, z M2 dla karty SD FluidNC): lista posortowana po nazwie, walidacja nazwy pliku (`driver::check_file_name`: niepusta, do 255 znaków, bez znaków sterujących — koniec wiersza nie dopisze drugiego polecenia do `$SD/Run=`); GRBL bez karty, Lihuiyu i Moshi — `Unsupported`;
  - polecenia: `machine.files` z `supported`, `total_bytes` i `reason` (urządzenie bez pamięci plików bez błędu), `device.sd_upload {text}` (plik z dysku bez kompilacji, do 16 MB, pole `source`), osobne parametry `device.sd_run`/`sd_delete`; okno `files.open`/`files.close`;
  - GUI: okno „Lista plików na sterowniku” (`web/src/files.ts`, przycisk „Pliki na sterowniku” w oknie Laser): lista z rozmiarem i sumą, uruchom, usuń z potwierdzeniem, wyślij projekt albo plik z dysku, komunikat dla K40/Moshi; pl/en; procedura REQ-03-042 w `docs/procedures/M3-gui-checks.md`;
  - checklista testu K40 M2 Nano (część A: oryginalny sterownik CH341DLL, część B: WinUSB) z odwzorowaniem listy `UNVERIFIED` z ADR 0014 na punkty.
  - Druga para red/green (wyścig w CI 42488075278): `stop()` GRBL między `send_job` a odebraniem zadania przez wątek I/O — zadanie startowało po resecie i nadpisywało stan „zatrzymane”. Wątek I/O nie uruchamia już zadania zatrzymanego przed startem; test `req_13_011_012_stop_before_io_thread_starts_job` odtwarza wyścig deterministycznie. CI 42489452244 zielony.

- **M3.3 — zakończony 2026-11-08** (emulatory). 20 wymagań `verified`. REQ-13-030 (M2 Nano), REQ-13-040 (CH341DLL) i REQ-13-049 („od pudełka”) są `in_progress` do testu sprzętowego K40 w M3.4. Odstępstw nie ma. ADR 0014. Para commitów red/green:
  - `bc_drivers::ch341`: trait `Ch341` (dane i adres EPP, stan) — natywnie przez USB z M3.1 (`Ch341Usb`, żądania sterujące w `UsbBulk`), CH341DLL ładowana w czasie działania (`Ch341Dll`, transport `ch341_dll`), emulator za interfejsem USB (`EmuUsb`);
  - `bc_drivers::lihuiyu`: protokół EGV (pakiety 30 B + CRC-8, stany, kierunki, odległości, kody prędkości dla 10 wariantów płyt, raster natywny `G`), sterownik `LihuiyuDriver` (bazowanie, przejazd szybki/powolny, pauza, stop, potwierdzenie ID płyty MD5), kompilator `lihuiyu` (`.egv`, dither bez PWM), emulator z buforem i torem → PNG;
  - `bc_drivers::moshi`: protokół (permutacja bitów kodów, programy), sterownik z wysyłką blokami, kompilator `moshi` (`.mos`), emulator;
  - profil `k40` (płyta, ID, dpi, skala), `Caps.pwm`, urządzenia fikcyjne „Symulator K40 (Lihuiyu)” i „Symulator Moshi”, `machine.list.boards`;
  - GUI: pole mocy nieaktywne z komunikatem bez PWM (panel Warstwy, okno ustawień warstwy), sekcja „Płyta K40” w oknie Laser, „Dodaj K40/Moshi” w kreatorze USB; procedura REQ-13-045 w `docs/procedures/M3-gui-checks.md`;
  - szczegóły niepotwierdzone na sprzęcie (`UNVERIFIED`) wypisuje ADR 0014 — wejście do checklisty K40 w M3.4.

- **M3.2 — wyłączony z projektu 2026-11-07 (DEV-007).** Przed decyzją zebrano publiczne źródła o protokole Ruidy (EduTech Wiki, MeerK40t, ruida-protocol-analyzer). Lista plików w pamięci, wrapper ZhiYuan, wykrywanie przez UDP broadcast i mapowanie U/Z KT332N nie miały publicznego opisu. Kodu Ruidy nie napisano.
- **M3.1 — zakończony 2026-11-07.** 6 wymagań `verified`, bez odstępstw. ADR 0013. Para commitów red/green:
  - `bc_drivers::usb`: transport USB bulk na `nusb` (`UsbBackend`, `UsbBulk`, `UsbTransport`, `TransportSpec::Usb`), rodziny `USB_FAMILIES` (CH341 K40/Moshi, galvo BJJCZ), stan dostępu (WinUSB/brak/sterownik producenta na Windows, uprawnienia na Linuksie), urządzenie fikcyjne `FakeUsb` (`BURNCRAFT_FAKE_USB=1`);
  - transport UDP (`UdpTransport`, `TransportSpec::Udp` z portem lokalnym);
  - kreator: `machine.find` z polem `usb`, polecenie `machine.usb_guide` (Zadig + WinUSB, alternatywa CH341 DLL; reguły udev), sekcja „Urządzenia USB” i przewodnik w oknie dialogowym okna Laser (pl/en);
  - `packaging/linux/99-burncraft.rules`, `burncraft-cli udev`, `udevadm verify` w CI;
  - procedury `docs/procedures/M3-gui-checks.md` (zrzuty `img/m3/`, generator `web/e2e/procedures-m3.spec.ts`).

## ADR-y etapu

- **ADR 0013** — źródła wiedzy o protokołach (REQ-00-018): wolno korzystać z publicznej dokumentacji, opisów protokołów i z otwartych implementacji czytanych jako źródło wiedzy (np. MeerK40t — MIT, K40 Whisperer — licencja do sprawdzenia przy pierwszym użyciu), bez kopiowania kodu, struktur ani tekstów; każdy fakt o protokole ma w kodzie odnośnik do źródła, a lista źródeł jest w `THIRD_PARTY.md`; szczegóły niepotwierdzone na sprzęcie dostają `// UNVERIFIED:` jak w M2.
- **ADR 0014** — sterownik Lihuiyu i Moshi (ADR o Ruidzie nie powstaje, DEV-007): ramka EGV, CRC-8, maszyna stanów potwierdzeń, tablice kodowania prędkości per wariant płyty, raster „step mode”, dither zamiast mocy, emulator.
- Transport USB (`nusb`, kreator sterownika na Windows, reguły udev) — część ADR 0013 albo osobny ADR przy M3.1.

## Nowe zależności

- **`nusb`** — transport USB bulk (czysty Rust, MIT/Apache-2.0, WinUSB na Windows, usbfs na Linuksie, IOKit na macOS); decyzja zleceniodawcy 2026-11-07.
- UDP — biblioteka standardowa (bez nowych zależności).
- CH341 DLL (REQ-13-040) — nie jest zależnością budowania: program ładuje sterownik producenta zainstalowany przez użytkownika (`libloading`, MIT, jeśli potrzebne).

## Decyzje zleceniodawcy (2026-11-07)

- **Sprzęt**: zleceniodawca ma K40 z płytą M2 Nano — test sprzętowy K40 w M3.4 (REQ-13-030, 040, 049, REQ-19-016). Ruidy RDC6445G nie ma (DEV-005).
- **Ruida**: „Rezygnujemy z Ruidy.” — Ruida wyłączona z projektu (DEV-007, zastępuje DEV-005).
- **Biblioteka USB**: `nusb` (czysty Rust, MIT/Apache-2.0) — zgoda na pobranie z crates.io przy M3.1.
- **Kreator sterownika na Windows (REQ-13-020)**: wykrywa brak sterownika WinUSB i prowadzi krok po kroku przez instalację zewnętrznym narzędziem (np. Zadig), którego BurnCraft nie dołącza; alternatywą jest oryginalny sterownik CH341 DLL (REQ-13-040).
- **Źródła wiedzy (REQ-00-018, ADR 0013)**: otwarte projekty (MeerK40t, K40 Whisperer) tylko jako źródło wiedzy o protokołach, bez kopiowania kodu; odnośniki do źródeł w kodzie i `THIRD_PARTY.md`. Uwaga z 2026-11-07: pytanie do zleceniodawcy podawało licencję GPL dla obu projektów; MeerK40t ma licencję MIT, licencję K40 Whisperer sprawdzimy przy pierwszym użyciu (decyzja „tylko wiedza” nie zależy od licencji).
- **Pakiety `.lbzip` (REQ-02-095)**: bez plików przykładowych — przeniesione do M4 („Nie, przenieś do M4”, DEV-006).

## Ryzyka

- Kodowanie prędkości Lihuiyu różni się między płytami i firmware. Emulatory sprawdzają spójność kodowania, ale nie zgodność z konkretną płytą — to potwierdza dopiero sprzęt. Każdy niepotwierdzony szczegół dostaje `// UNVERIFIED:` ze źródłem.
- USB na macOS i Linuksie wymaga uprawnień (udev, odłączenie sterownika jądra CH341). Testy CI używają atrap; prawdziwy port tylko w testach sprzętowych.
- Limit minut GitHub Actions nie dotyczy już repozytorium publicznego (standardowe runnery są bezpłatne); zasada „jeden push na przyrost” i pełna weryfikacja lokalna obowiązują dalej.
