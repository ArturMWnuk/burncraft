# ADR 0004 — Format pliku `.bcraft`

- Status: przyjęta (M0, 2026-10-28)
- Wymagania: REQ-00-016, REQ-02-046…052, REQ-20-008

## Decyzja

`.bcraft` to archiwum ZIP:

| Wpis | Kompresja | Zawartość |
|---|---|---|
| `mimetype` | stored, pierwszy | `application/x-burncraft` (rozpoznanie pliku bez rozpakowania) |
| `document.json` | deflate | dokument wg `schemas/document.schema.json` |
| `notes.md` | deflate | notatki dokumentu (Markdown) |
| `notes/<nazwa>.png\|jpg` | stored | obrazy notatek, w tekście `![](notes/<nazwa>)` (M1.4c3, REQ-03-049) |
| `thumbnail.png` | stored | miniatura 256 px (dla Eksploratora/Quick Look/menedżerów plików — M7) |
| `images/…`, `fonts/…` | — | osadzone obrazy (PNG / 16-bit TIFF) i fonty — M4 |

### `document.json`

- Pole `schema` (liczba całkowita) = wersja schematu; obecnie **1**.
- Obiekty mają `id` (unikalne w dokumencie, `< next_id`), `type` (`path`/`rect`/`ellipse`/`group`), `layer` (indeks), `visible`, `locked`, `transform` (macierz afiniczna `[a,b,c,d,e,f]`, niewypiekana w geometrię) i pola geometrii zależne od typu. Ścieżki mają dane w składni SVG `d` w układzie z ADR 0003.
- Kształty parametryczne (`rect` z `radius`, `ellipse`) zachowują parametry (REQ-02-033).

### Odczyt

1. Odczyt `mimetype` (inny = odmowa).
2. **Migracja**: `migrate()` podnosi JSON z dowolnej starszej wersji do bieżącej, krok po kroku (0→1 jest zaimplementowane i przetestowane). Plik z **nowszą** wersją jest odrzucany z czytelnym komunikatem, a nie czytany częściowo.
3. **Walidacja schematem JSON** (`jsonschema`), potem kontrola niezmienników, których schemat nie wyraża: unikalne id, istniejące warstwy, skończone transformacje, poprawne dane `d`.
4. Zapis jest atomowy: plik tymczasowy, potem zamiana.

## Uzasadnienie

- ZIP + JSON: czytelny (diff, narzędzia zewnętrzne, odzyskiwanie ręczne), łatwo osadzić binaria, ten sam wzorzec co ODF/EPUB.
- Schemat w osobnym pliku JSON może być używany przez zewnętrzne narzędzia i dokumentację.
- Notatki poza JSON-em — przyjazne dla edycji ręcznej i podglądu.

## Konsekwencje

- Każda zmiana schematu = nowa wersja `schema` + krok w `migrate()` + test migracji (wzór: `req_02_052_migration_from_schema_0`).
- Szablony `.bcraft-template` (M1) używają tego samego kontenera z innym `mimetype`.
