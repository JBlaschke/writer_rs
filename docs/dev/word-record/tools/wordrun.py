"""Measure one probe document in Word: line tops from the object model and
baselines of page 1 from window captures at 500 % zoom.

usage: wordrun.py <in.docx> <out dir> [--captures N] [--step N]

Writes <out dir>/word.json (lines with `top`, and `baseline` for the lines
of page 1 that the captures show)."""
import json
import pathlib
import subprocess
import sys

TOOLS = pathlib.Path(__file__).parent


def run(*args):
    result = subprocess.run(
        [sys.executable, *map(str, args)],
        capture_output=True, text=True, timeout=600,
    )
    if result.returncode != 0:
        raise SystemExit(result.stdout + result.stderr)
    return result.stdout


def main():
    args = [arg for arg in sys.argv[1:] if not arg.startswith("--")]
    options = dict(zip(sys.argv[1:], sys.argv[2:]))
    docx, out = pathlib.Path(args[0]), pathlib.Path(args[1])
    captures = int(options.get("--captures", 3))
    step = options.get("--step", "5")
    out.mkdir(parents=True, exist_ok=True)
    run(TOOLS / "wordq.py", docx, out / "word.json", "--zoom", "500")
    shots = [out / "word-0.png"]
    run(TOOLS / "wordshot.py", shots[0], "--up", "60")
    for number in range(1, captures):
        shots.append(out / f"word-{number}.png")
        run(TOOLS / "wordshot.py", shots[-1], "--down", step)
    print(run(TOOLS / "wordcap.py", *shots, "--zoom", "500", "--debug",
              "--json", out / "word-base.json"), end="")
    name = (out / "word.name").read_text()
    closing = subprocess.run(
        ["osascript", "-e",
         f'tell application "Microsoft Word" to close document "{name}" '
         "saving no"],
        capture_output=True, text=True, timeout=60,
    )
    if closing.returncode != 0:
        print("close failed:", closing.stderr.strip())
    scratch = pathlib.Path.home() / "Documents" / "writer-spacing-scratch"
    for stale in (scratch / name, scratch / ("~$" + name[2:])):
        if stale.exists():
            stale.unlink()
    lines = json.loads((out / "word.json").read_text())
    bases = json.loads((out / "word-base.json").read_text())
    first_page = [line for line in lines if line["page"] == 1]
    if len(bases) > len(first_page):
        print(f"warning: {len(bases)} baselines for "
              f"{len(first_page)} lines on page 1")
    for line, base in zip(first_page, bases):
        line["baseline"] = base["baseline"]
        line["cap_top"] = base["cap_top"]
    (out / "word.json").write_text(json.dumps(lines, indent=1))
    previous = None
    for line in lines:
        gap = ""
        if previous is not None and "baseline" in line \
                and "baseline" in previous:
            gap = f"{line['baseline'] - previous['baseline']:7.2f}"
        base = (f"{line['baseline']:8.2f}" if "baseline" in line
                else f"{'':>8}")
        print(f"p{line['page']} top={line['top']:8.2f} base={base} "
              f"gap={gap:>7}  {line['style']:<14} {line['text'][:30]}")
        previous = line


if __name__ == "__main__":
    main()
