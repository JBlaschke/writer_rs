//! The fonts every build has: typst-assets' font files (vendored-assets.md
//! §3.1), stored deflated by `build.rs` and inflated on first use.
//!
//! typst-kit's `embedded-fonts` feature would link the files as they are,
//! 3.1 MiB more than their deflated copies. [`embedded`] serves the same fonts
//! in the same order, so font selection and output are unchanged; inflating
//! them takes some 20 ms once per process.

use std::io::Read;
use std::sync::OnceLock;

use flate2::read::DeflateDecoder;
use typst::foundations::Bytes;
use typst::text::{Font, FontInfo};

/// The font files, one raw deflate stream each, back to back (`build.rs`).
static FONT_DATA: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/embedded-fonts.deflate"));

// `FONT_SIZES`: each file's deflated and original length, in order.
include!(concat!(env!("OUT_DIR"), "/embedded-fonts.rs"));

/// The embedded fonts with their metadata, face by face in the order of
/// `typst_assets::fonts()`: what `typst_kit::fonts::embedded()` yields with
/// typst-kit's `embedded-fonts` feature, which writer does not enable.
pub fn embedded() -> impl Iterator<Item = (Font, FontInfo)> {
    files().iter().flat_map(|data| {
        Font::iter(data.clone()).map(|font| {
            let info = font.info().clone();
            (font, info)
        })
    })
}

/// The inflated font files, kept for the rest of the process.
fn files() -> &'static [Bytes] {
    static FILES: OnceLock<Vec<Bytes>> = OnceLock::new();
    FILES.get_or_init(|| {
        let mut rest = FONT_DATA;
        FONT_SIZES
            .iter()
            .map(|&(packed, size)| {
                let (file, next) = rest.split_at(packed);
                rest = next;
                let mut data = Vec::with_capacity(size);
                DeflateDecoder::new(file)
                    .read_to_end(&mut data)
                    .expect("build.rs writes complete deflate streams");
                Bytes::new(data)
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflated_files_are_typst_assets_fonts() {
        let expected: Vec<&[u8]> = typst_assets::fonts().collect();
        assert_eq!(files().len(), expected.len());
        for (file, expected) in files().iter().zip(expected) {
            assert!(file.as_slice() == expected);
        }
        let packed: usize = FONT_SIZES.iter().map(|(packed, _)| packed).sum();
        assert_eq!(packed, FONT_DATA.len());
    }

    #[test]
    fn faces_carry_their_metadata_in_file_order() {
        let mut families: Vec<String> = Vec::new();
        for (font, info) in embedded() {
            assert_eq!(font.info(), &info);
            if !families.contains(&info.family) {
                families.push(info.family);
            }
        }
        let expected = [
            "Libertinus Serif", "New Computer Modern Math",
            "New Computer Modern", "DejaVu Sans Mono",
        ];
        assert_eq!(families, expected);
    }
}
