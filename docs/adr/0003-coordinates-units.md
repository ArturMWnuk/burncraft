# ADR 0003 — Układ współrzędnych i jednostki

- Status: przyjęta (M0, 2026-10-28)
- Wymagania: REQ-15-011, REQ-15-012, REQ-20-007

## Decyzja

1. **Jednostka wewnętrzna: milimetr** (`f64`). Cale istnieją tylko w warstwie prezentacji (pola edycji, przełącznik mm/in, REQ-03-027/028). Format `.bcraft` zapisuje `"units": "mm"` na stałe.
2. **Oś Y w górę**, jak w maszynach i CAD: punkt (0, 0) to **lewy dolny róg obszaru roboczego**, X w prawo, Y w górę.
3. **Kąty w stopniach, dodatnie przeciwnie do ruchu wskazówek zegara** (konwencja matematyczna). Wewnętrznie radiany.
4. **Prostokąt** (`shape.rect`) jest opisany lewym dolnym narożnikiem `(x, y)` oraz `w`, `h` > 0.
5. **Origin maszyny** (narożnik bazowania: lewy-przód, prawy-tył itd.) jest właściwością profilu urządzenia (M2). Kompilator zadań przelicza współrzędne dokumentu na współrzędne sterownika. Dokument nie zależy od maszyny.
6. **Ścieżki SVG** w poleceniach (`shape.path {d}`) są interpretowane w układzie BurnCraft (Y w górę). Import plików SVG (Y w dół) odwraca oś (M1, `bc-io`).
7. **Podgląd PNG** (`render_preview`) odwraca oś Y przy rasteryzacji; wynik zawiera `bbox` (mm) i `px_per_mm`, żeby agent mógł przeliczyć piksel na współrzędne.

## Uzasadnienie

- Agent MCP liczy współrzędne bez znajomości maszyny. Jeden stały układ (mm, Y w górę) usuwa całą klasę błędów „lustrzanego” zadania. Układ jest zapisany w polu `instructions` serwera i w raporcie `inspect_document` (`coordinate_system`).
- Zależność od narożnika maszyny zostaje w jednym miejscu — kompilatorze sterownika (bc-plan/bc-drivers), gdzie i tak powstaje transformacja do kroków/impulsów.

## Konsekwencje

- Testy podglądu sprawdzają odwrócenie osi (`bc-sim`: `req_15_022_render_design_png`).
- Import `.lbrn2` (M1) musi przeliczyć współrzędne formatu `.lbrn2` (Y w górę względem origin projektu) — do zweryfikowania na plikach użytkownika.
