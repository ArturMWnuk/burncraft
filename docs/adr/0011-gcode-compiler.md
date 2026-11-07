# ADR 0011 — Kompilator G-code (`bc-drivers::gcode`)

- Status: przyjęta (M2.2, 2026-11-04)
- Wymagania: REQ-13-015, 016, 085…090, 105…108; REQ-02-089, 090; REQ-10-006, 019; REQ-15-057; REQ-19-006

## Kontekst

Planer (ADR 0009) daje zadanie `bc_plan::Job`: operacje z ustawieniami (M3/M4, air, Z, pauza, opóźnienie) i odcinki z mocą i prędkością. Sterowniki (M2.3, ADR 0010) mają metodę `compile(Job)` do eksportu offline, a ten sam strumień poleceń ma iść do maszyny przez dowolny transport. Testy golden porównują wynik bajt w bajt (REQ-19-006), więc kompilator musi być deterministyczny.

## Decyzja

### 1. Miejsce i interfejs

- Kompilator jest w `bc-drivers` (moduł `gcode`) i zależy tylko od `bc-doc`, `bc-geom` i `bc-plan`. Nie zna transportów ani wejścia/wyjścia: `compile(driver, &Job, &Context) -> MachineFile` jest czystą funkcją (REQ-13-016). `MachineFile` zawiera bajty, liczbę wierszy, SHA-256, czas szacowany i ostrzeżenia. Plik i strumień do maszyny to te same wiersze.
- `Context` składa się z dokumentu: nazwa, `GcodeSettings`, `StartSettings`, makra warstw, kinematyka (do M73). Sterownik z M2.3 wywoła ten sam kompilator w `Driver::compile` i przy wysyłce.
- Sterowniki w M2.2: `grbl` (tryb laserowy `$32=1`) i `gcode` (ogólny CNC: Mach3/Mach4, UCCNC, Masso, LinuxCNC). Inne nazwy kończą się błędem `unsupported_driver` (Ruida, Lihuiyu w M3, galvo w M5).

### 2. Dialekty

| | `grbl` | `gcode` (ogólny) |
|---|---|---|
| wiązka | M4 (moc dynamiczna) albo M3 wg warstwy; G0 wyłącza wiązkę w trybie laserowym | zawsze M3; jawne M5 przed przejazdem i M3 przed cięciem (M4 w CNC to obroty przeciwne) |
| moc | `S` = moc % · `s_max` / 100 (`$30`, domyślnie 1000) | tak samo |
| odcinki bez wiązki | `G1 … S0` (overscan, perforacja) | `G1 … S0` |
| postój z wiązką | `M3 S…`, `G4 P…` | tak samo |

### 3. Ustawienia w dokumencie (schemat 5)

- `Document.gcode: GcodeSettings`: szablony nagłówka i stopki (REQ-13-105) z polami `{name}`, `{driver}`, `{time}`, `{time_s}`, `{units}`, `{s_max}`, `{passes}`, `{x0}`, `{y0}`, `{x1}`, `{y1}`; blok jednostek (REQ-13-106: pusty = `G90` + `G21`/`G20` z `units`, inaczej wstawiany dosłownie); makra zadania `job_pre`/`job_post` (REQ-13-108); `s_max`, `decimals`, `compress`, `progress_m73`, kod air assist (`M7`/`M8`), drugi laser (wybór narzędzia i offset), oś Z (`use_z`, `z_lift`), powrót do startu.
- `LayerParams.gcode_pre`/`gcode_post` — makra warstwy (REQ-13-107) z polami `{layer}`, `{label}`, `{pass}`, `{speed}`, `{power}` (S), `{power_pct}`. Makro pre jest przed pierwszą operacją warstwy, post po ostatniej (operacje warstwy są kolejne).
- `Document.start: StartSettings` (REQ-10-006): „Start From” (`absolute`, `current`, `user_origin`) i 9-punktowy origin zadania. Do czasu profili urządzeń (M2.3) te ustawienia są w dokumencie, tak jak kinematyka.
- Migracja 4 → 5 dodaje wartości domyślne.

### 4. Układ współrzędnych i start

