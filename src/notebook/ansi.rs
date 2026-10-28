//! ANSI stream styles and deterministic notebook output (notebook-mapping.md §5).

use anstyle::{Ansi256Color, AnsiColor, Color, Effects, RgbColor, Style};
use anstyle_parse::{DefaultCharAccumulator, Params, Parser, Perform};

use crate::ir::{Attrs, Block};

/// A text segment with the terminal style in force when it was printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiSpan {
    /// Printable text, tabs and line breaks, without control sequences.
    pub text: String,
    /// Parsed effects and colours, before bold and inverse colour conversion.
    pub style: Style,
}

/// Notebook colours, in the order of the sixteen ANSI palette entries.
pub const PALETTE: [(u8, u8, u8); 16] = [
    (0x3e, 0x42, 0x4d),
    (0xe7, 0x5c, 0x58),
    (0x00, 0xa2, 0x50),
    (0xdd, 0xb6, 0x2b),
    (0x20, 0x8f, 0xfb),
    (0xd1, 0x60, 0xc4),
    (0x60, 0xc6, 0xc8),
    (0xc5, 0xc1, 0xb4),
    (0x28, 0x2c, 0x36),
    (0xb2, 0x2b, 0x31),
    (0x00, 0x74, 0x27),
    (0xb2, 0x7d, 0x12),
    (0x00, 0x65, 0xca),
    (0xa0, 0x31, 0x96),
    (0x25, 0x8f, 0x8f),
    (0xa1, 0xa6, 0xb2),
];

const ANSI_COLORS: [AnsiColor; 16] = [
    AnsiColor::Black,
    AnsiColor::Red,
    AnsiColor::Green,
    AnsiColor::Yellow,
    AnsiColor::Blue,
    AnsiColor::Magenta,
    AnsiColor::Cyan,
    AnsiColor::White,
    AnsiColor::BrightBlack,
    AnsiColor::BrightRed,
    AnsiColor::BrightGreen,
    AnsiColor::BrightYellow,
    AnsiColor::BrightBlue,
    AnsiColor::BrightMagenta,
    AnsiColor::BrightCyan,
    AnsiColor::BrightWhite,
];

#[derive(Default)]
struct StyledText {
    spans: Vec<AnsiSpan>,
    style: Style,
}

impl Perform for StyledText {
    fn print(&mut self, character: char) {
        if let Some(last) = self.spans.last_mut()
            && last.style == self.style
        {
            last.text.push(character);
        } else {
            self.spans.push(AnsiSpan {
                text: character.to_string(),
                style: self.style,
            });
        }
    }

    fn execute(&mut self, byte: u8) {
        if matches!(byte, b'\n' | b'\t') {
            self.print(char::from(byte));
        }
    }

    fn csi_dispatch(
        &mut self, params: &Params, intermediates: &[u8], ignore: bool,
        action: u8,
    ) {
        if action != b'm' || !intermediates.is_empty() || ignore {
            return;
        }
        let groups: Vec<_> = params.iter().collect();
        let mut index = 0;
        while let Some(group) = groups.get(index) {
            let code = group[0];
            if matches!(code, 38 | 48) {
                let (color, consumed) = extended_color(&groups[index..]);
                if let Some(color) = color {
                    self.style = if code == 38 {
                        self.style.fg_color(Some(color))
                    } else {
                        self.style.bg_color(Some(color))
                    };
                }
                index += consumed;
            } else {
                self.sgr(code, group);
                index += 1;
            }
        }
    }
}

impl StyledText {
    fn sgr(&mut self, code: u16, group: &[u16]) {
        let effect = match code {
            1 | 5 | 6 => Some((Effects::BOLD, true)),
            2 => Some((Effects::DIMMED, true)),
            3 => Some((Effects::ITALIC, true)),
            4 => Some((Effects::UNDERLINE, group.get(1) != Some(&0))),
            7 => Some((Effects::INVERT, true)),
            8 => Some((Effects::HIDDEN, true)),
            9 => Some((Effects::STRIKETHROUGH, true)),
            21 | 22 => Some((Effects::BOLD | Effects::DIMMED, false)),
            23 => Some((Effects::ITALIC, false)),
            24 => Some((Effects::UNDERLINE, false)),
            27 => Some((Effects::INVERT, false)),
            28 => Some((Effects::HIDDEN, false)),
            29 => Some((Effects::STRIKETHROUGH, false)),
            _ => None,
        };
        if let Some((effect, enabled)) = effect {
            let effects = self.style.get_effects();
            self.style = self.style.effects(if enabled {
                effects.insert(effect)
            } else {
                effects.remove(effect)
            });
        } else {
            self.style = match code {
                0 => Style::new(),
                30..=37 => self.style.fg_color(ansi(code - 30)),
                90..=97 => self.style.fg_color(ansi(code - 90 + 8)),
                40..=47 => self.style.bg_color(ansi(code - 40)),
                100..=107 => self.style.bg_color(ansi(code - 100 + 8)),
                39 => self.style.fg_color(None),
                49 => self.style.bg_color(None),
                _ => self.style,
            };
        }
    }
}

