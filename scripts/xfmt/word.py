"""Word's baselines for a DOCX, computed from its XML by the model of
design.md §0.3 and R12 §2 (Word's figures marked (u) in R13); Word itself is
not run.

- A line is a box. An exact pitch h (`w:lineRule="exact"`) puts 0.8 h above
  the baseline and 0.2 h below. A multiple L = w:line / 240 of the single
  line (`auto`) makes the box L times the line's natural height, the largest
  A * size above the baseline plus the largest (s - A) * size below it over
  the line's runs, and puts min(1, L) of the ascent above the baseline.
  `atLeast` is modelled only where the pitch does not exceed the natural
  height (T103 measures the rest).
- Between two paragraphs the larger of the upper one's space after and the
  lower one's space before holds; `w:contextualSpacing` drops a space
  between two paragraphs of one style.
- The first line of the document sits its space before and its ascent below
  the top margin. A forced page break (`w:br w:type="page"`,
  `w:pageBreakBefore`) or a line whose box would cross the bottom margin
  starts a new page, where the first block loses its space before (R12
  §2.4). Widow control and keep rules are not modelled.
- A line's runs are its text runs, the run of a line break that ends it,
  and, on the last line, the paragraph mark.
- A table, a drawing, an equation or a frame has no modelled height: the
  lines after it get positions relative to each other again only, until the
  next page starts.

Fonts: s and A of each family come from `metrics`, a dict family (lower
case) -> (s, A)."""
import zipfile
import xml.etree.ElementTree as ET

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
M = "{http://schemas.openxmlformats.org/officeDocument/2006/math}"
TWIP = 1 / 20
OPAQUE = (
    W + "drawing", W + "pict", W + "object", M + "oMath", M + "oMathPara"
)


def attr(element, name):
    return None if element is None else element.get(W + name)


def flag(element):
    """A toggle property: present and not switched off."""
    if element is None:
        return None
    return attr(element, "val") not in ("0", "false", "off")


class Props:
    """Resolved paragraph and run properties, in twips and half points."""

    def __init__(self):
        self.before = 0
        self.after = 0
        self.line = None
        self.rule = "auto"
        self.contextual = False
        self.break_before = False
        self.frame = False
        self.size = 20
        self.family = None

    def copy(self):
        other = Props()
        other.__dict__.update(self.__dict__)
        return other

    def apply_ppr(self, ppr):
        if ppr is None:
            return
        spacing = ppr.find(W + "spacing")
        if spacing is not None:
            for name in ("before", "after", "line"):
                value = attr(spacing, name)
                if value is not None:
                    setattr(self, name, int(value))
            self.rule = attr(spacing, "lineRule") or self.rule
        toggles = (
            ("contextual", "contextualSpacing"),
            ("break_before", "pageBreakBefore"),
        )
        for name, tag in toggles:
            value = flag(ppr.find(W + tag))
            if value is not None:
                setattr(self, name, value)
        if ppr.find(W + "framePr") is not None:
            self.frame = True

    def apply_rpr(self, rpr):
        if rpr is None:
            return
        size = attr(rpr.find(W + "sz"), "val")
        if size is not None:
            self.size = int(size)
        fonts = rpr.find(W + "rFonts")
        family = attr(fonts, "ascii") or attr(fonts, "hAnsi")
        if family:
            self.family = family


class Styles:
    def __init__(self, xml):
        root = ET.fromstring(xml) if xml else None
        self.base = Props()
        self.styles = {}
        self.default = None
        if root is None:
            return
        defaults = root.find(W + "docDefaults")
        if defaults is not None:
            self.base.apply_rpr(defaults.find(f"{W}rPrDefault/{W}rPr"))
            self.base.apply_ppr(defaults.find(f"{W}pPrDefault/{W}pPr"))
        for style in root.findall(W + "style"):
            sid = attr(style, "styleId")
            self.styles[sid] = style
            if (
                attr(style, "type") == "paragraph"
                and attr(style, "default") in ("1", "true")
            ):
                self.default = sid

    def chain(self, sid):
        seen = []
        while sid in self.styles and sid not in seen:
            seen.append(sid)
            sid = attr(self.styles[sid].find(W + "basedOn"), "val")
        return reversed(seen)

    def paragraph(self, sid):
        props = self.base.copy()
        for name in self.chain(sid or self.default):
            style = self.styles[name]
            props.apply_ppr(style.find(W + "pPr"))
            props.apply_rpr(style.find(W + "rPr"))
        return props

    def character(self, props, sid):
        for name in self.chain(sid):
            props.apply_rpr(self.styles[name].find(W + "rPr"))


