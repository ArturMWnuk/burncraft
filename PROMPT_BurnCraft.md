# PROMPT: Zbuduj „BurnCraft” — otwarty, wieloplatformowy program do projektowania i sterowania laserami (pełny program CAD/CAM do laserów + serwer MCP)

> Ten dokument jest kompletnym zadaniem dla agenta programistycznego (lub zespołu). Czytaj go w całości przed rozpoczęciem pracy. **Procedura kontroli zgodności z sekcji 21 jest wiążąca na każdym etapie: nie wolno pominąć, uprościć ani zastąpić żadnego wymagania bez pisemnej zgody zleceniodawcy.** Sekcje oznaczone **[MUST]** są obowiązkowe w wersji 1.0, **[SHOULD]** — w wersji 1.x, **[LATER]** — na mapie drogowej (architektura musi je umożliwiać już teraz). Wymagania oznaczone **[POSTULAT]** to funkcje, o które użytkownicy komercyjnych programów do laserów proszą od lat i których te programy nie mają (stan: październik 2026) — to one mają wyróżnić BurnCraft.

---

## 0. Rola, cel i zasady nadrzędne

Jesteś głównym architektem i programistą programu **BurnCraft** — aplikacji desktopowej do przygotowywania projektów (CAD/CAM 2D) i bezpośredniego sterowania ploterami laserowymi: diodowymi, CO2 (portalowymi), światłowodowymi/UV/CO2 galvo. Program ma obejmować **pełny zakres funkcji profesjonalnego programu do laserów** (wszystkie funkcje edycji, ustawień cięcia, podglądu, sterowania maszyną, kamer i trybów specjalnych), a ponadto:

1. Posiadać **wbudowany serwer MCP** (Model Context Protocol), pozwalający agentowi AI projektować, przygotowywać i — po spełnieniu warunków bezpieczeństwa — wykonywać zadania na laserze.
2. Obsługiwać **wszystkie popularne sterowniki laserów**, w tym **K40 ze sterownikiem Lihuiyu M2/M3 Nano** (nieobsługiwanym przez popularne programy komercyjne), DSP Ruida/Trocen/TopWisdom/Leetro/Moshi, rodzinę G-code (GRBL i pochodne) oraz galvo EZCAD2/EZCAD3/BSL.
3. Działać na **Windows 10+, macOS 11+ (x64 i Apple Silicon) oraz Linux x64 i ARM64 (w tym Raspberry Pi 4/5)** — brak wersji na Linuksa w popularnych programach komercyjnych to jedna z najczęstszych skarg.
4. Zrealizować niezrealizowane postulaty społeczności (sekcja 17).

### Zasady clean-room (obowiązkowe)
- Nie dekompiluj, nie deasembluj i nie kopiuj kodu, ikon, tekstów, dokumentacji ani zasobów innych programów do laserów. Specyfikacja opisuje **zachowanie**, nie implementację. Własna nazwa, własna identyfikacja wizualna, własne ikony, własny format pliku.
- Interoperacyjność jest dozwolona i wymagana: **import plików `.lbrn`/`.lbrn2`** (XML), import bibliotek materiałów `.clb`, import bibliotek grafik `.lbart`, import ustawień urządzeń z pakietów `.lbzip` — implementuj na podstawie analizy plików, które użytkownik sam posiada (format XML jest czytelny), nie kodu programów, które je zapisują.
- Wiedzę o protokołach sterowników czerp z publicznych, otwartych źródeł: MeerK40t (MIT — protokoły Lihuiyu, Moshi, Ruida, Balor/BJJCZ, GRBL), K40 Whisperer (GPL — tylko jako źródło wiedzy, chyba że cały projekt wybierze GPL), Balor (BJJCZ LMC), publiczne opracowania protokołu Ruida (np. wiki EduTech „Ruida”, projekty `ruida-laser`, `thunderlaser`), dokumentacja GRBL/grblHAL/FluidNC/Smoothieware/Marlin/RepRapFirmware. Zawsze respektuj licencje; w `THIRD_PARTY.md` dokumentuj każde źródło.
- Licencja projektu: zaproponuj (domyślnie **GPL-3.0-or-later** dla aplikacji, **MIT/Apache-2.0** dla biblioteki rdzenia i SDK wtyczek) i uzasadnij w `docs/adr/0001-license.md`.

---

## 1. Architektura i stos technologiczny

### 1.1 Podział na moduły [MUST]
Rdzeń ma być **bezgłowy (headless)**; GUI, CLI, serwer MCP i klient webowy to cienkie warstwy nad tym samym API poleceń.

```
burncraft/
  crates/
    bc-geom        # geometria: ścieżki Béziera, booleany, offsety, spłaszczanie, indeks przestrzenny
    bc-doc         # model dokumentu, warstwy, obiekty, undo/redo (command pattern), serializacja .bcraft
    bc-text        # fonty (TTF/OTF/SHX/Hershey/jednokreskowe), shaping (HarfBuzz), tekst zmienny
    bc-image       # przetwarzanie rastrów, dithering, trasowanie (potrace-like), maski
    bc-io          # import/eksport: SVG, DXF, AI/PDF, EPS, HPGL/PLT, Gerber, DWG*, CDR*, RLD, .lbrn2, obrazy
    bc-plan        # planer cięcia: kolejność, optymalizacja, kerf, mostki, overscan, wypełnienia, rastry
    bc-sim         # symulacja/podgląd ścieżek, estymacja czasu z modelem przyspieszeń
    bc-drivers     # sterowniki (trait Driver) + transporty (serial, USB, TCP, UDP, WebSocket, HTTP)
    bc-camera      # kamery USB/sieciowe, kalibracja obiektywu, wyrównanie, AprilTag/ArUco
    bc-materials   # biblioteka materiałów, testy materiałowe
    bc-commands    # REJESTR POLECEŃ: każda akcja = polecenie z id, schematem JSON parametrów, undo
    bc-mcp         # serwer MCP (stdio + Streamable HTTP), zasoby, prompty, polityka bezpieczeństwa
    bc-script      # API skryptowe (Python + JS/Lua), system wtyczek
    bc-cli         # burncraft-cli: operacje wsadowe, konwersja, wysyłka zadań, mcp --connect
  app/             # GUI desktop
  web/             # klient przeglądarkowy/tabletowy (ten sam frontend co GUI)
  bridge/          # BurnCraft Bridge: obraz dla Raspberry Pi łączący laser z siecią
```

### 1.2 Stos [SHOULD — możesz zmienić z uzasadnieniem w ADR]
- **Rdzeń: Rust** (bezpieczeństwo pamięci przy sterowaniu sprzętem, łatwa kompilacja na ARM). Biblioteki: `kurbo`/`lyon` (krzywe, teselacja), Clipper2 (booleany, offsety), `rstar` (R-tree), `image` + własne dithery, `rustybuzz`/`ttf-parser`/`fontdb`, `usvg` (SVG), `nusb`/`rusb` (USB), `serialport`, `tokio`, oficjalny Rust SDK MCP (`rmcp`), OpenCV lub `apriltag-rs` dla kamer.
- **GUI: Tauri 2 + TypeScript**, renderowanie obszaru roboczego w **WebGL2/WebGPU** (płynne 60 fps przy 200 tys. węzłów i 50 MPix obrazów — kafelkowanie/LOD). Alternatywa: C++20/Qt 6 z rdzeniem przez FFI.
- Ten sam frontend serwowany przez `burncraft --serve` / Bridge jako **aplikacja webowa na tablet/iPad/telefon** [POSTULAT: wersja iPadOS — 325 głosów; mobilny „Designer” — 112; projektowanie z iPada — 39; gesty pinch-zoom — 25].

### 1.3 Rejestr poleceń (fundament GUI, skryptów i MCP) [MUST]
- Każda operacja użytkownika (menu, przycisk, skrót, przeciągnięcie uchwytu, zmiana pola) jest poleceniem: `id` (np. `shape.offset`, `layer.set`, `device.jog`), opis, schemat JSON parametrów i wyniku, flagi (`undoable`, `requires_device`, `safety_tier`), domyślny skrót.
- GUI wywołuje polecenia wyłącznie przez rejestr → pełna automatyzowalność i zgodność 1:1 między GUI a MCP.
- Polecenia działają na jawnych `ids` obiektów lub na bieżącym zaznaczeniu. Wsad (`batch`) = jeden krok historii.
- Edytor skrótów klawiszowych i makr (sekwencje poleceń + surowe komendy do sterownika) budowany na tym rejestrze.

