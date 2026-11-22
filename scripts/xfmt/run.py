#!/usr/bin/env python3
"""The cross-format measurement harness (T102, docs/dev/xfmt-harness.md).

usage: run.py [--writer BIN] [--out DIR] [--manifest FILE]
              [--check | --update] [--strict] [--verbose] [NAME ...]
       run.py [--writer BIN] [--out DIR] --doc FILE [--config FILE]
              [--media print|screen] [--root SELECTOR] [--width PX]

Builds each reference document of tests/xfmt/xfmt.toml (or the one --doc
names) as HTML, PDF, DOCX and Typst source with writer, reads every text
baseline of the PDF (Ghostscript txtwrite at 7200 dpi), of Chrome's screen
layout of the HTML and of Chrome's printed PDF of it (one headless Chrome
over the DevTools protocol, cdp-print.mjs), and computes Word's from the
DOCX's XML (word.py). It prints one table per document, one row per gap
between consecutive PDF lines, and a summary line.

--check compares each table with tests/xfmt/expected/<name>.txt and exits 1
when one differs; --update writes them. Exit status 2: a document could not
be built or measured. --strict also exits 1 when a gap is outside
tolerance.

Python 3.11 or later, standard library only; Node 22 or later; Ghostscript;
Google Chrome or Chromium (CHROME_PATH)."""
import argparse
import difflib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))

import baselines  # noqa: E402
import metrics  # noqa: E402
import word  # noqa: E402

PT_PER_PX = 0.75
WORD_TOLERANCE = 0.3
SCREEN_TOLERANCE = 0.3
PRINT_TOLERANCE = 0.5
MEAN_TOLERANCE = 0.1
MEAN_MINIMUM = 8
GRID = 0.75
GRID_TOLERANCE = 0.01
PROFILE_PREFIX = "xfmt-chrome-"
DEFAULT_ROOT = "[data-writer-document]"
CHROME_TIMEOUT = 300


class Failure(Exception):
    pass


# Sources ---------------------------------------------------------------

def build(doc, writer, out):
    """Builds `doc` into `out`; returns the paths of its outputs."""
    if doc.get("html"):
        return {"html": doc["html"]}
    source = doc["source"]
    if out.exists():
        shutil.rmtree(out)
    command = [str(writer), "--quiet", "build", str(source),
               "--to", "html,pdf,docx,typst", "-o", str(out)]
    if doc.get("config"):
        command[3:3] = ["--config", str(doc["config"])]
    done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode != 0:
        raise Failure(f"writer build failed: {done.stderr.strip()}")
    stem = source.stem
    paths = {kind: out / f"{stem}.{kind}" for kind in ("html", "pdf", "docx")}
    for kind, path in paths.items():
        if not path.exists():
            raise Failure(f"writer wrote no {kind}: {path}")
    paths["design"] = out / f"{stem}_assets" / "writer-design.json"
    return paths


def word_lines(doc, paths, reference):
    """Word's lines from the model, with line breaks borrowed from the
    reference layout."""
    if "docx" not in paths:
        return []
    names = word.families(paths["docx"])
    known = {}
    if paths["design"].exists():
        design = json.loads(paths["design"].read_text())
        for family, line in design.get("lines", {}).items():
            known[family.lower()] = (line["single"], line["ascent"])
    dirs = [doc["dir"], ROOT / "vendor" / "typst-assets" / "files" / "fonts"]
    known.update(metrics.lookup(names - known.keys(), [str(d) for d in dirs]))

    def wrap(text):
        """The reference lines that, joined, make `text`, when it takes
        more than one."""
        whole = squash(text)
        for start, line in enumerate(reference):
            if not line["full"] or not whole.startswith(line["full"]):
                continue
            parts = [line["text"]]
            joined = line["full"]
            for other in reference[start + 1:]:
                if joined == whole or not whole.startswith(
                        joined + other["full"]):
                    break
                parts.append(other["text"])
                joined += other["full"]
            if joined == whole and len(parts) > 1:
                return parts
        return None

    _, lines = word.model(paths["docx"], known, wrap)
    return [
        dict(line, key=key(line["text"]), full=squash(line["text"]))
        for line in lines if line["text"]
    ]


