"""Predicted Word values for the Word record (T103).

Every value follows from R12's line box and spacing model (design.md §0.3,
R12 §2.2-2.4) and R13 §3.3-3.4, with each family's s and A computed from
its font tables as src/docx/lines.rs does:

    s = (winAscent + winDescent
         + max(0, hhea lineGap - (winAscent + winDescent
                                  - hhea ascender + hhea descender)))
        / unitsPerEm
    A = s - |hhea descender| / unitsPerEm

An auto line of w:line L at size z is (L/240) s z tall with
min(1, L/240) A z above the baseline; an exact line of h is 0.8 h above
and 0.2 h below. Between two paragraphs the larger of the upper one's
space after and the lower one's space before holds; a paragraph border
adds its w:space and its width. A table edge does not collapse spaces:
the paragraph's space, the rule and the cell paragraph's space add up.

usage: python3 predict.py           print the reading sheet (Markdown)
       python3 predict.py --write   replace the sheet in README.md
       python3 predict.py --fonts   recompute s and A from the font files
                                    (Word's DFonts and ~/Library/Fonts)
"""
import pathlib
import struct
import sys

HERE = pathlib.Path(__file__).parent
DFONTS = pathlib.Path(
    "/Applications/Microsoft Word.app/Contents/Resources/DFonts"
)
USER_FONTS = pathlib.Path.home() / "Library" / "Fonts"

# family: unitsPerEm, winAscent, winDescent, hhea ascender, descender,
# lineGap, and the file and face they were read from (2026-11-22, Word
# 16.115.1's DFonts; Source Sans 3 from ~/Library/Fonts, the file R12 used).
FONTS = {
    "Source Sans 3": (1000, 934, 288, 1024, -400, 0,
                      USER_FONTS / "SourceSans3-Regular.ttf", 0),
    "Calibri": (2048, 1950, 550, 1950, -550, 0,
                DFONTS / "Calibri.ttf", 0),
    "Calibri Light": (2048, 1950, 550, 1950, -550, 0,
                      DFONTS / "calibril.ttf", 0),
    "Consolas": (2048, 1884, 514, 1521, -527, 350,
                 DFONTS / "Consola.ttf", 0),
    "Cambria Math": (2048, 6383, 5045, 1946, -455, 0,
                     DFONTS / "Cambria.ttc", 1),
    # Cambria's face: its win, hhea and typo lines all equal Cambria
    # Math's hhea and typo lines (2401/2048), so it stands for Word using
    # those in place of Cambria Math's tall win metrics.
    "Cambria": (2048, 1946, 455, 1946, -455, 0,
                DFONTS / "Cambria.ttc", 0),
}


def single_and_ascent(upm, win_ascent, win_descent, ascender, descender,
                      line_gap):
    win = win_ascent + win_descent
    extra = max(0, line_gap - (win - (ascender - descender)))
    single = (win + extra) / upm
    return single, single + descender / upm


METRICS = {
    family: single_and_ascent(*row[:6]) for family, row in FONTS.items()
}


def read_font(path, face):
    """hhea and OS/2 numbers of one face, as FONTS lists them."""
    data = path.read_bytes()
    start = 0
    if data[:4] == b"ttcf":
        start = struct.unpack(">I", data[12 + 4 * face:16 + 4 * face])[0]
    count = struct.unpack(">H", data[start + 4:start + 6])[0]
    tables = {}
    for index in range(count):
        row = start + 12 + 16 * index
        tag, _, offset, _ = struct.unpack(">4sIII", data[row:row + 16])
        tables[tag.decode("latin1")] = offset
    upm = struct.unpack(">H", data[tables["head"] + 18:tables["head"] + 20])
    hhea = struct.unpack(">hhh", data[tables["hhea"] + 4:tables["hhea"] + 10])
    win = struct.unpack(">HH", data[tables["OS/2"] + 74:tables["OS/2"] + 78])
    return (upm[0], win[0], win[1], *hhea)


def check_fonts():
    for family, row in FONTS.items():
        path, face = row[6], row[7]
        if not path.exists():
            print(f"{family}: {path} not found")
            continue
        found = read_font(path, face)
        single, ascent = single_and_ascent(*found)
        status = "ok" if found == row[:6] else f"DIFFERS from {row[:6]}"
        print(f"{family}: s {single:.6f}, A {ascent:.6f} "
              f"({path.name} face {face}: {found}) {status}")


