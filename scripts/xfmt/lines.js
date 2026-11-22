// Evaluated in the page by cdp-print.mjs: the text lines of the document
// body in Chrome's layout, in CSS px from the top of the document.
//
// A line is a run of text fragments that sit side by side. Its baseline
// is the first fragment's that is aligned to the baseline (not a
// superscript or a subscript): the fragment's top plus the ascent Chrome
// gives its font, read from a probe element with the same font. Each
// line also reports the offset of its block's first baseline below the
// block's content top: Chrome's place for the baseline in the first line
// box, from which run.py derives the residual c (R13 §3.2).
(root => {
    const scope = document.querySelector(root) ?? document.body;
    const host = document.createElement("div");
    host.style.cssText =
        "position:absolute;left:0;top:0;visibility:hidden;white-space:nowrap";
    document.body.appendChild(host);
    const ascents = new Map();

    // The distance from a text fragment's top to its baseline, in px.
    const ascentOf = style => {
        const key = [
            style.fontStyle, style.fontWeight, style.fontStretch,
            style.fontSize, style.fontFamily
        ].join("|");
        if (!ascents.has(key)) {
            const span = document.createElement("span");
            for (const name of [
                "fontStyle", "fontWeight", "fontStretch", "fontSize",
                "fontFamily", "fontVariant", "fontFeatureSettings"
            ]) {
                span.style[name] = style[name];
            }
            span.style.lineHeight = "normal";
            const mark = document.createElement("span");
            mark.style.cssText =
                "display:inline-block;width:0;height:0;vertical-align:baseline";
            span.append(mark, document.createTextNode("H"));
            host.appendChild(span);
            const range = document.createRange();
            range.selectNodeContents(span.lastChild);
            const top = range.getClientRects()[0].top;
            ascents.set(key, mark.getBoundingClientRect().top - top);
            span.remove();
        }
        return ascents.get(key);
    };

    const blockOf = element => {
        let node = element;
        while (node && node !== scope) {
            const display = getComputedStyle(node).display;
            if (!display.startsWith("inline") && display !== "contents") {
                return node;
            }
            node = node.parentElement;
        }
        return scope;
    };

    const shifted = element => {
        for (let node = element; node && node !== scope;
             node = node.parentElement) {
            const style = getComputedStyle(node);
            if (style.display.startsWith("inline")
                && style.verticalAlign !== "baseline") {
                return true;
            }
            if (!style.display.startsWith("inline")) {
                return false;
            }
        }
        return false;
    };

    // Every fragment: one per line a text node occupies.
    const fragments = [];
    const walker = document.createTreeWalker(scope, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
        const node = walker.currentNode;
        const parent = node.parentElement;
        if (!parent || !node.textContent.trim()) {
            continue;
        }
        const style = getComputedStyle(parent);
        if (style.visibility !== "visible") {
            continue;
        }
        const range = document.createRange();
        range.selectNodeContents(node);
        const rects = Array.from(range.getClientRects())
            .filter(rect => rect.width > 0 && rect.height > 0);
        if (rects.length === 0) {
            continue;
        }
        // Split the text where its fragments change: one character at a
        // time, only for a node that spans several lines.
        let texts = [node.textContent];
        if (rects.length > 1) {
            texts = rects.map(() => "");
            let index = 0;
            for (let at = 0; at < node.length; at++) {
                range.setStart(node, at);
                range.setEnd(node, at + 1);
                const char = range.getClientRects()[0];
                if (char) {
                    while (index + 1 < rects.length
                           && Math.abs(char.top - rects[index].top) > 0.5
                           && Math.abs(char.top - rects[index + 1].top)
                              <= 0.5) {
                        index++;
                    }
                }
                texts[index] += node.textContent[at];
            }
        }
        const ascent = ascentOf(style);
        const block = blockOf(parent);
        const offBaseline = shifted(parent);
        rects.forEach((rect, index) => {
            fragments.push({
                top: rect.top + scrollY, bottom: rect.bottom + scrollY,
                left: rect.left, right: rect.right,
                baseline: rect.top + scrollY + ascent,
                text: texts[index], size: parseFloat(style.fontSize),
                block, offBaseline
            });
        });
    }
    host.remove();

    // Fragments to lines: a fragment continues the current line when it
    // lies to the right of it and overlaps it vertically by at least half.
    const lines = [];
    for (const fragment of fragments) {
        const line = lines[lines.length - 1];
        if (line) {
            const overlap = Math.min(line.bottom, fragment.bottom)
                - Math.max(line.top, fragment.top);
            const height = Math.min(
                line.bottom - line.top, fragment.bottom - fragment.top
            );
            if (fragment.left >= line.right - 1 && overlap >= height / 2) {
                line.parts.push(fragment);
                line.right = Math.max(line.right, fragment.right);
                continue;
            }
        }
        lines.push({
            top: fragment.top, bottom: fragment.bottom,
            right: fragment.right, parts: [fragment]
        });
    }

    // Each block's first baseline below its content top.
    const firsts = new Map();
    const rows = lines.map(line => {
        const main = line.parts.find(part => !part.offBaseline)
            ?? line.parts[0];
        let text = "";
        for (const part of line.parts) {
            const piece = part.text.replace(/\s+/g, " ");
            text += text && !/\s$/.test(text) && !/^\s/.test(piece)
                ? " " + piece : piece;
        }
        if (!firsts.has(main.block)) {
            const box = main.block.getBoundingClientRect();
            const style = getComputedStyle(main.block);
            const content = box.top + scrollY + parseFloat(style.borderTopWidth)
                + parseFloat(style.paddingTop);
            firsts.set(main.block, main.baseline - content);
        }
        const block = main.block;
        return {
            baseline: main.baseline, text: text.trim(), size: main.size,
            offset: firsts.get(block),
            block: block.tagName.toLowerCase(),
            style: block.closest("[data-style]")?.dataset.style ?? null,
            className: block.className || null
        };
    });
    return rows;
})
