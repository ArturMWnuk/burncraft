# ADR 0010 — Sterowniki, transporty i emulator GRBL (`bc-drivers`)

- Status: przyjęta (M2.3, 2026-11-04)
- Wymagania: REQ-13-001…013, 017, 018, 022, 024, 028, 029, 050, 053…070, 072…084, 191, 192, 196, 198; REQ-19-007, 012; REQ-16-004, 005; REQ-10-001…004, 007, 008, 010, 012…016, 018, 020, 021; REQ-11-028; REQ-14-016, 017; REQ-01-040, 041

## Kontekst

M2.2 dał kompilator G-code niezależny od transportu (ADR 0011). M2.3 łączy BurnCraft z maszyną: trait `Driver` z §13.1, transporty (serial, TCP, WebSocket), rodzina GRBL z odmianami, emulator do testów i urządzenie fikcyjne. Rejestr poleceń (`bc-commands`) jest synchroniczny, a `stop()` musi działać zawsze i natychmiast.

## Decyzja

### 1. Model wykonania: wątek połączenia zamiast async

- Każde połączenie ma **jeden wątek wejścia/wyjścia**, który jest właścicielem transportu i silnikiem protokołu: odczyt z krótkim limitem czasu, zapis z kolejki, zapytania `?` w tle (co 200 ms), strumieniowanie zadania, kolejka zadań.
- Metody `Driver` są synchroniczne, ale nie blokują na czas ruchu: polecenia (home, jog, ustawienia) czekają tylko na odpowiedź `ok`/`error`, a `send_job` zwraca uchwyt zadania od razu. Postęp i stan czyta się z migawki `MachineState`.
- Bajty czasu rzeczywistego (`!`, `~`, `0x18`, nadpisania, `0x85`) omijają kolejkę wierszy. `stop()` wysyła wstrzymanie i soft reset przed wszystkim innym, także w trakcie strumieniowania.
- Uzasadnienie: §13.1 opisuje operacje jako asynchroniczne względem GUI, co ten model spełnia. Tokio nie jest potrzebne w rdzeniu (rejestr, CLI i MCP stdio są synchroniczne), a jeden wątek na połączenie upraszcza porządek bajtów i testy deterministyczne.

### 2. Transporty

Trait `Transport` (zapis bajtów, odczyt z limitem czasu, opis). Implementacje:

| Transport | Implementacja | Uwagi |
|---|---|---|
| Serial / USB-CDC | crate `serialport` | lista portów z VID/PID: FTDI `0403`, CH340 `1A86`, CP210x `10C4`; na Linuksie wymaga `libudev-dev` |
| TCP (Telnet 23) | `std::net::TcpStream` | ESP3D, FluidNC Telnet, mosty Wi-Fi |
| WebSocket | `tungstenite` (klient synchroniczny) | FluidNC/ESP3D; ramki tekstowe i binarne |
| Emulator | w procesie | urządzenie fikcyjne i testy |

### 3. GRBL

- **Strumieniowanie**: character counting (bufor RX 128 B, wysyłane jest tylko tyle, ile mieści się w 127 B niepotwierdzonych) albo send-response (jeden wiersz na `ok`). Bufor wstępny: całe zadanie jest skompilowane przed startem, a pierwsza porcja wypełnia bufor RX od razu (REQ-16-004). Monitor niedopełnienia: maszyna przechodzi w `Idle`, a do wysłania zostały wiersze → ostrzeżenie `buffer_underrun` w stanie zadania (REQ-16-005).
- **Status**: `<Stan|MPos|WPos|WCO|Bf|FS|Ov|Pn>` (1.1) i `<Stan,MPos,WPos>` (0.9). Pin `D` = drzwi/pokrywa. Brak danych o chłodzeniu i temperaturze w GRBL = `None`.
- **Polecenia**: `$H`, `$X`, `$J=` (0.9: `G91 G0`), `G10 L20 P1` (zero użytkownika), `G38.2` (sonda, `[PRB:…]`), `$$`/`$N=v` z kopią w katalogu preferencji przed zapisem i diffem, opisy i zakresy ustawień, kody błędów i alarmów z opisami po polsku, kreator `$32`/`$30`.
- **Nadpisania**: feed i moc przez bajty czasu rzeczywistego 1.1 (`0x90…0x94`, `0x99…0x9D`).
- **Odmiany** (GRBL 0.9, 1.1f/h, grblHAL, FluidNC, GRBL-LPC, GRBL-ESP32, GRBL-M3 i producenci): tabela profili z rozpoznaniem po powitaniu, domyślnym `s_max`, prędkością portu i obsługą funkcji. Szczegóły producentów bez sprzętu mają `// UNVERIFIED:` w tabeli.
- **Karta SD** (FluidNC): `$SD/List`, `$SD/Run=`, wysyłka pliku XMODEM-CRC po `$Xmodem/Receive=` (UNVERIFIED). Inne odmiany: brak w capabilities.

### 4. Emulator GRBL

Własny emulator w Rust (`bc_drivers::emu`): bufor RX 128 B (przepełnienie = błąd emulatora wykrywany w testach), bufor planera 16 bloków, wykonanie ruchów w czasie wirtualnym (z mnożnikiem albo natychmiast), stany Idle/Run/Hold/Jog/Alarm/Home/Door, `$$`, sonda, karta SD, nadpisania, zapis toru z wiązką. Wynik renderuje się do PNG (`bc-sim`) i porównuje z podglądem planera (REQ-19-012). Emulator jest urządzeniem fikcyjnym w GUI i CLI (`burncraft-cli emulator --tcp`). W CI na Linuksie ten sam G-code przechodzi przez grbl-sim (budowany ze źródeł GRBL w CI, poza repozytorium) i przez emulator; test porównuje odpowiedzi i pozycję końcową (REQ-19-007).

### 5. Urządzenia

- **Profil** (`DeviceProfile`): nazwa, sterownik, odmiana, transport, obszar roboczy, prędkość portu, tryb strumieniowania, `s_max`, offset czerwonego lasera i polecenia jego włączenia. Profile są w `devices.json` w katalogu preferencji; urządzenie fikcyjne „Symulator” jest zawsze dostępne.
- **Menedżer** (`Machines`) trzyma profile i połączenia; jest współdzielony przez zakładki dokumentów. Kilka urządzeń może być połączonych naraz (REQ-13-028); polecenia przyjmują `device` albo działają na urządzeniu wybranym w sesji.
- **Kreator „Znajdź mój laser”**: lista portów (z VID/PID) i próba połączenia na znanych prędkościach z rozpoznaniem powitania GRBL.

### 6. Start zadania

Ustawienia startu z dokumentu (ADR 0011 §4) decydują o układzie współrzędnych pliku. Ramka (prostokąt albo kontur — otoczka wypukła) to przejazd bez wiązki, a z czerwonym laserem — z korektą offsetu i poleceniami włączenia lasera pozycjonującego. Kolejka zadań i powtarzanie N razy z opóźnieniem wykonuje wątek połączenia.

## Konsekwencje

- Polecenia `machine.*` (lista, profile, połączenie, status, ustawienia, kreator) i `device.*` (ruch: poziom `motion`, test wiązki: `fire`, zapis ustawień: `config`), `job.*` (start: `fire`, pauza/wznowienie: `motion`, stop: zawsze dozwolony), `macro.*`.
- Narzędzia MCP dla urządzeń (M2.4) wywołają te same polecenia.
- CI na Linuksie instaluje `libudev-dev` (serialport).