class Box:
    """A line box: its height and the part above the baseline (pt)."""

    def __init__(self, height, above):
        self.height = height
        self.above = above
        self.below = height - above


def auto(size, line, family):
    single, ascent = METRICS[family]
    ratio = line / 240
    return Box(ratio * single * size, min(1, ratio) * ascent * size)


def exact(height):
    return Box(height, 0.8 * height)


# Design (b), R12's probe design: Source Sans 3 11 pt at 1.35 (w:line 228),
# P = 3.6 mm (204 twips, 10.2 pt).
SS = "Source Sans 3"
BODY = auto(11, 228, SS)
CODE = auto(10, 228, "Consolas")
CODE277 = auto(10, 277, "Consolas")
NOTE = auto(9, 228, SS)
CAPTION = auto(10, 228, SS)
SINGLE11 = auto(11, 240, SS)
X12 = exact(12.1)
P = 10.2
COMPACT = 1.8
# Design (a), no design: Calibri 11 pt at w:line 264, Body Text 6/6 pt.
BODY_A = auto(11, 264, "Calibri")
H1_A = auto(18, 264, "Calibri Light")
CODE_A = auto(10, 264, "Consolas")
NOTE_A = auto(9, 264, "Calibri")
CAPTION_A = auto(10, 264, "Calibri")


def total(*terms):
    return sum(value for value, _ in terms)


def explain(*terms):
    return " + ".join(f"{value:.2f} {label}" for value, label in terms)


ROWS = []


def row(key, docx, read, method, predicted, model, source, task):
    ROWS.append((key, docx, read, method, predicted, model, source, task))


def gap(key, docx, read, terms, source, task, method="C, Q", tops=None,
        note=""):
    """A baseline gap; `tops` is the line-top gap when it differs."""
    value = f"{total(*terms):.2f}"
    if tops is not None:
        value += f" (Q {total(*tops):.2f})"
    model = explain(*terms)
    if tops is not None:
        model += f"; Q: {explain(*tops)}"
    if note:
        model += f"; {note}"
    row(key, docx, read, method, value, model, source, task)


def body_gap(key, docx, read, space, source, task, method="C, Q", what=""):
    """Two body lines (design b) with `space` between their boxes."""
    gap(key, docx, read,
        [(BODY.below, "below"), (space, what or "space"),
         (BODY.above, "above")],
        source, task, method)


def sheet_atleast():
    docx = "w01"
    lift = 30 - SINGLE11.below
    row("A1", docx, "HA2 → HB1 baselines (into a 30 pt atLeast line)", "C",
        f"H1 {X12.below + lift:.2f}; H2 {X12.below + SINGLE11.above:.2f}; "
        f"H3 {X12.below + 24:.2f}",
        f"{X12.below:.2f} below X12 + above HB1. H1, the extra above the "
        f"text: 30 - {SINGLE11.below:.2f} = {lift:.2f}; H2, ascent kept as "
        f"in an auto line: {SINGLE11.above:.2f}; H3, as an exact line: 24",
        "R13: no value (§3.1 M3 leaves atLeast to T103)", "T106")
    gap("A2", docx, "HB1 → HB2 (the atLeast line's pitch)",
        [(30, "w:line 600")], "–", "T106")
    row("A3", docx, "HB2 → HC1 baselines (out of the atLeast line)", "C",
        f"H1 {SINGLE11.below + X12.above:.2f}; "
        f"H2 {30 - SINGLE11.above + X12.above:.2f}; "
        f"H3 {6 + X12.above:.2f}",
        f"below HB2 + {X12.above:.2f} above X12. Below: H1 "
        f"{SINGLE11.below:.2f}, H2 {30 - SINGLE11.above:.2f}, H3 6",
        "R13: no value", "T106")
    gap("A4", docx, "HC2 → HD1 (into a 10 pt atLeast line, below a "
        "single line)",
        [(X12.below, "below X12"), (SINGLE11.above, "single-line ascent")],
        "–", "T106", method="C")
    gap("A5", docx, "HD1 → HD2", [(SINGLE11.height, "single line 1.424 em")],
        "–", "T106")
    gap("A6", docx, "HD2 → HE1",
        [(SINGLE11.below, "single-line descent"),
         (X12.above, "above X12")], "–", "T106", method="C")


