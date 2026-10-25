//! Embedded assets, package lookup, and third-party notices.
//!
//! Contract: vendored-assets.md §5.1–§5.3 and §8.1–§8.2. Owner: T00. Paths are
//! relative to `assets/`, for example `vendor/katex/katex.min.js`. Debug builds
//! read assets from disk; release builds embed them in the binary.
//!
//! The two modes look files up differently (debug: a file-system path, release:
//! an exact key), so [`get`] normalizes the path and requires an exact,
//! case-sensitive match of a listed file in both modes. That way a path that
//! works in a debug test also works in the shipped binary.
//!
//! OS and editor junk (`.DS_Store`, AppleDouble `._*`, `Thumbs.db`, `*.swp`,
//! `*~`) is excluded so a file Finder drops into `assets/` is never listed,
//! served or embedded. `build.rs` makes release builds re-embed `assets/` when
//! files are added or removed.

use std::borrow::Cow;
use std::path::Path;

use crate::error::{Error, Result};

/// The embedded vendor assets, notices, writer scripts, themes, packages, and
/// scaffolds.
#[derive(rust_embed::Embed)]
#[folder = "assets/"]
#[include = "vendor/**/*"]
#[include = "licenses/**/*"]
#[include = "writer/**/*"]
#[include = "themes/**/*"]
#[include = "typst/**/*"]
#[include = "docx/**/*"]
#[include = "latex/**/*"]
#[include = "math/**/*"]
#[include = "l10n/**/*"]
#[include = "scaffold/**/*"]
#[exclude = "**/*.map"]
#[exclude = "**/.DS_Store"]
#[exclude = "**/._*"]
#[exclude = "**/Thumbs.db"]
#[exclude = "**/*.swp"]
#[exclude = "**/*~"]
pub struct Assets;

/// The version of the vendored KaTeX JavaScript, CSS, and fonts.
pub const KATEX_VERSION: &str = "0.19.1";

/// The version of the vendored morphdom browser script.
pub const MORPHDOM_VERSION: &str = "2.7.8";

/// The embedded `@preview/mitex` package version, also used by emitted Typst
/// imports.
pub const MITEX_VERSION: &str = "0.2.7";

/// Returns the bytes of the embedded file at `path` (relative to `assets/`), if
/// present.
///
/// The path is normalized lexically first: `/` and `\` both separate segments,
/// empty and `.` segments are dropped and `..` removes the previous segment, so
/// `vendor/katex/fonts/../katex.min.css` finds `vendor/katex/katex.min.css`. A
/// path that is empty or climbs above `assets/` yields `None`. The final lookup
/// is exact and case-sensitive, in debug and release builds alike.
pub fn get(path: &str) -> Option<Cow<'static, [u8]>> {
    let key = normalize(path)?;
    // Debug builds resolve `key` on disk, which also matches `Reveal.js` on a
    // case-insensitive file system (macOS) and reaches files the embedded map
    // would not contain. Release builds look the key up exactly; insist on that
    // here too.
    if cfg!(debug_assertions) && !Assets::iter().any(|name| name == key) {
        return None;
    }
    Assets::get(&key).map(|file| file.data)
}

/// Returns the embedded file at `path` as UTF-8 text.
///
/// `path` is normalized as in [`get`]. Fails with [`Error::Other`] when the
/// file does not exist or is not valid UTF-8.
pub fn get_str(path: &str) -> Result<String> {
    let data = get(path).ok_or_else(|| {
        Error::Other(format!("embedded asset not found: {path}"))
    })?;
    String::from_utf8(data.into_owned()).map_err(|e| {
        Error::Other(format!("embedded asset {path} is not valid UTF-8: {e}"))
    })
}

/// Lists every embedded file whose path starts with `prefix`, sorted
/// lexicographically.
///
/// The match is a plain string prefix on the normalized paths (`/`-separated,
/// no `.` or `..` segments, no leading `/`), so pass a trailing `/` to list a
/// directory (`list("vendor/katex/fonts/")`); nested files are included. Every
/// listed path is accepted by [`get`].
pub fn list(prefix: &str) -> Vec<String> {
    let mut paths: Vec<String> = Assets::iter()
        .filter(|path| path.starts_with(prefix))
        .map(Cow::into_owned)
        .collect();
    paths.sort();
    paths
}

