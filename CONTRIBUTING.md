# Jak współtworzyć BurnCraft

## Zasady

1. **Clean-room**: nie dekompiluj, nie kopiuj kodu, tekstów ani zasobów innych programów do laserów. Opisuj zachowanie, nie implementację.
2. **Zakres = `requirements.yaml`**. Każda zmiana funkcji wskazuje wymaganie (`REQ-…`). Nowa funkcja spoza rejestru wymaga dodania wymagania za zgodą zleceniodawcy, a zmiana lub pominięcie — wpisu w `DEVIATIONS.md`.
3. **Testy przed implementacją**. Nazwa testu zawiera identyfikator wymagania: `req_06_004_atkinson_dither` (zakres: `req_01_030_to_035_…`, kilka: `req_15_017_018_…`).
4. **Polecenia**: każda operacja użytkownika to polecenie w `bc-commands` z polem `requirements: &["REQ-…"]`. GUI, MCP i skrypty nie modyfikują dokumentu bezpośrednio.
5. Licencja wkładu: rdzeń `MIT OR Apache-2.0`, aplikacje `GPL-3.0-or-later` (ADR 0001).

## Przed wysłaniem zmian

```bash
cargo fmt --all
```
```bash
cargo clippy --workspace --all-targets -- -D warnings
```
```bash
cargo test --workspace
```
```bash
python tools/req-check/req_check.py --write
```

Do tego unittest (`tools/req-check/test_req_check.py`, `tools/i18n/test_ftl.py`) i Playwright (`PW_CHANNEL=msedge` lokalnie). Testy E2E i req-check uruchamiaj z logami (`--test-log`).

## CI i minuty GitHub Actions

Repozytorium jest prywatne, a minuty Actions są limitowane (Windows liczony ×2, macOS ×10). Zasady (decyzja zleceniodawcy 2026-11-04):

1. **Jeden push na przyrost** (albo jego wyraźnie zamkniętą część), a nie na każdą parę commitów „testy (red)” / „implementacja (green)”. Kolejność red → green widać w historii commitów, więc push może zawierać kilka par.
2. **Przed pushem pełna weryfikacja lokalna** (powyżej). CI przy pushu sprawdza tylko Linux x64; Windows sprawdzasz lokalnie.
3. Commity zmieniające wyłącznie dokumentację (`docs/**`, `*.md`) nie uruchamiają CI. Łącz je z najbliższym pushem kodu, jeśli to możliwe.
4. Pełna macierz (pięć platform, okno Tauri na trzech systemach, fuzzing) działa raz w tygodniu, jeśli były commity, oraz ręcznie (`workflow_dispatch`). Ręczny przebieg jest obowiązkowy przed zamknięciem etapu i nie powinien być uruchamiany częściej.
5. Nie ponawiaj przebiegu „na próbę”. Najpierw przeczytaj log; niestabilny test popraw (osobny commit), zamiast go powtarzać.

## Dodawanie sterownika lasera

1. **Wymagania**: znajdź w `requirements.yaml` pozycje rodziny (sekcja 13.3) i zaplanuj ich weryfikację (`emu`/`hw`).
2. **Źródła**: dopisz do `THIRD_PARTY.md` każde źródło wiedzy o protokole z licencją. Fragmentów GPL nie kopiuj do rdzenia.
3. **Moduł** `crates/bc-drivers/src/<rodzina>/` (od M2): implementacja traitu `Driver` (§13.1): `capabilities`, `connect`, `status`, `home`, `jog`, `move_to`, `set_origin`, `frame`, `test_fire`, `send_job`, `pause`/`resume`/`stop`, `read_settings`/`write_settings` (z kopią przed zapisem), pliki na sterowniku i `compile` (eksport offline).
4. **Kompilator** zadań ma być niezależny od transportu (bajty/G-code z planu `bc-plan`) i **deterministyczny**.
5. **Emulator** w `crates/bc-drivers/emulators/<rodzina>`: przyjmuje strumień, odpowiada jak sterownik (ACK/BUSY/CRC), renderuje ruch do PNG porównywanego z podglądem planera.
6. **Testy golden**: projekt referencyjny → plik w `tests/golden/<rodzina>/`. Zmiana bajtów wymaga świadomej aktualizacji pliku wzorcowego.
7. Niepewne szczegóły oznacz `// UNVERIFIED: <źródło>` i dopisz do planu weryfikacji sprzętowej.
8. **Macierz**: zaktualizuj `docs/hardware-matrix.md` (poziom: sprzęt / emulator / eksperymentalne).
9. **Bezpieczeństwo**: `stop()` zawsze dostępny i najszybszy; ruch → tier `motion`, zapis ustawień → `config`, emisja → `fire`.
