"""Testy tools/req-check (uruchamianie: python -m unittest -v tools/req-check/test_req_check.py)."""
import pathlib
import sys
import tempfile
import textwrap
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import req_check  # noqa: E402

BASE = {"sec": "2.3", "pri": "MUST", "ms": "M0", "verify": "auto", "text": "x"}


def repo(reqs, code="", milestones=None, deviations="", files=None):
    d = pathlib.Path(tempfile.mkdtemp())
    rows = []
    for r in reqs:
        r = {**BASE, **r}
        rows.append("  - {" + ", ".join(f"{k}: {v!r}" if not isinstance(v, str) else f'{k}: "{v}"' for k, v in r.items()) + "}")
    (d / "requirements.yaml").write_text("requirements:\n" + "\n".join(rows) + "\n", encoding="utf-8")
    (d / "crates" / "x" / "src").mkdir(parents=True)
    (d / "crates" / "x" / "src" / "lib.rs").write_text(code, encoding="utf-8")
    if milestones:
        (d / "milestones.yaml").write_text("milestones:\n" + "".join(f"  {k}: {{status: {v}}}\n" for k, v in milestones.items()), encoding="utf-8")
    (d / "DEVIATIONS.md").write_text(deviations, encoding="utf-8")
    (d / "REQUIREMENTS.md").write_text(f"# R\n{req_check.SUMMARY_START}\n{req_check.SUMMARY_END}\n", encoding="utf-8")
    for name, text in (files or {}).items():
        (d / name).parent.mkdir(parents=True, exist_ok=True)
        (d / name).write_text(text, encoding="utf-8")
    return d


def errors(d, logs=()):
    return req_check.check(d, [pathlib.Path(p) for p in logs])[2]