### 1.4 Tryb okna sterowanego [MUST]
`burncraft --control <port>` (domyślnie **7984**, 127.0.0.1) uruchamia GUI z kanałem sterowania (JSON-RPC/WebSocket). `burncraft-cli mcp --connect 7984` wystawia MCP po stdio, działając na żywym oknie (użytkownik widzi zmiany na bieżąco). Bez działającego okna CLI uruchamia sesję bezgłową.

---

## 2. Model dokumentu i pliki

### 2.1 Obiekty [MUST]
Ścieżka (dowolne podścieżki: linie, łuki, kubiczne i kwadratowe Béziery), prostokąt (z promieniem narożników), elipsa, wielokąt foremny, gwiazda, podwójna gwiazda, trójkąt/pięciokąt/ośmiokąt, tekst (edytowalny, na ścieżce, w ramce), tekst zmienny, kod kreskowy/2D, obraz rastrowy (z maską, przycięciem, ustawieniami obrazu), grupa, obiekt wirtualnej tablicy (dynamiczna kopia), obiekt-odniesienie (klon), obiekt pomocniczy (linia prowadząca, nie wycinana), „model 3D/mapa głębi” dla grawerowania 3D.
- Atrybuty: przypisanie do warstwy (kolor), transformacja afiniczna (nie „wypiekana” do czasu polecenia), blokada pozycji / **blokada tylko rozmiaru** [POSTULAT], widoczność, nazwa, notatka, priorytet, punkt startowy/kierunek ścieżki, tagi.
- Kształty parametryczne zachowują parametry (Okno Właściwości Kształtu) do czasu konwersji na ścieżkę.

### 2.2 Warstwy (kolory) [MUST]
- 30 domyślnych warstw kolorów + **narzędzia T1/T2** (warstwy specjalne: T1 = obszar „wytnij zaznaczone/ramka”, T2 = warstwa pomocnicza niewycinana) — z możliwością **zmiany koloru, nazwy i dodawania własnych kolorów palety** [POSTULAT: zmiana koloru warstwy 73 + 27 + 19 głosów; własna paleta 20].
- **Panel listy obiektów/rysowania (drzewo obiektów jak w programach graficznych)** z przeciąganiem, ukrywaniem, blokowaniem, zmianą kolejności rysowania i wyszukiwaniem [POSTULAT: 140 głosów].
- Nazwy warstw zapisywane per plik [POSTULAT: 27].

### 2.3 Format natywny [MUST]
- `.bcraft` = ZIP z `document.json` (wersjonowany schemat JSON, walidowany), osadzonymi obrazami (PNG/16-bit TIFF), fontami (opcjonalnie), miniaturą, notatkami. Zgodność wstecz + migracje.
- **Pliki szablonów** (`.bcraft-template`) — zapis szablonu otwiera nowy dokument.
- Autozapis co N min + odzyskiwanie po awarii + kopie zapasowe preferencji z przeglądarką kopii (BackupBrowser).
- Miniatury w Eksploratorze Windows / Quick Look / menedżerach plików Linux.
- **Wiele dokumentów w zakładkach** z kopiowaniem między nimi [POSTULAT: 33 + 25 + 24 głosów].

### 2.4 Import / eksport [MUST, chyba że zaznaczono]
| Format | Import | Eksport | Uwagi |
|---|---|---|---|
| SVG (Inkscape/Illustrator/Affinity, auto-DPI) | ✔ | ✔ | mapowanie warstw/grup SVG → warstwy cięcia wg koloru obrysu/wypełnienia **lub wg nazwy warstwy SVG** [POSTULAT 29]; zagnieżdżone SVG; obrazy osadzone |
| DXF (R12–2018, w tym splajny, bloki, łuki, tekst — opcja „ignoruj tekst”) | ✔ | ✔ (AC1009, AC1021) | jednostki z nagłówka |
| AI (PDF-based), PDF | ✔ | ✔ PDF | kanał alfa w AI |
| **EPS / PS** | ✔ (przez Ghostscript, jeśli dostępny) | — | [POSTULAT 90] |
| HPGL/PLT | ✔ | ✔ | |
| **Gerber RS-274X / Excellon** | ✔ | — | [POSTULAT 36] — dla PCB (maski, izolacja ścieżek) |
| **DWG** | [SHOULD] przez ODA/LibreDWG | — | [POSTULAT 70] |
| **CDR** | [SHOULD] przez libcdr | — | [POSTULAT 62] |
| **RLD (RDWorks)** | [SHOULD] | — | [POSTULAT 44] |
| `.rd` (Ruida) | [SHOULD] podgląd/dekodowanie | ✔ (plik dla Ruidy na USB) | |
| G-code (`.nc`, `.gc`, `.gcode`) | ✔ podgląd/uruchamianie | ✔ | |
| `.lbrn`, `.lbrn2`, `.clb`, `.lbart`, `.lbzip` | ✔ | — | interoperacyjność |
| PNG, JPG, BMP, GIF, TIFF (8/16 bit), WebP, HEIC* | ✔ | ✔ (przetworzona bitmapa) | |
| STL/OBJ → mapa głębi | [SHOULD] | — | do grawerowania 3D |
- **Eksport do PDF i drukowanie „proof”** projektu w skali 1:1 z legendą warstw [POSTULAT: Print Proof 72; Export PDF/Print 17].
- Wtyczka/makro do CorelDRAW i rozszerzenie Inkscape/Illustrator „Wyślij do BurnCraft” [SHOULD].
- **Integracje webowe** (menu „Narzędzia web”: generatory zewnętrzne wysyłające projekt do BurnCraft przez lokalny endpoint) [SHOULD].

---

## 3. Interfejs użytkownika [MUST]

