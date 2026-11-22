"""List each text line's page, baseline (pt from the top of its page, 0.01 pt
precision), size and text for one or more PDFs, and the gap to the previous
line.
usage: baselines.py <a.pdf> [<b.pdf> ...]
With two PDFs, lines are paired in order by text and the gaps compared.

Ghostscript's txtwrite device runs at 7200 dpi, 100 units per pt; at 720 dpi
odd multiples of 0.75 pt fall on half units (R13 §0.2). A span's bounding box
starts at its baseline (R12 §9). Spans on one page that overlap vertically
form one line (see `lines`); a space joins two spans with a gap of more than
0.2 em."""
import html
import re
import subprocess
import sys

SPAN = re.compile(r'<span bbox="(\d+) (\d+) (\d+) (\d+)" font="([^"]*)" size="([\d.]+)">')
CHAR = re.compile(r'<char bbox="[^"]*" c="([^"]*)"/>')
UNITS_PER_PT = 100
MM = 25.4 / 72


def spans(pdf):
    """Every text span: [page, baseline, left, right, size, font, text],
    with lengths in pt."""
    out = subprocess.run(
        ["gs", "-q", "-dNOPAUSE", "-dBATCH", "-dSAFER", "-sDEVICE=txtwrite",
         "-r7200", "-dTextFormat=0", "-sOutputFile=-", str(pdf)],
        capture_output=True, text=True, check=True,
    ).stdout
    page = 0
    found = []
    current = None
    for line in out.splitlines():
        if line.startswith("<page"):
            page += 1
        match = SPAN.search(line)
        if match:
            left, y, right = (int(match.group(i)) for i in (1, 2, 3))
            current = [
                page, y / UNITS_PER_PT, left / UNITS_PER_PT,
                right / UNITS_PER_PT, float(match.group(6)),
                match.group(5).split("+")[-1], ""
            ]
            found.append(current)
            continue
        match = CHAR.search(line)
        if match and current is not None:
            current[6] += html.unescape(match.group(1))
    return found


def lines(pdf):
    """Every text line: (page, baseline, left, size, font, text), lengths in
    pt, sorted by page and baseline.

    A span's glyphs take about 0.75 of its size above the baseline and 0.25
    below. A span joins a line when they overlap vertically by at least half
    the smaller one's height, so that a superscript or a subscript joins its
    line; the line's baseline is that of its largest span."""
    rows = []
    for page, y, left, right, size, font, text in sorted(
            spans(pdf), key=lambda span: (span[0], span[1], span[2])):
        top, bottom = y - 0.75 * size, y + 0.25 * size
        row = rows[-1] if rows else None
        if row is not None and row["page"] == page:
            overlap = min(bottom, row["bottom"]) - max(top, row["top"])
            if overlap >= 0.5 * min(size, row["size"]):
                row["parts"].append((left, right, size, text))
                if size > row["size"]:
                    row.update(size=size, y=y, font=font, top=top,
                               bottom=bottom)
                continue
        rows.append({
            "page": page, "y": y, "size": size, "font": font, "top": top,
            "bottom": bottom, "parts": [(left, right, size, text)],
        })
    result = []
    for row in rows:
        parts = sorted(row["parts"])
        text = ""
        end = None
        for left, right, size, piece in parts:
            if end is not None and left - end > 0.2 * size:
                text += " "
            text += piece
            end = right if end is None else max(end, right)
        result.append((
            row["page"], row["y"], parts[0][0], row["size"], row["font"],
            " ".join(text.split())
        ))
    return sorted(result, key=lambda line: (line[0], line[1], line[2]))


def show(rows):
    previous = None
    for page, y, x, size, font, text in rows:
        gap = "" if previous is None or previous[0] != page else \
            f"{y - previous[1]:6.2f}"
        print(f"p{page} {y:8.2f}pt {y * MM:7.2f}mm {gap:>7} {size:5.1f}pt"
              f"  {text[:44]}")
        previous = (page, y)


def compare(a, b):
    used = set()
    pairs = []
    for row in a:
        key = row[5].replace(" ", "")[:16]
        for index, other in enumerate(b):
            if index not in used and other[5].replace(" ", "")[:16] == key:
                used.add(index)
                pairs.append((row, other))
                break
    print(f"{'A baseline':>12} {'A gap':>7} {'B baseline':>12} {'B gap':>7}"
          f" {'B-A gap':>8}  text")
    last = None
    for row, other in pairs:
        if last is None or last[0][0] != row[0] or last[1][0] != other[0]:
            gaps = ("", "", "")
        else:
            ga = row[1] - last[0][1]
            gb = other[1] - last[1][1]
            gaps = (f"{ga:7.2f}", f"{gb:7.2f}", f"{gb - ga:+8.2f}")
        print(f"p{row[0]} {row[1]:8.2f} {gaps[0]:>7} p{other[0]} {other[1]:8.2f}"
              f" {gaps[1]:>7} {gaps[2]:>8}  {row[5][:40]}")
        last = (row, other)


if __name__ == "__main__":
    docs = [lines(path) for path in sys.argv[1:]]
    if len(docs) == 2:
        compare(*docs)
    else:
        for rows in docs:
            show(rows)
