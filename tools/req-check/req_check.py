#!/usr/bin/env python3
"""req-check — kontrola zgodności rejestru wymagań z kodem i testami (spec. sekcja 21.2).

Użycie:
    python tools/req-check/req_check.py [--repo .] [--test-log LOG ...] [--write]

Kończy się kodem 1, gdy:
  * rejestr ma błędy formalne (pola, identyfikatory, wartości),
  * wymaganie przypisane do zamkniętego etapu (milestones.yaml) nie jest verified/deviation,
  * wymaganie done/verified nie ma testu ani udokumentowanej procedury ręcznej,
  * procedura ręczna wskazuje nieistniejący zrzut ekranu,
  * test (lub odwołanie REQ-… w kodzie) wskazuje nieistniejące wymaganie,
  * status deviation nie wskazuje zatwierdzonego wpisu w DEVIATIONS.md,
  * (z --test-log) test wymagania verified nie przeszedł albo nie został uruchomiony.
Z --write regeneruje docs/traceability.md i zestawienie w REQUIREMENTS.md.
"""
from __future__ import annotations

import argparse
import collections
import pathlib
import re
import sys

import yaml

PRIORITIES = {"MUST", "SHOULD", "LATER"}
MILESTONES = [f"M{i}" for i in range(8)] + ["LATER"]
VERIFY = {"auto", "emu", "hw", "manual"}
STATUSES = ["todo", "in_progress", "done", "verified", "deviation"]
REQUIRED = ["id", "sec", "pri", "ms", "verify", "status", "text"]
OPTIONAL = {"postulat", "conflict", "tests", "procedure", "deviation", "note", "code"}
ID_RE = re.compile(r"^REQ-(\d{2})-(\d{3})$")
TEST_FN_RE = re.compile(r"\b(?:fn|def)\s+(\w*req_\d{2}_\d{3}\w*)\s*\(")
# Playwright: test("req_03_001_…", …) / test.skip(…) — nazwa testu jak identyfikator
TS_TEST_RE = re.compile(r"\btest(?:\.\w+)?\(\s*[\"'`](req_\d{2}_\d{3}\w*)[\"'`]")
SKIP_DIRS = {"target", "node_modules", "dist", "test-results", "playwright-report"}
TAG_RE = re.compile(r"req_(\d{2})_(\d{3})((?:_(?:to_)?\d{3})*)")
REQ_LITERAL_RE = re.compile(r"\bREQ-\d{2}-\d{3}\b")
LOG_RE = re.compile(r"(?:^test\s+(\S+)|^(\w+)\s+\(\S+\))\s+\.\.\.\s+(ok|FAILED|ERROR|ignored|skipped|fail)", re.M)
SCAN_GLOBS = ["crates/**/*.rs", "app/**/*.rs", "tools/**/*.py", "app/**/*.ts", "web/**/*.ts"]
# obrazy w procedurach ręcznych (Markdown): ![opis](ścieżka)
IMG_RE = re.compile(r"!\[[^\]]*\]\(([^)\s]+)\)")
# kod w Markdown (bloki ``` i `…`) nie zawiera obrazów
CODE_BLOCK_RE = re.compile(r"```.*?```", re.S)
CODE_SPAN_RE = re.compile(r"`[^`\n]*`")
SUMMARY_START = "<!-- req-check:start -->"
SUMMARY_END = "<!-- req-check:end -->"


def tags_in_name(name: str) -> list[str]:
    """req_15_017_018 -> [REQ-15-017, REQ-15-018]; req_01_030_to_035 -> 030..035."""
    out = []
    for m in TAG_RE.finditer(name):
        sec, first, rest = m.group(1), int(m.group(2)), m.group(3)
        nums, prev = [first], first
        for part in re.findall(r"_(to_)?(\d{3})", rest):
            n = int(part[1])
            if part[0]:
                nums.extend(range(prev + 1, n + 1))
            else:
                nums.append(n)
            prev = n
        out.extend(f"REQ-{sec}-{n:03d}" for n in nums)
    return out


