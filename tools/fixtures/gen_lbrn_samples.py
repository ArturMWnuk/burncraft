"""Próbki `.lbrn2` (FormatVersion 1) i `.lbrn` (FormatVersion 0) do testów importu.

Pliki są napisane od zera przez ten skrypt (projekt BurnCraft): zawierają tylko elementy,
które czyta importer `crates/bc-io/src/lbrn.rs` (`CutSetting`, `Shape` z `XForm`), bez
miniatur, metadanych i ustawień innych programów. Kształty mają parametry z prób
użytkownika (wymiary i położenia), więc testy `crates/bc-io/tests/lbrn_samples.rs`
sprawdzają te same wartości. Kontury napisu „Lorem ipsum” to własny, prosty krój
blokowy (odcinki i krzywe Béziera), nie kontury fontu.

Uruchomienie z katalogu głównego repozytorium:  python tools/fixtures/gen_lbrn_samples.py
"""

from __future__ import annotations

import math
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
FIX = ROOT / "crates" / "bc-io" / "tests" / "fixtures"
FUZZ = ROOT / "fuzz" / "corpus" / "lbrn2_import"


def f(v: float) -> str:
    """Liczba bez zbędnych zer (do 6 miejsc po przecinku)."""
    s = f"{v:.6f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


# ---------- ścieżki: wierzchołki z uchwytami i prymitywy ----------

class Path:
    """Wierzchołki (x, y, c0, c1) i prymitywy (typ, i, j); kontury zamknięte."""

    def __init__(self) -> None:
        self.verts: list[tuple[float, float, tuple | None, tuple | None]] = []
        self.prims: list[tuple[str, int, int]] = []

    def polygon(self, pts: list[tuple[float, float]], dx: float = 0, dy: float = 0) -> None:
        base = len(self.verts)
        for x, y in pts:
            self.verts.append((x + dx, y + dy, None, None))
        n = len(pts)
        for k in range(n):
            self.prims.append(("L", base + k, base + (k + 1) % n))

    def ellipse(self, cx: float, cy: float, rx: float, ry: float, ccw: bool = True) -> None:
        """Elipsa z 4 krzywych Béziera (uchwyty k = 0,5523)."""
        k = 4 * (math.sqrt(2) - 1) / 3
        base = len(self.verts)
        angles = [0, 90, 180, 270] if ccw else [0, 270, 180, 90]
        sign = 1 if ccw else -1
        for a in angles:
            t = math.radians(a)
            x, y = cx + rx * math.cos(t), cy + ry * math.sin(t)
            # styczna w kierunku obiegu
            tx, ty = -rx * math.sin(t) * sign, ry * math.cos(t) * sign
            c0 = (x + k * tx, y + k * ty)
            c1 = (x - k * tx, y - k * ty)
            self.verts.append((x, y, c0, c1))
        for i in range(4):
            self.prims.append(("B", base + i, base + (i + 1) % 4))

    # zapis zwięzły .lbrn2
    def vert_list(self) -> str:
        out = []
        for x, y, c0, c1 in self.verts:
            s = f"V{f(x)} {f(y)}"
            s += f"c0x{f(c0[0])}c0y{f(c0[1])}" if c0 else "c0x1"
            s += f"c1x{f(c1[0])}c1y{f(c1[1])}" if c1 else "c1x1"
            out.append(s)
        return "".join(out)

    def prim_list(self) -> str:
        return "".join(f"{t}{i} {j}" for t, i, j in self.prims)

    # zapis elementami .lbrn
    def elements(self, ind: str) -> list[str]:
        out = []
        for x, y, c0, c1 in self.verts:
            a = f'vx="{f(x)}" vy="{f(y)}"'
            if c0:
                a += f' c0x="{f(c0[0])}" c0y="{f(c0[1])}"'
            if c1:
                a += f' c1x="{f(c1[0])}" c1y="{f(c1[1])}"'
            out.append(f"{ind}<V {a}/>")
        out += [f'{ind}<P T="{t}" p0="{i}" p1="{j}"/>' for t, i, j in self.prims]
        return out


def raw_path(verts: list[tuple], prims: str) -> Path:
    p = Path()
    p.verts = list(verts)
    toks = []
    cur = ""
    for ch in prims:
        if ch in "LB":
            if cur:
                toks.append(cur)
            cur = ch
        else:
            cur += ch
    toks.append(cur)
    for t in toks:
        i, j = t[1:].split()
        p.prims.append((t[0], int(i), int(j)))
    return p