def sheet_widows():
    docx = "w02"
    default = "R13 0.25: likely on, about 2 lines (u)"
    # key, test, line first on the next page if on, if off, that page
    cases = [
        ("W1", "1, no w:widowControl, orphan", "HB1", "HB2", 2, default),
        ("W2", "2, no w:widowControl, widow", "HD4", "HD5", 4, default),
        ("W3", "3, w:widowControl on, orphan", "HF1", None, 6, "control"),
        ("W4", "4, w:widowControl on, widow", "HK4", None, 8, "control"),
        ("W5", "5, w:widowControl off, orphan", None, "HM2", 10, "control"),
        ("W6", "6, w:widowControl off, widow", None, "HP5", 12, "control"),
    ]
    for key, test, on, off, page, source in cases:
        values = []
        if on:
            values.append(f"on: {on}")
        if off:
            values.append(f"off: {off}")
        model = ("36 lines of 20 pt fit a page; widow control moves a "
                 "lone first line (orphan) to the next page, or takes a "
                 "second line along with a lone last line (widow)")
        row(key, docx, f"Test {test}: the first test line on p{page}",
            "P", "; ".join(values), model, source, "T118")


def sheet_family(first, docx, family, alias=None):
    """Three readings from `first` on; `alias` gives the values if Word
    lays the family out with another face's metrics."""
    single, ascent = METRICS[family]
    s24, a24 = 24 * single, 24 * ascent
    source = "R13 §0.3: s 1.2207; §6.4: A about 0.952"
    if family != "Calibri":
        source = "– (R13 §3.1 M3: computed from the tables)"
    alt = [""] * 3
    if alias:
        other_s, other_a = METRICS[alias]
        label = f"; with hhea metrics (as {alias}):"
        alt = [f"{label} {6 + 24 * other_a:.2f}",
               f"{label} {24 * other_s:.2f}",
               f"{label} {24 * (other_s - other_a) + 24:.2f}"]
    row(f"F{first}", docx, f"{family}: HA2 → HB1 baselines (gives A)", "C",
        f"{6 + a24:.2f}", f"6 below the exact 30 pt fence + 24 A "
        f"({ascent:.6f}){alt[0]}", source, "T106")
    row(f"F{first + 1}", docx, f"{family}: HB1 → HB2 (gives s)", "C, Q",
        f"{s24:.2f}", f"24 s ({single:.6f}){alt[1]}", source, "T106")
    row(f"F{first + 2}", docx, f"{family}: HB2 → HC1 baselines (check)",
        "C", f"{s24 - a24 + 24:.2f}",
        f"24 (s - A) = {s24 - a24:.2f} + 24 above the fence{alt[2]}",
        source, "T106")


def sheet_lists():
    docx = "w07"
    body_gap("L1", docx, "HA1 → HB1, paragraph → tight item", P,
             "R12 §8 Word 25.00", "T112", what="P (Compact 1.8 before)")
    body_gap("L2", docx, "HB1 → HB2, tight item → item", COMPACT,
             "R12 §8 Word 16.80", "T112", what="Compact")
    body_gap("L3", docx, "HB2 → HC1, item → nested item", COMPACT,
             "–", "T112", what="Compact")
    body_gap("L4", docx, "HC2 → HB3, nested item → next item", COMPACT,
             "–", "T112", what="Compact")
    body_gap("L5", docx, "HB3 → HD1, tight list → paragraph", COMPACT,
             "R12 §8 Word 16.60; R13 2.3 (b)", "T112",
             what="Compact after (paragraph before 0)")
    body_gap("L6", docx, "HD1 → HE1, paragraph → loose item", P,
             "–", "T112", what="P")
    body_gap("L7", docx, "HE1 → HE2, loose item → item", P,
             "R12 §8 Word 25.20; R13 2.4 (b)", "T112", what="P")
    body_gap("L8", docx, "HE3 → HF1, loose list → paragraph", P,
             "–", "T112", what="P")
    body_gap("L9", docx, "HG1 → HG2, paragraphs in one item", P,
             "R13 2.5 (a) only", "T112", what="P")
    body_gap("L10", docx, "HH1 → HK1, paragraph → term", P,
             "R13 2.19 (b): 25.08 (u)", "T112", what="max(P, 6)")
    body_gap("L11", docx, "HK1 → HL1, term → definition", 0,
             "–", "T112", what="term after 0, definition before 0")
    body_gap("L12", docx, "HL1 → HK2, definition → next term", 6,
             "R13 2.19 (b): 20.88 (u)", "T112", what="6 + 6 → 6")
    body_gap("L13", docx, "HL2 → HM1, definition → paragraph", 6,
             "–", "T112", what="definition after 6")
    body_gap("L14", docx, "HM1 → HN1, paragraph → M4 item", P,
             "R13 M4", "T112", what="P (LI before 0)")
    body_gap("L15", docx, "HN1 → HN2, M4 item → item", 2.75,
             "R13 M4: S = 2.75", "T112", what="S")
    body_gap("L16", docx, "HN2 → HP1, M4 item → nested item", 2.75,
             "R13 M4", "T112", what="S")
    body_gap("L17", docx, "HP2 → HN3, M4 nested → next item", 2.75,
             "R13 M4", "T112", what="S")
    body_gap("L18", docx,
             "HN3 → HQ1, M4 last item (after P) → paragraph", P,
             "R13 M4", "T112", what="direct after P")