def squash(text):
    return re.sub(r"\s+", "", text)


MARKER = re.compile(r"^(?:[^\w\s]|\d+[.)]|[a-zA-Z][.)]|[ivxlc]+[.)])\s+")
LEADER = re.compile(r"(?:\s*[.·…]){3,}\s*\S*$")


def key(text):
    """What lines are matched by across formats: the text without a list
    marker, a leader and page number, or spaces; its first 16
    characters."""
    text = LEADER.sub("", MARKER.sub("", text.strip()))
    return squash(text)[:16]


def pdf_lines(path):
    return [
        {"page": page, "y": y, "text": text, "size": size,
         "key": key(text), "full": squash(text)}
        for page, y, _, size, _, text in baselines.lines(path)
    ]


def screen_lines(path):
    rows = json.loads(Path(path).read_text())
    return [
        {"page": None, "y": row["baseline"] * PT_PER_PX, "text": row["text"],
         "offset": row["offset"] * PT_PER_PX,
         "style": row["style"] or row["className"] or row["block"],
         "key": key(row["text"]), "full": squash(row["text"])}
        for row in rows if row["text"]
    ]


# Chrome ----------------------------------------------------------------

def run_chrome(jobs, out, verbose):
    """Runs cdp-print.mjs over `jobs` in one Chrome. However this ends (an
    error, a timeout, Ctrl-C, SIGTERM), no process of this run's Chrome
    profile outlives it and the profile is removed."""
    if not jobs:
        return None, ""
    jobs_file = out / "jobs.json"
    jobs_file.write_text(json.dumps(jobs, indent=1))
    profile = tempfile.mkdtemp(prefix=PROFILE_PREFIX)
    command = ["node", str(HERE / "cdp-print.mjs"), str(jobs_file),
               "--profile", profile]
    stdout = stderr = ""
    proc = subprocess.Popen(command, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True,
                            start_new_session=True)
    try:
        stdout, stderr = proc.communicate(timeout=CHROME_TIMEOUT)
    except subprocess.TimeoutExpired:
        stderr = f"cdp-print: no result in {CHROME_TIMEOUT} s"
    finally:
        stop(proc, profile)
    version = stdout.splitlines()[0] if stdout else None
    if verbose and stderr.strip():
        print(stderr.strip(), file=sys.stderr)
    return version, stderr


def stop(proc, profile):
    """Ends cdp-print (SIGTERM lets it close Chrome), then kills anything
    still running with the profile and removes it."""
    for sig, wait in ((signal.SIGTERM, 5), (signal.SIGKILL, 5)):
        if proc.poll() is not None:
            break
        try:
            os.killpg(proc.pid, sig)
            proc.wait(wait)
        except (ProcessLookupError, subprocess.TimeoutExpired):
            pass
    for _ in range(20):
        found = subprocess.run(["pgrep", "-f", profile], capture_output=True,
                               text=True).stdout.split()
        if not found:
            break
        for pid in found:
            try:
                os.kill(int(pid), signal.SIGKILL)
            except (ProcessLookupError, ValueError):
                pass
        time.sleep(0.1)
    shutil.rmtree(profile, ignore_errors=True)


# Matching and the table -------------------------------------------------

def match(reference, other):
    """Index in `other` of each reference line, by key, in order."""
    found = {}
    matcher = difflib.SequenceMatcher(
        None, [line["key"] for line in reference],
        [line["key"] for line in other], autojunk=False
    )
    for a, b, size in matcher.get_matching_blocks():
        for step in range(size):
            found[a + step] = b + step
    return found


def fmt(value, width=7):
    return f"{value:{width}.2f}" if value is not None else f"{'-':>{width}}"


