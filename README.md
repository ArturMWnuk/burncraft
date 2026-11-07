# BurnCraft

Otwarty, wieloplatformowy program do projektowania (CAD/CAM 2D) i sterowania laserami: diodowymi, CO2 i galvo. Ma wbudowany serwer MCP dla agentów AI. Specyfikacja: [`PROMPT_BurnCraft.md`](PROMPT_BurnCraft.md); zakres i postęp: [`REQUIREMENTS.md`](REQUIREMENTS.md).

**Stan: etap M0 (fundamenty).** Działa rdzeń bezgłowy: model dokumentu, format `.bcraft`, rejestr poleceń, podgląd PNG i serwer MCP po stdio. GUI, import plików, planer i sterowniki powstaną w kolejnych etapach.

## Szybki start

```bash
cargo build --release
```
```bash
cargo test --workspace
```
```bash
target/release/burncraft-cli commands
```
```bash
target/release/burncraft-cli mcp
```

Konfiguracja klienta MCP (Claude Code / Claude Desktop): [`docs/mcp.md`](docs/mcp.md).

## Struktura

| Katalog | Zawartość |
|---|---|
| `crates/bc-geom` | geometria 2D |
| `crates/bc-doc` | model dokumentu, historia, format `.bcraft` |
| `crates/bc-commands` | rejestr poleceń (wspólne API GUI/CLI/MCP/skryptów) |
| `crates/bc-sim` | podgląd PNG (później symulacja i estymacja czasu) |
| `crates/bc-mcp` | serwer MCP, polityka bezpieczeństwa, audyt |
| `crates/bc-cli` | `burncraft-cli` |
| `crates/bc-{text,image,io,plan,drivers,camera,materials,script}` | szkielety modułów kolejnych etapów |
| `app/`, `web/`, `bridge/` | GUI (M1), klient webowy i Bridge (M6) |
| `schemas/` | schemat JSON `document.json` |
| `docs/adr/` | decyzje architektoniczne 0001–0005 |
| `tools/req-check/` | kontrola zgodności z rejestrem wymagań |

## Licencja

Rdzeń: MIT OR Apache-2.0; aplikacje: GPL-3.0-or-later ([ADR 0001](docs/adr/0001-license.md)).
