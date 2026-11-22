"""Read text baselines out of captures of Word's document window.

The probe's lines start with a capital H and hold only flat-footed capitals
and digits, so the left stem of the H runs from the cap height down to the
baseline. For every text line the script follows that stem and reports where
it starts and ends, to a fraction of a pixel (the half-brightness crossings),
in points from the page's top edge.

The first capture must show the page's top edge. Further captures of the
same page, scrolled down, are chained to it through the lines they share.

usage: wordcap.py <capture.png> [<more.png> ...] [--zoom 500]
                  [--json out.json] [--debug]"""
import json
import subprocess
import sys


def load(path):
    size = subprocess.run(
        ["magick", "identify", "-format", "%w %h", path],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    width, height = int(size[0]), int(size[1])
    raw = subprocess.run(
        ["magick", path, "-colorspace", "Gray", "-depth", "8", "gray:-"],
        capture_output=True, check=True,
    ).stdout
    assert len(raw) == width * height, (len(raw), width, height)
    return width, height, raw


def crossing(values, level):
    """Index (fractional) where `values` first rises through `level`."""
    for index in range(1, len(values)):
        before, after = values[index - 1], values[index]
        if before < level <= after:
            return index - 1 + (level - before) / (after - before)
    return None


def read_capture(path, debug=False):
    """Text lines of one capture: stem top and bottom in pixel-edge
    coordinates, and the page's top edge row when it is visible."""
    width, height, raw = load(path)

    def pixel(x, y):
        return raw[y * width + x]

    first, last = 150, height - 50      # the view ends above the status bar
    middle = (first + last) // 2
    row = [pixel(x, middle) for x in range(width)]
    background = row[5]
    left = next(x for x in range(5, width) if abs(row[x] - background) > 12)
    probe_x = left + 12
    paper = pixel(probe_x, middle)
    top = None
    for y in range(first, last):
        value = pixel(probe_x, y)
        if abs(value - background) <= 4:
            continue
        # The first row that is not window background: the page outline if
        # it is brighter than the paper, else the page starts above the view.
        if value > paper + 20:
            top = y
        break
    x0, x1 = left + 20, width - 60
    span = min(x1, x0 + 1600)
    ink = max(pixel(x, y) for y in range(first, last, 3)
              for x in range(x0, span, 2))
    level = paper + (ink - paper) * 0.5
    if debug:
        print(path.split("/")[-1], "background", background, "paper", paper,
              "ink", ink, "left edge", left, "top edge", top)

    def has_ink(y):
        base = y * width
        return any(raw[base + x] > level for x in range(x0, span))

    start_row = first if top is None else top + 4
    rows = [has_ink(y) for y in range(start_row, last)]
    runs = []
    start = None
    for index, flag in enumerate(rows + [False]):
        if flag and start is None:
            start = index
        elif not flag and start is not None:
            runs.append((start_row + start, start_row + index - 1))
            start = None

    lines = []
    for y_top, y_bottom in runs:
        if y_top <= start_row + 1 or y_bottom >= last - 3:
            continue                     # cut off by the edge of the view
        # The H's left stem, found a quarter of the way down (above the
        # crossbar): the first run of inked columns.
        quarter = y_top + (y_bottom - y_top) // 4
        stem_left = next(
            (x for x in range(x0, x1) if pixel(x, quarter) > level), None
        )
        if stem_left is None:
            continue
        stem_right = stem_left
        while pixel(stem_right + 1, quarter) > level:
            stem_right += 1
        if stem_right - stem_left < 2:
            band = [stem_left, stem_right]
        else:
            band = list(range(stem_left + 1, stem_right))
        y_from, y_to = y_top - 6, y_bottom + 7
        profile = [
            sum(pixel(x, y) for x in band) / len(band)
            for y in range(y_from, y_to)
        ]
        half = paper + (max(profile) - paper) * 0.5
        rise = crossing(profile, half)
        fall = crossing(profile[::-1], half)
        if rise is None or fall is None:
            continue
        # A crossing at index i + f puts the glyph edge at pixel edge
        # y_from + i + f + 0.5 (pixel centres sit at half-integers).
        cap_top = y_from + rise + 0.5
        baseline = y_from + (len(profile) - 1 - fall) + 0.5
        lines.append({
            "stem": stem_right - stem_left + 1,
            "top_px": cap_top, "base_px": baseline,
        })
    return top, lines


def main():
    paths = [arg for arg in sys.argv[1:]
             if arg.endswith(".png")]
    options = dict(zip(sys.argv[1:], sys.argv[2:]))
    zoom = float(options.get("--zoom", 500)) / 100
    debug = "--debug" in sys.argv

    top, lines = read_capture(paths[0], debug)
    if top is None:
        raise SystemExit("the first capture does not show the page's top edge")
    merged = [dict(line, y=line["base_px"] - top, t=line["top_px"] - top)
              for line in lines]
    for path in paths[1:]:
        _, more = read_capture(path, debug)
        # The shift that maps this capture onto the merged lines: the one
        # for which most baselines coincide.
        best = (0, None)
        for known in merged:
            for line in more[:4]:
                shift = known["y"] - line["base_px"]
                hits = sum(
                    1 for a in more for b in merged
                    if abs(a["base_px"] + shift - b["y"]) < 0.8
                )
                if hits > best[0]:
                    best = (hits, shift)
        hits, shift = best
        if hits < 2:
            raise SystemExit(f"{path}: cannot chain (only {hits} shared line)")
        if debug:
            print(path.split("/")[-1], "chained on", hits, "lines, shift",
                  round(shift, 1), "px")
        for line in more:
            y = line["base_px"] + shift
            if all(abs(y - known["y"]) >= 0.8 for known in merged):
                merged.append(dict(line, y=y, t=line["top_px"] + shift))
        merged.sort(key=lambda line: line["y"])

    result = []
    previous = None
    for line in merged:
        entry = {
            "baseline": round(line["y"] / zoom, 3),
            "cap_top": round(line["t"] / zoom, 3),
            "stem_px": line["stem"],
        }
        gap = ""
        if previous is not None:
            gap = f"{entry['baseline'] - previous['baseline']:7.2f}"
        print(f"baseline={entry['baseline']:8.2f}pt gap={gap:>7}  "
              f"cap={entry['baseline'] - entry['cap_top']:5.2f}pt "
              f"stem={entry['stem_px']}px")
        result.append(entry)
        previous = entry
    if "--json" in options:
        with open(options["--json"], "w") as handle:
            json.dump(result, handle, indent=1)


if __name__ == "__main__":
    main()