def section(root):
    """Page height and the top and bottom margins, in pt."""
    sect = root.find(f"{W}body/{W}sectPr")
    size = sect.find(W + "pgSz") if sect is not None else None
    margin = sect.find(W + "pgMar") if sect is not None else None
    height = int(attr(size, "h") or 16838) * TWIP
    width = int(attr(size, "w") or 11906) * TWIP
    top = abs(int(attr(margin, "top") or 1440)) * TWIP
    bottom = abs(int(attr(margin, "bottom") or 1440)) * TWIP
    left = int(attr(margin, "left") or 1440) * TWIP
    right = int(attr(margin, "right") or 1440) * TWIP
    return {
        "height": height, "width": width, "top": top, "bottom": bottom,
        "text_width": width - left - right
    }


def geometry(path):
    """The page geometry of the DOCX at `path`, as `section` gives it."""
    with zipfile.ZipFile(path) as archive:
        return section(ET.fromstring(archive.read("word/document.xml")))


def blocks(body):
    """The body's paragraphs in order, with None for a table."""
    for child in body:
        if child.tag == W + "p":
            yield child
        elif child.tag == W + "tbl":
            yield None
        elif child.tag in (W + "sdt", W + "customXml"):
            content = child.find(W + "sdtContent")
            yield from blocks(content if content is not None else child)


def paragraph_lines(paragraph, styles):
    """The paragraph's properties and its lines: each a dict with `text`,
    `fonts` (family, size in pt) and `page_break` (a forced break follows
    the line)."""
    ppr = paragraph.find(W + "pPr")
    sid = attr(ppr.find(W + "pStyle"), "val") if ppr is not None else None
    props = styles.paragraph(sid)
    props.apply_ppr(ppr)
    mark = props.copy()
    if ppr is not None:
        mark.apply_rpr(ppr.find(W + "rPr"))
    opaque = any(
        next(paragraph.iter(tag), None) is not None for tag in OPAQUE
    )
    lines = [{"text": "", "fonts": [], "page_break": False}]
    for run in paragraph.iter(W + "r"):
        rprops = props.copy()
        rpr = run.find(W + "rPr")
        if rpr is not None:
            styles.character(rprops, attr(rpr.find(W + "rStyle"), "val"))
            rprops.apply_rpr(rpr)
        font = (rprops.family, rprops.size / 2)
        for item in run:
            line = lines[-1]
            if item.tag == W + "t":
                line["text"] += item.text or ""
                line["fonts"].append(font)
            elif item.tag in (W + "tab", W + "noBreakHyphen", W + "sym"):
                line["text"] += " " if item.tag == W + "tab" else "-"
                line["fonts"].append(font)
            elif item.tag in (W + "br", W + "cr"):
                line["fonts"].append(font)
                kind = attr(item, "type")
                if kind == "page":
                    line["page_break"] = True
                if kind != "column":
                    lines.append({"text": "", "fonts": [], "page_break": False})
    lines[-1]["fonts"].append((mark.family, mark.size / 2))
    # A break that ends the paragraph leaves no line of its own after it.
    if len(lines) > 1 and not lines[-1]["text"].strip():
        tail = lines.pop()
        lines[-1]["fonts"].extend(tail["fonts"])
    return props, lines, opaque


def box(props, fonts, metrics):
    """(above, below) the baseline in pt, or None where the model has none
    (a font whose metrics are unknown, or `atLeast` above the natural
    height)."""
    if props.line is not None and props.rule == "exact":
        pitch = props.line * TWIP
        return 0.8 * pitch, 0.2 * pitch
    ascent = descent = 0.0
    for family, size in fonts:
        known = metrics.get((family or "").lower())
        if known is None:
            return None
        single, above = known
        ascent = max(ascent, above * size)
        descent = max(descent, (single - above) * size)
    natural = ascent + descent
    if props.line is None or props.rule == "auto":
        multiple = (props.line or 240) / 240
        top = min(1.0, multiple) * ascent
        return top, multiple * natural - top
    pitch = props.line * TWIP
    if pitch <= natural:
        return ascent, descent
    return None