def short(text, width=22):
    text = LEADER.sub("", text)
    return text if len(text) <= width else text[:width - 1] + "…"


def table(name, sources):
    """The rows and the summary of one document, as text lines, and the
    numbers of gaps checked and outside tolerance.

    The rows follow the reference flow, the PDF's lines (the screen
    layout's when there is no PDF). A line that neither the screen layout
    nor Word has, furniture of the PDF, is shown at its position from the
    page top. Every other line gets a row: its position from the page top
    when it is the first on its page, else its gap to the line before."""
    pdf = sources.get("pdf") or []
    reference = pdf or sources.get("screen") or []
    others = {kind: (lines, match(reference, lines))
              for kind, lines in sources.items() if lines}
    body = [found for kind, (lines, found) in others.items()
            if kind in ("screen", "word") and lines is not reference]

    def get(kind, index):
        if kind not in others or index is None:
            return None
        lines, found = others[kind]
        return lines[found[index]] if index in found else None

    def residual(index):
        """c of the line: Chrome's offset of its block's first baseline
        minus Word's part of the box above the baseline (R13 §3.2)."""
        screen, model = get("screen", index), get("word", index)
        if screen is None or model is None:
            return None
        return screen["offset"] - model["above"]

    out = [f"== {name}",
           f"{'Word':>7} {'PDF':>7} {'screen':>7} {'scr+c':>7} "
           f"{'print':>7}  {'verdict':<18} line"]
    checked = outside = 0
    print_gaps = []
    shift = 0
    last = None
    for index, line in enumerate(reference):
        values = dict.fromkeys(("word", "pdf", "screen", "scr+c", "print"))
        faults = []
        compared = False
        printed = get("print", index)
        if pdf and printed and printed["page"] - line["page"] != shift:
            shift = printed["page"] - line["page"]
            faults.append(f"Pr page {printed['page']}")
            compared = True
        if body and not any(index in found for found in body):
            values["pdf"] = line["y"]
            if printed and printed["page"] == line["page"]:
                values["print"] = printed["y"]
                compared = True
                diff = printed["y"] - line["y"]
                if abs(diff) > PRINT_TOLERANCE:
                    faults.append(f"Pr{diff:+.2f}")
            label = f"p{line['page']} at: {short(line['text'])}"
        else:
            up, last = last, index
            previous = reference[up] if up is not None else None
            model = get("word", index)
            c = residual(index)
            top = previous is None or (line["page"] is not None
                                       and previous["page"] != line["page"])
            if top:
                values["pdf"] = line["y"] if pdf else None
                if model and model["anchored"] and model["first"]:
                    values["word"] = model["y"]
                if printed:
                    values["print"] = printed["y"]
                    if (pdf and c is not None
                            and printed["page"] == line["page"]):
                        compared = True
                        diff = printed["y"] - c - line["y"]
                        if abs(diff) > PRINT_TOLERANCE:
                            faults.append(f"Pr{diff:+.2f}")
                label = f"p{line['page'] or 1} top: {short(line['text'])}"
            else:
                values["pdf"] = line["y"] - previous["y"] if pdf else None
                upper = get("word", up)
                if (model and upper and model["segment"] == upper["segment"]
                        and model["page"] == upper["page"]):
                    values["word"] = model["y"] - upper["y"]
                screen, upper = get("screen", index), get("screen", up)
                c_up = residual(up)
                if screen and upper:
                    values["screen"] = screen["y"] - upper["y"]
                    if c is not None and c_up is not None:
                        values["scr+c"] = values["screen"] - (c - c_up)
                upper = get("print", up)
                if printed and upper and printed["page"] == upper["page"]:
                    values["print"] = printed["y"] - upper["y"]
                    if pdf and c is not None and c_up is not None:
                        print_gaps.append(
                            values["print"] - (c - c_up) - values["pdf"]
                        )
                label = (f"{short(previous['text'])} → "
                         f"{short(line['text'])}")
            style = (model or {}).get("style") or (
                get("screen", index) or {}).get("style")
            if style:
                label += f" ({style})"
            if pdf and values["word"] is not None:
                compared = True
                diff = values["word"] - values["pdf"]
                if abs(diff) > WORD_TOLERANCE:
                    faults.append(f"W{diff:+.2f}")
            if pdf and values["scr+c"] is not None:
                compared = True
                diff = values["scr+c"] - values["pdf"]
                if abs(diff) > SCREEN_TOLERANCE:
                    faults.append(f"H{diff:+.2f}")
        verdict = " ".join(faults) if faults else ("ok" if compared else "-")
        checked += compared
        outside += bool(faults)
        out.append(
            f"{fmt(values['word'])} {fmt(values['pdf'])} "
            f"{fmt(values['screen'])} {fmt(values['scr+c'])} "
            f"{fmt(values['print'])}  {verdict:<18} {label}"
        )
    summary = f"{name}: {checked} gaps checked, {outside} outside tolerance"
    printed = sources.get("print") or []
    if printed:
        off = [line for line in printed
               if abs(line["y"] - GRID * round(line["y"] / GRID))
               > GRID_TOLERANCE]
        summary += (f"; print: {len(printed)} baselines, {len(off)} off the "
                    f"{GRID} pt grid")
        for line in off:
            out.append(f"off grid: p{line['page']} {line['y']:.2f} "
                       f"{short(line['text'])}")
        if print_gaps:
            mean = sum(print_gaps) / len(print_gaps)
            verdict = ""
            if len(print_gaps) >= MEAN_MINIMUM:
                verdict = (" ok" if abs(mean) <= MEAN_TOLERANCE
                           else " outside")
            summary += (f", mean gap after c {mean:+.2f} pt against the PDF "
                        f"over {len(print_gaps)} gaps{verdict}")
    out.append(f"xfmt result: {summary}")
    return out, checked, outside