# ---------- krój blokowy „Lorem ipsum” (własny) ----------

def lorem_ipsum() -> Path:
    """Napis „Lorem ipsum”: wysokość wersalika 18 mm, linia bazowa y = 0, środek w x = 0."""
    p = Path()
    adv, w = 11.0, 8.0
    x = -5.5 * adv  # 11 znaków ze spacją, wyśrodkowane
    for ch in "Lorem ipsum":
        if ch == "L":
            p.polygon([(0, 0), (w, 0), (w, 2), (2, 2), (2, 18), (0, 18)], x)
        elif ch == "o":
            p.ellipse(x + 4, 6.5, 4, 6.5)
            p.ellipse(x + 4, 6.5, 2, 4.5, ccw=False)
        elif ch == "r":
            p.polygon([(0, 0), (2, 0), (2, 9), (4, 11), (7, 11), (7, 13), (3.5, 13), (2, 11.5), (2, 13), (0, 13)], x)
        elif ch == "e":
            p.ellipse(x + 4, 6.5, 4, 6.5)
            p.polygon([(2, 7), (2, 9), (6, 9), (6, 7)], x)
        elif ch == "m":
            p.polygon([(0, 0), (2, 0), (2, 10), (3, 11), (3, 0), (5, 0), (5, 10), (6, 11), (6, 0),
                       (8, 0), (8, 12), (7, 13), (5, 13), (4, 12), (3, 13), (0, 13)], x)
        elif ch == "i":
            p.polygon([(0, 0), (2, 0), (2, 13), (0, 13)], x + 3)
            p.polygon([(0, 15), (2, 15), (2, 17.5), (0, 17.5)], x + 3)
        elif ch == "p":
            p.polygon([(0, -5), (2, -5), (2, 13), (0, 13)], x)
            p.ellipse(x + 5, 6.5, 3, 6.5)
            p.ellipse(x + 5, 6.5, 1.5, 4.5, ccw=False)
        elif ch == "s":
            p.polygon([(0, 0), (8, 0), (8, 7.5), (2, 7.5), (2, 11), (8, 11), (8, 13), (0, 13),
                       (0, 5.5), (6, 5.5), (6, 2), (0, 2)], x)
        elif ch == "u":
            p.polygon([(0, 13), (0, 1), (1, 0), (8, 0), (8, 13), (6, 13), (6, 2), (2, 2), (2, 13)], x)
        x += adv
    return p


# ---------- dokumenty ----------

def cut(index: int) -> list[str]:
    return [
        '    <CutSetting type="Cut">',
        f'        <index Value="{index}"/>',
        f'        <name Value="C{index:02d}"/>',
        '        <maxPower Value="20"/>',
        '        <speed Value="100"/>',
        "    </CutSetting>",
    ]


def doc(fv: int, body: list[str], cuts: int = 1) -> str:
    lines = ['<?xml version="1.0" encoding="UTF-8"?>', f'<LightBurnProject FormatVersion="{fv}">']
    for i in range(cuts):
        lines += cut(i)
    lines += body
    lines.append("</LightBurnProject>")
    return "\n".join(lines) + "\n"


def xf(a, b, c, d, e, g) -> str:
    return f"<XForm>{' '.join(f(v) for v in (a, b, c, d, e, g))}</XForm>"


def shape_path(fv: int, p: Path, ind: str, tag: str = "Shape", attrs: str = 'Type="Path" CutIndex="0"',
               form: str = "<XForm>1 0 0 1 0 0</XForm>") -> list[str]:
    out = [f"{ind}<{tag} {attrs}>", f"{ind}    {form}"]
    if fv == 1:
        out += [f"{ind}    <VertList>{p.vert_list()}</VertList>", f"{ind}    <PrimList>{p.prim_list()}</PrimList>"]
    else:
        out += p.elements(ind + "    ")
    out.append(f"{ind}</{tag}>")
    return out


TEXT_AT = (93.069916, 118.92541)
GROUP_AT = (39.812897, -16.509392)


