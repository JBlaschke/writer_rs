"""Print the line metrics of font files: units per em, hhea, typo and win
metrics, and Word's single line s and ascent A in em (design.md §7.4).
usage: metrics.py <font.ttf|otf|ttc> [...]

s and A follow src/docx/lines.rs (`sfnt_line_metrics`): s is (winAscent +
winDescent + max(0, hhea lineGap - (winAscent + winDescent - hhea ascender -
|hhea descender|))) / unitsPerEm, or the hhea ascender, descender and line
gap without an OS/2 table; A is s less the hhea descender."""
import os
import re
import struct
import sys

# s and A of families whose files may be missing on the machine, from R12
# §2.1 and R13 §0.3. Calibri's A is computed from the font tables, not
# measured (R13 §6.4); so are Consolas's (Consola.ttf, which comes with
# Microsoft Word on macOS: win 1884 + 514, hhea descender 527, gap 350).
KNOWN = {
    "source sans 3": (1.424, 1.024),
    "libertinus serif": (1.14, 0.894),
    "dejavu sans mono": (2384 / 2048, 1901 / 2048),
    "calibri": (2500 / 2048, 1950 / 2048),
    "consolas": (2398 / 2048, 1871 / 2048),
}


def tables(data, start):
    count = struct.unpack(">H", data[start + 4:start + 6])[0]
    found = {}
    for index in range(count):
        at = start + 12 + 16 * index
        tag, _, offset, length = struct.unpack(">4sIII", data[at:at + 16])
        found[tag.decode("latin1")] = (offset, length)
    return found


def family(data, table):
    """The typographic family (name 16), else the family (name 1)."""
    if "name" not in table:
        return None
    base = table["name"][0]
    count, strings = struct.unpack(">HH", data[base + 2:base + 6])
    names = {}
    for index in range(count):
        at = base + 6 + 12 * index
        platform, encoding, _, name, length, offset = struct.unpack(
            ">HHHHHH", data[at:at + 12]
        )
        if name not in (1, 16):
            continue
        raw = data[base + strings + offset:base + strings + offset + length]
        if platform in (0, 3):
            text = raw.decode("utf-16-be", "replace")
        elif platform == 1 and encoding == 0:
            text = raw.decode("mac_roman", "replace")
        else:
            continue
        names.setdefault(name, text)
    return names.get(16) or names.get(1)


def read(path, index=0):
    """The metrics of face `index` in the font file at `path`, as a dict."""
    data = open(path, "rb").read()
    start = 0
    if data[:4] == b"ttcf":
        start = struct.unpack(">I", data[12 + 4 * index:16 + 4 * index])[0]
    table = tables(data, start)
    head = table["head"][0]
    units = struct.unpack(">H", data[head + 18:head + 20])[0]
    at = table["hhea"][0]
    ascent, descent, gap = struct.unpack(">hhh", data[at + 4:at + 10])
    result = {
        "family": family(data, table), "units": units,
        "hhea": (ascent, descent, gap),
    }
    descent = -descent
    height = ascent + descent + max(gap, 0)
    if "OS/2" in table:
        at = table["OS/2"][0]
        selection = struct.unpack(">H", data[at + 62:at + 64])[0]
        typo = struct.unpack(">hhh", data[at + 68:at + 74])
        win = struct.unpack(">HH", data[at + 74:at + 78])
        windows = sum(win)
        height = windows + max(0, max(gap, 0) - (windows - ascent - descent))
        result.update(
            typo=typo, win=win, use_typo=bool(selection & 128)
        )
    result["single"] = height / units
    result["ascent"] = (height - descent) / units
    return result


FONT_DIRS = (
    "~/Library/Fonts", "/Library/Fonts", "/System/Library/Fonts",
    "/System/Library/Fonts/Supplemental",
    "/Applications/Microsoft Word.app/Contents/Resources/DFonts",
    "/usr/share/fonts", "~/.local/share/fonts", "~/.fonts",
    "C:/Windows/Fonts",
)
STYLED = re.compile(
    r"bold|italic|light|black|medium|semi|extra|thin|heavy"
    r"|condensed|narrow|oblique|[a-z]b\.|[a-z][iz]\.",
    re.I
)


def family_of(path):
    """The family name of the first face in a font file, reading only its
    table directory and name table."""
    with open(path, "rb") as handle:
        head = handle.read(12)
        start = 0
        if head[:4] == b"ttcf":
            start = struct.unpack(">I", handle.read(4))[0]
            handle.seek(start)
            head = handle.read(12)
        count = struct.unpack(">H", head[4:6])[0]
        directory = handle.read(16 * count)
        for index in range(count):
            tag, _, offset, length = struct.unpack(
                ">4sIII", directory[16 * index:16 * index + 16]
            )
            if tag == b"name":
                handle.seek(offset)
                name = handle.read(length)
                return family(name, {"name": (0, length)})
    return None


def styled(path):
    """Whether a font file's name marks a weight or a style."""
    return bool(STYLED.search(os.path.basename(path)))


def lookup(names, dirs=()):
    """s and A for each family in `names` (lower case): from the first font
    file of that family under `dirs` and then FONT_DIRS, preferring a file
    whose name marks no weight or style; else from KNOWN. Families found
    nowhere are left out."""
    wanted = {name.lower() for name in names}
    found = {}
    for root in [*dirs, *FONT_DIRS]:
        root = os.path.expanduser(root)
        if not wanted - found.keys() or not os.path.isdir(root):
            continue
        paths = []
        for folder, _, files in os.walk(root):
            paths += [
                os.path.join(folder, name) for name in files
                if name.lower().endswith((".ttf", ".otf", ".ttc"))
            ]
        paths.sort(key=lambda path: (styled(path), path))
        for path in paths:
            try:
                name = (family_of(path) or "").lower()
            except (OSError, struct.error):
                continue
            if name in wanted and name not in found:
                metrics = read(path)
                found[name] = (metrics["single"], metrics["ascent"])
    for name in wanted - found.keys():
        if name in KNOWN:
            found[name] = KNOWN[name]
    return found


if __name__ == "__main__":
    for path in sys.argv[1:]:
        m = read(path)
        extra = ""
        if "win" in m:
            extra = (
                f" typo {m['typo'][0]} {m['typo'][1]} {m['typo'][2]}"
                f" win {m['win'][0]} {m['win'][1]}"
                f" useTypo {m['use_typo']}"
            )
        print(
            f"{path.split('/')[-1]}: {m['family']!r} upm {m['units']}"
            f" hhea {m['hhea'][0]} {m['hhea'][1]} {m['hhea'][2]}{extra}"
            f" s={m['single']:.4f} A={m['ascent']:.4f}"
        )
