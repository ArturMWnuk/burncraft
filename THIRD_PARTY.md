# Źródła zewnętrzne i zależności

Wymaganie REQ-00-019: każde źródło wiedzy i każda biblioteka. Zasada clean-room (REQ-00-014): kod, ikony, teksty i zasoby innych programów do laserów nie są dekompilowane ani kopiowane. Interoperacyjność z plikami `.lbrn2/.clb/.lbart/.lbzip` powstaje wyłącznie z analizy plików użytkownika.

## Biblioteki (zależności bezpośrednie, M0)

| Biblioteka | Wersja | Licencja | Użycie |
|---|---|---|---|
| kurbo | 0.13 | MIT OR Apache-2.0 | krzywe Béziera, transformacje, bbox, długość (bc-geom) |
| clipper2-rust | 1.2 | BSL-1.0 | port Clipper2 (A. Johnson): booleany i offsety (bc-geom, ADR 0006) |
| usvg | 0.49 | Apache-2.0 OR MIT | parsowanie i normalizacja SVG (bc-io, ADR 0008) |
| roxmltree | 0.21 | MIT OR Apache-2.0 | XML: etykiety warstw SVG, .lbrn2 (bc-io) |
| dxf | 0.6 | MIT | odczyt/zapis DXF R12–2018 (bc-io) |
| libfuzzer-sys | 0.4 | (MIT OR Apache-2.0) + libFuzzer Apache-2.0 WITH LLVM-exception | fuzzing importerów (`fuzz/`, tylko CI) |
| serde, serde_json | 1 | MIT OR Apache-2.0 | serializacja |
| schemars | 1 | MIT | schematy JSON parametrów poleceń |
| jsonschema | 0.60 | MIT | walidacja `document.json` |
| zip | 9 | MIT | kontener `.bcraft` |
| tiny-skia | 0.12 | BSD-3-Clause | rasteryzacja podglądu |
| base64 | 0.22 | MIT OR Apache-2.0 | obrazy w odpowiedziach MCP |
| clap | 4 | MIT OR Apache-2.0 | CLI |
| tracing, tracing-subscriber | 0.1 / 0.3 | MIT | logi |
| thiserror | 2 | MIT OR Apache-2.0 | typy błędów |
| PyYAML (narzędzie req-check) | 6 | MIT | odczyt rejestru wymagań |
| tokio | 1 | MIT | środowisko asynchroniczne hosta GUI (bc-host, ADR 0007) i MCP Streamable HTTP (bc-mcp, ADR 0012) |
| axum | 0.8 | MIT | HTTP + WebSocket kanału sterowania (bc-host), MCP Streamable HTTP z SSE (bc-mcp, ADR 0012) |
| tungstenite | 0.30 | MIT OR Apache-2.0 | klient WebSocket `mcp --connect` (bc-cli), testy hosta, transport WebSocket FluidNC/ESP3D (bc-drivers, ADR 0010) |
| serialport | 4.10 | MPL-2.0 | transport serial / USB-CDC i lista portów z VID/PID (bc-drivers, ADR 0010); biblioteka bez zmian, licencja plikowa MPL zgodna z GPL aplikacji |
| grbl-sim, GRBL | — | GPL-3.0 | tylko CI na Linuksie: budowane ze źródeł w trakcie testu porównawczego z emulatorem (REQ-19-007); nie trafiają do repozytorium ani produktu |
| getrandom | 0.4 | MIT OR Apache-2.0 | losowy token sesji kanału (bc-host), identyfikator sesji MCP HTTP (bc-mcp) |
| futures-core | 0.3 | MIT OR Apache-2.0 | trait `Stream` strumienia SSE (bc-mcp, ADR 0012); zależność axum, bez nowego pobrania |

### Frontend `web/` (tylko narzędzia budowania i testów — nic z nich nie trafia do kodu aplikacji poza wynikiem kompilacji TypeScript)

| Pakiet npm | Wersja | Licencja | Użycie |
|---|---|---|---|
| typescript | 5.6 | Apache-2.0 | kompilator TS |
| vite | 6.4 | MIT | budowanie frontendu |
| @playwright/test | 1.64 | Apache-2.0 | testy E2E GUI (REQ-19-013) |

Pełna lista zależności przechodnich: `cargo tree --workspace`. Przed wydaniem 1.0 trzeba wygenerować raport licencji (np. `cargo about`).

## Pliki użytkownika (interoperacyjność, clean-room)

| Plik | Pochodzenie | Użycie |
|---|---|---|
| pliki `.lbrn`/`.lbrn2` dostarczone przez użytkownika (2026-10-28 i 2026-10-29) | prywatne, poza repozytorium | analiza formatu: CutSetting, Shape (Rect, Ellipse, Polygon, Path z VertList/PrimList i V/P, Text z BackupPath, Group), XForm; FormatVersion 0 i 1 |
| `crates/bc-io/tests/fixtures/{plik_przykladowy,elipsa,hex,krzywe,text,grupa}.lbrn*`, `fuzz/corpus/lbrn2_import/seed-rects.lbrn2` | własne, generowane przez `tools/fixtures/gen_lbrn_samples.py` (2026-11-07) | tylko elementy czytane przez importer; wymiary i położenia kształtów z prób użytkownika, kontury napisu własnym krojem blokowym; testy REQ-02-091/092 |

## Źródła wiedzy o protokołach (do wykorzystania od M2/M3)

| Źródło | Licencja | Zakres | Sposób użycia |
|---|---|---|---|
| MeerK40t | MIT | Lihuiyu (EGV), Moshi, Ruida, Balor/BJJCZ, GRBL | wiedza + ew. przeniesienie fragmentów z zachowaniem noty MIT |
| K40 Whisperer | GPL-3.0 | Lihuiyu M2 Nano | **tylko** jako źródło wiedzy; bez kopiowania do rdzenia MIT/Apache (ADR 0001) |
| Balor (BJJCZ LMC) | GPL-3.0 | protokół list galvo | tylko jako źródło wiedzy |
| Opracowania protokołu Ruida (wiki EduTech, `ruida-laser`, `thunderlaser`) | wg projektu | swizzling, UDP, polecenia | wiedza |
| Dokumentacja GRBL / grblHAL / FluidNC / Smoothieware / Marlin / RepRapFirmware | GPL/MIT (dokumentacja) | G-code, `$$`, realtime | wiedza |
| Specyfikacja Model Context Protocol | MIT | protokół MCP | implementacja serwera |

Niepewne szczegóły protokołów oznacza się w kodzie `// UNVERIFIED:` z odnośnikiem do źródła (REQ-20-004).