def sheet_quotes():
    docx = "w08"
    body_gap("Q1", docx, "HA1 → HB1, paragraph → quote", P,
             "R12 §8 Word 25.20; R13 2.25 (b)", "T112", method="Q",
             what="max(P, 5)")
    body_gap("Q2", docx, "HB1 → HB2, inside the quote", 5,
             "R13 2.26 (a) only", "T112", method="Q", what="5 + 5 → 5")
    body_gap("Q3", docx, "HB2 → HC1, into the nested quote", 5,
             "R13 2.27: same style", "T112", method="Q", what="5")
    body_gap("Q4", docx, "HC1 → HB3, out of the nested quote", 5,
             "–", "T112", method="Q", what="5")
    body_gap("Q5", docx, "HB3 → HD1, quote → paragraph", 5,
             "R12 §8 Word 19.80; R13 2.25 (b)", "T112", method="Q",
             what="max(5, 0)")
    gap("Q6", docx, "HD1 → HE1, paragraph, rule, paragraph",
        [(BODY.below, "below"), (P, "P"), (BODY.height, "rule's empty line"),
         (1, "border space"), (0.75, "border"), (6, "rule after"),
         (BODY.above, "above")],
        "R13 2.31 (a) only", "T112", method="Q")


def sheet_code():
    docx = "w09"
    into = [(BODY.below, "below"), (P, "P"), (CODE.above, "code above")]
    into_q = [(BODY.height, "body line"), (P, "P")]
    out = [(CODE.below, "code below"), (6, "code after"),
           (BODY.above, "above")]
    out_q = [(CODE.height, "code line"), (6, "code after")]
    pitch = [(CODE.height, "Consolas 10 pt at w:line 228")]
    for key, what, first, last in (
            ("K1", "today", "HA1 → HB1", "HB3 → HC1"),
            ("K4", "c1", "HC1 → HD1", "HD3 → HE1"),
            ("K7", "c2", "HE1 → HF1", "HF3 → HG1")):
        number = int(key[1:])
        today = key == "K1"
        sources = ["R12 §8 Word 23.00", "R12 §8 Word 11.20",
                   "R12 §8 Word 19.20"]
        if not today:
            sources = ["–", "R13 3.7 (a) only", "–"]
        gap(key, docx, f"{first}, paragraph → code ({what})", into,
            sources[0], "T113", tops=into_q)
        line = {"K1": "HB1 → HB2", "K4": "HD1 → HD2", "K7": "HF1 → HF2"}
        gap(f"K{number + 1}", docx, f"{line[key]}, code line pitch ({what})",
            pitch, sources[1], "T113")
        gap(f"K{number + 2}", docx, f"{last}, code → paragraph ({what})",
            out, sources[2], "T113", tops=out_q)
    gap("K10", docx, "HH2 → HK1, two c1 blocks",
        [(CODE.below, "code below"), (6, "6 + 6 → 6"),
         (CODE.above, "code above")], "–", "T113")
    gap("K11", docx, "HM2 → HN1, two c2 blocks",
        [(CODE.height, "contextualSpacing drops both")], "–", "T113")
    gap("K12", docx, "HP1 → HQ1, paragraph → code (c3, w:line 277)",
        [(BODY.below, "below"), (P, "P"), (CODE277.above, "code above")],
        "R13 §3.1 M3: a code line height on the mono face", "T113",
        tops=into_q)
    gap("K13", docx, "HQ1 → HQ2, code line pitch (c3)",
        [(CODE277.height, "Consolas 10 pt at w:line 277")],
        "R13 §3.1 M3", "T113")
    gap("K14", docx, "HQ3 → HR1, code → paragraph (c3)",
        [(CODE277.below, "code below"), (6, "code after"),
         (BODY.above, "above")], "R13 §3.1 M3", "T113",
        tops=[(CODE277.height, "code line"), (6, "code after")])
    row("K15", docx, "Shading of HD, HF, HQ: one grey block per code block, "
        "no white seam between lines; HQ2's yellow bar spans the full width",
        "V", "yes", "per-line paragraphs with equal shading and no space",
        "R13 3.19", "T113")


