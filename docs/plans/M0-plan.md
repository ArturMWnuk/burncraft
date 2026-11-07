# Plan etapu M0 — fundamenty

Wymaganie procesowe REQ-20-001. Zakres etapu = wszystkie pozycje `ms: M0` w `requirements.yaml` (filtr: `python -c "import yaml;[print(r['id'],r['text']) for r in yaml.safe_load(open('requirements.yaml',encoding='utf-8'))['requirements'] if r['ms']=='M0']"`).

## Kroki

1. Rejestr wymagań `requirements.yaml` + `REQUIREMENTS.md` (§21.1) — przed kodem.
2. ADR 0001–0005: licencja, stos, układ współrzędnych, format pliku, bezpieczeństwo MCP.
3. Szkielet repozytorium wg §1.1 (14 crate'ów, `app/`, `web/`, `bridge/`), CI.
4. Rdzeń: `bc-geom` (ścieżki), `bc-doc` (model, historia, `.bcraft` ze schematem i migracjami), `bc-commands` (rejestr poleceń, sesja, wsad, undo/redo), `bc-sim` (podgląd PNG).
5. `bc-mcp` + `burncraft-cli mcp`: `list_commands`, `describe_command`, `execute`, `batch`, `undo`, `redo`, `inspect_document`, `render_preview`; zasoby; polityka read/design; dziennik audytu.
6. `tools/req-check` + macierz śledzenia.
7. Przegląd zgodności `docs/reviews/M0-review.md` (§21.3).

## Decyzje etapu

- Moduły z §1.1 bez funkcji w M0 są szkieletami (crate z dokumentacją etapu docelowego). Ich funkcje są osobnymi wymaganiami późniejszych etapów.
- Wymagania „całościowe”, których nie da się zweryfikować przed powstaniem GUI/web (REQ-00-005 pełny MCP z wykonywaniem zadań, REQ-01-002 cienkie warstwy GUI/web), przypisano do M6. M0 dostarcza ich fundament.