def load(repo: pathlib.Path):
    data = yaml.safe_load((repo / "requirements.yaml").read_text(encoding="utf-8"))
    reqs = data.get("requirements") or []
    ms_path = repo / "milestones.yaml"
    ms = yaml.safe_load(ms_path.read_text(encoding="utf-8")).get("milestones", {}) if ms_path.exists() else {}
    return reqs, ms


def scan_code(repo: pathlib.Path):
    tests: dict[str, list[str]] = collections.defaultdict(list)  # REQ -> ["plik::fn"]
    fn_names: set[str] = set()
    literals: list[tuple[str, str]] = []  # (REQ, plik)
    for g in SCAN_GLOBS:
        for f in sorted(repo.glob(g)):
            if SKIP_DIRS.intersection(f.relative_to(repo).parts):
                continue
            text = f.read_text(encoding="utf-8", errors="replace")
            rel = f.relative_to(repo).as_posix()
            if f.suffix == ".ts":
                names = [m.group(1) for m in TS_TEST_RE.finditer(text)]
            else:
                kw = "def" if f.suffix == ".py" else "fn"
                names = [m.group(1) for m in TEST_FN_RE.finditer(text) if m.group(0).startswith(kw)]
            for name in names:
                fn_names.add(name)
                for r in tags_in_name(name):
                    tests[r].append(f"{rel}::{name}")
            for m in REQ_LITERAL_RE.finditer(text):
                literals.append((m.group(0), rel))
    return tests, fn_names, literals


def parse_logs(paths: list[pathlib.Path]) -> dict[str, str]:
    res: dict[str, str] = {}
    for p in paths:
        for m in LOG_RE.finditer(p.read_text(encoding="utf-8", errors="replace")):
            name = (m.group(1) or m.group(2)).split("::")[-1]
            st = m.group(3).lower()
            # wynik negatywny ma pierwszeństwo przed pozytywnym tej samej nazwy
            if res.get(name) not in ("failed", "error", "fail"):
                res[name] = st
    return res