def sheet_equations():
    docx = "w10"
    row("E1", docx, "HA1 → HE1, paragraph → unnumbered equation", "Q, C",
        f"Q {BODY.height + P:.2f}; C {BODY.below + P + BODY.above:.2f} (M)",
        f"Q: {BODY.height:.2f} body line + {P:.2f} max(P, 6); C under M, "
        "the math line taking the body's box", "R13 3.22 (a) only",
        "T113")
    row("E2", docx, "HE1 → HB1, unnumbered equation → paragraph", "Q, C",
        f"{BODY.height + 6:.2f} (M)",
        f"Q: the equation's box + 6 (max(6, 0)); under M "
        f"{BODY.height:.2f} + 6. Record the box: Q - 6", "–", "T113")
    row("E3", docx, "HB1 → HE2, paragraph → numbered equation (a table)",
        "Q, C", f"{BODY.height + P + 6:.2f}",
        f"{BODY.height:.2f} body line + {P:.2f} P + 6 the cell's before: "
        "no collapse at a table edge (C the same under M)", "–", "T113")
    row("E4", docx, "HE2 → HC1, numbered equation → paragraph", "Q, C",
        f"{BODY.height + 6:.2f} (M)",
        "the row's height (6 + box + 6, the taller of the equation and "
        "the (1) cell) less the cell's before, paragraph before 0",
        "–", "T113")
    row("E5", docx, "HC1 → HE3, paragraph → fraction equation", "Q",
        f"{BODY.height + P:.2f}",
        f"{BODY.height:.2f} + {P:.2f}, whatever the equation's box",
        "–", "T113")
    row("E6", docx, "HE3 → HD1, fraction equation → paragraph", "Q",
        "box + 6", "record the fraction's box (Q - 6); no model value",
        "–", "T113")


def sheet_tables():
    docx = "w11"
    start = [(BODY.below, "below"), (P, "P")]
    cell_in = [(COMPACT, "cell before"), (BODY.above, "above")]
    cell_out = [(BODY.below, "below"), (COMPACT, "cell after")]
    gap("T1", docx, "HA1 → HB1, paragraph → header row",
        start + [(1, "top rule")] + cell_in,
        "R12 §8 Word 27.67 (other cells)", "T114")
    gap("T2", docx, "HB1 → HC1, header row → row",
        cell_out + [(0.5, "header rule")] + cell_in, "–", "T114")
    gap("T3", docx, "HC1 → HD1, row → row", cell_out + cell_in,
        "R13 4.6 (a) only", "T114")
    gap("T4", docx, "HD1 → HE1, last row → paragraph",
        cell_out + [(1, "bottom rule"), (BODY.above, "above")],
        "R12 §8 Word 18.67 (other cells)", "T114")
    gap("T5", docx, "HG1 → HH1, two tables, today's separator",
        cell_out + [(1, "rule"), (BODY.height, "`<w:p/>` line"),
                    (P, "its after (docDefaults)"), (1, "rule")] + cell_in,
        "R13 M10: about 14.8 pt line", "T114")
    gap("T6", docx, "HN1 → HP1, two tables, M10's 0.7 pt separator",
        cell_out + [(1, "rule"), (9.5, "before"), (0.7, "exact line"),
                    (1, "rule")] + cell_in, "R13 M10", "T114")
    gap("T7", docx, "HR1 → HS1, paragraph → 3 pt top rule",
        start + [(3, "top rule")] + cell_in,
        "R13 §3.3 rule 3: T1 + 2", "T114")
    gap("T8", docx, "HS1 → HT1, 1.5 pt header rule",
        cell_out + [(1.5, "header rule")] + cell_in,
        "R13 §3.3 rule 3: T2 + 1", "T114")
    gap("T9", docx, "HU1 → HV1, 3 pt bottom rule → paragraph",
        cell_out + [(3, "bottom rule"), (BODY.above, "above")],
        "R13 §3.3 rule 3: T4 + 2", "T114")
    gap("T10", docx, "HX1 → HY1, last row → paragraph with before 10.2",
        cell_out + [(1, "bottom rule"), (P, "direct before"),
                    (BODY.above, "above")], "R13 M10", "T114")


