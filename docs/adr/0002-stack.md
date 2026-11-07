# ADR 0002 — Stos technologiczny

- Status: przyjęta (M0, 2026-10-28)
- Wymagania: REQ-01-001…019, REQ-01-020, REQ-01-021, REQ-20-006

## Decyzja

| Warstwa | Wybór | Uwagi |
|---|---|---|
| Rdzeń | **Rust** (edycja 2021, MSRV 1.88 — od M1.3; rozwój na 1.99) | zgodnie z §1.2 |
| Geometria | `kurbo` 0.13 (krzywe, transformacje, bbox, długość) | booleany/offsety: Clipper2 (M1) |
| Model dokumentu | własny (`bc-doc`), serde + JSON | ADR 0004 |
| Walidacja `document.json` | `jsonschema` 0.60 (bez funkcji sieciowych) | schemat w `schemas/` |
| Schematy parametrów poleceń | `schemars` 1.x — generowane z typów Rust | jedno źródło prawdy dla parsera i schematu |
| Rasteryzacja podglądu | `tiny-skia` 0.12 (czysty Rust, bez GPU) | podgląd po stronie rdzenia, także bezgłowo |
| Archiwum `.bcraft` | `zip` 9 (tylko deflate) | |
| CLI | `clap` 4 | |
| Logi | `tracing` + `tracing-subscriber`, zawsze na stderr | stdout należy do MCP |
| MCP | **własna implementacja JSON-RPC 2.0 po stdio** w `bc-mcp` | zob. niżej |
| GUI (M1) | Tauri 2 + TypeScript, obszar roboczy WebGL2/WebGPU | ten sam frontend dla `web/` |

## MCP: własna warstwa zamiast `rmcp`

§1.2 wymienia `rmcp` (oficjalny Rust SDK) jako przykład biblioteki. W M0 serwer jest napisany ręcznie (~600 linii), bo:

1. Potrzebna część protokołu (initialize, tools/list|call ze `structuredContent`/`outputSchema`, resources/list|read|templates, ping) jest mała i stabilna.
2. Pełna kontrola nad polityką bezpieczeństwa i dziennikiem audytu w jednym miejscu (`tools::call`).
3. Brak `tokio` w drzewie zależności rdzenia na tym etapie — prostszy build na ARM/RPi.

To rozstrzygnięcie równoważnych opcji (§21.4: wybór biblioteki), a nie zmiana funkcji. Gdy dojdą Streamable HTTP, subskrypcje, postęp, anulowanie i elicitation (M2/M6), wybór zostanie oceniony ponownie: `rmcp` vs rozbudowa własnej warstwy. Ewentualna zmiana dostanie nowy ADR. Ocena z M2.4: [ADR 0012](0012-mcp-streamable-http.md) — własna warstwa zostaje.

## Historia cofania

M0 używa mementa (kopia dokumentu przed każdym poleceniem). Jest proste i poprawne, ale przy 100 tys. obiektów (§16) zużywa za dużo pamięci. W M1 obiekty przejdą na współdzielenie struktury (`Arc` + copy-on-write) bez zmiany API `Session`.

## Konsekwencje

- Każdy moduł z §1.1 jest osobnym crate'em w workspace `Cargo.toml`. Moduły bez implementacji w M0 to szkielety z etapem docelowym w dokumentacji.
- Zależności sieciowe (`reqwest`, `hyper`, `tokio`) są poza drzewem w M0 — zweryfikowane `cargo tree` (REQ-16-009).