fn ansi(index: u16) -> Option<Color> {
    ANSI_COLORS.get(usize::from(index)).copied().map(Color::Ansi)
}

fn extended_color(groups: &[&[u16]]) -> (Option<Color>, usize) {
    let colon = groups[0].len() > 1;
    let values: Vec<_> = if colon {
        groups[0][1..].to_vec()
    } else {
        groups.iter().skip(1).map(|group| group[0]).collect()
    };
    let consumed = if colon {
        1
    } else {
        match values.first() {
            Some(5) => 3,
            Some(2) => 5,
            _ => 2,
        }
        .min(groups.len())
    };
    if !colon && groups[1..consumed].iter().any(|group| group.len() != 1) {
        return (None, consumed);
    }
    let color = match values.as_slice() {
        [5, index, ..] if *index <= 255 && (!colon || values.len() == 2) => {
            Some(Color::Ansi256(Ansi256Color(*index as u8)))
        }
        [2, red, green, blue, ..] if !colon || values.len() == 4 => {
            rgb_color(*red, *green, *blue)
        }
        [2, _, red, green, blue] if colon => rgb_color(*red, *green, *blue),
        _ => None,
    };
    (color, consumed)
}

fn rgb_color(red: u16, green: u16, blue: u16) -> Option<Color> {
    let (red, green, blue) = (
        u8::try_from(red).ok()?,
        u8::try_from(green).ok()?,
        u8::try_from(blue).ok()?,
    );
    Some(Color::Rgb(RgbColor(red, green, blue)))
}

/// Parses terminal output, discarding all controls except SGR, tabs and newlines.
pub fn parse(text: &str) -> Vec<AnsiSpan> {
    let mut parser = Parser::<DefaultCharAccumulator>::new();
    let mut output = StyledText::default();
    for byte in text.bytes() {
        parser.advance(&mut output, byte);
    }
    output.spans
}

/// Removes terminal controls while preserving printable text, tabs and newlines.
pub fn strip(text: &str) -> String {
    parse(text).into_iter().map(|span| span.text).collect()
}

/// Serializes parsed spans using a complete style and reset for each styled span.
pub fn canonical(spans: &[AnsiSpan]) -> String {
    spans
        .iter()
        .map(|span| {
            if span.style.is_plain() {
                span.text.clone()
            } else {
                format!(
                    "{}{}{}",
                    span.style.render(),
                    span.text,
                    span.style.render_reset()
                )
            }
        })
        .collect()
}

/// Builds plain IR text and an optional private canonical style representation.
pub fn code_block(text: &str, class: &str) -> Block {
    let spans = parse(text);
    let mut attrs = Attrs { classes: vec![class.into()], ..Attrs::default() };
    if spans.iter().any(|span| !span.style.is_plain()) {
        attrs.pairs.insert("ansi".into(), canonical(&spans));
    }
    Block::CodeBlock {
        lang: None,
        text: spans.into_iter().map(|span| span.text).collect(),
        attrs,
        span: None,
    }
}

fn palette_index(color: Color) -> Option<u8> {
    match color {
        Color::Ansi(color) => Some(Ansi256Color::from(color).index()),
        Color::Ansi256(Ansi256Color(index)) if index < 16 => Some(index),
        _ => None,
    }
}

/// Returns rendered foreground and background, applying bold before inversion.
pub fn colors(style: &Style) -> (Option<Color>, Option<Color>) {
    let mut foreground = style.get_fg_color();
    let mut background = style.get_bg_color();
    if style.get_effects().contains(Effects::BOLD)
        && let Some(index) = foreground.and_then(palette_index)
        && index < 8
    {
        foreground = Some(Color::Ansi(ANSI_COLORS[usize::from(index + 8)]));
    }
    if style.get_effects().contains(Effects::INVERT) {
        std::mem::swap(&mut foreground, &mut background);
    }
    (foreground, background)
}