def sheet_notes():
    docx = "w12"
    bottom = 841.89 - 85.04
    gap("N1", docx, "HN2 → HN3, line pitch in a note",
        [(NOTE.height, "9 pt at w:line 228")],
        "R12 §8: 12.15 (computed); R13 4.28 (a) only", "T115", method="C")
    gap("N2", docx, "HN3 → HP2, note → note (one note line between)",
        [(NOTE.below, "below"), (3, "note after"), (NOTE.height, "HP1 line"),
         (NOTE.above, "above")], "R13 4.29: 3 pt", "T115", method="C")
    row("N3", docx, "HP3's baseline from the page's top edge", "C",
        f"{bottom - NOTE.below:.2f} or {bottom - 3 - NOTE.below:.2f}",
        f"the text area ends at {bottom:.2f}; the last note's box ends "
        "there, or 3 pt higher if Word keeps its space after", "–", "T115")
    row("N4", docx, "Note 1's top → note 2's top (footnote snippet)", "F",
        f"{3 * NOTE.height + 3:.2f}", f"3 × {NOTE.height:.2f} + 3",
        "R13 4.29", "T115")
    row("N5", docx, "Separator line (wordcap's lower edge) → HN2 baseline",
        "C", "record", "Word's built-in separator in a Normal paragraph "
        f"({BODY.height:.2f} line, {P:.2f} after from docDefaults); "
        "where Word draws the line in it is not in the model",
        "R13 4.26 (u)", "T115")


def sheet_captions():
    docx = "w13"
    row("C1", docx, "HA1 → figure line, line tops", "Q",
        f"{BODY.height + P:.2f}", f"{BODY.height:.2f} + {P:.2f} max(P, 6)",
        "R13 1.37 (a) only", "T110")
    row("C2", docx, "figure line → caption, line tops", "Q", "box + 3",
        "the 72 pt image's line box + 3 (3 after, 3 before): record the "
        "box (Q - 3); no model value", "R13 1.35 (a): 3 pt", "T110")
    row("C3", docx, "caption → HB1, line tops", "Q",
        f"{CAPTION.height + 6:.2f}",
        f"{CAPTION.height:.2f} caption line (10 pt) + 6 max(6, 0)",
        "R13 1.35 (a): 6 pt", "T110")
    row("C4", docx, "HB1 → table caption, line tops", "Q",
        f"{BODY.height + P:.2f}", f"{BODY.height:.2f} + {P:.2f} max(P, 3)",
        "–", "T110")
    row("C5", docx, "table caption → HS1, line tops", "Q",
        f"{CAPTION.height + 6 + 1 + COMPACT:.2f}",
        f"{CAPTION.height:.2f} + 6 caption after + 1 top rule + 1.8 cell "
        "before", "–", "T110")
    gap("C6", docx, "HT1 → HC1, table → paragraph (as T4)",
        [(BODY.below, "below"), (COMPACT, "cell after"),
         (1, "bottom rule"), (BODY.above, "above")], "–", "T110")