/// Returns the asset directory in debug builds, or `None` in release builds.
///
/// The development server watches theme and package files in this directory.
pub fn disk_dir() -> Option<&'static Path> {
    if cfg!(debug_assertions) {
        Some(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")))
    } else {
        None
    }
}

/// Returns a vendored Typst package directory if its manifest is embedded.
///
/// The returned path has the form
/// `vendor/typst-packages/<namespace>/<name>/<version>/`.
pub fn typst_package_dir(
    namespace: &str, name: &str, version: &str,
) -> Option<String> {
    let dir = format!("vendor/typst-packages/{namespace}/{name}/{version}/");
    get(&format!("{dir}typst.toml")).map(|_| dir)
}

/// Lists the embedded notice files in byte order (vendored-assets.md §8.1).
pub fn license_paths() -> Vec<String> {
    list("")
        .into_iter()
        .filter(|path| {
            if path.starts_with("licenses/") || path == "vendor/VERSIONS.toml" {
                return true;
            }
            if !path.starts_with("vendor/") {
                return false;
            }
            let file_name =
                path.rsplit('/').next().unwrap_or_default().to_lowercase();
            file_name.contains("licen")
                || file_name.contains("notice")
                || file_name.contains("copying")
                || file_name.starts_with("manifest-")
                || file_name.starts_with("readme-")
        })
        .collect()
}

/// Returns the complete notice text printed by `writer --licenses`.
pub fn licenses() -> String {
    let mut text = format!(
        "writer {}: third-party notices for the files and data embedded in this binary.\n\
         Licenses of the linked Rust crates: THIRD-PARTY-LICENSES/ in the release archive, or each crate's source on crates.io.\n\n",
        env!("CARGO_PKG_VERSION")
    );
    for path in license_paths() {
        text.push_str(&format!("==> {path} <==\n"));
        if let Some(bytes) = get(&path) {
            let notice = String::from_utf8_lossy(&bytes);
            text.push_str(&notice);
            if !notice.ends_with('\n') {
                text.push('\n');
            }
        }
        text.push('\n');
    }
    text
}

/// Normalizes an asset path lexically (see [`get`]); `None` if empty or
/// escaping `assets/`.
fn normalize(path: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalize_drops_empty_and_dot_segments() {
        let clean = Some("vendor/reveal/dist/reveal.js".to_string());
        for path in [
            "vendor/reveal/dist/reveal.js",
            "./vendor/reveal/dist/reveal.js",
            "vendor/./reveal/dist/./reveal.js",
            "vendor//reveal///dist/reveal.js",
            "/vendor/reveal/dist/reveal.js",
            "vendor/reveal/dist/reveal.js/",
            "vendor\\reveal\\dist\\reveal.js",
        ] {
            assert_eq!(normalize(path), clean, "{path}");
        }
    }

    #[test]
    fn normalize_resolves_parent_segments() {
        assert_eq!(
            normalize("vendor/katex/fonts/../katex.min.css").as_deref(),
            Some("vendor/katex/katex.min.css")
        );
        assert_eq!(
            normalize("writer/../vendor/VERSIONS.toml").as_deref(),
            Some("vendor/VERSIONS.toml")
        );
        assert_eq!(
            normalize("vendor/reveal/css/theme/./fonts/../../theme/white.scss")
                .as_deref(),
            Some("vendor/reveal/css/theme/white.scss")
        );
    }

    #[test]
    fn normalize_rejects_empty_and_escaping_paths() {
        for path in [
            "", "/", ".", "./", "..", "../assets/vendor/VERSIONS.toml",
            "vendor/../../Cargo.toml", "vendor/..",
        ] {
            assert_eq!(normalize(path), None, "{path}");
        }
    }
}
