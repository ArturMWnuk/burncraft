# Macierz zgodności sprzętu

Poziomy: **sprzęt** = zweryfikowane na fizycznej maszynie (z checklistą), **emulator** = testy na emulatorze/golden, **eksperymentalne** = zaimplementowane bez weryfikacji, **—** = brak implementacji.

Stan na M2.5 (2026-11-07): sterownik GRBL (serial, TCP, WebSocket) z odmianami działa na emulatorze (ADR 0010), GUI maszyny (okna Laser, Przesuń, Konsola, Makra, Podgląd) — na emulatorze. Test na laserze diodowym zleceniodawcy (REQ-19-017, 2026-11-05): Two Trees TTS-20 PRO, płyta MKS DLC32 V4.00, GRBL 1.1h, USB CH340 — **zgodny** (14 z 16 punktów „tak”, 2 pominięte), wyniki w `docs/procedures/M2-hardware-grbl.md`. Usterki z testu poprawione w M2.5. Eksport G-code: ADR 0011.

| Rodzina | Modele (wymagania) | Etap | Poziom | Uwagi |
|---|---|---|---|---|
| Lihuiyu (K40) | M2 Nano, M3 Nano, M1, M, B1, B2, B, A, 6C6879 (REQ-13-030…049) | M3 | — | priorytet: M2 Nano „od pudełka” |
| Moshi | MS10105 V4.x (REQ-13-051, 052) | M3 | — | |
| GRBL i pochodne | 0.9, 1.1, grblHAL, FluidNC, LPC, ESP32, M3 + producenci (REQ-13-050, 053…091) | M2 | emulator | M2.2: eksport G-code; M2.3: połączenie, strumieniowanie, jog, ramki, ustawienia, sonda, karta SD (FluidNC) na emulatorze i grbl-sim; szczegóły producentów UNVERIFIED |
| Smoothieware, Marlin, RRF/Duet, CNC ogólny | (REQ-13-092…108) | M2/M6 | emulator (CNC ogólny: eksport) | M2.2: sterownik `gcode` (Mach3/Mach4, UCCNC, Masso, LinuxCNC) — szablony, G20/G21, makra; Smoothie/Marlin/RRF od M3/M6 |
| xTool | D1…F2, Wi-Fi HTTP (REQ-13-109…118) | M6 | — | |
| Ruida | RDC6442G…Hydra/GT5, Thunder, Omtech (REQ-13-119…146, 184, 186) | — | **nieobsługiwane** | wyłączone z projektu decyzją zleceniodawcy 2026-11-07 (DEV-007) |
| Trocen, TopWisdom | (REQ-13-147…154) | M6 | — | do zbadania |
| Leetro | MPC65xx (REQ-13-155) | M7 | — | SHOULD |
| LightObject, Gweike | X7, T9, G2, M3 Ultra (REQ-13-156…159) | M6 | — | do zbadania |
| BJJCZ (EZCAD2) | LMCV4-FIBER-M, DIGIT, CO2, UV, MOPA (REQ-13-160…173) | M5 | — | |
| EZCAD3 | DLC2-M4-2D/3D (REQ-13-174…178) | M6 | — | |
| BSL | — | M7 | — | SHOULD |
| Glowforge | — | — | **nieobsługiwane** | zamknięta chmura (FAQ, M7) |
| GRBL — Two Trees TTS-20 PRO (laser diodowy zleceniodawcy) | MKS DLC32 V4.00, GRBL 1.1h.2023112101, CH340 115200 bd (REQ-19-017) | M2 | **sprzęt** | 2026-11-05, checklista M2-hardware-grbl.md: połączenie, Konsola, bazowanie, alarm i odblokowanie, jog XY, pozycja, znacznik głowicy, podgląd, ramka, cięcie (30 × 30 mm, Ø10 mm), grawer, pauza/wznowienie, Stop (ALARM:3, $X), start od miejsca — tak; bez osi Z (krok 6) i bez testu odłączenia USB (krok 16, decyzja zleceniodawcy) |
| Urządzenie fikcyjne | symulator | M2 | emulator | „Symulator” (emulator GRBL w procesie), także `burncraft-cli emulator --tcp` |

Pełne opisy pozycji zawiera `requirements.yaml` (sekcja 13.3).
