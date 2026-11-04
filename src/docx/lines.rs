//! Word's single line, the unit of `w:lineRule="auto"` spacing.
//!
//! HTML and Typst scale a `line_height` multiple by the font size. Word's
//! `auto` rule scales the font's single line instead: the OS/2 Windows
//! ascent plus descent, and any `hhea` line gap beyond them, per em (Calibri
//! 1.22, Source Sans 3 1.42). Writer divides the multiple by the single line
//! of the font it applies to, so a Word line is `n × size` as in the other
//! formats while still scaling with each paragraph's size.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use typst::foundations::Bytes;
use typst::text::{Font, FontBook, FontInfo, FontVariant};

use crate::design::{Design, FontRef, LineHeight};

/// Calibri's single line, 2500/2048 em. Calibri is the font of writer's
/// generated styles, and the value stands in for a family writer cannot
/// read.
pub const DEFAULT_SINGLE_LINE: f64 = 2500.0 / 2048.0;

/// Single lines of the design's font families, in ems.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WordLines {
    /// Keyed by lowercase family name.
    single: BTreeMap<String, f64>,
}

impl WordLines {
    /// Reads the single lines of the families the design names, when a
    /// `line_height` multiple needs them.
    ///
    /// The sources are those of the PDF, nearest first: the design's font
    /// files, the `font_paths` directories, writer's embedded fonts and,
    /// with `system_fonts`, the system's fonts. A family found in none of
    /// them uses [`DEFAULT_SINGLE_LINE`].
    pub fn load(
        design: &Design, font_paths: &[PathBuf], system_fonts: bool,
    ) -> Self {
        let mut lines = Self::default();
        let multiple = |height: Option<LineHeight>| {
            matches!(height, Some(LineHeight::Multiple(_)))
        };
        if !design.declared
            || !(multiple(design.text.line_height)
                || design
                    .styles
                    .values()
                    .any(|style| multiple(style.line_height)))
        {
            return lines;
        }
        let mut missing = families(design);
        let files: Vec<Font> = design
            .font_files()
            .iter()
            .filter_map(|path| std::fs::read(path).ok())
            .flat_map(|data| Font::iter(Bytes::new(data)))
            .collect();
        lines.resolve(&mut missing, files.iter().map(Font::info), |index| {
            files.get(index).cloned()
        });
        for dir in font_paths {
            if missing.is_empty() {
                break;
            }
            let found: Vec<_> = typst_kit::fonts::scan(dir)
                .map(|(path, info)| (path.path, path.index, info))
                .collect();
            lines.resolve_paths(&mut missing, &found);
        }
        if !missing.is_empty() {
            let embedded: Vec<_> = typst_kit::fonts::embedded().collect();
            lines.resolve(
                &mut missing,
                embedded.iter().map(|(_, info)| info),
                |index| embedded.get(index).map(|(font, _)| font.clone()),
            );
        }
        if system_fonts && !missing.is_empty() {
            lines.resolve_paths(&mut missing, system());
        }
        lines
    }

    /// Records a family's single line in ems.
    #[must_use]
    pub fn with(mut self, family: &str, single_line: f64) -> Self {
        self.single.insert(family.to_lowercase(), single_line);
        self
    }

    /// The family's single line in ems, or [`DEFAULT_SINGLE_LINE`].
    pub fn single_line(&self, family: Option<&str>) -> f64 {
        family
            .and_then(|family| self.single.get(&family.to_lowercase()))
            .copied()
            .unwrap_or(DEFAULT_SINGLE_LINE)
    }

    /// The `w:line` value, in 240ths of a single line, that makes an `auto`
    /// line `n` times the font size in `family`.
    pub fn auto_line(&self, n: f64, family: Option<&str>) -> u32 {
        (n * 240.0 / self.single_line(family)).round() as u32
    }

    /// Picks each missing family's regular face from one source.
    fn resolve<'a>(
        &mut self, missing: &mut BTreeSet<String>,
        infos: impl Iterator<Item = &'a FontInfo>,
        load: impl Fn(usize) -> Option<Font>,
    ) {
        if missing.is_empty() {
            return;
        }
        let book = FontBook::from_infos(infos.cloned());
        missing.retain(|family| {
            let found = book
                .select(family, FontVariant::default())
                .and_then(&load)
                .and_then(|font| single_line(&font));
            if let Some(value) = found {
                self.single.insert(family.clone(), value);
            }
            found.is_none()
        });
    }

    fn resolve_paths(
        &mut self, missing: &mut BTreeSet<String>,
        found: &[(PathBuf, u32, FontInfo)],
    ) {
        self.resolve(missing, found.iter().map(|(_, _, info)| info), |index| {
            let (path, face, _) = found.get(index)?;
            let data = std::fs::read(path).ok()?;
            Font::new(Bytes::new(data), *face)
        });
    }
}

/// Word's single line of `font` in ems: the OS/2 Windows ascent plus
/// descent and the external leading, the part of the `hhea` line gap that
/// exceeds their difference from the `hhea` ascent plus descent.
pub fn single_line(font: &Font) -> Option<f64> {
    sfnt_single_line(font.data(), font.index())
}