# Driver ----------------------------------------------------------------

def load_manifest(path, names):
    manifest = tomllib.loads(path.read_text())
    docs = []
    for entry in manifest.get("doc", []):
        doc = dict(entry)
        for field in ("source", "html", "config"):
            if doc.get(field):
                doc[field] = (path.parent / doc[field]).resolve()
        main = doc.get("source") or doc.get("html")
        doc.setdefault("name", main.stem)
        doc["dir"] = main.parent
        if not names or doc["name"] in names:
            docs.append(doc)
    missing = set(names) - {doc["name"] for doc in docs}
    if missing:
        raise SystemExit(f"run.py: not in the manifest: {sorted(missing)}")
    return docs


def main():
    parser = argparse.ArgumentParser(
        description="Cross-format baseline tables (docs/dev/xfmt-harness.md)"
    )
    parser.add_argument("names", nargs="*", help="documents of the manifest")
    parser.add_argument("--writer", default=os.environ.get(
        "XFMT_WRITER", ROOT / "target" / "release" / "writer"))
    parser.add_argument("--out", default=ROOT / "target" / "xfmt")
    parser.add_argument("--manifest",
                        default=ROOT / "tests" / "xfmt" / "xfmt.toml")
    parser.add_argument("--doc", help="one document (.md, or .html as is)")
    parser.add_argument("--config", help="writer.toml for --doc")
    parser.add_argument("--media", default="print")
    parser.add_argument("--root", default=DEFAULT_ROOT)
    parser.add_argument("--width", type=float)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--update", action="store_true")
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    # Ctrl-C, SIGTERM and SIGHUP end the run through run_chrome's finally.
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, lambda number, _: sys.exit(128 + number))
    started = time.monotonic()
    out = Path(args.out).resolve()
    writer = Path(args.writer).resolve()
    if args.doc:
        path = Path(args.doc).resolve()
        doc = {"name": path.stem, "dir": path.parent, "media": args.media,
               "root": args.root, "width": args.width}
        doc["html" if path.suffix == ".html" else "source"] = path
        if args.config:
            doc["config"] = Path(args.config).resolve()
        docs = [doc]
        expected_dir = None
    else:
        manifest = Path(args.manifest).resolve()
        docs = load_manifest(manifest, args.names)
        expected_dir = manifest.parent / "expected"
    out.mkdir(parents=True, exist_ok=True)

    errors = {}
    jobs = []
    built = {}
    for doc in docs:
        name = doc["name"]
        target = out / name
        try:
            if doc.get("source") and not writer.exists():
                raise Failure(f"no writer binary at {writer}; build it with "
                              "`cargo build --release` or pass --writer")
            paths = build(doc, writer, target)
        except (Failure, OSError) as error:
            errors[name] = str(error)
            continue
        target.mkdir(parents=True, exist_ok=True)
        width = doc.get("width")
        if width is None and "docx" in paths:
            page = word.section(word.ET.fromstring(
                word.zipfile.ZipFile(paths["docx"]).read("word/document.xml")
            ))
            width = page["text_width"] / PT_PER_PX
        paths["screen"] = target / "screen.json"
        paths["print"] = target / "print.pdf"
        for kind in ("screen", "print"):
            if paths[kind].exists():
                paths[kind].unlink()
        jobs.append({
            "html": str(paths["html"]), "lines": str(paths["screen"]),
            "pdf": str(paths["print"]) if doc.get("print", True) else None,
            "media": doc.get("media", "print"),
            "root": doc.get("root", DEFAULT_ROOT),
            "width": round(width or 800),
        })
        built[name] = paths
    version, stderr = run_chrome(jobs, out, args.verbose)
    if version:
        print(version)

    status = 0
    total_checked = total_outside = 0
    for doc in docs:
        name = doc["name"]
        if name in errors:
            print(f"== {name}\nxfmt error: {errors[name]}")
            status = 2
            continue
        paths = built[name]
        try:
            if not paths["screen"].exists():
                why = [line for line in stderr.splitlines()
                       if str(paths["html"]) in line]
                raise Failure("Chrome did not measure the HTML"
                              + (f": {why[0]}" if why else ""))
            sources = {"screen": screen_lines(paths["screen"])}
            if "pdf" in paths:
                sources["pdf"] = pdf_lines(paths["pdf"])
            if paths["print"].exists():
                sources["print"] = pdf_lines(paths["print"])
            reference = sources.get("pdf") or sources["screen"]
            sources["word"] = word_lines(doc, paths, reference)
            lines, checked, outside = table(name, sources)
        except (Failure, OSError, subprocess.CalledProcessError,
                ValueError, KeyError) as error:
            print(f"== {name}\nxfmt error: {error}")
            status = 2
            continue
        total_checked += checked
        total_outside += outside
        text = "\n".join(lines) + "\n"
        print(text, end="")
        if args.strict and outside:
            status = max(status, 1)
        if expected_dir is None:
            continue
        expected = expected_dir / f"{name}.txt"
        if args.update:
            expected.parent.mkdir(parents=True, exist_ok=True)
            expected.write_text(text)
        elif args.check:
            if not expected.exists():
                print(f"xfmt check: {name}: no expected table {expected}")
                status = max(status, 1)
                continue
            diff = list(difflib.unified_diff(
                expected.read_text().splitlines(), text.splitlines(),
                f"expected/{name}.txt", "this run", lineterm=""
            ))
            if diff:
                print("\n".join(diff))
                print(f"xfmt check: {name}: differs from the expected table")
                status = max(status, 1)
            else:
                print(f"xfmt check: {name}: as expected")
    elapsed = time.monotonic() - started
    print(f"xfmt: {len(docs)} documents, {total_checked} gaps checked, "
          f"{total_outside} outside tolerance, {elapsed:.1f} s")
    return status


if __name__ == "__main__":
    sys.exit(main())
