"""Ask Microsoft Word where it lays out each line of a DOCX.

Word's object model reports the top of the line that holds a character
("vertical position relative to page", in points, whole twips). This script
opens a copy of the document, lists every paragraph, and asks for the line
top at the start of each paragraph and after each manual line break.

The copy has `w:updateFields` removed (Word would ask about updating fields)
and lives in a folder under ~/Documents. The document is left open: Word
cannot close documents while a dialog of its own is waiting.

usage: wordq.py <in.docx> <out.json> [--capture <out.png> --zoom N]"""
import json
import pathlib
import re
import subprocess
import sys
import time
import zipfile

SCRATCH = pathlib.Path.home() / "Documents" / "writer-spacing-scratch"
TOOLS = pathlib.Path(__file__).parent
SEP = "¦"

OPEN = '''
set source to (POSIX file "{docx}")
tell application "Microsoft Word"
    open source
    set d to active document
    set n to count paragraphs of d
    set acc to ""
    repeat with i from 1 to n
        set r to text object of paragraph i of d
        set acc to acc & i & "{sep}" & (start of content of r) & "{sep}" ¬
            & (end of content of r) & "{sep}" & (content of r) & "{sep}{sep}"
    end repeat
    return acc
end tell
'''

TOPS = '''
tell application "Microsoft Word"
    set d to active document
    set acc to ""
    repeat with s in {{{offsets}}}
        set r to create range d start s end s
        set y to get range information r ¬
            information type vertical position relative to page
        set p to get range information r ¬
            information type active end page number
        set acc to acc & s & "{sep}" & y & "{sep}" & p & linefeed
    end repeat
    return acc
end tell
'''

VIEW = '''
tell application "Microsoft Word"
    set w to active window
    set percentage of zoom of view of w to {zoom}
    set vertical percent scrolled of w to {scroll}
    return name of active document
end tell
'''


def osascript(script, timeout=180):
    result = subprocess.run(
        ["osascript", "-e", script],
        capture_output=True, text=True, timeout=timeout,
    )
    if result.returncode != 0:
        raise SystemExit("osascript failed: " + result.stderr.strip())
    return result.stdout


def strip_update_fields(source, target):
    with zipfile.ZipFile(source) as zin, \
            zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "word/settings.xml":
                text = data.decode("utf-8")
                text = re.sub(r"<w:updateFields[^>]*/>", "", text)
                data = text.encode("utf-8")
            zout.writestr(item, data)


def paragraph_styles(docx):
    """The style id of each body paragraph, in document order."""
    with zipfile.ZipFile(docx) as archive:
        xml = archive.read("word/document.xml").decode("utf-8")
    body = xml.split("<w:body>", 1)[1]
    styles = []
    for chunk in re.findall(r"<w:p[ >].*?</w:p>|<w:p/>", body, flags=re.S):
        found = re.search(r'<w:pStyle w:val="([^"]+)"', chunk)
        styles.append(found.group(1) if found else "")
    return styles


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
    args = [arg for arg in sys.argv[1:] if not arg.startswith("--")]
    options = dict(zip(sys.argv[1:], sys.argv[2:]))
    source, output = pathlib.Path(args[0]), pathlib.Path(args[1])
    SCRATCH.mkdir(exist_ok=True)
    # A fresh name each time: Word does not reopen a document it has open.
    stamp = time.strftime("%H%M%S")
    docx = SCRATCH / f"q{stamp}-{source.stem}.docx"
    strip_update_fields(source, docx)

    listing = osascript(OPEN.format(docx=docx, sep=SEP))
    # The reported tops are snapped to screen pixels at the zoom in effect,
    # so zoom in first: 500 % puts them on a 0.2 pt grid.
    zoom = int(options.get("--zoom", 500))
    scroll = int(options.get("--scroll", 0))
    name = osascript(VIEW.format(zoom=zoom, scroll=scroll)).strip()
    if name != docx.name:
        raise SystemExit(f"Word's active document is {name}, not {docx.name}")
    paragraphs = []
    for row in listing.split(SEP + SEP):
        if not row.strip():
            continue
        index, start, end, text = row.split(SEP, 3)
        paragraphs.append({
            "index": int(index), "start": int(start), "end": int(end),
            "text": text,
        })
    styles = paragraph_styles(source)

    lines = []
    for number, paragraph in enumerate(paragraphs):
        text = paragraph["text"]
        starts = [0] + [k + 1 for k, char in enumerate(text) if char == "\x0b"]
        pieces = re.split("\x0b", text.rstrip("\r\x07"))
        for line, (offset, piece) in enumerate(zip(starts, pieces)):
            lines.append({
                "paragraph": paragraph["index"], "line": line + 1,
                "of": len(pieces), "offset": paragraph["start"] + offset,
                "style": styles[number] if number < len(styles) else "?",
                "text": piece.strip(),
            })
    offsets = ", ".join(str(line["offset"]) for line in lines)
    answer = osascript(TOPS.format(offsets=offsets, sep=SEP))
    tops = {}
    for row in answer.splitlines():
        if row.strip():
            offset, top, page = row.split(SEP)
            tops[int(offset)] = (float(top.replace(",", ".")), int(page))
    for line in lines:
        line["top"], line["page"] = tops[line["offset"]]

    if "--capture" in options:
        title = name[:-5] if name.endswith(".docx") else name
        subprocess.run(
            ["screencapture", "-x", "-o", "-l", window_id(title),
             options["--capture"]],
            check=True, timeout=60,
        )
    output.write_text(json.dumps(lines, indent=1))
    output.with_suffix(".name").write_text(docx.name)
    last = None
    for line in lines:
        gap = ""
        if last is not None and last["page"] == line["page"]:
            gap = f"{line['top'] - last['top']:7.2f}"
        print(f"p{line['page']} top={line['top']:8.2f} {gap:>7}  "
              f"{line['style']:<12} {line['text'][:40]}")
        last = line


if __name__ == "__main__":
    main()
