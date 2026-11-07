# ADR 0013 — Źródła wiedzy o protokołach, transporty USB i UDP (M3.1)

Status: przyjęty 2026-11-07 · Etap: M3.1 · Wymagania: REQ-00-018, REQ-13-019, 020, 021, 023, 193

## Kontekst

Etap M3 wprowadza sterowniki K40 Lihuiyu, Moshi i Ruida. Ich protokoły nie mają oficjalnej, publicznej dokumentacji producentów. REQ-00-018 wymaga, żeby wiedza o protokołach pochodziła wyłącznie z publicznych, otwartych źródeł, z poszanowaniem licencji. Zasada clean-room (REQ-00-014) zabrania kopiowania kodu i zasobów innych programów do laserów.

K40 (płyty Lihuiyu, Moshi) łączy się przez układ CH341 w trybie równoległym (USB `1a86:5512`), bez portu szeregowego, więc potrzebny jest transport USB bulk. Ruida używa UDP (porty 50200/40200).

## Decyzje

### 1. Źródła wiedzy o protokołach (REQ-00-018)

Decyzja zleceniodawcy (2026-11-07, „Tak, tylko jako wiedza”):

- Dozwolone źródła: publiczne opisy protokołów, dokumentacja, wpisy na forach i wiki, specyfikacja BurnCraft (§13.3) oraz otwarte projekty czytane **wyłącznie jako źródło wiedzy** o protokole (np. MeerK40t, K40 Whisperer, opisy protokołu Balor).
- Zakaz kopiowania kodu, struktur danych, tablic w postaci źródłowej, komentarzy i tekstów z tych projektów. Kod BurnCraft powstaje od zera na podstawie opisu zachowania (bajty na łączu, znaczenie pól).
- Każdy fakt o protokole ma w kodzie komentarz ze źródłem. Fakt niepotwierdzony na sprzęcie dostaje `// UNVERIFIED:` (jak w M2) i trafia do checklisty testu sprzętowego.
- Lista źródeł z licencjami jest w `THIRD_PARTY.md` (sekcja „Źródła wiedzy o protokołach”). Licencję źródła sprawdza się przy jego pierwszym użyciu i zapisuje w tej tabeli.

### 2. Transport USB bulk (REQ-13-019)

- Biblioteka **`nusb` 0.2** (czysty Rust, MIT/Apache-2.0; decyzja zleceniodawcy 2026-11-07): WinUSB na Windows, usbfs na Linuksie, IOKit na macOS. Bez kodu C i bez dołączania libusb.
- `bc_drivers::usb`: `UsbBackend` (lista urządzeń, otwarcie) i `UsbBulk` (transfery bulk z limitem czasu) oddzielają transport od systemu. `UsbTransport` implementuje trait `Transport` z ADR 0010, więc sterowniki M3 nie zależą od biblioteki USB.
- `TransportSpec::Usb { vid, pid, serial }` w profilu urządzenia. Interfejs i punkty końcowe pochodzą z tabeli rodzin `USB_FAMILIES` (CH341 K40/Moshi, galvo BJJCZ dla M5).
- Urządzenie fikcyjne `FakeUsb` (pętla zwrotna, zadany stan dostępu) zastępuje sprzęt w testach. Host uruchomiony z `BURNCRAFT_FAKE_USB=1` pokazuje urządzenia fikcyjne w GUI (testy E2E, zrzuty do procedur).

### 3. Stan dostępu i przewodnik (REQ-13-020, REQ-13-021)

- Windows: stan z nazwy sterownika urządzenia (`nusb`): WinUSB = gotowe, brak = „brak sterownika”, inny = „sterownik producenta”.
- Linux i macOS: znane rodziny laserów są sprawdzane próbą otwarcia: odmowa uprawnień oznacza brak reguł udev, zajęte — inny program.
- Przewodnik (`usb::driver_guide`, polecenie `machine.usb_guide`) zależy od stanu urządzenia:
  - Windows: instalacja WinUSB narzędziem Zadig, którego BurnCraft nie dołącza (decyzja zleceniodawcy); dla K40 alternatywą jest oryginalny sterownik CH341 (REQ-13-040, M3.3);
  - Linux: reguły udev.
- Reguły udev generuje `usb::udev_rules()` z tabeli rodzin. Plik `packaging/linux/99-burncraft.rules` jest ich kopią (test sprawdza zgodność), a `burncraft-cli udev` je wypisuje. Dostęp daje `TAG+="uaccess"` (zalogowany użytkownik) i `MODE="0660"`.
- Kroki przewodnika w GUI pochodzą z tłumaczeń Fluent (pl/en) według rodzaju (`kind`). Tekst polski z rdzenia służy CLI i MCP.

### 4. Transport UDP (REQ-13-023)

- `TransportSpec::Udp { host, port, local_port }`, `UdpTransport`: każdy zapis to jeden datagram, a odczyt zwraca bajty datagramów od tego samego adresu i portu (inne są pomijane).
- Port lokalny jest konfigurowalny, bo Ruida wysyła odpowiedzi na port 40200.
- Na Windows błąd ICMP „port nieosiągalny” (`ConnectionReset`) przy odczycie jest traktowany jak brak danych.
- Ramki, sumy kontrolne i potwierdzenia Ruidy należą do sterownika (ADR 0014), nie do transportu.

### 5. Kreator „Znajdź mój laser” (REQ-13-193)

`machine.find` zwraca, obok portów szeregowych, pole `usb`: urządzenia znanych rodzin z ich stanem i przewodnikiem. Inne urządzenia USB (mysz, klawiatura) nie są pokazywane. W GUI okno Laser pokazuje je w sekcji „Urządzenia USB” z przyciskiem „Jak podłączyć…”, który otwiera przewodnik w oknie dialogowym. Wykrywanie Ruidy przez UDP broadcast (REQ-13-194) jest w M3.2.

## Konsekwencje

- Sterowniki M3.2/M3.3 dostają gotowe transporty i testują się na emulatorach przez te same `Transport`.
- Punkty końcowe CH341 i BJJCZ są `UNVERIFIED` do testu sprzętowego K40 (M3.4) i etapu galvo (M5).
- Prawdziwe urządzenia USB nie są testowane w CI. Testy używają `FakeUsb`, a sprzęt sprawdza checklista K40.