/// [`single_line`] read from the `head`, `hhea` and `OS/2` tables of face
/// `index` in an sfnt file or collection.
fn sfnt_single_line(data: &[u8], index: u32) -> Option<f64> {
    let u16_at = |at: usize| {
        data.get(at..at + 2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
    };
    let u32_at = |at: usize| {
        data.get(at..at + 4).map(|bytes| {
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        })
    };
    let start = if data.get(..4)? == b"ttcf" {
        u32_at(12 + 4 * usize::try_from(index).ok()?)? as usize
    } else {
        0
    };
    let count = usize::from(u16_at(start + 4)?);
    let table = |tag: &[u8; 4]| {
        (0..count)
            .map(|record| start + 12 + 16 * record)
            .find(|&record| data.get(record..record + 4) == Some(&tag[..]))
            .and_then(|record| u32_at(record + 8))
            .map(|offset| offset as usize)
    };
    let signed = |at: usize| u16_at(at).map(|value| f64::from(value as i16));
    let units = f64::from(u16_at(table(b"head")? + 18)?);
    let hhea = table(b"hhea")?;
    let ascent = signed(hhea + 4)?;
    let descent = -signed(hhea + 6)?;
    let gap = signed(hhea + 8)?.max(0.0);
    let height = match table(b"OS/2") {
        Some(os2) => {
            let windows =
                f64::from(u16_at(os2 + 74)?) + f64::from(u16_at(os2 + 76)?);
            windows + (gap - (windows - ascent - descent)).max(0.0)
        }
        None => ascent + descent + gap,
    };
    let value = height / units;
    (value.is_finite() && value > 0.0).then_some(value)
}

/// The design's font families, lowercase as [`FontBook::select`] expects.
fn families(design: &Design) -> BTreeSet<String> {
    [&design.fonts.body, &design.fonts.heading, &design.fonts.mono]
        .into_iter()
        .flatten()
        .map(|font| font.family.as_str())
        .chain(design.styles.values().filter_map(|style| match &style.font {
            Some(FontRef::Family(family)) => Some(family.as_str()),
            _ => None,
        }))
        .map(str::to_lowercase)
        .collect()
}

/// The system's fonts, scanned once per process.
fn system() -> &'static [(PathBuf, u32, FontInfo)] {
    static SYSTEM: OnceLock<Vec<(PathBuf, u32, FontInfo)>> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        typst_kit::fonts::system()
            .map(|(path, info)| (path.path, path.index, info))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::Font as DesignFont;

    fn embedded(family: &str) -> Font {
        typst_kit::fonts::embedded()
            .find(|(_, info)| info.family == family)
            .map(|(font, _)| font)
            .expect("embedded family")
    }

    #[test]
    fn single_line_reads_windows_metrics_and_external_leading() {
        // DejaVu Sans Mono: Windows ascent 1901 and descent 483, the same
        // hhea ascent and descent, no line gap, 2048 units per em.
        let value = single_line(&embedded("DejaVu Sans Mono")).unwrap();
        assert!((value - 2384.0 / 2048.0).abs() < 1e-9, "{value}");
    }

    #[test]
    fn auto_line_divides_the_multiple_by_the_single_line() {
        let lines = WordLines::default().with("Source Sans 3", 1.424);
        assert_eq!(lines.auto_line(1.35, Some("source sans 3")), 228);
        assert_eq!(lines.auto_line(1.35, Some("Unknown")), 265);
        assert_eq!(lines.auto_line(1.35, None), 265);
        assert_eq!(
            WordLines::default().auto_line(1.0, None),
            (240.0 / DEFAULT_SINGLE_LINE).round() as u32
        );
    }

    #[test]
    fn load_reads_declared_families_only_for_multiples() {
        let mut design = Design { declared: true, ..Design::default() };
        design.fonts.body = Some(DesignFont {
            family: "DejaVu Sans Mono".into(),
            files: Vec::new(),
        });
        assert_eq!(WordLines::load(&design, &[], false), WordLines::default());
        design.text.line_height = Some(LineHeight::Multiple(1.35));
        let lines = WordLines::load(&design, &[], false);
        let expected = 2384.0 / 2048.0;
        assert!(
            (lines.single_line(Some("DejaVu Sans Mono")) - expected).abs()
                < 1e-9
        );
        assert_eq!(lines.single_line(Some("Calibri")), DEFAULT_SINGLE_LINE);
    }

    /// A copy of `font` whose OS/2 Windows ascent is `ascent`.
    fn patched(font: &Font, ascent: u16) -> Vec<u8> {
        let mut data = font.data().to_vec();
        let count = u16::from_be_bytes([data[4], data[5]]) as usize;
        let os2 = (0..count)
            .map(|index| 12 + 16 * index)
            .find(|&record| &data[record..record + 4] == b"OS/2")
            .map(|record| {
                let bytes = &data[record + 8..record + 12];
                u32::from_be_bytes(bytes.try_into().unwrap()) as usize
            })
            .expect("OS/2 table");
        data[os2 + 74..os2 + 76].copy_from_slice(&ascent.to_be_bytes());
        data
    }

    #[test]
    fn load_prefers_declared_files_then_font_paths_over_embedded_fonts() {
        // Libertinus Serif: Windows ascent 894 and descent 246 per 1000.
        let font = embedded("Libertinus Serif");
        assert_eq!(single_line(&font), Some(1.14));
        let files = tempfile::tempdir().unwrap();
        let path = files.path().join("libertinus.otf");
        std::fs::write(&path, patched(&font, 1054)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("libertinus.otf"), patched(&font, 954))
            .unwrap();
        let mut design = Design { declared: true, ..Design::default() };
        design.text.line_height = Some(LineHeight::Multiple(1.2));
        design.fonts.body = Some(DesignFont {
            family: "Libertinus Serif".into(),
            files: vec![path],
        });
        let paths = [dir.path().to_path_buf()];
        let lines = WordLines::load(&design, &paths, false);
        assert_eq!(lines.single_line(Some("Libertinus Serif")), 1.3);
        design.fonts.body.as_mut().unwrap().files.clear();
        let lines = WordLines::load(&design, &paths, false);
        assert_eq!(lines.single_line(Some("Libertinus Serif")), 1.2);
        let lines = WordLines::load(&design, &[], false);
        assert_eq!(lines.single_line(Some("Libertinus Serif")), 1.14);
    }
}
