# Serwer MCP BurnCraft

Wymagania: REQ-18-014…017, sekcja 15. Stan: M2.4 — transporty stdio i Streamable HTTP (127.0.0.1), narzędzia projektowe i urządzeń, zasoby z subskrypcjami, prompt `prepare_cut_file`, postęp i anulowanie, polityka read/design/motion (potwierdzenie przez elicitation), nadrzędny `stop_job`, watchdog joga, dziennik audytu.

## Uruchomienie

```bash
cargo build --release -p bc-cli
```
```bash
burncraft-cli mcp
```

Opcje: `--open plik.bcraft` (otwórz dokument na starcie), `--connect [PORT]` (żywe okno GUI), `--audit-log ŚCIEŻKA`, `--no-audit-log`. Poziom logów: zmienna `BURNCRAFT_LOG` (`error`…`trace`, domyślnie `info`). Logi trafiają zawsze na stderr.

### Praca na żywym oknie (`--connect`)

Agent może pracować na dokumencie otwartym w GUI, a użytkownik widzi zmiany na bieżąco (ADR 0007):

```bash
burncraft-cli serve
```
```bash
burncraft-cli mcp --connect 7984
```

- `serve` uruchamia host GUI na `127.0.0.1:7984` i wypisuje adres frontendu do otwarcia w przeglądarce (okno Tauri `burncraft --control` — M1.4c).
- `mcp --connect` jest przezroczystym pośrednikiem stdio ↔ kanał sterowania. Polityka bezpieczeństwa i dziennik audytu działają w hoście tak samo jak w trybie bezgłowym.
- Token kanału host zapisuje w `control-<port>.token` w katalogu danych użytkownika (Windows: `%LOCALAPPDATA%\BurnCraft`); CLI czyta go stamtąd albo z `--token-file` / zmiennej `BURNCRAFT_TOKEN`.
- Gdy nic nie słucha na porcie, CLI uruchamia sesję bezgłową. Gdy host odrzuci token, CLI kończy się błędem — nie przechodzi po cichu na osobny dokument.

## Konfiguracja klientów

**Claude Code** — `.mcp.json` w katalogu projektu (przykład: [`examples/mcp.json`](../examples/mcp.json)):

```json
{
  "mcpServers": {
    "burncraft": {
      "command": "C:/Users/<ty>/Documents/Claude/BurnCraft/target/release/burncraft-cli.exe",
      "args": ["mcp"]
    }
  }
}
```