/// Converts an ANSI palette, indexed or true colour to its RGB components.
pub fn rgb(color: Color) -> (u8, u8, u8) {
    if let Some(index) = palette_index(color) {
        return PALETTE[usize::from(index)];
    }
    match color {
        Color::Rgb(RgbColor(red, green, blue)) => (red, green, blue),
        Color::Ansi256(Ansi256Color(index)) if index < 232 => {
            let offset = index - 16;
            let component =
                |value| if value == 0 { 0 } else { 55 + 40 * value };
            (
                component(offset / 36),
                component(offset / 6 % 6),
                component(offset % 6),
            )
        }
        Color::Ansi256(Ansi256Color(index)) => {
            let grey = (index - 232) * 10 + 8;
            (grey, grey, grey)
        }
        // All sixteen named ANSI colours were handled by palette_index.
        Color::Ansi(_) => {
            unreachable!("named ANSI colours have palette entries")
        }
    }
}

/// Renders escaped notebook output lines, dropping one trailing newline.
pub fn to_html(spans: &[AnsiSpan]) -> Vec<String> {
    let mut lines = vec![String::new()];
    let mut trailing_newline = false;
    let mut any_text = false;
    for span in spans {
        let normalized = span.text.replace("\r\n", "\n");
        for (index, text) in normalized.split('\n').enumerate() {
            if index > 0 {
                lines.push(String::new());
            }
            if !text.is_empty() {
                any_text = true;
                // There is always at least the initial line.
                lines
                    .last_mut()
                    .expect("output line exists")
                    .push_str(&html_segment(text, span.style));
            }
        }
        trailing_newline = normalized.ends_with('\n');
    }
    if trailing_newline {
        lines.pop();
    }
    if !any_text && lines.len() == 1 {
        lines.clear();
    }
    lines
}

fn html_segment(text: &str, style: Style) -> String {
    let text = crate::html::emit::escape(text);
    if style.is_plain() {
        return text;
    }
    let mut classes = Vec::new();
    let mut properties = Vec::new();
    let (foreground, background) = colors(&style);
    let inverse = style.get_effects().contains(Effects::INVERT);
    for (color, side, property) in
        [(foreground, "fg", "color"), (background, "bg", "background-color")]
    {
        if let Some(color) = color {
            if let Some(index) = palette_index(color) {
                let name = [
                    "black", "red", "green", "yellow", "blue", "magenta",
                    "cyan", "white",
                ][usize::from(index % 8)];
                let intense = if index >= 8 { "-intense" } else { "" };
                classes.push(format!("ansi-{name}{intense}-{side}"));
            } else {
                let (red, green, blue) = rgb(color);
                properties
                    .push(format!("{property}: rgb({red},{green},{blue})"));
            }
        } else if inverse {
            classes.push(format!("ansi-default-inverse-{side}"));
        }
    }
    for (effect, name) in [
        (Effects::BOLD, "bold"),
        (Effects::DIMMED, "dim"),
        (Effects::ITALIC, "italic"),
        (Effects::UNDERLINE, "underline"),
        (Effects::STRIKETHROUGH, "strike"),
        (Effects::HIDDEN, "hidden"),
    ] {
        if style.get_effects().contains(effect) {
            classes.push(format!("ansi-{name}"));
        }
    }
    let classes = attribute("class", &classes.join(" "));
    let properties = attribute("style", &properties.join("; "));
    format!("<span{classes}{properties}>{text}</span>")
}

