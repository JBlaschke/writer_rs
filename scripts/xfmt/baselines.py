"""List each text line's page, baseline (mm from the top, 0.01 pt precision),
size and text for one or more PDFs, and the gap to the previous line.
usage: baselines.py <a.pdf> [<b.pdf> ...]
With two PDFs, lines are paired in order by text and the gaps compared."""
import re
import subprocess
import sys

SPAN = re.compile(r'<span bbox="(\d+) (\d+) (\d+) (\d+)" font="([^"]*)" size="([\d.]+)">')
CHAR = re.compile(r'<char bbox="[^"]*" c="([^"]*)"/>')
MM = 25.4 / 7200


def lines(pdf):
    out = subprocess.run(
        ["gs", "-q", "-dNOPAUSE", "-dBATCH", "-dSAFER", "-sDEVICE=txtwrite",
         "-r7200", "-dTextFormat=0", "-sOutputFile=-", pdf],
        capture_output=True, text=True, check=True,
    ).stdout
    page = 0
    spans = []
    current = None
    for line in out.splitlines():
        if line.startswith("<page"):
            page += 1
        match = SPAN.search(line)
        if match:
            current = [page, int(match.group(2)), int(match.group(1)),
                       float(match.group(6)), match.group(5), ""]
            spans.append(current)
            continue
        match = CHAR.search(line)
        if match and current is not None:
            current[5] += match.group(1)
    merged = {}
    for page, y, x, size, font, text in spans:
        key = (page, y)
        if key in merged:
            row = merged[key]
            row[0] = min(row[0], x)
            row[3] += text
        else:
            merged[key] = [x, size, font, text, x]
    rows = []
    for (page, y), (x, size, font, text, _) in sorted(merged.items()):
        rows.append((page, y * MM, x * MM, size, font.split("+")[-1], text))
    return rows


def show(rows):
    previous = None
    for page, y, x, size, font, text in rows:
        gap = "" if previous is None or previous[0] != page else \
            f"{y - previous[1]:6.2f}"
        print(f"p{page} {y:7.2f}mm {gap:>7} {size:5.1f}pt  {text[:44]}")
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