**Claude Desktop** — `claude_desktop_config.json` (Windows: `%APPDATA%\Claude\`, macOS: `~/Library/Application Support/Claude/`), przykład: [`examples/claude_desktop_config.json`](../examples/claude_desktop_config.json). Format jest ten sam (`mcpServers` → `command`, `args`).

## Konwencje

- Jednostki: mm; oś Y w górę; (0,0) = lewy dolny róg obszaru roboczego; kąty w stopniach CCW (ADR 0003).
- Polecenia działają na `ids` albo na bieżącym zaznaczeniu. Nowy obiekt zostaje zaznaczony.
- Edycja węzłów: `node.list {id}` zwraca kontury z węzłami (`p`, `hin`, `hout`, `smooth`). Węzły są numerowane kolejno we wszystkich konturach, a segment `k` zaczyna się w węźle `k`. Polecenia `node.move`, `node.add`, `node.delete`, `node.break`, `node.join`, `node.smooth`, `node.corner`, `node.segment`, `node.align`, `node.set_handle` i `path.cut` przyjmują współrzędne świata (mm), także dla obiektów w grupach.
- Przycinanie i deformacje: `path.trim {id, at}` usuwa fragment między najbliższymi przecięciami z innymi obiektami (prowadnice nie tną). `path.erase {points, width, ids?}` wymazuje części ścieżek pod pociągnięciem; bez `ids` dotyczy obiektów pod pociągnięciem. `path.warp {cols, rows, points, bounds?, ids?}` to wypaczanie siatką biliniową: 1×1 to 4 punkty, większa siatka to deformacja siatką, 1 wiersz to obwiednia; punkty podaje się wierszami od dołu. Prowadnicę tworzy `shape.guide {p0, p1}` (obiekt pomocniczy na warstwie T2).
- Drzewo obiektów: `object.arrange {ids?, parent?, index?}` przenosi obiekty do grupy (`parent`) albo na poziom główny (`parent: null`) na pozycję `index` w kolejności rysowania; położenie w świecie się nie zmienia.
- Skróty klawiszowe GUI: `shortcuts.list` zwraca wszystkie polecenia ze skrótem domyślnym i obowiązującym. `shortcuts.set {id, shortcut, force?}` i `shortcuts.reset {id?}` mają poziom `config`. Skrót zajęty przez inne polecenie daje błąd `shortcut_conflict`, a `force: true` przenosi go na nowe polecenie.
- Parametry kształtów: `shape.set_params {id, params}` zmienia pola kształtu parametrycznego bez konwersji na ścieżkę, np. `{"radius": 3}` dla prostokąta, `{"sides": 8}` dla wielokąta, `{"points": 7, "r_middle": 7.5}` dla gwiazdy. Pole, którego kształt nie ma, albo wartość poza zakresem daje `invalid_params`.
- Historia: `edit.history_preview {offset, max_size?}` zwraca miniaturę PNG (base64) stanu odległego o `offset` kroków (−k = po k cofnięciach, +k = po k ponowieniach) z etykietą kroku i liczbą obiektów. Niczego nie zmienia.
- Notatki z obrazami: `document.add_note_image {data_base64, name?}` dodaje PNG/JPEG (do 10 MB) i dopisuje do notatek odwołanie `![](notes/<nazwa>)`. `document.note_image {name}` zwraca obraz, a `document.remove_note_image {name}` usuwa go razem z odwołaniem. Obrazy są zapisywane w `.bcraft` jako `notes/<nazwa>`.
- Błąd polecenia to wynik z `isError: true` i `{code, message}` (`invalid_params`, `not_found`, `locked`, `no_device`, `confirmation_required`, …). Błąd protokołu (nieznane narzędzie, zły JSON) to błąd JSON-RPC.

## Narzędzia

| Narzędzie | Opis |
|---|---|
| `list_commands {filter?}` | lista poleceń rejestru |
| `describe_command {id}` | schemat parametrów/wyniku, flagi, skrót, wymagania |
| `execute {id, params?}` | wykonanie polecenia |
| `batch {steps}` | wsad, atomowo, jeden krok historii |
| `undo`, `redo` | historia |
| `inspect_document {detail?}` | warstwy, obiekty, zaznaczenie, statystyki |
| `render_preview {mode?, source?, bbox?, max_size?, show_workspace?}` | PNG: `design` (projekt w kolorach warstw), `toolpath` (ścieżki zadania z planera: cięcie w kolorach warstw, przejazdy szare, ruch bez wiązki jasnoszary; warstwy z wyłączonym Show pominięte; `source: "gcode"` — program wczytany przez `gcode.import`); `burn_sim` od M4 |
| `plan_job {selection_only?, selection_origin?, power_limit?, rate_per_hour?, threads?}` | = `plan.job`: czas, długości, przebiegi, operacje, ostrzeżenia, koszt, hash; `power_limit` i `rate_per_hour` domyślnie z preferencji `limits.max_power` i `costs.machine_rate_per_hour` |
| `export_machine_file {driver, path, selection_only?}` | = `gcode.export`: plan + kompilacja + zapis pliku dla sterownika `grbl` (G-code GRBL, tryb laserowy) albo `gcode` (ogólny CNC); inne sterowniki → błąd `unsupported_driver` (Ruida/Lihuiyu od M3, galvo od M5) |
| `screenshot` | zrzut obszaru roboczego okna (PNG) przy `--connect`; bez okna błąd `no_window` |
| `query_objects {layer?, type?, bbox?, text?}` | wyszukiwanie obiektów (tekst: nazwa, notatka, tagi) |
| `get_object {id}` | pełne dane obiektu + raport (bbox, długość) |
| `draw_shape {type, params, layer?}` | `rect`, `ellipse`, `polygon`, `star` (z `r_middle` = podwójna), `arc` (środek+kąty albo `through`) |
| `draw_path {d? \| points?, closed?, layer?}` | ścieżka SVG `d` albo polilinia |
| `transform` | = `object.transform` |
| `boolean {ids?, op}` | `union`, `difference` (pierwszy minus reszta), `intersection`, `xor`, `cut` |
| `offset {ids?, distance, direction?, corners?, delete_original?}` | `outward`/`inward`/`both`; `sharp`/`round`/`bevel` |
| `array_grid`, `array_circular` | = `array.grid`, `array.circular` |
| `align {ids?, mode, to?}`, `distribute {ids?, axis, by?}` | wyrównanie i rozkład |
| `set_layer {color, settings}`, `assign_layer {ids?, color}` | `color` = indeks warstwy albo `#rrggbb` |
| `open_file {path}` | `.bcraft`, `.bcraft-template`, `.lbrn2` (jako nowy dokument) |
| `save_file {path?}` | zapis `.bcraft` |
| `import_file {path, options?}` | SVG (auto-DPI), DXF R12–2018, HPGL/PLT, `.lbrn2`, G-code `.nc`/`.gc`/`.gcode` (jak `gcode.import`); `options`: `layer_by` (`color`/`name`/`single` + `layer`), `dpi`, `ignore_text`, `units`, `group` |
| `export_file {path, format?, ids?, include_helpers?}` | `svg` (mm), `dxf` (AC1021), `dxf-r12` (AC1009), `hpgl`; `.gcode`/`.nc`/`.gc` albo `format: "gcode"` — G-code GRBL (jak `gcode.export`) |

Narzędzia-skróty mają schematy wejścia i wyjścia identyczne z poleceniami rejestru (`describe_command`). Pełna lista poleceń: `list_commands`. M1.1 dodaje m.in. `path.convert`, `path.reverse`, `path.set_start`, `path.break_apart`, `path.close`, `path.join`, `path.remove_duplicates`, `path.reduce_nodes`, `path.convex_hull`, `path.chamfer`, `path.fillet`, `object.order`, `object.equalize`, `measure.distance`, `measure.angle`, `layer.add`.

M1.2 (przez `execute`):
- **zakładki**: `doc.new`, `doc.list`, `doc.switch`, `doc.close` (z niezapisanymi zmianami tylko z `force`). Wszystkie narzędzia i zasoby działają na aktywnej zakładce.
- **schowek** między zakładkami: `edit.copy`, `edit.cut`, `edit.paste {offset?}`. Klony stają się samodzielne, brakujące warstwy są dodawane.
- **struktura**: `array.virtual {ids?, layout}` (dynamiczna tablica `grid`/`circular`), `array.set_layout`, `array.bake`, `object.clone {ids?, offset?}`, `object.unlink`, `shape.guide {p0, p1}` (linia pomocnicza na T2), `object.set {helper}`.
- **pliki**: `file.save_template {path: *.bcraft-template}` (otwarcie szablonu tworzy nowy dokument), `file.recover_list`, `file.recover {path}`.
- **preferencje**: `prefs.get`, `prefs.backups`, `prefs.backup_diff` (odczyt; różnice kopii względem bieżących preferencji jako lista `{key, backup, current}` z kluczami z kropkami); `prefs.set`, `prefs.restore` mają poziom `config`, więc przez MCP wymagają potwierdzenia. W GUI te same polecenia obsługuje okno Ustawienia → „Kopie preferencji…” (REQ-02-056).

Polecenia rdzenia domykające M1 (przez `execute`; te same wywołuje GUI):
- `shape.tangent_circles {ids? | circles?: [[cx, cy, r] ×3], filter?: all|outer|inner, layer?}` — okręgi styczne do trzech okręgów (problem Apoloniusza, do 8 rozwiązań). `ids` wskazują okręgi (elipsy o równych półosiach, skalowane równomiernie). Wynik: `ids` i `circles` z rodzajem styczności `external`/`encloses`/`inside` dla każdego okręgu danego. `outer` = styczny zewnętrznie do wszystkich, `inner` = obejmujący wszystkie (REQ-04-019).
- `path.remove_overlaps {ids?, tolerance? = 0.01}` — przycina odcinki pokrywające się z wcześniejszymi (w kolejności obiektów, także w obrębie jednej ścieżki), żeby każda linia była cięta raz. Krzywe są usuwane tylko w całości, gdy są identyczne. Obiekty pokryte w całości są usuwane (`removed`). Wynik zawiera też `removed_length` w mm (REQ-04-040).
- `path.smooth {ids?, smoothness? = 1, corner_angle? = 60}` — odcinki stają się gładkimi krzywymi przez te same węzły. Węzły ze zmianą kierunku większą niż `corner_angle`° pozostają narożnikami (REQ-04-043).
- `path.lines_to_arcs {ids?, tolerance? = 0.05}` — ciągi co najmniej 3 odcinków leżących na okręgu (w tolerancji, także strzałka nad odcinkiem) zamienia na łuki ≤ 180°. Wynik: `arcs` (REQ-04-044).
- `path.boolean_preview {ids?}` — **asystent booleanów**: podgląd wyników `union`, `difference` (dla każdego obiektu jako bazy), `intersection`, `xor` i `cut` bez zmiany dokumentu. Każda opcja ma `area`, `pieces`, `d` (SVG) i `apply`. `apply` to gotowe wywołanie `path.boolean` dla tej opcji. Poziom `read` (REQ-04-069).
- `array.along_path {ids?, path, count? | spacing?, rotate? = true}` — kopie rozłożone wzdłuż ścieżki `path`. Na ścieżce otwartej od początku do końca, na zamkniętej równo na obwodzie. Z `rotate` kopie są obracane zgodnie ze styczną (REQ-04-078).
- `path.resize_slots {ids?, from, to, tolerance? = 0, match_tolerance? = 0.05}` — zmiana grubości materiału w kształtach z wpustami i czopami. Wpust (nacięcie krawędzi albo otwór) o szerokości `from` dostaje szerokość `to + tolerance`, symetrycznie. Czop o długości `from` dostaje długość `to`. Rozpoznawane są kształty U z trzech odcinków na ścieżkach złożonych z odcinków. Wynik: `slots`, `tabs` (REQ-04-082).
- `object.dock {ids?, direction: left|right|up|down, gap? = 0}` — przesuwa obiekty, aż ich obrys oprze się o obrys innego widocznego obiektu w pasie ruchu albo o krawędź obszaru roboczego. Zaznaczone obiekty układają się jeden za drugim (REQ-04-091).
- `object.quick_nest {ids?, sheet?: {x, y, w, h}, spacing? = 0, rotate? = true}` — Quick Nest: rozmieszcza obrysy prostokątne na arkuszu (domyślnie obszar roboczy) metodą MaxRects z obrotem o 90°. Nie mieszczące się obiekty zostają na miejscu (`unplaced`). Wynik zawiera `utilization` (REQ-04-095). Prawdziwy nesting kształtów (narzędzie `nest`) przewiduje etap M6.

**Planer** (M2.1, ADR 0009; przez `execute`, te same polecenia wywołują GUI i CLI). Jednostki: mm, mm/s, mm/s², s, % mocy. Zmiany `layer.params` i `plan.settings` są częściowe: podane pola zastępują bieżące, obiekty (np. `tabs`, `lead_in`) łączą się rekurencyjnie, a warianty z polem `kind` (`overscan`) są zastępowane w całości.
- `layer.params {index, params?, lpi?, dpi?}` — parametry cięcia warstwy z §7.2: `power2_max/min` (laser 2), `pass_delay` (s), `pause_message`, `z_offset`, `z_step`, `priority`, `pwm_hz`, `dynamic_power` (M4/M3), `kerf`, `kerf_x`, `kerf_y` (ten sam znak), `fill_kerf`, `tabs {enabled, size, count, spacing, skip_inner}`, `lead_in`/`lead_out {kind: none|line|arc, length, angle}`, `overcut`, `perforation {cut, gap}`, `dot_mode {dwell_ms, spacing}`, `ramp {length, outer_only}`, `corner_ramp`, `bidirectional`, `scan_angle`, `cross_hatch`, `interval` (albo `lpi`/`dpi`), `overscan {kind: off|auto|percent, percent?}`, `scan_offsets [{speed, offset}]`, `fill_grouping: all|groups|each`, `sublayers [{name, output, mode, speed?, power_max?, power_min?, passes?, interval?, scan_angle?}]`. Podstawowe pola (nazwa, tryb, prędkość, moc, przejścia, Output, Show, air assist) zmienia `layer.set`.
- `plan.settings {settings}` — ustawienia planu dokumentu: `cut_last` (grawer najpierw, wewnętrzne przed zewnętrznymi, cięcie na końcu), `inner_first`, `order_by_group`, `optimize_start`, `direction: auto|cw|ccw`, `two_opt`, `avoid_cut_parts`, `remove_overlaps`, `reduce_direction_changes`, `no_go_zones [{name, rect: [x0, y0, x1, y1], travel, beam}]`, `raster_band` (mm, 0 = bez pasów), `kinematics {max_speed_x, max_speed_y, accel_x, accel_y, junction_deviation, travel_speed, jerk_factor}`, `start: [x, y]`.
- `object.tabs {id, tabs: [{at: [x, y], size?}]}` — mostki ręczne w punktach konturu (mm, układ obszaru roboczego); pusta lista usuwa.
- `plan.tabs_wizard {ids?, count? | spacing?, size?, skip_inner? = true, replace? = true}` — kreator mostków: rozmieszcza mostki ręczne na konturach zamkniętych. Wynik: `{objects, tabs}`.
- `plan.job {selection_only?, selection_origin?, power_limit?, rate_per_hour?, threads?}` (poziom `read`, bez zmian w dokumencie) — wynik: `hash` (SHA-256 kanonicznego zadania, deterministyczny), `time {total_s, cut_s, travel_s, delay_s, dwell_s}`, `formatted_time`, `cut_length`, `travel_length`, `idle_length`, `passes`, `layers`, `ops [{layer, label, sublayer, pass, kind, mode, cut_length, time_s, pause}]`, `warnings [{code, message, layer, objects}]`, `bbox`, `origin`, `cost {rate_per_hour, machine_hours, machine_cost}`. Kody ostrzeżeń: `open_shape_in_fill`, `outside_workspace`, `duplicate`, `cut_before_engrave`, `power_above_limit`, `beam_no_go` (cięcie w strefie zakazanej dla wiązki pominięte), `travel_lift` (brak objazdu strefy), `image_mode` (tryb Image od M4). Długie planowanie zgłasza postęp i można je anulować (REQ-16-006).
- `plan.estimate` (te same parametry) — `{total_s, cut_s, travel_s, delay_s, dwell_s, formatted}`. Model: profil trapezowy z look-ahead, osobne prędkości i przyspieszenia osi X/Y, odchylenie na złączu (GRBL `$11`).
- CLI: `burncraft-cli plan <plik.bcraft> [--json] [--power-limit P] [--rate R]` i `burncraft-cli estimate <plik.bcraft> [--json]`.

**G-code** (M2.2, ADR 0011; przez `execute`, te same polecenia wywołują GUI, CLI i narzędzie `export_machine_file`). Kompilator jest niezależny od transportu: plik i strumień do maszyny to te same wiersze. Wynik jest deterministyczny (testy golden bajt w bajt). Sterowniki: `grbl` (GRBL 1.1 i pochodne w trybie laserowym `$32=1`: M4 dla mocy dynamicznej, M3 dla stałej, G0 bez wiązki) i `gcode` (ogólny CNC: zawsze M3, M5 przed przejazdem, komentarze w nawiasach).
- `gcode.compile {driver? = grbl, selection_only?, include_text?}` (poziom `read`) — wynik: `driver`, `extension`, `lines`, `bytes`, `hash` (SHA-256 pliku), `time_s`, `warnings [{code, message}]` (planera i kompilatora: `laser2_disabled`, `z_ignored`, `unknown_placeholder`), `text?`. Długa kompilacja zgłasza postęp i można ją anulować.
- `gcode.export {path, driver? = grbl, selection_only?}` — **Zapisz plik G-code**: planuje, kompiluje i zapisuje plik; wynik jak `gcode.compile` (bez treści) z `path`. W GUI: przycisk „Zapisz G-code…” na pasku głównym.
- `gcode.import {path, to_document? = true, layer? = 0, s_max?, laser_mode? = true}` — import programu `.nc`/`.gc`/`.gcode`: interpreter wykonuje G0/G1/G2/G3 (I/J), G4, G20/G21, G90/G91, G92/G92.1, M0/M1, M2/M30, M3/M4/M5, M7/M8/M9, M73, S, F, T i komentarze `;`/`( )`; nieobsługiwane słowa i polecenia `$…` trafiają do `warnings`. Wynik: `lines`, `moves`, `cut_length`, `travel_length`, `bbox`, `time_s`, `formatted_time`, `pauses`, `tools`, `dynamic_power`, `air_assist`, `ids`, `warnings`. `to_document` dodaje tor z wiązką jako ścieżki na warstwie `layer` (podgląd na kanwie, cofalne). Program zostaje w sesji do uruchomienia na urządzeniu (wysyłka od M2.3) i do podglądu `render_preview {mode: "toolpath", source: "gcode"}`. `file.import` robi to samo dla plików G-code.
- `gcode.program {include_text?}` (poziom `read`) — wczytany program: `loaded`, `path`, `lines`, `moves`, `time_s`, `hash`, `text?`.
- `gcode.settings {settings}` — ustawienia kompilatora dokumentu (zmiana częściowa, zapisywane w `.bcraft`): `header`, `footer` (szablony z polami `{name}`, `{driver}`, `{time}`, `{time_s}`, `{units}`, `{s_max}`, `{passes}`, `{x0}`, `{y0}`, `{x1}`, `{y1}`; nieznane pola zostają bez zmian i dają ostrzeżenie), `units: mm|inch`, `units_block` (pusty = `G90` + `G21`/`G20`, inaczej wstawiany dosłownie), `job_pre`, `job_post` (makra zadania), `s_max` (S dla 100 %, GRBL `$30`, domyślnie 1000), `decimals` (0–6, domyślnie 3), `compress` (pomijanie powtórzonych `G0/G1`, `F`, `S` i niezmienionych osi, `.5` zamiast `0.500`), `progress_m73` (`M73 P<proc.> R<min>`; domyślnie wyłączone, bo GRBL 1.1 zgłasza `error:20`), `air_code: M7|M8`, `laser2 {enabled, select1 = "T0", select2 = "T1", offset: [dx, dy]}`, `use_z`, `z_lift`, `return_to_start`. Makra warstw: `layer.params {index, params: {gcode_pre, gcode_post}}` z polami `{layer}`, `{label}`, `{pass}`, `{speed}` (F), `{power}` (S), `{power_pct}`.
- `job.start_settings {from?, origin?}` — „Start From”: `absolute` (współrzędne obszaru roboczego, `G90`), `current` (od bieżącej pozycji głowicy, `G91`, bez zmiany stanu maszyny), `user_origin` (od zera użytkownika, `G90`); `origin` — 9-punktowy origin ramki zadania: `top_left`, `top`, `top_right`, `left`, `center`, `right`, `bottom_left`, `bottom`, `bottom_right`. Zapisywane w `.bcraft` (schemat 5).
- Dwa lasery: warstwa z `power2_max` przy `laser2.enabled` jest wypalana laserem 1 (`T0`) i laserem 2 (`T1`, z offsetem i mocą `power2_max`). Air assist: `M8`/`M7` przed operacją warstwy z air, `M9` przy zmianie i na końcu. Pauza warstwy (`pause_message`) → `M5`, komentarz i `M0`.
- CLI: `burncraft-cli export <plik.bcraft> <wynik.gcode|.nc|.gc> [--driver grbl|gcode]`.

**Urządzenia** (M2.3, ADR 0010; przez `execute`, te same polecenia wywołują GUI i CLI; narzędzia MCP dla urządzeń — sekcja „Urządzenia i zadania” niżej). Sterownik `grbl` obsługuje GRBL 0.9, 1.1f/h, grblHAL, FluidNC, GRBL-LPC, GRBL-ESP32, GRBL-M3, K40 po modernizacji i odmiany producentów (Ortur, Atomstack, Sculpfun, Creality Falcon, Two Trees, NEJE, Comgrow, Longer, Snapmaker, Algolaser, Wainlux) przez serial/USB-CDC, TCP (Telnet 23) i WebSocket (FluidNC/ESP3D). Urządzenie fikcyjne „Symulator” (emulator GRBL) jest zawsze dostępne. Profile i makra są w `devices.json` i `macros.json` w katalogu preferencji, kopie ustawień maszyny w `machine-backups/`. Polecenia bez `device` działają na urządzeniu wybranym w sesji; kilka urządzeń może być połączonych naraz.
- `machine.list` — profile (`name`, `driver`, `variant`, `transport {kind: serial{port, baud} | tcp{host, port = 23} | websocket{url} | emulator{variant, speed?}}`, `workspace`, `stream: character_counting|send_response`, `s_max`, `pointer_offset`, `pointer_on`, `pointer_off`, `frame_speed`), `connected`, `selected`, `variants`.
- `machine.profile_save {profile}`, `machine.profile_delete {name}` — ręczne tworzenie profilu; `machine.find {probe? = true, bauds?}` — „Znajdź mój laser”: porty z VID/PID (FTDI, CH340, CP210x), rozpoznanie GRBL i gotowe profile.
- `machine.connect {name}`, `machine.disconnect {device?}`, `machine.select {name}`; `machine.status {device?}` (stan: `idle|run|hold|jog|alarm|door|home…`, `mpos`, `wpos`, `wco`, `buffer`, `overrides`, `lid_open`, `alarm`, `errors`, `variant`, `version`; zadanie; możliwości), `machine.settings` (`$$` z opisami, jednostkami i zakresami), `machine.laser_check` (kreator `$32=1` i `$30`), `machine.console`, `machine.files` (karta SD).
- Ruch (poziom `motion`): `device.home`, `device.unlock`, `device.jog {axis, distance, speed?}`, `device.jog_cancel`, `device.move_to {x?, y?, z?, speed?}`, `device.go_origin`, `device.set_origin {x?, y?, z?}`, `device.set_position {x, y}` (kliknięcie w obszarze), `device.move_to_selection {anchor?}`, `device.frame {kind: rect|contour, pointer?, selection_only?, speed?}`, `device.override {feed?, power?}`, `device.probe_z {distance, feed?, set_zero?}`, `job.pause`, `job.resume`.
- Wiązka (poziom `fire`): `device.test_fire {power, ms}`, `job.start {source?: document|gcode, selection_only?, repeat?, delay_s?, queue?, name?}`, `device.raw {line}`, `device.sd_run {name}`, `macro.run {name}`. Konfiguracja (`config`): `device.settings_write {changes: {"$110": "8000"}, dry_run?}` (walidacja, diff, kopia przed zapisem), `device.sd_upload {name}`, `device.sd_delete {name}`.
- `job.stop {device?}` — zawsze dozwolone (poziom `read`); bez urządzenia zatrzymuje wszystkie podłączone. `job.status` — wiersze wysłane/potwierdzone, rundy, kolejka, błędy, ostrzeżenie `buffer_underrun`.
- Makra: `macro.save {macro: {name, description?, steps: [{kind: command, id, params} | {kind: raw, line}]}}`, `macro.list`, `macro.delete {name}`.
- CLI: `burncraft-cli send <plik.bcraft|.gcode> --device "Nazwa" [--repeat N] [--delay S] [--json] [--prefs-dir]`, `burncraft-cli status --device "Nazwa" [--json]`, `burncraft-cli emulator --tcp 127.0.0.1:2323 [--variant grblhal] [--speed 10]` (emulator GRBL w sieci).

**Polecenia okna** (REQ-01-029). Każda operacja użytkownika w GUI jest poleceniem rejestru. Operacje widoku, narzędzi i okien dialogowych wykonuje samo okno. W opisie polecenia (`describe_command`, `list_commands`) mają `executor: "window"`; polecenia dokumentu mają `"document"`, a obszaru roboczego `"workbench"`. Z `--connect` agent wywołuje je przez `execute`, a host przekazuje je do okna. Bez okna zwracają błąd `no_window`. Nie mogą wchodzić do `batch`.
- `view.zoom {factor, at?: [x, y]}`, `view.pan {dx, dy}` (mm), `view.zoom_page`, `view.zoom_selection`, `view.actual_size`, `view.get` — wynik: `{cx, cy, zoom, visible}`.
- `tool.select {tool}`: `select`, `line`, `node`, `lasso`, `trim`, `eraser`, `rotate2`, `scale2`, `warp`, `rect`, `ellipse`, `polygon`, `star`.
- `tool.set_option {key, value}`: `sides`, `points`, `ratio`, `eraserWidth`, `warpMode`.
- `snap.set {key, enabled}`: `grid`, `nodes`, `mids`, `centers`, `edges`, `align`.
- `layer.set_active {layer}`, `transform.set_anchor {anchor}` (`tl`…`br`), `transform.lock_aspect {locked}`.
- `dialog.open {dialog: settings | shortcuts}`.
- `job.cancel` — przerywa trwającą długą operację (przycisk Anuluj przy pasku postępu).

**Długie operacje** (REQ-16-006). Polecenie trwające dłużej niż 100 ms wysyła wywołującemu klientowi kanału powiadomienia `gui.progress {command, fraction, done}`, nie częściej niż co 100 ms. Ostatnie powiadomienie ma `done: true` i przychodzi przed odpowiedzią. `gui.cancel` omija kolejkę żądań: przerywa trwające polecenie (także `execute` z MCP przez `--connect`), które kończy się błędem `cancelled`. Polecenia są atomowe, więc dokument zostaje bez zmian.

Opcje `burncraft-cli mcp`: `--autosave-minutes N` (domyślnie 5, 0 = wyłączony), `--autosave-dir`, `--prefs-dir`, `--http [PORT]` (Streamable HTTP), `--allow-motion`, `--jog-watchdog S`.

Warstwy: indeksy 0–29 to kolory, 30 = **T1** (ramka / „wytnij zaznaczone”, niewypalana), 31 = **T2** (pomocnicza, niewypalana), od 32 — własne kolory dodane przez `layer.add`.

### Przykłady

```json
{"name": "list_commands", "arguments": {"filter": "shape."}}
```
→ `structuredContent.commands`: `[{"id": "shape.ellipse", "safety_tier": "design", "undoable": true, …}, …]`

```json
{"name": "execute", "arguments": {"id": "shape.rect", "params": {"x": 10, "y": 10, "w": 100, "h": 60, "radius": 3, "layer": 1}}}
```
→ `{"id": "shape.rect", "result": {"id": 1}}`

```json
{"name": "batch", "arguments": {"steps": [
  {"id": "shape.rect", "params": {"x": 0, "y": 0, "w": 100, "h": 60}},
  {"id": "shape.ellipse", "params": {"cx": 50, "cy": 30, "rx": 10, "ry": 10, "layer": 2}},
  {"id": "selection.all"},
  {"id": "object.transform", "params": {"translate": [20, 20], "rotate": 15}},
  {"id": "layer.set", "params": {"index": 2, "name": "Grawer", "mode": "fill", "speed": 300, "power_max": 40}}
]}}
```
→ `{"results": [{"id": 1}, {"id": 2}, {"ids": [1, 2]}, {"ids": [1, 2]}, {...warstwa...}]}`. Błąd kroku: `{"code": …, "message": …, "step": n}`, a dokument wraca do stanu sprzed wsadu.

```json
{"name": "inspect_document", "arguments": {"detail": "summary"}}
```
→ `{"objects": [{"id": 1, "type": "rect", "layer": 0, "bbox": [x0, y0, x1, y1], "transform": [...], "length": 320.0, "closed": true, "nodes": 5, …}], "layers": [...], "selection": [...], "stats": {...}}`

```json
{"name": "render_preview", "arguments": {"bbox": [0, 0, 150, 100], "max_size": 600}}
```
→ `content[0]` = `{"type": "image", "mimeType": "image/png", "data": "…"}`; `structuredContent` = `{"width": 600, "height": 400, "bbox": [...], "px_per_mm": 4.0}`

```json
{"name": "execute", "arguments": {"id": "layer.params", "params": {"index": 0, "params": {"kerf": 0.1, "tabs": {"enabled": true, "count": 4}, "lead_in": {"kind": "arc", "length": 2}}}}}
{"name": "plan_job", "arguments": {"power_limit": 80}}
```
→ `{"time": {"total_s": 42.7, …}, "formatted_time": "00:00:43", "cut_length": 812.4, "travel_length": 96.1, "passes": 2, "warnings": [{"code": "cut_before_engrave", "layer": 0, …}], "cost": {…}, "hash": "…"}`

```json
{"name": "render_preview", "arguments": {"mode": "toolpath", "max_size": 800}}
```

```json
{"name": "export_machine_file", "arguments": {"driver": "grbl", "path": "C:/projekty/pudelko.gcode"}}
```
→ `{"path": "C:/projekty/pudelko.gcode", "driver": "grbl", "lines": 412, "bytes": 5310, "hash": "…", "time_s": 93.4, "warnings": []}`

```json
{"name": "execute", "arguments": {"id": "file.save", "params": {"path": "C:/projekty/pudelko.bcraft"}}}
```

### Urządzenia i zadania (M2.4, ADR 0012)

Narzędzia są nazwami poleceń rejestru z M2.3 — schematy wejścia i wyjścia są te same co w `describe_command`, poziom bezpieczeństwa też.

| Narzędzie | Polecenie | Poziom | Opis |
|---|---|---|---|
| `list_devices` | `machine.list` | read | profile, podłączone, wybrane, odmiany GRBL |
| `select_device {name}` | `machine.select` | design | wybór podłączonego urządzenia |
| `connect {name}` | `machine.connect` | design | połączenie z profilem i wybór |
| `disconnect {device?}` | `machine.disconnect` | design | rozłączenie (domyślnie wybranego) |
| `device_status {device?}` | `machine.status` | read | stan, pozycja, alarm, zadanie, możliwości (też zasób z subskrypcją) |
| `home {device?}` | `device.home` | motion | bazowanie `$H` |
| `jog {axis, distance, speed?, device?}` | `device.jog` | motion | jog osi x/y/z (mm, mm/s); watchdog |
| `move_to {x?, y?, z?, speed?, device?}` | `device.move_to` | motion | ruch do punktu (układ roboczy) |
| `set_origin {x?, y?, z?, device?}` | `device.set_origin` | motion | zero użytkownika w bieżącej pozycji |
| `frame {kind?: rect\|contour, pointer?, selection_only?, speed?, device?}` | `device.frame` | motion | ramka bez wiązki |
| `pause_job {device?}` | `job.pause` | motion | wstrzymanie (feed hold) |
| `resume_job {device?}` | `job.resume` | motion | wznowienie; odmowa przy otwartej pokrywie i w alarmie |
| `stop_job {device?}` | `job.stop` | zawsze | natychmiastowy stop; bez urządzenia — wszystkie podłączone |
| `read_machine_settings {device?}` | `machine.settings` | read | `$$` z opisami, jednostkami i zakresami |

Start zadania (`start_job`, poziom `fire`), `test_fire` i zapis ustawień (`write_machine_settings`, `config`) wymagają potwierdzenia w GUI — etap M6. Do tego czasu przez MCP są odrzucane; zadanie uruchamia użytkownik w GUI albo `burncraft-cli send`.

```json
{"name": "connect", "arguments": {"name": "Symulator"}}
{"name": "jog", "arguments": {"axis": "x", "distance": 10, "speed": 20}}
{"name": "device_status", "arguments": {}}
{"name": "stop_job", "arguments": {}}
```

## Zasoby

`burncraft://document`, `burncraft://commands`, `burncraft://object/{id}`, `burncraft://layer/{color}` (indeks `0`–`29` albo `%23rrggbb`), `burncraft://command/{id}`, `burncraft://device/{id}/status` (= `machine.status`; `id` = nazwa profilu w kodowaniu procentowym, np. `Symulator`), `burncraft://job/{id}` (`{device, job, progress, log}`: stan zadania, postęp 0–1, dziennik wymiany ze sterownikiem; `id` z `device_status.job.id`). `resources/list` wymienia też zasoby podłączonych urządzeń i ich zadań.

**Subskrypcje** (`resources/subscribe {uri}`, `resources/unsubscribe {uri}`): serwer co 100 ms porównuje treść zasobu i wysyła subskrybującemu klientowi `notifications/resources/updated {uri}`; treść czyta się potem przez `resources/read`. Dla `burncraft://document` zmianą jest każdy krok historii, cofnięcie, zmiana zaznaczenia i zakładki. Przez HTTP powiadomienia idą strumieniem `GET /mcp`.

## Prompty

`prompts/list`, `prompts/get {name, arguments?}`.

- `prepare_cut_file {selection_only?, material?}` — lista kontrolna przed cięciem (otwarte kształty w wypełnieniu, duplikaty, kerf, kolejność operacji, obszar roboczy i moc) z bieżącymi ostrzeżeniami `plan_job` i wskazaniem narzędzi (`plan_job`, `render_preview mode=toolpath`, `query_objects`). Prompt nie zmienia dokumentu.

## Postęp i anulowanie

- `tools/call` z `params._meta.progressToken` → `notifications/progress {progressToken, progress, total: 1}` (rosnąco, co najmniej co 50 ms) dla długich poleceń (planowanie, kompilacja, tablice, operacje na wielu obiektach).
- `notifications/cancelled {requestId, reason?}` przerywa żądanie w toku: polecenie kończy się bez zmian w dokumencie (polecenia są atomowe), a serwer nie wysyła odpowiedzi. Anulowanie nieznanego lub zakończonego żądania jest ignorowane. Anulowanie przerywa też oczekiwanie na potwierdzenie użytkownika.

## Streamable HTTP (M2.4, ADR 0012)

```bash
burncraft-cli mcp --http
```

- Punkt końcowy `http://127.0.0.1:7985/mcp` (`--http 0` = dowolny wolny port; pierwszy wiersz stdout to adres). Nasłuch tylko na 127.0.0.1; LAN z tokenem i TLS — M6.
- `POST /mcp` — jedna wiadomość JSON-RPC (`Accept: application/json, text/event-stream`). `initialize` zwraca nagłówek `Mcp-Session-Id`, który klient dołącza do kolejnych żądań (razem z `MCP-Protocol-Version`). `tools/call` odpowiada strumieniem SSE (postęp, `elicitation/create`, odpowiedź), inne żądania — JSON; powiadomienia i odpowiedzi klienta — `202`.
- `GET /mcp` (`Accept: text/event-stream`) — strumień powiadomień serwera (`notifications/resources/updated`). `DELETE /mcp` — koniec sesji.
- Błędy: brak sesji `400`, nieznana sesja `404`, nieobsługiwana wersja protokołu `400`, obcy `Host`/`Origin` (inny niż `127.0.0.1`, `localhost`, `[::1]`) `403`. Sesja bez aktywności przez 30 min wygasa.
- Wszyscy klienci HTTP pracują na jednym dokumencie i tych samych urządzeniach; każdy ma własne subskrypcje i watchdog.

Konfiguracja klienta (np. Claude Code, `.mcp.json`):

```json
{ "mcpServers": { "burncraft": { "type": "http", "url": "http://127.0.0.1:7985/mcp" } } }
```

## Polityka bezpieczeństwa

Szczegóły: [ADR 0005](adr/0005-mcp-security.md), [ADR 0012](adr/0012-mcp-streamable-http.md).

- `read` i `design` — bez pytań.
- `motion` — potwierdzenie użytkownika. Klient z `capabilities.elicitation` dostaje `elicitation/create` z opisem polecenia (urządzenie, parametry) i schematem `{confirm: boolean}`; tylko `action: "accept"` z `confirm: true` wykonuje polecenie. Bez elicitation polecenie jest odrzucane z `code: "confirmation_required"`. Użytkownik może dopuścić ruch bez pytań przy uruchomieniu: `burncraft-cli mcp --allow-motion`.
- `config`, `fire` — jawne potwierdzenie w GUI albo przez elicitation (M6); do tego czasu odrzucane z `confirmation_required`. Dla połączeń nielokalnych `fire` zawsze wymaga potwierdzenia.
- `stop_job` jest zawsze dozwolony, bez potwierdzeń, i omija kolejkę żądań: działa także wtedy, gdy serwer wykonuje długie polecenie albo czeka na potwierdzenie (także z innej sesji HTTP). Fizyczny E-stop kończy zadanie alarmem; `resume_job` i ruch w alarmie są odrzucane do odblokowania przez operatora.
- Utrata klienta (koniec stdin, `DELETE`, wygaśnięcie sesji) nie przerywa zadania — działa jak zamknięcie panelu w GUI; `burncraft-cli mcp` po końcu stdin czeka, aż zadanie się skończy.
- Watchdog: jog zlecony przez klienta jest anulowany, gdy klient milczy dłużej niż limit (domyślnie 5 s, `--jog-watchdog S`) albo zostanie utracony. Przy długim jogu wywołuj `device_status` albo `ping`.
- Każde wywołanie trafia do dziennika audytu (JSON Lines): `%LOCALAPPDATA%\BurnCraft\mcp-audit.jsonl` / `~/Library/Logs/BurnCraft/mcp-audit.jsonl` / `$XDG_STATE_HOME/burncraft/mcp-audit.jsonl`.
