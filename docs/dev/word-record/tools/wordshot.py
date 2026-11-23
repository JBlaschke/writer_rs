"""Capture the window of Word's active document, after optional zooming and
scrolling. Works while the screen is locked (the capture is of the window's
own contents).

usage: wordshot.py <out.png> [--zoom N] [--up N] [--down N] [--percent N]"""
import pathlib
import subprocess
import sys

TOOLS = pathlib.Path(__file__).parent


def osascript(script, timeout=60):
    result = subprocess.run(
        ["osascript", "-e", script],
        capture_output=True, text=True, timeout=timeout,
    )
    if result.returncode != 0:
        raise SystemExit("osascript failed: " + result.stderr.strip())
    return result.stdout.strip()


def window_id(title):
    listing = subprocess.run(
        ["swift", str(TOOLS / "winlist.swift"), "Word"],
        capture_output=True, text=True, timeout=120,
    ).stdout
    for row in listing.splitlines():
        if row.rstrip().endswith("title: " + title):
            return row.split()[0]
    raise SystemExit("no Word window titled " + title)


def main():
    options = dict(zip(sys.argv[1:], sys.argv[2:]))
    # A centred line can leave the view scrolled sideways; the captures
    # need the page's left edge.
    steps = ["set horizontal percent scrolled of w to 0"]
    if "--zoom" in options:
        steps.append("set percentage of zoom of view of w to "
                     + options["--zoom"])
    if "--percent" in options:
        steps.append("set vertical percent scrolled of w to "
                     + options["--percent"])
    if "--up" in options:
        steps.append("small scroll w up " + options["--up"])
    if "--down" in options:
        steps.append("small scroll w down " + options["--down"])
    script = (
        'tell application "Microsoft Word"\n'
        "set w to active window\n"
        + "\n".join(steps)
        + "\nreturn name of active document\nend tell"
    )
    name = osascript(script)
    title = name[:-5] if name.endswith(".docx") else name
    subprocess.run(
        ["screencapture", "-x", "-o", "-l", window_id(title), sys.argv[1]],
        check=True, timeout=60,
    )
    print("captured", title)


if __name__ == "__main__":
    main()
