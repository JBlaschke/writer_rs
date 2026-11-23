"""Close every document Word has open whose name starts with `q` and ends
with `.docx` (the scratch copies made by wordq.py), without saving, and
delete the scratch copies. Prints what Word still has open afterwards.

usage: wordclose.py"""
import pathlib
import subprocess

SCRATCH = pathlib.Path.home() / "Documents" / "writer-spacing-scratch"


def osascript(script):
    result = subprocess.run(
        ["osascript", "-e", script],
        capture_output=True, text=True, timeout=60,
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def main():
    code, names = osascript(
        'tell application "Microsoft Word" to return name of every document'
    )
    print("open before:", names)
    for name in [part.strip() for part in names.split(",")]:
        if name.startswith("q") and name.endswith(".docx"):
            code, answer = osascript(
                f'tell application "Microsoft Word" to close document '
                f'"{name}" saving no'
            )
            print("close", name, "->", code, answer)
    if SCRATCH.exists():
        for stale in SCRATCH.iterdir():
            stale.unlink()
            print("removed", stale.name)
    code, names = osascript(
        'tell application "Microsoft Word" to return name of every document'
    )
    print("open after:", names)


if __name__ == "__main__":
    main()