- **Obszar roboczy**: siatka, linijki, obszar maszyny, widoczne położenie głowicy, nakładka obrazu z kamery, przyciąganie (siatka, obiekty, węzły, środki, punkty środkowe linii [POSTULAT 24], prowadnice automatyczne i ręczne), zoom do zaznaczenia/strony/**100% rzeczywistego rozmiaru** [POSTULAT 23], tryb ciemny/jasny/systemowy, ciemne tło obszaru, skalowanie High-DPI/4K [POSTULAT 18], gesty dotykowe i trackpada [POSTULAT 25].
- **Paski narzędzi**: główny, tworzenia, trybów, modyfikatorów, edycji numerycznej (X/Y/W/H/obrót, punkt odniesienia 9-punktowy, blokada proporcji, przełącznik mm/in **+ szybkie przeliczenie wartości mm↔in** [POSTULAT 57]), wyrównywania, węzłów; konfigurowalne, dokowalne, z zapisem układów.
- **Okna/panele**: Warstwy/Cięcia, Laser (start/pauza/stop, ramka, punkt startowy, pozycja zadania), Przesuń (jog XY/Z/oś obrotowa, kroki, prędkości **wolna/szybka** [POSTULAT 21], **zapamiętane pozycje XY z nazwami** [POSTULAT 21]), Konsola (kolorowanie składni, historia), Makra, Lista plików na sterowniku, Biblioteka materiałów, Biblioteka grafik, Kamera, Podgląd, Właściwości kształtu, Historia cofania (podgląd stanów i skok), Notatki (z obrazami [POSTULAT 21]), Menedżer fontów.
- **Tryb początkującego** (ukryte zaawansowane opcje) i pełny.
- Przyciski Start zielony / Stop czerwony (konfigurowalne kolory) [POSTULAT 60]; widoczny czas po zakończeniu zadania [POSTULAT 22]; licznik przejść „przejście 3/5” oprócz % [POSTULAT 17].
- Kursor zależny od narzędzia [POSTULAT 17]; menu kontekstowe w edycji węzłów [POSTULAT 21 + 18].
- Podpowiedzi kontekstowe i pomoc F1 do każdego pola.
- **Lokalizacja**: min. pl, en, de, fr, es, it, pt-BR, nl, cs, uk, ru, ja, zh-CN, zh-TW, ko, tr; pliki tłumaczeń w formacie Fluent/ICU, łatwe do uzupełniania przez społeczność.
- Dostępność: pełna obsługa klawiaturą, czytniki ekranu dla paneli, tryb wysokiego kontrastu, palety przyjazne daltonistom.
- **Kontroler gier (gamepad)** do jogowania, ramki, start/pauza, z edytorem mapowania [MUST].

---

## 4. Rysowanie, edycja i rozmieszczanie [MUST]

### 4.1 Tworzenie
Linie/polilinie z Bézierami (klik/przeciągnięcie), **wprowadzanie dokładnych długości i kątów podczas rysowania** [POSTULAT 43], **narzędzie łuku** (3 punkty, środek+promień+kąty) [POSTULAT 37], prostokąt, elipsa, wielokąty, gwiazdy, tekst, kody, **pomiar** (odległość, kąt), **wymiarowanie** (linie wymiarowe poziome/pionowe/wyrównane/kątowe/promień jako obiekty pomocnicze nie wycinane, z opcją wycięcia/grawerowania) [POSTULAT: 92 + 50 + 46], generator okręgów stycznych do 3 okręgów, prowadnice, **gumka/narzędzie wymazywania** dla wektorów [POSTULAT 25], **zaznaczanie lasso/odręczne** [POSTULAT 21].

### 4.2 Edycja
Edycja węzłów (dodaj/usuń/przełam/połącz/wygładź/narożnik/linia↔krzywa/ustaw punkt startowy/odwróć kierunek; **dokładna pozycja węzła numerycznie** [POSTULAT 21]; **wyrównywanie węzłów** [POSTULAT 23]), przycinanie (trim) do przecięć, nożyczki, konwersja do ścieżki, auto-łączenie z tolerancją, zamykanie ścieżek (z tolerancją), usuwanie duplikatów i nakładających się odcinków, rozbijanie, optymalizacja kształtów (redukcja węzłów, wygładzanie, łuki z odcinków) [POSTULAT: Reduce Nodes], wypaczanie i deformacja (4-punktowa, siatka, obwiednia — z tymczasowym undo), **nowe narzędzie wypaczania tekstu do dowolnego kształtu („word art”)** [POSTULAT 45 + 27 + 20], obrót/skala dwupunktowa, obrys „gumką” (convex hull), **fazowanie 45°/chamfer** i zaokrąglanie narożników (fillet) per węzeł [POSTULAT 38], **edycja wewnątrz grup bez rozgrupowania** [POSTULAT 34].

### 4.3 Modyfikacja i łączenie
Odbicie lustrzane, offset (wewn./zewn./oba, narożniki ostre/zaokrąglone/ścięte, usuwanie oryginału), booleany (suma, różnica, iloczyn, XOR, wycięcie kształtów; asystent booleanów; operacje „stosowe” na wielu obiektach), tablica prostokątna (z przesunięciem wierszy, wirtualna/dynamiczna, **losowe rozmiary/obroty/przesunięcia** [POSTULAT 73]), tablica kołowa, kopiowanie wzdłuż ścieżki, tekst po ścieżce, zaokrąglenie/fillet, zrównanie szerokości/wysokości, zmiana rozmiaru wpustów i czopów (slot/tab resizer z tolerancją materiału), mostki/tabulatory (ręczne i automatyczne, kreator).

### 4.4 Rozmieszczanie
Wyrównywanie (do zaznaczenia, pierwszego/ostatniego, strony, pozycji lasera), rozkład równomierny, dokowanie, przesuwanie o wartość, blokady, zmiana kolejności rysowania, grupowanie, **Quick Nest (prostokąty)** oraz **prawdziwy nesting kształtów** (NFP / genetyczny jak SVGnest/Deepnest: obroty co N°, odbicia, odstęp, części wewnątrz otworów, wiele arkuszy, wykorzystanie resztek, nesting na obrazie z kamery wokół nieregularnego materiału) [POSTULAT: Auto-arrange 113 + „najczęściej proszone” w społeczności].

### 4.5 Generatory [POSTULAT, SHOULD]
- **Zawiasy żywe (living hinge)** — wzory: proste, falowe, plaster miodu, spiralne; parametry: grubość, szerokość, gięcie [84].
- Generator **pudełek** (finger joint, z kerfem, z pokrywką/przegrodami), puzzli, kół zębatych, napisów pierścieniowych (ring text jak w EZCAD) [19], **słojów drewna** [30], wzorów kreskowania (fale, kropki, tekstury, kafle) [34], gradientów/ramp mocy [19].
- Każdy generator = polecenie z parametrami (dostępne też przez MCP).

---

## 5. Tekst, tekst zmienny, kody [MUST]

- Fonty systemowe, z folderu użytkownika, jednokreskowe (Hershey, SHX, SVG-fonts, „stick fonts”) do grawerowania liniowego; **menedżer fontów** z wyszukiwaniem, ulubionymi, ukrywaniem, **kategoriami i listą ostatnio używanych** [POSTULAT 25 + 24 + 17].
- Edycja tekstu bezpośrednio w obszarze roboczym i w oknie (wielowierszowy, **różne fonty/rozmiary/style w obrębie jednego tekstu** — rich text) [POSTULAT: 39 + 22 + 18 + 18], wyrównanie, odstępy, kerning, **tekst pionowy** [POSTULAT 149], zawijanie w ramce, **rozbijanie bloku tekstu na wiersze** [POSTULAT 19], **alternatywne glify OpenType i ligatury** [POSTULAT 33], spawanie liter (weld), **sprawdzanie pisowni** (Hunspell, słowniki wielojęzyczne) [POSTULAT 87].
- **Tekst zmienny**: numeracja seryjna (start/krok/zakres/format), data/czas, pola z CSV/XLSX, łączenie korespondencji (merge), podgląd rekordu, automatyczny przyrost po każdym zadaniu, pomijanie pustych linii; **wiele plików CSV w projekcie** [POSTULAT 270]; **zmienne obrazy/logo z CSV** (kolumna ze ścieżką obrazu/SVG) [POSTULAT 143]; zmienne w kodach kreskowych.
- **Kody**: 1D (Code128, Code39, Code93, EAN-8/13, UPC-A/E, ITF, Codabar, GS1-128), 2D (QR z korekcją, Data Matrix/GS1, PDF417, Aztec, MaxiCode), znaczniki AprilTag/ArUco; parametry modułu, cichej strefy, inwersji (dla anodowanego aluminium), kompensacji wypalenia.

---

## 6. Obrazy [MUST]

- Tryby: **Threshold, Ordered, Atkinson, Floyd–Steinberg („Dither”), Stucki, Jarvis, Sierra, Burkes, Newsprint, Halftone (kąt/LPI/kształt kropki), Sketch, Grayscale (modulacja mocy), 3D Slice (cięcie warstwami głębi), Depth map 8/16-bit (głębokie grawerowanie galvo), tryb cieniowania dla CO2 (dither o zmiennej długości impulsu, odpowiednik „HackTone”)**.
- Dostosowania: jasność, kontrast, gamma, ostrzenie (radius/strength), wyrównanie histogramu, krzywa mocy w skali szarości / **zaawansowana krzywa** [POSTULAT 17], **progi bieli i czerni** [POSTULAT 18], negatyw, lustro, przepuszczanie przez ustawioną rozdzielczość DPI, **„pass-through”** (bez przetwarzania), przycinanie, maska wektorowa, **usuwanie tła** (lokalny model segmentacji, offline) [POSTULAT 30], **kalibracja rozmiaru obrazu** po dwóch punktach [POSTULAT 35].
- Trasowanie obrazu do wektorów (próg, odcięcie, wygładzanie, optymalizacja, ignorowanie małych, trasowanie szkicu/linii centralnej — centerline), trasowanie wielu plików wsadowo, konwersja zaznaczenia do bitmapy, zapis przetworzonej bitmapy.
- Podgląd wyniku dithera 1:1 z uwzględnieniem DPI i średnicy plamki.

---

## 7. Warstwy i ustawienia cięcia [MUST]

### 7.1 Tryby warstwy
**Line** (kontur), **Fill** (wypełnienie skanowaniem; tryby: wszystkie kształty razem / grupy razem / każdy osobno), **Offset Fill** (wypełnienie koncentryczne), **Image**, **Fill+Line**, **Flood Fill** (obszarami, by nie skakać po pustkach).

### 7.2 Parametry wspólne
Nazwa, Output (wł./wył.), Show, Air assist, prędkość, moc maks./min. (osobno dla 2 laserów), liczba przejść, **opóźnienie między przejściami** i **pauza między warstwami (z komunikatem, np. „zmień materiał/ostrość”)** [POSTULAT: 77 + 22], Z offset i Z step na przejście, priorytet, częstotliwość PWM (gdzie wspierane), tryb stałej mocy (GRBL M3) vs dynamicznej (M4), **Kerf offset** (z **osobnym kerfem X i Y** [POSTULAT 80] i **kerfem dla Fill** [POSTULAT 28]), **wizualizacja kerfu w edytorze i podglądzie** [POSTULAT 22 + 20], mostki/tabulatory (rozmiar, liczba, odstęp, pomijanie kształtów wewnętrznych), lead-in/lead-out (linia/łuk, długość, kąt), overcut zamkniętych kształtów, **tryb perforacji** (cięcie/przerwa), **dot mode** (czas, odstęp), ramp (długość rampy mocy, tylko na zewnątrz), **rampa mocy dla narożników** (min moc w zwolnieniach), tryb **skanowania dwukierunkowego**, **kąt skanowania**, **cross-hatch**, **interwał/LPI/DPI**, **overscanning** (auto/ręczny %), **kompensacja przesunięcia skanowania (scan offset)** zależnie od prędkości (tabela), **sub-layers** (wiele przebiegów o różnych ustawieniach w jednej warstwie), wyjście „Cut through” → **zawsze cięcie na końcu** (opcja globalna: grawer najpierw, wewnętrzne przed zewnętrznymi, cięcie ostatnie) [POSTULAT 81], **ostrzeżenie, gdy warstwa Line/Cut wypada przed Fill** [POSTULAT 24].

### 7.3 Ustawienia galvo
Częstotliwość Q, szerokość impulsu (MOPA), moc, prędkość, liczba pętli, kreskowanie: do **8 hatchy** na warstwę z kątem, interwałem, przyrostem kąta per przejście, trybami (jednokierunkowy, dwukierunkowy, offsetowy, ciągły „bow”), auto-obrót kreskowania, kontur przed/po; **wobble** (średnica, odstęp, kształt), opóźnienia: start/stop/polygon/jump/mark, TC laser on/off, kompensacja końca, przyspieszenie, prędkość skoku; mark/jump delay; tryb „Split”; skok Z; **głębokie grawerowanie z mapy głębi z automatycznym krokiem Z** [MUST w 1.0 — w popularnych programach komercyjnych dostępne najwyżej jako funkcja eksperymentalna].

### 7.4 Biblioteka materiałów [MUST]
Hierarchia materiał → grubość → operacja; przypisanie do urządzenia/typu źródła; **notatki, zdjęcia, ogniskowa obiektywu** przy każdym wpisie [POSTULAT 33 + 30 + 19]; **wybór materiału bezpośrednio w edytorze ustawień warstwy** [POSTULAT 27]; łączenie bibliotek; eksport/import (CSV/JSON/.clb); **biblioteka zdalna/wspólna** (Git/HTTP/folder sieciowy) dla wielu stanowisk.

### 7.5 Testy [MUST]
- **Generator testu materiału** (siatka moc×prędkość lub dowolne 2 parametry: interwał, częstotliwość, przejścia, Q-pulse; etykiety; rozmiar do 400 mm), **z własnymi kształtami/tekstem/obrazem jako próbka** [POSTULAT 35], wysyłka bezpośrednio do lasera.
- Test ogniskowej (rampa Z lub pochylona linia), test interwału, test scan offset, **test kerfu** (wycięcie wzorca i kreator obliczania kerfu) [POSTULAT 17], test korekcji pola galvo.

---

## 8. Optymalizacja i planer [MUST]

- Kolejność: wg warstwy, priorytetu, grupy; wewnętrzne przed zewnętrznymi; **wybór najlepszego punktu startowego**; wybór kierunku; minimalizacja przejazdów (heurystyka + 2-opt); unikanie przejazdów nad wyciętymi częściami („podnoszenie” trajektorii lub objazd); **strefy zakazane dla przejazdów i wiązki** (no-go zones, np. zaciski, kamera) [POSTULAT: 19 + 18 + 19]; usuwanie nakładających się linii; redukcja zmian kierunku; cięcie zaznaczonych; użycie początku zaznaczenia; kolejność z „rzędu skanowania” dla rastrów (dzielenie na pasy).
- **Moc zależna od położenia X/Y** (mapa kompensacji mocy dla nierównej ogniskowej/zużycia lustra, oraz efekt artystyczny) [POSTULAT 37]; **różne maksymalne prędkości/przyspieszenia osi X i Y** w modelu czasu i rastrowaniu [POSTULAT 32].
- Szybki (wielowątkowy, przyrostowy) planer — przeliczenie podglądu < 1 s dla typowych projektów.

---

## 9. Podgląd, symulacja, koszty [MUST]

- Podgląd z odtwarzaniem (suwak, prędkość), pokazywanie przejazdów, kolejności, kerfu, kierunku, punktów startowych, rastrów z symulacją wypalenia (model plamki i ciepła — przybliżony).
- Estymacja czasu oparta o model przyspieszeń/jerku pobrany z ustawień sterownika (GRBL `$120/$121`, Ruida accel), **samokalibrująca się** (porównanie z rzeczywistymi czasami z historii zadań).
- **Kalkulator kosztów**: materiał (m²/arkusz), czas maszyny (stawka/h), energia, marża → cena; widoczny w podglądzie i w eksporcie oferty PDF [POSTULAT 30 + 27].
- „Rozpocznij od tego miejsca” z podglądu oraz **ręczne zaznaczanie obszarów do ponownego wycięcia (recut)**: zaznacz fragment ścieżek w podglądzie lub na obrazie z kamery → zadanie tylko dla tych odcinków [POSTULAT 258].
- **Wznawianie zadania po awarii/przerwie zasilania** od ostatniego potwierdzonego bloku (dla sterowników, które to umożliwiają; dla G-code — od numeru linii z bezpiecznym dojazdem) [POSTULAT 17].

---

## 10. Pozycjonowanie, start zadania [MUST]

- Start From: Absolute Coords / Current Position / User Origin; 9-punktowy Job Origin (także **środek-środek dla galvo**) [POSTULAT 271]; zapamiętanie w pliku.
- Ramka: prostokąt, **ramka konturowa (rubber band)**, ramka ciągła (galvo, powtarzana do zatrzymania), **ramka z czerwonym laserem z korektą offsetu**, **naprzemienne powtarzanie obu trybów ramki** [POSTULAT 26].
- Ustawienie punktu zerowego użytkownika, idź do zera, home, „przesuń laser do zaznaczenia”, „ustaw pozycję lasera” klikając w obszarze.
- Wyślij do pamięci sterownika (Ruida/Trocen) / zapisz na kartę SD / zapisz plik G-code.
- Kolejka zadań (wiele zadań w sekwencji), powtarzanie N razy z opóźnieniem, **historia zadań** (czas, ustawienia, materiał, miniatura, zużycie, wynik; ponowne uruchomienie) [POSTULAT 71], **liczniki eksploatacji i przypomnienia serwisowe** (godziny tuby/diody, czyszczenie optyki, smarowanie) [POSTULAT 56].

---

## 11. Tryby specjalne [MUST]

- **Rotary**: wałki (roller) i uchwyt (chuck), konfiguracja osi (Y lub A), mm na obrót, średnica obiektu/wałka, **zestawy zapisanych konfiguracji dla produktów** [POSTULAT 24], dzielenie na sekcje, rotacja tylko na przejazdach (galvo: rotary marking z podziałem na plastry, środek wyjściowy edytowalny wizualnie), zmienna w custom G-code dla średnicy.
- **Print & Cut**: rejestracja dwoma (i **więcej — 3/4 dla korekcji skosu i skali**) znacznikami, kamerą lub czerwonym punktem [POSTULAT: Improved print & cut 17].
- **Kamera**: patrz sekcja 12.
- **Repeat marking** i **Split marking** (galvo: duże prace z przesuwem liniowym/obrotowym), **Feeder** (podajnik/przenośnik — sekcje z przesunięciem materiału), **Center finder**, **Cylinder correction** (galvo na walcu bez rotary), **Taper warp** (stożki), **Galvo lens calibration** (korekcja pola 9/25/49-punktowa, import plików `.cor`), **Dual laser** (dwie głowice/źródła z offsetem, nazwy źródeł) — **dostępne we wszystkich edycjach** [POSTULAT 45].
- **Projekcja** (projektor nad stołem jako podgląd) i **checklista bezpieczeństwa przed startem** (edytowalna).
- **Sondy Z / autofokus**: obsługa sond (GRBL G38, Ruida autofocus, xTool/Thunder czujniki wysokości) i mapowania wysokości powierzchni (mesh) z korektą Z w zadaniu.
- **Tryb nożowy/ploter tnący, plazma, frez lekki**: offset noża ciągnionego (drag knife), narzędzie styczne, podnoszenie Z/M-kody narzędzia, wyjście dla ploterów winylowych (HPGL) [POSTULAT: 49 + 38 + 18 + 30 „Tool release”] — [SHOULD], przez abstrakcję „Tool” w urządzeniu.

---

## 12. Kamery [MUST]

- Kamery USB (UVC), sieciowe (RTSP/MJPEG/HTTP snapshot), kamery wbudowane popularnych maszyn (xTool, Thunder, WeCreat, Gweike, Omtech — przez ich API, jeśli publiczne lub dokumentowalne), **kamery USB podłączone do Bridge** [POSTULAT 19], **dwie i więcej kamer** (widok mozaiki, zszywanie).
- Kalibracja obiektywu (szachownica/kropki/**AprilTag** z wynikiem jakości), wyrównanie kamery do obszaru lasera (wzorzec wypalony + kliknięcia lub automatycznie przez znaczniki), kamera na głowicy (skanowanie i składanie obrazu całego stołu), ustawienia ekspozycji/balansu/ostrości, presety, zapis tła, rozmycie tła, przechwycenie surowe i zrektyfikowane.
- Funkcje: aktualizacja nakładki, **śledzenie pozycji materiału** (wykrywanie krawędzi i obrotu arkusza, automatyczne dopasowanie projektu), nesting na materiale z kamery, rozpoznawanie znaczników do print&cut, wykrywanie wcześniej wyciętych obszarów (resztki).

---

## 13. Sterowniki i maszyny [MUST — rdzeń projektu]

### 13.1 Abstrakcja
```text
trait Driver {
  fn capabilities() -> Caps;          // osie, rotary, Z, raster natywny, pamięć plików, air, 2 lasery, kamera, sonda, czujnik pokrywy...
  async fn connect(transport) / disconnect();
  async fn status() -> MachineState;  // pozycja, stan (idle/run/hold/alarm), pokrywa, chłodzenie, temp., błędy
  async fn home(), jog(axis, dist, speed), move_to(), set_origin();
  async fn frame(kind), test_fire(power, ms);        // test_fire = safety tier "fire"
  async fn send_job(Job) -> JobHandle;               // streaming lub upload pliku
  async fn pause(), resume(), stop();                 // stop: zawsze dozwolony, najwyższy priorytet
  async fn read_settings() / write_settings();        // ustawienia maszyny (z backupem przed zapisem)
  async fn list_files() / run_file() / delete_file(); // gdy wspierane
  fn compile(Job) -> Vec<u8> / String;                // offline: eksport pliku dla sterownika
}
```
Kompilator zadań (bc-plan → strumień poleceń ruchu i mocy) jest niezależny od transportu; każdy sterownik ma **emulator/symulator** do testów (sekcja 19).

### 13.2 Transporty
Serial/USB-CDC (FTDI, CH340, CP210x), **USB bulk przez libusb/WinUSB** (z kreatorem instalacji sterownika na Windows i regułami udev na Linuksie), TCP, UDP, WebSocket, HTTP/REST, Wi-Fi (mDNS/UDP discovery), **BurnCraft Bridge** (RPi w sieci zespala USB/serial/UDP z LAN-em, z obsługą kamer), wieloinstancyjność (kilka laserów naraz z jednego komputera), wybór urządzenia z CLI (`--device "Nazwa"`).

### 13.3 Lista obsługiwanych rodzin
| Rodzina | Modele / firmware | Transport | Uwagi implementacyjne |
|---|---|---|---|
| **K40 – Lihuiyu (LHYMICRO-GL)** | **M2 Nano**, M3 Nano, M1, M, B1, B2, B, A, 6C6879-LASER-M2/M3 | USB CH341 (VID 0x1A86 PID 0x5512) — natywnie przez libusb **lub** przez oryginalny sterownik CH341 DLL (Windows) | Protokół „EGV”: pakiety 30 bajtów + CRC-8 (Onewire), potwierdzenia stanu (OK/BUSY/CRC error/task complete), komendy kierunku/kroku z prędkością kodowaną zależnie od wariantu płyty, raster natywnie „step mode” (krok co N), brak PWM — moc tylko potencjometrem → UI musi to jasno pokazywać (ustawienie mocy w BurnCraft nieaktywne; zamiast niego dither/prędkość). Konfiguracja: wariant płyty, ID płyty (serial), kroki/mm (1000 dpi), korekcja skali X/Y, home, „rapid” vs ruch powolny, pauza/wznowienie. Wiedza: MeerK40t (moduł `lihuiyu`), K40 Whisperer. **To musi działać „od pudełka” dla niezmodyfikowanego K40.** |
| K40 po modernizacji | Cohesion3D Mini/LaserBoard, Awesome.tech, SKR + GRBL-LPC/Smoothie/grblHAL | Serial/USB | przez rodzinę G-code |
| **Moshi** | Moshidraw MS10105 (V4.x) | USB CH341 | protokół Moshi (wg MeerK40t), plik całego zadania wysyłany blokami |
| **GRBL** | GRBL 0.9/1.1(f/h), **grblHAL**, **FluidNC**, GRBL-LPC, GRBL-ESP32, GRBL-M3 (diody), odmiany producentów (Ortur, Atomstack, Sculpfun, Creality Falcon, Two Trees, NEJE (G-code), Comgrow, Longer, Snapmaker GRBL, Algolaser, Wainlux, LaserPecker? — gdy G-code) | Serial, TCP (Telnet 23), **WebSocket (FluidNC/ESP3D)** | strumieniowanie „character counting” (bufor 127/RX) i tryb „send-response”, zapytania `?` w tle, nadpisania czasu rzeczywistego (feed/power override), `$J=` jog, `$H`, `$X`, `$$` odczyt/zapis z kopią zapasową, kody błędów/alarmów z opisami, `$32=1` (laser mode) i `$30` (S-max) — kreator sprawdzający konfigurację, M3/M4, kompresja G-code (pomijanie powtórzeń, skrócone współrzędne), rastry z `S` per piksel, **M73 postęp**, obsługa 2 laserów (M3/M4 + wybór narzędzia), air assist M7/M8, rotary oś A lub Y |
| **Smoothieware** | Smoothieboard, C3D | Serial/USB, TCP | „fire” `fire`, raster przez S |
| **Marlin** | 2.x z LASER_FEATURE / Inline laser | Serial | `M3 I` (inline), synchronizacja `M400` |
| **RepRapFirmware / Duet** | Duet 2/3 (M452) | HTTP/serial | |
| **CNC G-code ogólny** | Mach3/Mach4, UCCNC, **Masso**, LinuxCNC, Snapmaker | plik / serial | szablony nagłówka/stopki, edytowalny blok G20/G21, makra pre/post per warstwa i per zadanie [POSTULAT: Gcode pre/post scripts] |
| **xTool** | D1/D1 Pro/F1/S1/M1/P2/F2… | USB (G-code), **Wi-Fi (HTTP API)** [POSTULAT 58 + 47 + 19], kamera i czujnik wysokości P2 [POSTULAT 26] | |
| **Ruida** | RDC6442G/S, RDC6445G/GZ, RDC6332G/M, RDLC-320A, RDC6422G, R5-DSP, RDV6442G, KT332N (ARM), RD8445, Hydra/GT5 | **UDP 50200→40200** (Ethernet), USB-serial, plik `.rd` na pendrive | „swizzling” bajtów (magic 0x88 domyślnie, zależny od modelu — konfigurowalny), suma kontrolna pakietów UDP, potwierdzenia ACK/NACK, wysyłka całego zadania do pamięci, start/pauza/stop, ramka, read/write parametrów maszyny (zakres, przyspieszenia, offset lasera 2, autofokus, rotary), lista/uruchamianie/usuwanie plików w pamięci, odczyt stanu, raster natywny (scan offset), mapowanie osi U/Z KT332N (wyłączalne), wrapper UDP zgodny z ZhiYuan |
| **Trocen (AWC)** | AWC708C/LITE, AWC608, AWC7813, TL-3120 | Ethernet/USB | protokół zbliżony koncepcyjnie (inne kodowanie), upload `.ud`? (zbadać) |
| **TopWisdom** | TL-403/TL-410/TL-3120? (zbadać zgodność) | USB/Ethernet | |
| **Leetro** | MPC6515/6525/6535 | USB/Ethernet | [POSTULAT 46] — [SHOULD], wymaga RE z zachowaniem zasad prawa (interoperacyjność, art. 75 ust. 2 pkt 3 ustawy o prawie autorskim / dyrektywa 2009/24/WE) |
| **LightObject** | X7 DSP, **T9 DSP** [POSTULAT 20] | Ethernet | |
| **Gweike** | G2, M3 Ultra [POSTULAT 163] | do zbadania | |
| **Galvo BJJCZ (EZCAD2)** | LMCV4-FIBER-M, LMCV4-DIGIT, LMC-CO2, UV, MOPA (JPT/Raycus/MAX) | USB (VID 0x9588 PID 0x9899) | protokół listowy (wg Balor/MeerK40t): bloki 0x0C00 poleceń 12-bajtowych, ustawienia lasera (Q, moc, MO, PRF, FPK), opóźnienia, port I/O (pedał, czujniki), oś rotary/ Z przez porty kroków, czerwona ramka, korekcja pola (`.cor` EZCAD), **sterowanie osią Z i auto-krok Z w zadaniu** (funkcja wycofana z popularnych programów komercyjnych) |
| **Galvo EZCAD3 (DLC2-M4)** | DLC2-M4-2D/3D | USB | [MUST w 1.x] markowanie, rotary, split, Z |
| **Galvo BSL** | BSL (Beijing) | USB | [SHOULD], kalibracja |
| **Inne** | Emblaser 2/3/Pro, Cubiio 2, Thunder (Ruida), WeCreat (G-code/HTTP), Omtech (Ruida), Glowforge (zamknięta chmura — **nieobsługiwane**, wyjaśnić w FAQ), Trotec/Epilog/ULS (sterowniki drukarkowe — [LATER] eksport przez drukarkę wirtualną/sterownik producenta) [POSTULAT: Trotec 22] | | |
| **Wirtualna drukarka BurnCraft** | — | system druku | „drukuj” z dowolnego programu → dokument w BurnCraft [POSTULAT 36] |
| **Urządzenie fikcyjne/symulator** | — | — | praca bez maszyny, pełny podgląd i eksport |

- **Kreator „Znajdź mój laser”** (skanowanie portów, USB VID/PID, UDP broadcast Ruida, mDNS) + ręczne tworzenie profilu + **pakiety producentów** (profile urządzeń, biblioteki materiałów, menu z linkami producenta) importowane przeciągnięciem.
- Edytor „Ustawień maszyny” (parametry w sterowniku) z opisami, walidacją zakresów, kopią przed zapisem i diffem.
- Jasno dokumentowana macierz zgodności (`docs/hardware-matrix.md`) z poziomem: *zweryfikowane na sprzęcie / na emulatorze / eksperymentalne*.

---

## 14. Automatyzacja, skrypty, integracje

- **API skryptowe i wtyczki** [POSTULAT: 114 + 79 + 40 + 38]: Python (osadzony) i JavaScript; dostęp do całego rejestru poleceń, zdarzeń (otwarcie pliku, start/koniec zadania, błąd, zmiana stanu maszyny), własnych paneli UI (HTML), własnych generatorów, importerów i sterowników (trait w Rust + ABI dla wtyczek dynamicznych lub WASM). Menedżer wtyczek z podpisem i uprawnieniami. [MUST: API skryptowe; SHOULD: wtyczki UI]
- **CLI**: `burncraft-cli open|convert|plan|estimate|export|send|status|mcp` — wsadowe przygotowanie plików, CI dla produkcji.
- **UDP automation** (zgodna koncepcyjnie z „SendUDP”: `LOADFILE:path`, `START`, `STOP`, `PAUSE`, `STATUS`, `FORCECLOSE`, `LASER:Nazwa`) — dla istniejących skryptów użytkowników.
- **Webhooki, MQTT, integracja Home Assistant** (stan maszyny, postęp, alarmy; sterowanie wyciągiem, sprężarką, chłodziarką) [POSTULAT: 19 + Webhooks].
- **Wspólne ustawienia dla wielu użytkowników** (folder sieciowy, profile użytkowników, tryb tylko-do-odczytu dla operatorów).

---

## 15. Serwer MCP [MUST]

### 15.1 Transport i tryby
- **stdio** (przez `burncraft-cli mcp` — tryb bezgłowy lub `--connect <port>` do żywego okna) oraz **Streamable HTTP** na `127.0.0.1` (opcjonalnie LAN z tokenem i TLS). Zgodność z aktualną specyfikacją MCP (narzędzia ze `structuredContent` i `outputSchema`, zasoby z subskrypcjami, prompty, powiadomienia o postępie, anulowanie, **elicitation** do potwierdzeń).
- Instrukcje serwera (pole `instructions`) krótko opisują: jednostki (mm, y w dół lub w górę — **zdecyduj i opisz**; zalecam: współrzędne obszaru roboczego w mm, y w górę zgodnie z maszyną, origin wg profilu), że każda akcja to polecenie, typowy przepływ i zasady bezpieczeństwa.

### 15.2 Narzędzia (tools)
Ogólne (odbicie rejestru poleceń):
- `list_commands {filter?}`, `describe_command {id}`, `execute {id, params}`, `batch {steps[]}` (jeden krok historii), `undo`, `redo`.
- `inspect_document {detail?}` — warstwy (ustawienia), obiekty (id, typ, warstwa, bbox, transformacja, długość ścieżki, czy zamknięta), zaznaczenie, statystyki.
- `query_objects {layer?, type?, bbox?, text?}`, `get_object {id}`.
- `render_preview {mode: design|toolpath|burn_sim, bbox?, max_size?}` → PNG; `screenshot` (okno).
- `open_file`, `save_file`, `import_file {path, options}`, `export_file {path, format}`.

Projektowe (wygodne skróty na polecenia):
- `draw_shape {type, params, layer}`, `draw_path {d|points, closed, layer}`, `add_text {text, font, size, pos, path_id?, variable?}`, `add_barcode`, `place_image {path, dpi, mode}`, `trace_image`, `transform {ids, translate/rotate/scale/mirror, anchor}`, `boolean`, `offset`, `array_grid`, `array_circular`, `align`, `distribute`, `nest {ids, sheet, spacing, rotations}`, `generate {generator: box|living_hinge|gear|..., params}`, `set_layer {color, settings}`, `assign_layer {ids, color}`.

Materiały i planowanie:
- `material_library_search {query}`, `apply_material {layer, entry_id}`, `create_material_test {params}`.
- `plan_job {selection_only?}` → statystyki: czas szacowany, długość cięcia/przejazdów, liczba przejść, ostrzeżenia (otwarte kształty w Fill, poza obszarem, duplikaty, cięcie przed grawerem, moc > limitu), koszt.
- `export_machine_file {driver, path}` (G-code, .rd itp.).

Maszyna (podlegają polityce bezpieczeństwa 15.4):
- `list_devices`, `select_device`, `connect`, `disconnect`, `device_status` (także jako zasób z subskrypcją).
- `home`, `jog {axis, distance, speed}`, `move_to`, `set_origin`, `frame {kind}` — tier **motion**.
- `start_job {job_plan_id, confirm_token?}`, `test_fire` — tier **fire**.
- `pause_job`, `resume_job`, **`stop_job` (zawsze dozwolone, bez potwierdzeń)**.
- `read_machine_settings`, `write_machine_settings {changes, confirm_token}` — tier **config** (zawsze z kopią i diffem).
- `camera_capture {camera?, rectified}` → PNG, `camera_update_overlay`, `detect_material_outline`.

### 15.3 Zasoby i prompty
- Zasoby: `burncraft://document`, `burncraft://object/{id}`, `burncraft://layer/{color}`, `burncraft://commands`, `burncraft://command/{id}`, `burncraft://device/{id}/status` (subskrypcja), `burncraft://job/{id}` (postęp, log), `burncraft://materials`, `burncraft://history/jobs`.
- Prompty: `prepare_cut_file` (sprawdź otwarte kształty, duplikaty, kerf, kolejność), `engrave_photo`, `make_box`, `run_material_test`, `batch_serial_numbers`.

### 15.4 Bezpieczeństwo (laser = ryzyko pożaru i obrażeń) [MUST]
- Poziomy: `read` < `design` < `motion` < `config` < `fire`. Domyślna polityka dla klientów MCP: `read`+`design` bez pytań; `motion` po potwierdzeniu użytkownika; `config` i `fire` **wyłącznie po jawnym potwierdzeniu w GUI BurnCraft** (okno dialogowe z miniaturą, czasem, maks. mocą, materiałem, urządzeniem) lub przez MCP elicitation, gdy GUI nieobecne — konfigurowalne w Ustawieniach (nigdy nie można wyłączyć potwierdzeń `fire` dla połączeń nielokalnych).
- Tokeny potwierdzenia są jednorazowe, powiązane z konkretnym `job_plan_id` i hashem planu (zmiana dokumentu unieważnia token).
- Globalne limity: maks. moc, maks. czas zadania, wymagany stan pokrywy/chłodzenia/wyciągu (gdy sterownik raportuje), wymagany „operator obecny” (np. przycisk w GUI w ostatnich N minutach).
- Pełny dziennik audytu wszystkich akcji MCP (kto/kiedy/co/parametry/wynik).
- `stop_job` i fizyczny E-stop zawsze nadrzędne; utrata połączenia klienta MCP w trakcie zadania nie przerywa go (zachowuje się jak GUI), ale watchdog zatrzymuje ruchy jog.

---

## 16. Wydajność, niezawodność, jakość [MUST]

- 100 000 obiektów / 2 mln węzłów: płynne przewijanie i zaznaczanie; obraz 20 000×20 000 px: podgląd dithera kafelkowo < 3 s.
- Strumieniowanie bez „zacięć” (G-code: ≥ 1000 linii/s na RPi 4; bufor wstępny; monitor niedopełnienia bufora z ostrzeżeniem).
- Brak blokowania UI przy operacjach > 100 ms (wątki robocze, anulowanie, paski postępu).
- Deterministyczne wyniki planera (te same wejścia → ten sam strumień bajtów) — warunek testów golden.
- Telemetria wyłącznie opt-in; pełne działanie offline; **brak DRM/licencji online** (często krytykowane w programach komercyjnych), aktualizacje z podpisem.
- Logowanie z poziomami, „Zbierz informacje do pomocy” (paczka: ustawienia bez danych osobowych, log, wersje).

---

## 17. Niezrealizowane postulaty społeczności (backlog wyróżniający) — podsumowanie

Źródło: publiczna tablica głosowań użytkowników komercyjnego programu do laserów (Fider, sortowanie „most wanted”, październik 2026) oraz forum. Każdy punkt jest już wpięty w sekcje wyżej; tutaj priorytetyzacja:

**Priorytet A (1.0):** Linux x64/ARM64 + Raspberry Pi • klient webowy/tablet (iPad) • wiele CSV i zmienne obrazy w tekście zmiennym • ręczne zaznaczanie obszarów do ponownego cięcia • tekst pionowy • panel listy obiektów • API skryptowe/sterowanie z zewnątrz (MCP + Python) • prawdziwy nesting kształtów • wymiarowanie • import EPS • sprawdzanie pisowni • generator zawiasów żywych • „cięcie zawsze na końcu” • osobny kerf X/Y i kerf dla Fill • pauza/opóźnienie między warstwami i przejściami • zmiana kolorów warstw/własna paleta • losowe tablice • proof/PDF/druk • historia zadań • liczniki serwisowe • K40 M2 Nano • xTool Wi-Fi • dwa lasery w każdej edycji • wpisywanie dokładnych długości przy rysowaniu • narzędzie łuku • fazowanie • galvo: Z na żywo, auto-krok Z, origin środek-środek, pełne EZCAD3.
**Priorytet B (1.x):** DWG, CDR, Gerber, RLD • system wtyczek z UI • nóż ciągniony/winyl/plazma • Leetro, LightObject T9, Gweike • moc zależna od XY • kalkulator kosztów • generator słojów drewna i dodatkowe wzory kreskowania • usuwanie tła • edycja w grupach • alternatywne glify OpenType • zakładki dokumentów • gumka wektorowa • kategorie/ostatnie fonty • strefy zakazane • wznawianie po awarii • gradient/rampa mocy • ring text • Home Assistant/MQTT • test kerfu • print&cut na 3–4 znaczniki • notatki ze zdjęciami w bibliotece materiałów/grafik • biblioteka grafik z podkategoriami, tagami, folderami i drukiem katalogu [63 + 31 + 23 + 22 + 21].
**Priorytet C (LATER):** natywne aplikacje iOS/Android, wirtualna drukarka na wszystkich OS, sterowniki drukarkowe Trotec/Epilog/ULS, współpraca wielu użytkowników na jednym dokumencie.

---

## 18. Dokumentacja [MUST]
- Podręcznik użytkownika (pl + en) generowany ze źródeł Markdown, kontekstowa pomoc F1 dla każdego polecenia (z rejestru), samouczki „pierwszy projekt”, „pierwszy test materiału”, przewodniki per sterownik (w tym **K40 krok po kroku: instalacja sterownika USB, wybór płyty, kalibracja kroków**), rozwiązywanie problemów (jakość, połączenie, GRBL błędy, Ruida sieć, galvo sterowniki).
- Dokumentacja MCP: lista narzędzi z przykładami JSON, przykładowa konfiguracja klienta (`claude_desktop_config.json`, Claude Code `.mcp.json`), polityka bezpieczeństwa.
- ADR-y dla kluczowych decyzji; `docs/hardware-matrix.md`; `CONTRIBUTING.md` z instrukcją dodawania sterownika.

---

## 19. Testy i kryteria akceptacji [MUST]

- **Testy jednostkowe** geometrii (booleany/offsety na zbiorze trudnych przypadków, fuzzing `cargo-fuzz` dla importerów SVG/DXF/.lbrn2).
- **Testy golden** kompilatorów sterowników: projekt referencyjny → bajty/G-code porównane z zatwierdzonym plikiem; zmiany wymagają świadomej aktualizacji.
- **Emulatory**: GRBL (np. grbl-sim + własny symulator bufora), Ruida (serwer UDP odpowiadający ACK, dekodujący swizzling i wizualizujący ruch), Lihuiyu (emulator pakietów z CRC i stanami BUSY/OK), BJJCZ (emulator list poleceń), xTool HTTP (mock). Każdy emulator renderuje wynik do PNG, porównywanego z podglądem planera (tolerancja w px).
- **Testy E2E GUI** (Playwright na frontendzie) oraz **testy MCP** (klient testowy wykonujący scenariusze: „zaprojektuj pudełko 100×60×40 ze sklejki 3 mm, wygeneruj plan, wyeksportuj G-code”; „spróbuj start_job bez tokenu → odmowa”).
- **Testy sprzętowe** (ręczne, z checklistą) na: K40 M2 Nano, GRBL diodowy, Ruida RDC6445, galvo LMCV4 — wyniki w macierzy sprzętu.
- CI: Windows, macOS (x64+arm64), Linux (x64+arm64), budowa paczek MSI/DMG/AppImage/deb/Flatpak + obraz Bridge.

**Kryteria akceptacji 1.0:** (1) projekt `.lbrn2` z 10 warstwami importuje się i daje identyczną geometrię (± 0,01 mm); (2) K40 M2 Nano: jog, home, ramka, cięcie wektorowe i grawer rastrowy działają na sprzęcie; (3) GRBL i Ruida: pełny cykl zadania z pauzą/wznowieniem/stopem; (4) galvo LMCV4: markowanie z hatchami i rotary; (5) agent MCP potrafi bez GUI wykonać scenariusz testowy end-to-end aż do eksportu pliku, a start lasera wymaga potwierdzenia użytkownika; (6) wszystkie wymagania [MUST] z rejestru `REQUIREMENTS.md` mają status `verified` (test lub udokumentowana procedura ręczna) albo zatwierdzone odstępstwo w `DEVIATIONS.md`, a `tools/req-check` przechodzi w CI (sekcja 21).

---

## 20. Sposób pracy i kamienie milowe

1. **M0 — fundamenty (rejestr wymagań `REQUIREMENTS.md` wg sekcji 21.1, ADR, szkielet repo, CI, rejestr poleceń, model dokumentu, format `.bcraft`, MCP „read/design” na pustym dokumencie).**
2. **M1 — edytor wektorowy** (sekcje 2–4), import/eksport SVG/DXF/.lbrn2, podgląd.
3. **M2 — planer + G-code/GRBL** (sekcje 7–10), emulator GRBL, testy golden, MCP plan/export.
4. **M3 — K40 Lihuiyu + Ruida** (emulatory, potem sprzęt).
5. **M4 — obrazy, tekst zmienny, kody, biblioteka materiałów, testy materiałowe.**
6. **M5 — galvo (BJJCZ), rotary, kamery, print&cut.**
7. **M6 — postulaty priorytetu A, klient webowy/Bridge, skrypty, dopracowanie MCP (tier fire z potwierdzeniami).**
8. **M7 — stabilizacja, dokumentacja, wydanie 1.0.**

Na każdym etapie: najpierw plan etapu z listą identyfikatorów wymagań (REQ-…), które etap obejmuje, i ADR; potem testy, potem implementacja; na koniec **przegląd zgodności wg sekcji 21.3** i raport etapu. Nie oznaczaj funkcji jako gotowej bez testu. Gdy szczegół protokołu jest niepewny, oznacz go `// UNVERIFIED:` z odnośnikiem do źródła i zaplanuj weryfikację na sprzęcie lub z przechwytu USB/sieci dostarczonego przez użytkownika.

Zacznij od M0, krok 1: zbuduj `REQUIREMENTS.md` (sekcja 21.1) i przedstaw jego podsumowanie (bez czekania na akceptację, chyba że już na tym etapie widzisz konieczne odstępstwa). Następnie: struktura repozytorium, ADR-y 0001–0005 (licencja, stos, układ współrzędnych i jednostki, format pliku, model bezpieczeństwa MCP) i pierwsza działająca wersja `burncraft-cli mcp` z narzędziami `list_commands`, `execute`, `inspect_document`, `render_preview`.

---

## 21. Kontrola zgodności ze specyfikacją i zarządzanie odstępstwami [MUST — obowiązuje przez cały projekt]

Cel: żadne wymaganie tej specyfikacji nie może „zginąć” po drodze. Pominięcie lub zmiana funkcji jest dopuszczalna wyłącznie jako **jawne, zatwierdzone odstępstwo**.

### 21.1 Rejestr wymagań (`REQUIREMENTS.md` + `requirements.yaml`)
- Przed napisaniem jakiegokolwiek kodu rozbij całą specyfikację (sekcje 0–20) na **atomowe wymagania**: jedno wymaganie = jedna sprawdzalna funkcja lub właściwość. Każdy punkt listy, każdy wiersz tabeli (format, sterownik), każdy parametr ustawień warstwy, każdy tryb dithera, każde narzędzie MCP i każdy [POSTULAT] to osobna pozycja. Wyliczenia w jednym zdaniu („Threshold, Ordered, Atkinson…”) rozbij na osobne wymagania.
- Każde wymaganie ma: `id` (`REQ-<sekcja>-<nr>`, np. `REQ-06-004`), cytat lub parafrazę ze specyfikacji z numerem sekcji, priorytet ([MUST]/[SHOULD]/[LATER]), etap docelowy (M0–M7), **kryterium weryfikacji** (test automatyczny / emulator / sprzęt / przegląd ręczny), status (`todo` → `in_progress` → `done` → `verified`, albo `deviation`), odnośniki do kodu i testów.
- **Kontrola kompletności rejestru:** dołącz zestawienie „sekcja → liczba wymagań” oraz listę fragmentów specyfikacji, których świadomie nie uznałeś za wymaganie (np. zdania opisowe), z uzasadnieniem. Przedstaw zleceniodawcy podsumowanie rejestru (do wglądu — nie wymaga akceptacji) i kontynuuj pracę. Jeśli przy rozbijaniu specyfikacji wykryjesz sprzeczności lub wymagania niewykonalne, zgłoś je jako odstępstwa (21.4).
- Rejestr jest jedynym źródłem prawdy o zakresie. Specyfikacja nie może być zmieniana „po cichu” — zmiana zakresu = odstępstwo (21.4) lub nowe wymaganie dodane za zgodą.

### 21.2 Śledzenie w kodzie i testach
- Testy oznaczaj identyfikatorami wymagań (np. atrybut/nazwa `req_06_004_atkinson_dither`); polecenia w rejestrze poleceń mają pole `requirements: [REQ-…]`.
- Skrypt `tools/req-check` (uruchamiany w CI) generuje macierz śledzenia `docs/traceability.md` i **kończy się błędem**, gdy: wymaganie przypisane do zakończonego etapu nie ma statusu `verified` lub zatwierdzonego `deviation`; wymaganie `done/verified` nie ma testu ani udokumentowanej procedury ręcznej; test odwołuje się do nieistniejącego REQ; status `deviation` nie ma odnośnika do zatwierdzonego wpisu w `DEVIATIONS.md`.

### 21.3 Przegląd zgodności po każdym etapie (bramka etapu)
Po zakończeniu każdego etapu, **zanim rozpoczniesz następny**, wykonaj poniższe kroki i przedstaw zleceniodawcy raport (do wglądu). Akceptacja zleceniodawcy jest wymagana **wyłącznie dla odstępstw** — etap bez odstępstw zamykasz sam i przechodzisz dalej:
1. **Ponowne przeczytanie** sekcji specyfikacji objętych etapem (nie polegaj na pamięci ani na wcześniejszym streszczeniu) i porównanie ich z rejestrem — czy nic nie zostało pominięte przy rozbijaniu.
2. **Weryfikację każdego REQ etapu na działającym programie**: uruchomienie testu, emulatora lub programu (GUI/CLI/MCP) i sprawdzenie, że funkcja faktycznie istnieje, jest dostępna z interfejsu i z rejestru poleceń/MCP oraz robi to, co opisuje specyfikacja. Sam fakt istnienia kodu nie jest dowodem.
3. **Raport zgodności** w `docs/reviews/M<n>-review.md` i w odpowiedzi, w formie tabeli: `REQ | opis | status | dowód (test/zrzut/log) | uwagi`, z podsumowaniem: liczba wymagań etapu, zweryfikowane, częściowe, brakujące, odstępstwa. Częściowe i brakujące wymień **na początku** raportu, nie na końcu.
4. **Przegląd regresji**: uruchomienie pełnego zestawu testów i `req-check` — wymagania z wcześniejszych etapów nie mogą stracić statusu `verified`.
5. **Zamknięcie etapu.** Etap jest zamknięty, gdy wszystkie jego wymagania mają status `verified` albo zatwierdzone odstępstwo, a `req-check` przechodzi. Brakujące lub częściowe wymagania najpierw dokończ; jeśli nie da się ich wykonać zgodnie ze specyfikacją, zgłoś odstępstwo (21.4) i nie zamykaj etapu do czasu decyzji zleceniodawcy. Nigdy nie przenoś ich po cichu do kolejnego etapu. Jeśli etap jest zamknięty bez otwartych odstępstw, przejdź do następnego bez czekania.

### 21.4 Odstępstwa — tylko za zgodą
- Odstępstwem jest każde: pominięcie wymagania, przesunięcie do późniejszego etapu, częściowa realizacja, zmiana zachowania względem specyfikacji, zastąpienie technologii wskazanej jako [MUST], obniżenie progu wydajności lub zmiana decyzji z ADR.
- Gdy w trakcie pracy stwierdzisz, że odstępstwo jest konieczne (niemożliwe technicznie, sprzeczne wymagania, brak dostępu do sprzętu/dokumentacji, ryzyko prawne lub bezpieczeństwa), **przerwij pracę nad tym wymaganiem i zapytaj zleceniodawcę od razu** — nie czekaj na koniec etapu i nie wybieraj rozwiązania samodzielnie. Prace niezależne od tej decyzji możesz kontynuować.
- Pytanie o odstępstwo zawiera: `REQ`, cytat ze specyfikacji, powód, co dokładnie zostało sprawdzone, **2–3 warianty** (w tym możliwie najbliższy specyfikacji) z kosztem i skutkami, rekomendację. Zadawaj jedno odstępstwo na pytanie.
- Każda decyzja trafia do `DEVIATIONS.md`: `DEV-<nr> | REQ | opis | powód | wybrany wariant | data | decyzja zleceniodawcy (cytat)`. Odstępstwo bez zapisanej decyzji zleceniodawcy jest traktowane jak błąd.
- Rozstrzygnięcie niejednoznaczności w specyfikacji, które nie zmienia funkcji (np. wybór jednej z dwóch równoważnych bibliotek), nie wymaga zgody, ale musi być zapisane w ADR i wymienione w raporcie etapu.

### 21.5 Odbiór końcowy
Przed wydaniem 1.0: pełny przegląd całego rejestru (wszystkie sekcje, nie tylko ostatni etap), raport `docs/reviews/release-1.0.md` z listą wszystkich wymagań [MUST] i ich dowodów, zestawienie wszystkich odstępstw oraz lista wymagań [SHOULD]/[LATER] przeniesionych do mapy drogowej. Raport jest do wglądu; akceptacji wymagają tylko odstępstwa, które nie zostały jeszcze zatwierdzone.
