# ADR 0001 — Licencja

- Status: przyjęta (M0, 2026-10-28)
- Wymagania: REQ-00-020, REQ-20-005

## Kontekst

Specyfikacja (§0) proponuje GPL-3.0-or-later dla aplikacji i MIT/Apache-2.0 dla biblioteki rdzenia i SDK wtyczek. Główne źródło wiedzy o protokołach sterowników, MeerK40t, jest na licencji MIT; K40 Whisperer — GPL-3.0 (wolno go używać tylko jako źródła wiedzy, chyba że cały projekt wybierze GPL). Producenci maszyn i autorzy integracji (Inkscape, generatory web, Home Assistant) powinni móc osadzać rdzeń bez przejmowania copyleftu.

## Decyzja

| Część | Licencja | Pakiety |
|---|---|---|
| Rdzeń i biblioteki | `MIT OR Apache-2.0` | `bc-geom`, `bc-doc`, `bc-text`, `bc-image`, `bc-io`, `bc-plan`, `bc-sim`, `bc-drivers`, `bc-camera`, `bc-materials`, `bc-commands`, `bc-mcp`, `bc-script` |
| Aplikacje | `GPL-3.0-or-later` | `bc-cli` (burncraft-cli), `app/` (GUI), `web/`, `bridge/` |
| SDK wtyczek (gdy powstanie) | `MIT OR Apache-2.0` | — |

Licencja jest ustawiona w `Cargo.toml` każdego pakietu (`license = …`), więc `cargo metadata` i narzędzia SBOM ją raportują.

## Uzasadnienie

- GPL dla aplikacji chroni przed zamkniętymi forkami całego programu — główne ryzyko po doświadczeniach społeczności z zamkniętymi programami do laserów licencjonowanymi online.
- Podwójna licencja MIT/Apache dla rdzenia to standard ekosystemu Rust: zgodność z MIT (MeerK40t) i Apache-2.0 (patenty), możliwość użycia w GPL-owej aplikacji.
- Kodu GPL (K40 Whisperer) nie wolno kopiować do rdzenia MIT/Apache. Jest wyłącznie źródłem wiedzy o zachowaniu protokołu; implementacja powstaje od zera i jest oznaczana w `THIRD_PARTY.md`.

## Konsekwencje

- Wkład w rdzeń jest przyjmowany na zasadach MIT OR Apache-2.0 (inbound = outbound), opisanych w `CONTRIBUTING.md`.
- Pełne teksty licencji (`LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-GPL`) trzeba dołączyć przed pierwszym wydaniem. W M0 repozytorium zawiera tylko ich identyfikatory SPDX (zob. `LICENSES/README.md`) — pobranie oficjalnych tekstów jest zadaniem otwartym.