def check(repo: pathlib.Path, logs: list[pathlib.Path], optional: list[str] | None = None):
    optional = optional or []
    reqs, milestones = load(repo)
    errors: list[str] = []
    warnings: list[str] = []
    by_id: dict[str, dict] = {}
    for i, r in enumerate(reqs):
        rid = r.get("id", f"#{i}")
        for k in REQUIRED:
            if k not in r:
                errors.append(f"{rid}: brak pola {k}")
        for k in r:
            if k not in REQUIRED and k not in OPTIONAL:
                errors.append(f"{rid}: nieznane pole {k}")
        if not ID_RE.match(str(rid)):
            errors.append(f"{rid}: zły format id")
        elif int(ID_RE.match(rid).group(1)) != int(str(r.get("sec", "-1")).split(".")[0]):
            errors.append(f"{rid}: id nie pasuje do sekcji {r.get('sec')}")
        if rid in by_id:
            errors.append(f"{rid}: zdublowany identyfikator")
        by_id[rid] = r
        if r.get("pri") not in PRIORITIES:
            errors.append(f"{rid}: priorytet {r.get('pri')}")
        if r.get("ms") not in MILESTONES:
            errors.append(f"{rid}: etap {r.get('ms')}")
        if r.get("verify") not in VERIFY:
            errors.append(f"{rid}: weryfikacja {r.get('verify')}")
        if r.get("status") not in STATUSES:
            errors.append(f"{rid}: status {r.get('status')}")

    tests, fn_names, literals = scan_code(repo)
    for rid, r in by_id.items():
        for t in r.get("tests") or []:
            if t not in fn_names:
                errors.append(f"{rid}: test {t} wskazany w rejestrze nie istnieje w kodzie")
            elif not any(x.endswith("::" + t) for x in tests.get(rid, [])):
                tests[rid].append(f"(rejestr)::{t}")
    for rid in sorted(tests):
        if rid not in by_id:
            errors.append(f"test {', '.join(tests[rid])} odwołuje się do nieistniejącego {rid}")
    for rid, f in literals:
        if rid not in by_id and not f.startswith("tools/req-check/test_"):
            errors.append(f"{f}: odwołanie do nieistniejącego {rid}")

    dev_text = (repo / "DEVIATIONS.md").read_text(encoding="utf-8") if (repo / "DEVIATIONS.md").exists() else ""
    results = parse_logs(logs)
    closed = {m for m, v in milestones.items() if (v or {}).get("status") == "closed"}
    missing_imgs = set()

    for rid, r in by_id.items():
        st = r.get("status")
        has_tests = bool(tests.get(rid))
        proc = r.get("procedure")
        if proc:
            ppath = repo / proc.split("#")[0]
            if not ppath.exists():
                errors.append(f"{rid}: procedura {proc} — brak pliku")
            else:
                ptext = ppath.read_text(encoding="utf-8")
                if rid not in ptext:
                    errors.append(f"{rid}: procedura {proc} nie opisuje {rid}")
                # zrzuty ekranu procedury (względem pliku procedury) muszą istnieć
                prose = CODE_SPAN_RE.sub("", CODE_BLOCK_RE.sub("", ptext))
                for img in IMG_RE.findall(prose):
                    if "://" in img:
                        continue
                    if not (ppath.parent / img.split("#")[0]).exists() and (ppath, img) not in missing_imgs:
                        missing_imgs.add((ppath, img))
                        errors.append(f"{rid}: procedura {proc} — brak obrazu {img}")
        if st in ("done", "verified") and not has_tests and not proc:
            errors.append(f"{rid}: status {st} bez testu i bez procedury ręcznej")
        if st == "deviation":
            dev = r.get("deviation")
            row = next((ln for ln in dev_text.splitlines() if dev and ln.startswith(f"| {dev} ")), None)
            cells = [c.strip() for c in row.strip().strip("|").split("|")] if row else []
            if not dev or row is None:
                errors.append(f"{rid}: deviation bez wpisu w DEVIATIONS.md")
            elif len(cells) < 7 or cells[6] in ("", "—", "-"):
                errors.append(f"{rid}: {dev} nie ma zapisanej decyzji zleceniodawcy")
        if r.get("ms") in closed and st not in ("verified", "deviation"):
            errors.append(f"{rid}: etap {r['ms']} zamknięty, a status = {st}")
        if st == "verified" and logs and has_tests:
            names = [t.split("::")[-1] for t in tests[rid]]
            outcomes = [results.get(n) for n in names]
            if any(o in ("failed", "error", "fail") for o in outcomes):
                errors.append(f"{rid}: test nie przeszedł ({', '.join(n for n, o in zip(names, outcomes) if o in ('failed', 'error', 'fail'))})")
            elif not any(o == "ok" for o in outcomes):
                # testy ze ścieżek --optional (np. app/ — osobne zadanie CI) mogą nie być w logach
                all_optional = all(any(t.startswith(p) for p in optional) for t in tests[rid])
                if not all_optional:
                    errors.append(f"{rid}: verified, ale żaden z testów nie został uruchomiony ({', '.join(names)})")
        if st == "todo" and has_tests:
            warnings.append(f"{rid}: ma testy, a status = todo")
    return reqs, tests, errors, warnings, milestones


def md_escape(s: str) -> str:
    return s.replace("|", "\\|")


