"""Testy plików tłumaczeń Fluent (REQ-03-059): te same klucze i atrybuty we
wszystkich językach, poprawna składnia, zmienne zgodne z polskim wzorcem.

Uruchamianie: python -m unittest -v tools/i18n/test_ftl.py
"""
import pathlib
import re
import unittest

LOCALES = pathlib.Path(__file__).resolve().parents[2] / "web" / "locales"
# identyfikatory Fluent: litera, potem litery, cyfry, „-” i „_”
MSG_RE = re.compile(r"^([a-zA-Z][a-zA-Z0-9_-]*)\s*=\s*(.*)$")
ATTR_RE = re.compile(r"^\s+\.([a-zA-Z][a-zA-Z0-9_-]*)\s*=\s*(.*)$")
VAR_RE = re.compile(r"\{\s*\$([a-z][a-z0-9_-]*)\s*\}")


def parse(path: pathlib.Path) -> dict[str, str]:
    """Klucze `id` i `id.atrybut` → treść; zgłasza błąd przy nierozpoznanej linii."""
    out: dict[str, str] = {}
    current = None
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if m := MSG_RE.match(line):
            current = m.group(1)
            if current in out:
                raise ValueError(f"{path.name}:{n}: zdublowany klucz {current}")
            out[current] = m.group(2)
        elif (m := ATTR_RE.match(line)) and current:
            out[f"{current}.{m.group(1)}"] = m.group(2)
        elif line.startswith((" ", "\t")) and current:
            out[current] += "\n" + line.strip()
        else:
            raise ValueError(f"{path.name}:{n}: nierozpoznana linia: {line!r}")
    return out


class T(unittest.TestCase):
    def test_req_03_059_locales_have_same_keys_and_variables(self):
        files = sorted(LOCALES.glob("*.ftl"))
        names = [f.stem for f in files]
        self.assertIn("pl", names)
        self.assertIn("en", names)
        base = parse(LOCALES / "pl.ftl")
        self.assertGreater(len(base), 50, "tłumaczenia obejmują interfejs")
        for f in files:
            other = parse(f)
            self.assertEqual(sorted(base), sorted(other), f"{f.name}: inne klucze niż pl.ftl")
            for k, v in base.items():
                self.assertEqual(sorted(VAR_RE.findall(v)), sorted(VAR_RE.findall(other[k])), f"{f.name}: zmienne w {k}")
                self.assertTrue(other[k].strip(), f"{f.name}: pusta wartość {k}")

    def test_req_03_061_english_is_not_a_copy_of_polish(self):
        pl, en = parse(LOCALES / "pl.ftl"), parse(LOCALES / "en.ftl")
        same = [k for k in pl if pl[k] == en[k] and re.search(r"[a-ząćęłńóśźż]{4,}", pl[k], re.I)]
        self.assertLess(len(same), len(pl) // 20, f"nieprzetłumaczone: {same[:10]}")
        self.assertFalse(re.search(r"[ąćęłńóśźż]", "".join(en.values()), re.I), "polskie znaki w en.ftl")


if __name__ == "__main__":
    unittest.main()