def sheet_defaults():
    docx = "w14"
    blocks = "w15"
    b, h1, code, note, cap = BODY_A, H1_A, CODE_A, NOTE_A, CAPTION_A

    def pair(key, read, space, source, task, method="C, Q", what="space"):
        gap(key, docx, read, [(b.below, "below"), (space, what),
                              (b.above, "above")], source, task, method)

    gap("D1", docx, "HA1 → HA2, body line pitch",
        [(b.height, "Calibri 11 pt at w:line 264")], "R13 0.4: 14.77 (u)",
        "T106")
    pair("D2", "HA2 → HB1, paragraph → paragraph", 6,
         "R13 0.5: 6 pt (u)", "T106", what="6 + 6 → 6")
    gap("D3", docx, "HB1 → HC1, body → Heading 1",
        [(b.below, "below"), (18, "H1 before"),
         (h1.above, "H1 above (18 pt)")], "R13 1.5: about 39.4 (u)", "T106",
        tops=[(b.height, "body line"), (18, "H1 before")])
    gap("D4", docx, "HC1 → HD1, Heading 1 → body",
        [(h1.below, "H1 below"), (6, "6 + 6 → 6"), (b.above, "above")],
        "R13 1.6: 6 pt", "T106",
        tops=[(h1.height, "H1 line"), (6, "6 + 6 → 6")])
    pair("D5", "HD1 → HE1, paragraph → tight list", 6,
         "R13 2.2: 20.77 (u)", "T112", what="max(6, 1.8)")
    gap("D6", docx, "HE1 → HE2, line pitch in an item",
        [(b.height, "w:line 264")], "R13 2.0: 14.77 (u)", "T112")
    pair("D7", "HE2 → HE3, tight item → item", COMPACT,
         "R13 2.1: 1.8 pt", "T112", what="Compact")
    pair("D8", "HE3 → HF1, tight list → paragraph", 6,
         "R13 2.3 (a): 20.77 (u)", "T112", what="max(1.8, 6)")
    pair("D9", "HF1 → HG1, paragraph → loose item", 6, "–", "T112",
         what="6")
    pair("D10", "HG1 → HG2, loose item → item", 6,
         "R13 2.4 (a): 20.77 (u)", "T112", what="6")
    pair("D11", "HG2 → HG3, paragraphs in one item", 6,
         "R13 2.5: 20.77 (u)", "T112", what="6")
    pair("D12", "HK1 → HL1, paragraph → term", 6, "–", "T112",
         what="max(6, 6)")
    gap("D13", docx, "HL1 → HM1, term → definition",
        [(b.height, "line; 0 after, 0 before")], "–", "T112")
    pair("D14", "HM1 → HN1, definition → paragraph", 6, "–", "T112",
         what="max(6, 6)")
    pair("D15", "HN1 → HP1, paragraph → quote", 6,
         "R13 2.25 (a): 20.77 (u)", "T112", method="Q", what="max(6, 5)")
    pair("D16", "HP1 → HP2, inside the quote", 5,
         "R13 2.26: 19.77 (u)", "T112", method="Q", what="5")
    pair("D17", "HP2 → HQ1, quote → paragraph", 6,
         "R13 2.25 (a): 20.77 (u)", "T112", method="Q", what="max(5, 6)")
    gap("D18", docx, "HQ1 → HR1, paragraph, rule, paragraph",
        [(b.below, "below"), (6, "max(6, 6)"), (b.height, "rule's line"),
         (1, "border space"), (0.75, "border"), (6, "max(6, 6)"),
         (b.above, "above")], "R13 2.31: about 43.3 (u)", "T112",
        method="Q")
    gap("D19", docx, "HR1 → HS1, paragraph → code",
        [(b.below, "below"), (6, "max(6, 6)"), (code.above, "code above")],
        "R13 3.12: 6 pt", "T113",
        tops=[(b.height, "body line"), (6, "space")])
    gap("D20", docx, "HS1 → HS2, code line pitch",
        [(code.height, "Consolas 10 pt at w:line 264")],
        "R13 3.7: about 12.9 (u)", "T113")
    gap("D21", docx, "HS2 → HT1, code → paragraph",
        [(code.below, "code below"), (6, "max(6, 6)"),
         (b.above, "above")], "R13 3.12: 6 pt", "T113",
        tops=[(code.height, "code line"), (6, "space")])
    row("D22", docx, "HT1 → HX1, paragraph → equation", "Q",
        f"{b.height + 6:.2f}", f"{b.height:.2f} + 6 max(6, 6)",
        "R13 3.22: 6 pt", "T113")
    row("D23", docx, "HX1 → HU1, equation → paragraph", "Q",
        f"{b.height + 6:.2f} (M)", "the equation's box + 6; under M the "
        "body's box", "R13 3.22: 6 pt", "T113")
    gap("D24", docx, "HN2 → HN3, line pitch in a note",
        [(note.height, "9 pt at w:line 264")], "R13 4.28: about 12.1 (u)",
        "T115", method="C")
    gap("D25", docx, "HN3 → HO2, note → note (one note line between)",
        [(note.below, "below"), (3, "note after"), (note.height, "HO1 line"),
         (note.above, "above")], "R13 4.29: 3 pt", "T115", method="C")
    gap("D26", blocks, "HU1 → HV1, paragraph → header row",
        [(b.below, "below"), (6, "after"), (1, "top rule"),
         (COMPACT, "cell before"), (b.above, "above")], "–", "T114")
    gap("D27", blocks, "HV1 → HW1, header row → row",
        [(b.below, "below"), (COMPACT, "after"), (0.5, "header rule"),
         (COMPACT, "before"), (b.above, "above")], "–", "T114")
    gap("D28", blocks, "HW1 → HY1, row → row",
        [(b.below, "below"), (COMPACT, "after"), (COMPACT, "before"),
         (b.above, "above")], "R13 4.6: about 18.4 (u)", "T114")
    gap("D29", blocks, "HY1 → HZ1, last row → paragraph",
        [(b.below, "below"), (COMPACT, "cell after"), (1, "bottom rule"),
         (6, "paragraph before"), (b.above, "above")], "–", "T114")
    row("D30", blocks, "HZ1 → figure line, line tops", "Q",
        f"{b.height + 6:.2f}", f"{b.height:.2f} + 6 max(6, 6)",
        "R13 1.37: 6 pt", "T110")
    row("D31", blocks, "figure line → caption, line tops", "Q", "box + 3",
        "record the image line's box (Q - 3)", "R13 1.35: 3 pt", "T110")
    row("D32", blocks, "caption → HZ2, line tops", "Q",
        f"{cap.height + 6:.2f}", f"{cap.height:.2f} caption line + 6",
        "R13 1.35: 6 pt", "T110")