fn attribute(name: &str, value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        format!(" {name}=\"{value}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_table() {
        for (on, off, effect) in [
            (1, 22, Effects::BOLD),
            (5, 21, Effects::BOLD),
            (6, 22, Effects::BOLD),
            (2, 22, Effects::DIMMED),
            (3, 23, Effects::ITALIC),
            (4, 24, Effects::UNDERLINE),
            (7, 27, Effects::INVERT),
            (8, 28, Effects::HIDDEN),
            (9, 29, Effects::STRIKETHROUGH),
        ] {
            let spans = parse(&format!("\x1b[{on}mA\x1b[{off}mB"));
            assert!(spans[0].style.get_effects().contains(effect));
            assert!(spans[1].style.is_plain());
        }
        for index in 0..16 {
            let fg = if index < 8 {
                30 + index
            } else {
                90 + index - 8
            };
            let bg = if index < 8 {
                40 + index
            } else {
                100 + index - 8
            };
            let spans = parse(&format!("\x1b[{fg};{bg}mA\x1b[39;49mB"));
            assert_eq!(spans[0].style.get_fg_color(), ansi(index));
            assert_eq!(spans[0].style.get_bg_color(), ansi(index));
            assert!(spans[1].style.is_plain());
        }
        assert!(parse("\x1b[4:0mA")[0].style.is_plain());
        assert!(parse("\x1b[1;2mA\x1b[;1mB\x1b[mC")[2].style.is_plain());
        assert!(parse("\x1b[38;2;300;0;0mX")[0].style.is_plain());
    }

    #[test]
    fn spec_html_examples() {
        for (text, expected) in [
            (
                "\x1b[1;31mE",
                "<span class=\"ansi-red-intense-fg ansi-bold\">E</span>",
            ),
            (
                "\x1b[31;7mX",
                "<span class=\"ansi-default-inverse-fg ansi-red-bg\">X</span>",
            ),
            (
                "\x1b[38;5;208;48;2;1;2;3mY",
                "<span style=\"color: rgb(255,135,0); background-color: rgb(1,2,3)\">Y</span>",
            ),
        ] {
            assert_eq!(to_html(&parse(text)), [expected]);
        }
        assert_eq!(
            to_html(&parse("<&\n\x1b[3m\"'\n\n")),
            ["&lt;&amp;", "<span class=\"ansi-italic\">&quot;&#39;</span>", "",]
        );
        assert!(to_html(&parse("")).is_empty());
        assert!(to_html(&parse("\n")).is_empty());
    }

    #[test]
    fn canonical_round_trip() {
        let traceback = concat!(
            "\x1b[31m---------------------------------------------------------------------------\x1b[39m\n",
            "\x1b[31mValueError\x1b[39m                                Traceback (most recent call last)\n",
            "\x1b[36mCell\x1b[39m\x1b[36m \x1b[39m\x1b[32mIn[1]\x1b[39m\x1b[32m, line 1\x1b[39m\n",
            "\x1b[32m----> \x1b[39m\x1b[32m1\x1b[39m \x1b[38;5;28;01mraise\x1b[39;00m ValueError(\x1b[33m'fail'\x1b[39m)\n",
            "\x1b[31mValueError\x1b[39m: fail",
        );
        let spans = parse(traceback);
        let canonical = canonical(&spans);
        assert_eq!(parse(&canonical), spans);
        assert_eq!(strip(&canonical), strip(traceback));
        let Block::CodeBlock { text, attrs, .. } =
            code_block(traceback, "output-error")
        else {
            panic!("ANSI text is a code block");
        };
        assert_eq!(text, strip(&attrs.pairs["ansi"]));
        assert!(!text.contains('\x1b'));
        assert_eq!(parse("A\x1b[0mB").len(), 1);
    }

    #[test]
    fn bold_is_intense_then_invert() {
        let style = parse("\x1b[1;31;7mX")[0].style;
        assert_eq!(colors(&style), (None, ansi(9)));
        assert_eq!(style.get_fg_color(), ansi(1));
        let style = parse("\x1b[1;38;5;2;44mX")[0].style;
        assert_eq!(colors(&style), (ansi(10), ansi(4)));
    }

    #[test]
    fn ansi256_and_rgb() {
        for (index, expected) in [
            (0, PALETTE[0]),
            (15, PALETTE[15]),
            (16, (0, 0, 0)),
            (208, (255, 135, 0)),
            (231, (255, 255, 255)),
            (232, (8, 8, 8)),
            (255, (238, 238, 238)),
        ] {
            assert_eq!(rgb(Color::Ansi256(Ansi256Color(index))), expected);
        }
        for sgr in [
            "38;2;10;20;30", "38:2:10:20:30", "38:2::10:20:30",
            "38:2:0:10:20:30",
        ] {
            assert_eq!(
                parse(&format!("\x1b[{sgr}mX"))[0].style.get_fg_color(),
                rgb_color(10, 20, 30)
            );
        }
        assert_eq!(
            parse("\x1b[38:5:81mX")[0].style.get_fg_color(),
            Some(Color::Ansi256(Ansi256Color(81)))
        );
    }

    #[test]
    fn non_sgr_sequences_dropped() {
        let text = "A\x1b[2KB\x1b[1AC\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\\x1bPdiscard\x1b\\\x1b(B\x1b[>4;2mD\0\r\x07\t\né";
        assert_eq!(strip(text), "ABClinkD\t\né");
        assert!(parse(text).iter().all(|span| span.style.is_plain()));
        let Block::CodeBlock { attrs, .. } = code_block(text, "output-text")
        else {
            panic!("text is a code block");
        };
        assert!(!attrs.pairs.contains_key("ansi"));
    }
}