def write_docs(repo: pathlib.Path, reqs, tests, milestones):
    sec_counts = collections.OrderedDict()
    for r in reqs:
        top = str(r["sec"]).split(".")[0]
        c = sec_counts.setdefault(top, collections.Counter())
        c["all"] += 1
        c[r["pri"]] += 1
        c[r["status"]] += 1
    ms_counts = collections.Counter(r["ms"] for r in reqs)
    ms_verified = collections.Counter(r["ms"] for r in reqs if r["status"] in ("verified", "deviation"))

    lines = [SUMMARY_START, "", "### Zestawienie: sekcja → liczba wymagań", "",
             "| Sekcja | Wymagań | MUST | SHOULD | LATER | verified | done | in_progress | todo | deviation |",
             "|---|---|---|---|---|---|---|---|---|---|"]
    for top, c in sec_counts.items():
        lines.append(f"| {int(top)} | {c['all']} | {c['MUST']} | {c['SHOULD']} | {c['LATER']} | {c['verified']} | {c['done']} | {c['in_progress']} | {c['todo']} | {c['deviation']} |")
    tot = collections.Counter()
    for c in sec_counts.values():
        tot.update(c)
    lines.append(f"| **Razem** | **{tot['all']}** | {tot['MUST']} | {tot['SHOULD']} | {tot['LATER']} | {tot['verified']} | {tot['done']} | {tot['in_progress']} | {tot['todo']} | {tot['deviation']} |")
    lines += ["", "### Etapy", "", "| Etap | Wymagań | verified/deviation | Stan etapu |", "|---|---|---|---|"]
    for m in MILESTONES:
        if ms_counts[m]:
            lines.append(f"| {m} | {ms_counts[m]} | {ms_verified[m]} | {(milestones.get(m) or {}).get('status', 'planned')} |")
    lines += ["", "### Pełna lista wymagań", ""]
    cur = None
    for r in reqs:
        top = str(r["sec"]).split(".")[0]
        if top != cur:
            cur = top
            lines += ["", f"#### Sekcja {int(top)}", "", "| ID | § | Pri | Etap | Weryf. | Status | Wymaganie |", "|---|---|---|---|---|---|---|"]
        extra = ""
        if r.get("postulat") is not None:
            extra += f" [POSTULAT{' ' + str(r['postulat']) if r['postulat'] else ''}]"
        if r.get("conflict"):
            extra += " ⚠ konflikt priorytetu"
        lines.append(f"| {r['id']} | {r['sec']} | {r['pri']} | {r['ms']} | {r['verify']} | {r['status']} | {md_escape(r['text'])}{extra} |")
    lines += ["", SUMMARY_END]
    block = "\n".join(lines)

    req_md = repo / "REQUIREMENTS.md"
    text = req_md.read_text(encoding="utf-8")
    if SUMMARY_START in text and SUMMARY_END in text:
        pre, rest = text.split(SUMMARY_START, 1)
        post = rest.split(SUMMARY_END, 1)[1]
        req_md.write_text(pre + block + post, encoding="utf-8", newline="\n")

    tr = ["# Macierz śledzenia (generowana przez tools/req-check — nie edytować ręcznie)", "",
          "| ID | Etap | Status | Testy | Procedura |", "|---|---|---|---|---|"]
    for r in reqs:
        t = "<br>".join(f"`{x}`" for x in tests.get(r["id"], []))
        tr.append(f"| {r['id']} | {r['ms']} | {r['status']} | {t} | {r.get('procedure', '')} |")
    (repo / "docs").mkdir(exist_ok=True)
    (repo / "docs" / "traceability.md").write_text("\n".join(tr) + "\n", encoding="utf-8", newline="\n")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--repo", default=str(pathlib.Path(__file__).resolve().parents[2]))
    ap.add_argument("--test-log", action="append", default=[], help="wyjście cargo test / unittest -v (można powtórzyć)")
    ap.add_argument("--write", action="store_true", help="regeneruj docs/traceability.md i zestawienie w REQUIREMENTS.md")
    ap.add_argument("--optional", action="append", default=[],
                    help="ścieżka (np. app/), której testy mogą nie występować w logach — buduje je osobne zadanie CI")
    a = ap.parse_args(argv)
    # konsola Windows (cp1252) nie zna polskich znaków komunikatów
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")
    repo = pathlib.Path(a.repo)
    reqs, tests, errors, warnings, milestones = check(repo, [pathlib.Path(p) for p in a.test_log], a.optional)
    if a.write:
        write_docs(repo, reqs, tests, milestones)
    for w in warnings:
        print(f"ostrzeżenie: {w}")
    for e in errors:
        print(f"BŁĄD: {e}")
    st = collections.Counter(r.get("status") for r in reqs)
    print(f"req-check: {len(reqs)} wymagań; " + ", ".join(f"{k}={st[k]}" for k in STATUSES) + f"; testy powiązane z {len(tests)} REQ; błędów: {len(errors)}")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