FAMILIES = [(1, "w03", "Calibri", None),
            (4, "w04", "Calibri Light", None),
            (7, "w05", "Consolas", None),
            (10, "w06", "Cambria Math", "Cambria")]


def build():
    sheet_atleast()
    sheet_widows()
    for first, docx, family, alias in FAMILIES:
        sheet_family(first, docx, family, alias)
    sheet_lists()
    sheet_quotes()
    sheet_code()
    sheet_equations()
    sheet_tables()
    sheet_notes()
    sheet_captions()
    sheet_defaults()


def markdown():
    """One table per input, in the order of the rows."""
    files = {path.name[:3]: path.name for path in HERE.glob("w*.docx")}
    head = ("| # | Read | Method | Predicted (pt) | From the model "
            "| R13, R12 | Word | Task |\n"
            "|---|---|---|---|---|---|---|---|\n")
    lines = []
    current = None
    for key, docx, read, method, predicted, model, source, task in ROWS:
        if docx != current:
            if current is not None:
                lines.append("\n")
            lines.append(f"### {files.get(docx, docx)}\n\n")
            lines.append(head)
            current = docx
        lines.append(f"| {key} | {read} | {method} | {predicted} "
                     f"| {model} | {source} |  | {task} |\n")
    return "".join(lines)


def main():
    if "--fonts" in sys.argv:
        check_fonts()
        return
    build()
    table = markdown()
    if "--write" in sys.argv:
        readme = HERE / "README.md"
        text = readme.read_text()
        begin, end = "<!-- sheet:begin -->\n", "<!-- sheet:end -->"
        before, rest = text.split(begin, 1)
        _, after = rest.split(end, 1)
        readme.write_text(before + begin + table + end + after)
    else:
        print(table, end="")


if __name__ == "__main__":
    main()