def text_shape(fv: int, ind: str, backup_at: tuple[float, float]) -> list[str]:
    attrs = 'Type="Text" CutIndex="0" Font="Sans" Str="Lorem ipsum" H="25" HasBackupPath="1"'
    out = [f"{ind}<Shape {attrs}>"]
    out += shape_path(fv, lorem_ipsum(), ind + "    ", "BackupPath", 'Type="Path" CutIndex="0"',
                      xf(1, 0, 0, 1, *backup_at))
    out += [f"{ind}    {xf(1, 0, 0, 1, *TEXT_AT)}", f"{ind}</Shape>"]
    return out


KRZYWE = raw_path(
    [(47, 138, (78.606598, 139.85106), (15.393402, 136.14894)),
     (52, 116, None, (52, 116)),
     (85, 67, None, None), (143, 133, None, None), (100, 135, None, None), (108, 158, None, None),
     (41, 169, (41, 169), None)],
    "B0 1L1 2L2 3L3 4L4 5L5 6B6 0",
)

GROUP_PATH = raw_path(
    [(66, 130, None, (66, 130)),
     (98, 156, None, None), (119, 136, None, None),
     (106, 163, (106, 163), None),
     (141, 180, (167.23289, 178.0096), (114.76711, 181.9904)),
     (215, 123, (194.31314, 91.229691), (235.68686, 154.77031)),
     (153, 57, (69.066963, 73.689072), (236.93304, 40.310928)),
     (24, 82, (10.290494, 105.38528), (37.709507, 58.614723)),
     (5, 136, (15.521908, 141.08199), (-5.5219078, 130.91801)),
     (53, 143, (66.297386, 130.31142), (39.702614, 155.68858))],
    "L0 1L1 2L2 3B3 4B4 5B5 6B6 7B7 8B8 9B9 0",
)


def samples(fv: int) -> dict[str, str]:
    s = "    "
    ellipse = lambda rx, ry, x, y, ind=s: [f'{ind}<Shape Type="Ellipse" CutIndex="0" Rx="{f(rx)}" Ry="{f(ry)}">',
                                           f"{ind}    {xf(1, 0, 0, 1, x, y)}", f"{ind}</Shape>"]
    polygon = lambda rx, ry, n, x, y, ind=s: [f'{ind}<Shape Type="Polygon" CutIndex="0" Rx="{f(rx)}" Ry="{f(ry)}" N="{n}">',
                                              f"{ind}    {xf(1, 0, 0, -1, x, y)}", f"{ind}</Shape>"]
    gi = s * 3
    group = [f'{s}<Shape Type="Group">', f"{s}    {xf(1, 0, 0, 1, *GROUP_AT)}", f"{s}    <Children>"]
    group += text_shape(fv, gi, (TEXT_AT[0] + GROUP_AT[0], TEXT_AT[1] + GROUP_AT[1]))
    group += polygon(12, 12, 6, 161, 134, gi)
    group += shape_path(fv, GROUP_PATH, gi)
    group += ellipse(21, 9.5, 150, 75.5, gi)
    group += ellipse(9, 7.5, 12, 175.5, gi)
    group += [f"{s}    </Children>", f"{s}</Shape>"]
    return {
        "elipsa": doc(fv, ellipse(87.5, 29.5, 127.5, 134.5)),
        "hex": doc(fv, polygon(50.5, 41, 6, 157.5, 86)),
        "krzywe": doc(fv, shape_path(fv, KRZYWE, s)),
        "text": doc(fv, text_shape(fv, s, TEXT_AT)),
        "grupa": doc(fv, group),
    }


def rects() -> str:
    body = []
    for cut_i, w, h in [(2, 55, 39), (0, 62, 44), (1, 231, 102)]:
        body += [f'    <Shape Type="Rect" CutIndex="{cut_i}" W="{w}" H="{h}" Cr="0">',
                 f"        {xf(1, 0, 0, 1, 169.5, 122)}", "    </Shape>"]
    return doc(1, body, cuts=3)


def main() -> int:
    out: dict[pathlib.Path, str] = {}
    for fv, ext in [(1, "lbrn2"), (0, "lbrn")]:
        for name, text in samples(fv).items():
            out[FIX / f"{name}.{ext}"] = text
    out[FIX / "plik_przykladowy.lbrn2"] = rects()
    out[FUZZ / "seed-rects.lbrn2"] = rects()
    for path, text in out.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8", newline="\n")
        print(f"{path.relative_to(ROOT)}: {len(text)} B")
    return 0


if __name__ == "__main__":
    sys.exit(main())
