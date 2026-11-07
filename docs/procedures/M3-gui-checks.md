# Procedury ręczne — etap M3

Zrzuty: `docs/procedures/img/m3/`, generator `web/e2e/procedures-m3.spec.ts` (`BC_SCREENSHOTS=1`). Urządzenia USB w zrzutach są fikcyjne (`BURNCRAFT_FAKE_USB=1`): K40 z układem CH341 bez sterownika, galvo BJJCZ ze sterownikiem WinUSB i mysz USB.

## REQ-00-018 — wiedza o protokołach wyłącznie z publicznych, otwartych źródeł

Procedura: przegląd ADR 0013, sekcji „Źródła wiedzy o protokołach” w `THIRD_PARTY.md` i komentarzy przy faktach o protokołach w kodzie (`crates/bc-drivers/src/usb.rs`).

Wynik (2026-11-07, M3.1): **zgodne**.
- ADR 0013 zapisuje zasadę zatwierdzoną przez zleceniodawcę: otwarte projekty służą tylko jako źródło wiedzy, bez kopiowania kodu, a każdy fakt ma odnośnik do źródła.
- Fakty o protokołach w M3.1 to VID/PID z §13.3 specyfikacji oraz punkty końcowe CH341 i BJJCZ z opisów otwartych projektów (oznaczone `UNVERIFIED`).
- Licencje: MeerK40t — MIT (sprawdzona). Pozostałe źródła mają adnotację „do sprawdzenia przy pierwszym użyciu”.

Procedurę trzeba powtórzyć przy M3.2 (Ruida) i M3.3 (Lihuiyu, Moshi), kiedy przybędzie źródeł.

## REQ-13-020 — kreator instalacji sterownika USB na Windows

Procedura: okno Laser → „Znajdź mój laser” (`machine.find`). W sekcji „Urządzenia USB” są lasery znanych rodzin ze stanem dostępu. Przy urządzeniu bez sterownika WinUSB przycisk „Jak podłączyć…” (`machine.usb_guide`) otwiera przewodnik w oknie dialogowym.

| Lista urządzeń USB | Przewodnik (pl) | Przewodnik (en) |
|---|---|---|
| ![Urządzenia USB](img/m3/usb-found.png) | ![Przewodnik WinUSB](img/m3/usb-guide-winusb.png) | ![WinUSB guide](img/m3/usb-guide-winusb-en.png) |

Wynik (2026-11-07): **zgodne**.
- K40 (CH341, `1A86:5512`) ma stan „brak sterownika” i przycisk przewodnika.
- Galvo BJJCZ ma stan „gotowe” i nie ma przewodnika; mysz USB nie jest pokazana.
- Przewodnik prowadzi przez instalację WinUSB narzędziem Zadig, którego BurnCraft nie dołącza (decyzja zleceniodawcy). Podaje USB ID do wyboru w narzędziu i alternatywę: oryginalny sterownik CH341 producenta (REQ-13-040, M3.3).
- Tłumaczenie angielskie jest kompletne.
- Stan „sterownik BurnCraft w etapie M3.3/M5” informuje, że sam sterownik lasera powstaje później.

Uwagi z przeglądu zrzutów (poprawione przed zatwierdzeniem): w pierwszej wersji przewodnik był w panelu bocznym i jego dolna część była ucięta. Teraz otwiera się w oknie dialogowym.

Do sprawdzenia na sprzęcie (test K40, M3.4): stan „brak sterownika” na prawdziwym urządzeniu i połączenie po instalacji WinUSB.

## REQ-13-021 — reguły udev na Linuksie

Procedura:
- Plik `packaging/linux/99-burncraft.rules` daje dostęp zalogowanemu użytkownikowi (`TAG+="uaccess"`, `MODE="0660"`) do urządzeń znanych rodzin. Generuje go `usb::udev_rules()`, a test sprawdza, że plik w repozytorium jest aktualny.
- `burncraft-cli udev` wypisuje reguły. Instalacja:

```
burncraft-cli udev | sudo tee /etc/udev/rules.d/99-burncraft.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

- Przewodnik w GUI przy stanie „brak uprawnień” pokazuje te same kroki (tłumaczenia `usb-guide-udev-*`).
- CI na Linuksie sprawdza składnię pliku poleceniem `udevadm verify`.

Wynik (2026-11-07): **zgodne**. Treść reguł sprawdzają test `req_13_021_udev_rules_file_matches_known_families` i test CLI `req_13_021_cli_prints_udev_rules`, a składnię — CI. Nadanie uprawnień prawdziwemu urządzeniu na Linuksie nie było sprawdzane: zleceniodawca testuje K40 na Windows.