class T(unittest.TestCase):
    def test_req_21_002_registry_fields_validated(self):
        d = repo([{"id": "REQ-02-001", "status": "todo"}, {"id": "REQ-02-001", "status": "bogus"}, {"id": "REQ-03-001", "status": "todo"}])
        e = "\n".join(errors(d))
        self.assertIn("zdublowany", e)
        self.assertIn("status bogus", e)
        self.assertIn("nie pasuje do sekcji", e)

    def test_req_21_005_tags_parsed_from_test_names(self):
        self.assertEqual(req_check.tags_in_name("req_15_017_018_undo"), ["REQ-15-017", "REQ-15-018"])
        self.assertEqual(req_check.tags_in_name("req_01_030_to_033_spec"), ["REQ-01-030", "REQ-01-031", "REQ-01-032", "REQ-01-033"])
        self.assertEqual(req_check.tags_in_name("req_15_093_policy_and_req_15_105_audit"), ["REQ-15-093", "REQ-15-105"])

    def test_req_21_008_closed_milestone_requires_verified(self):
        d = repo([{"id": "REQ-02-001", "status": "done", "procedure": "p.md"}], milestones={"M0": "closed"}, files={"p.md": "REQ-02-001"})
        self.assertTrue(any("zamknięty" in x for x in errors(d)))
        d2 = repo([{"id": "REQ-02-001", "status": "verified", "procedure": "p.md"}], milestones={"M0": "closed"}, files={"p.md": "REQ-02-001 ok"})
        self.assertEqual(errors(d2), [])

    def test_req_21_009_done_needs_test_or_procedure(self):
        d = repo([{"id": "REQ-02-001", "status": "done"}])
        self.assertTrue(any("bez testu" in x for x in errors(d)))
        ok = repo([{"id": "REQ-02-001", "status": "verified"}], code="#[test] fn req_02_001_zip() {}")
        self.assertEqual(errors(ok), [])
        bad_proc = repo([{"id": "REQ-02-001", "status": "done", "procedure": "brak.md"}])
        self.assertTrue(any("brak pliku" in x for x in errors(bad_proc)))

    def test_req_21_009_procedure_screenshots_must_exist(self):
        text = ("## REQ-03-001\n![siatka](img/m1/grid.png)\n![](https://example.com/x.png)\n"
                "Odwołanie w kodzie to nie obraz: `![](notes/a.png)`\n```md\n![](b.png)\n```\n")
        d = repo([{"id": "REQ-03-001", "status": "verified", "sec": "3", "procedure": "docs/p.md#req-03-001"}], files={"docs/p.md": text})
        e = errors(d)
        self.assertTrue(any("img/m1/grid.png" in x and "brak" in x for x in e), e)
        (d / "docs" / "img" / "m1").mkdir(parents=True)
        (d / "docs" / "img" / "m1" / "grid.png").write_bytes(b"\x89PNG\r\n\x1a\n")
        self.assertEqual(errors(d), [], "obrazy względem pliku procedury; adresy http pomijane")

    def test_req_21_010_test_references_unknown_req(self):
        d = repo([{"id": "REQ-02-001", "status": "todo"}], code="fn req_02_999_x() {}\nconst R: &str = \"REQ-07-123\";")
        e = errors(d)
        self.assertTrue(any("REQ-02-999" in x for x in e))
        self.assertTrue(any("REQ-07-123" in x for x in e))

    def test_req_21_011_deviation_needs_approved_entry(self):
        d = repo([{"id": "REQ-02-001", "status": "deviation"}])
        self.assertTrue(any("bez wpisu" in x for x in errors(d)))
        table = "| DEV-001 | REQ-02-001 | opis | powód | wariant B | 2026-10-28 | |\n"
        d2 = repo([{"id": "REQ-02-001", "status": "deviation", "deviation": "DEV-001"}], deviations=table)
        self.assertTrue(any("decyzji" in x for x in errors(d2)))
        table3 = "| DEV-001 | REQ-02-001 | opis | powód | wariant B | 2026-10-28 | „zgoda” — zleceniodawca |\n"
        d3 = repo([{"id": "REQ-02-001", "status": "deviation", "deviation": "DEV-001"}], deviations=table3)
        self.assertEqual(errors(d3), [])

    def test_verified_requires_passing_test_in_log(self):
        code = "#[test] fn req_02_001_zip() {}"
        d = repo([{"id": "REQ-02-001", "status": "verified"}], code=code)
        log = d / "t.log"
        log.write_text("test bcraft::tests::req_02_001_zip ... FAILED\n", encoding="utf-8")
        self.assertTrue(any("nie przeszedł" in x for x in errors(d, [log])))
        log.write_text("test other ... ok\n", encoding="utf-8")
        self.assertTrue(any("nie został uruchomiony" in x for x in errors(d, [log])))
        log.write_text("test bcraft::tests::req_02_001_zip ... ok\n", encoding="utf-8")
        self.assertEqual(errors(d, [log]), [])

    def test_playwright_tests_in_web_are_linked(self):
        spec = 'test("req_03_001_grid", async ({ page }) => {});\ntest.skip("req_03_002_rulers", () => {});\n'
        d = repo([{"id": "REQ-03-001", "status": "verified", "sec": "3"}, {"id": "REQ-03-002", "status": "todo", "sec": "3"}],
                 files={"web/e2e/app.spec.ts": spec, "web/node_modules/x/y.ts": 'test("req_09_999_x", () => {})'})
        reqs, tests, errs, _, _ = req_check.check(d, [])
        self.assertEqual(errs, [], "node_modules pomijane, test TS liczony")
        self.assertIn("REQ-03-001", tests)
        self.assertIn("REQ-03-002", tests)
        log = d / "e2e.log"
        log.write_text("test req_03_001_grid ... ok\n", encoding="utf-8")
        self.assertEqual(errors(d, [log]), [])

    def test_tests_from_other_ci_job_may_be_absent_from_logs(self):
        files = {"app/tests/window.rs": "#[test]\nfn req_01_042_window() {}\n"}
        d = repo([{"id": "REQ-01-042", "status": "verified", "sec": "1.4"}], files=files)
        log = d / "t.log"
        log.write_text("test other ... ok\n", encoding="utf-8")
        self.assertTrue(any("nie został uruchomiony" in x for x in errors(d, [log])))
        # --optional app/: testy z tej ścieżki buduje osobne zadanie CI
        errs = req_check.check(d, [log], optional=["app/"])[2]
        self.assertEqual(errs, [])
        # ale jeśli są w logu, muszą przejść
        log.write_text("test req_01_042_window ... FAILED\n", encoding="utf-8")
        errs = req_check.check(d, [log], optional=["app/"])[2]
        self.assertTrue(any("nie przeszedł" in x for x in errs))

    def test_req_21_007_writes_traceability(self):
        d = repo([{"id": "REQ-02-001", "status": "verified"}], code="#[test] fn req_02_001_zip() {}")
        reqs, tests, errs, _, ms = req_check.check(d, [])
        req_check.write_docs(d, reqs, tests, ms)
        tr = (d / "docs" / "traceability.md").read_text(encoding="utf-8")
        self.assertIn("req_02_001_zip", tr)
        self.assertIn("| 2 | 1 |", (d / "REQUIREMENTS.md").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