def model(path, metrics, wrap=None):
    """Word's lines for the DOCX at `path`: (page geometry, lines), each line
    a dict with `text`, `style`, `page`, `segment`, `y` (pt, from the page
    top when `anchored`, else relative within the segment), `above` (pt of
    the box above the baseline) and `first` (first line on its page).

    The model does not break text into lines. `wrap(text)`, when given,
    returns the texts of the lines a line of `text` takes (from another
    format's layout), else None for one line; each gets the same box."""
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        document = ET.fromstring(archive.read("word/document.xml"))
        xml = None
        if "word/styles.xml" in names:
            xml = archive.read("word/styles.xml")
        styles = Styles(xml)
    page = section(document)
    bottom = page["height"] - page["bottom"]
    result = []
    state = {
        "page": 1, "segment": 0, "y": page["top"], "anchored": True,
        "fresh": True, "start": True
    }

    def new_page():
        state.update(
            page=state["page"] + 1, segment=state["segment"] + 1,
            y=page["top"], anchored=True, fresh=True
        )

    def lose():
        state.update(
            segment=state["segment"] + 1, y=0.0, anchored=False, fresh=False
        )

    def place(text, edges, sid, below, space):
        """Places one line; returns the part of its box below the
        baseline."""
        above, down = edges
        y = state["y"] + below + space + above
        if (
            state["anchored"] and not state["fresh"]
            and y + down > bottom + 0.01
        ):
            new_page()
            y = state["y"] + above
        result.append({
            "text": text, "style": sid, "page": state["page"],
            "segment": state["segment"], "y": y, "above": above,
            "anchored": state["anchored"], "first": state["fresh"],
        })
        state.update(y=y, fresh=False, start=False)
        return down

    last = None
    for paragraph in blocks(document.find(W + "body")):
        if paragraph is None:
            lose()
            last = None
            continue
        props, lines, opaque = paragraph_lines(paragraph, styles)
        if props.frame:
            continue
        if opaque:
            lose()
            last = None
            continue
        ppr = paragraph.find(W + "pPr")
        sid = attr(ppr.find(W + "pStyle"), "val") if ppr is not None else None
        sid = sid or styles.default
        if props.break_before and not state["fresh"]:
            new_page()
        space = props.before * TWIP
        if last is not None:
            after = last[0].after * TWIP
            if props.contextual and last[1] == sid:
                space = 0.0
            if last[0].contextual and last[1] == sid:
                after = 0.0
            space = max(after, space)
        elif not state["start"]:
            space = 0.0
        below = 0.0 if last is None else last[2]
        if state["fresh"] and not state["start"]:
            space = below = 0.0
        for line in lines:
            edges = box(props, line["fonts"], metrics)
            if edges is None:
                lose()
                below = space = 0.0
                continue
            text = " ".join(line["text"].split())
            for part in (wrap(text) if wrap else None) or [text]:
                below = place(part, edges, sid, below, space)
                space = 0.0
            if line["page_break"]:
                new_page()
                below = 0.0
        last = (props, sid, below) if not state["fresh"] else None
    return page, result


def describe(path, metrics):
    """The model's lines as text, for checking it by hand."""
    page, lines = model(path, metrics)
    out = [
        f"page {page['width']:.2f} x {page['height']:.2f} pt, "
        f"margins {page['top']:.2f} / {page['bottom']:.2f}"
    ]
    for line in lines:
        where = "abs" if line["anchored"] else f"seg{line['segment']}"
        out.append(
            f"p{line['page']} {where:>5} {line['y']:8.2f} "
            f"above {line['above']:5.2f}  {line['style'] or '':14} "
            f"{line['text'][:40]}"
        )
    return "\n".join(out)


def families(path):
    """Every font family the DOCX names, in lower case."""
    found = set()
    with zipfile.ZipFile(path) as archive:
        for name in ("word/styles.xml", "word/document.xml"):
            if name not in archive.namelist():
                continue
            for fonts in ET.fromstring(archive.read(name)).iter(W + "rFonts"):
                for key in ("ascii", "hAnsi"):
                    if attr(fonts, key):
                        found.add(attr(fonts, key).lower())
    return found