- Wyjście jest w układzie dokumentu (mm, Y w górę, ADR 0003). Narożnik bazowania maszyny i kierunki osi należą do profilu urządzenia (M2.3, ADR 0010).
- `absolute`: współrzędne obszaru roboczego, `G90`.
- `user_origin`: współrzędne względem punktu origin zadania (9 punktów ramki cięcia), `G90`, w układzie roboczym ustawionym przez „ustaw zero” (G54, M2.3).
- `current`: współrzędne względem origin zadania w trybie przyrostowym `G91`. Plik nie zmienia stanu maszyny (bez `G92`), a przyrosty liczone są z zaokrąglonych pozycji bezwzględnych, więc błąd nie narasta. Stopka z `M2` przywraca `G90`.

### 5. Kompresja (REQ-13-086)

Gdy `compress = true`: pomijane są powtórzone słowa modalne (`G0`/`G1`, `F`, `S`), niezmienione osie i zbędne zera (`10.500` → `10.5`, `0.5` → `.5`). Gdy `false`: każdy ruch ma pełne słowa z ustaloną liczbą miejsc po przecinku. Kompilator nie łączy odcinków — liczba ruchów odpowiada zadaniu.

### 6. Rastry (REQ-13-087)

Raster to operacja `OpKind::Raster`, w której każdy odcinek `Cut` niesie moc piksela (przebiegi pikseli o tej samej mocy są scalone). `bc_plan::raster::bitmap_op` buduje taką operację z bitmapy w odcieniach szarości (obrazy w dokumencie dojdą w M4). Kompilator wypisuje `S` przy każdej zmianie mocy.

### 7. Postęp, dwa lasery, air assist

- M73 (REQ-13-088): `M73 P<procent> R<minuty>` na starcie, przy każdej zmianie pełnego procentu (czas z modelu `bc_plan::estimate`) i na końcu. Domyślnie wyłączone dla `grbl`, bo standardowy GRBL 1.1 zgłasza `error:20` na nieznane M-kody.
- Dwa lasery (REQ-13-089): operacja z `power2_max` przy włączonym drugim laserze jest wypisywana dwa razy: laser 1 po słowie wyboru narzędzia (`T0`), laser 2 po `T1`, z offsetem lasera 2 i mocą przeskalowaną `power2_max / power_max`. Bez włączonego drugiego lasera kompilator ostrzega `laser2_disabled`.
- Air assist (REQ-13-090): `M8` (albo `M7`) przed operacją z air, `M9` przy zmianie i na końcu.
- Przed pauzą (`M0`) i opóźnieniem (`G4`) kompilator wyłącza wiązkę (`M5`), bo w M3 wiązka świeci także w postoju.

### 8. Import G-code (REQ-02-089)

Interpreter `bc_drivers::gcode::sim` wykonuje program (G0/G1/G4/G20/G21/G90/G91/G92, M0/M2/M3/M4/M5/M7/M8/M9/M73, S/F/T, komentarze `;` i `( )`) i daje ruchy ze stanem wiązki. Import zamienia je na `Job` (podgląd ścieżek, czas, długości) i opcjonalnie na ścieżki w dokumencie. Treść programu zostaje w sesji jako program do uruchomienia; wysyłka do urządzenia to M2.3. Ten sam interpreter służy w testach M2.2 jako emulator semantyki G-code; pełny emulator GRBL (bufor, protokół, `?`, alarmy) powstaje w M2.3.

### 9. Determinizm i golden

Wyjście zależy tylko od zadania i ustawień: stała kolejność, liczby zaokrąglane do `decimals` (domyślnie 3) bez `-0`, komentarze w ASCII (polskie znaki transliterowane), bez daty i wersji programu. Testy golden (`crates/bc-drivers/tests/golden/`) porównują wynik projektu referencyjnego z zatwierdzonymi plikami. Plik golden aktualizuje się świadomie: `BC_UPDATE_GOLDEN=1 cargo test -p bc-drivers --test golden`.

## Konsekwencje

- Polecenia `gcode.compile`, `gcode.export`, `gcode.import`, `gcode.settings`, `job.start_settings`; `file.import` i `file.export` przekazują pliki `.nc`, `.gc`, `.gcode` do poleceń G-code. Narzędzie MCP `export_machine_file {driver, path}`.
- Szczegóły zależne od firmware (M73 w GRBL, wybór narzędzia dla dwóch laserów, wiązka w postoju przy M3) mają w kodzie `// UNVERIFIED:` ze źródłem i trafią do testu sprzętowego na koniec M2.
