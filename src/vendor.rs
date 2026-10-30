//! Embedded assets, package lookup, and third-party notices.
//!
//! Contract: vendored-assets.md §5.1–§5.3 and §8.1–§8.2. Owner: T00. Paths are
//! relative to `assets/`, for example `vendor/katex/katex.min.js`. Debug builds
//! read assets from disk; release builds embed them in the binary.
//!
//! The two modes look files up differently (debug: a file-system path, release:
//! an exact key), so [`get`] normalizes the path and requires an exact,
//! case-sensitive match of a listed file in both modes. That way a path that
//! works in a debug test also works in the shipped binary. Listing `assets/`
//! walks the whole tree, so debug builds keep the listing and take it again
//! only when it disagrees with the disk (see [`get`]).
//!
//! OS and editor junk (`.DS_Store`, AppleDouble `._*`, `Thumbs.db`, `*.swp`,
//! `*~`) is excluded so a file Finder drops into `assets/` is never listed,
//! served or embedded. `build.rs` makes release builds re-embed `assets/` when
//! files are added or removed.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::ops::Bound;
use std::path::Path;
use std::sync::{LazyLock, PoisonError, RwLock};

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
///
/// Debug builds read the file from disk on every call, so edits show at once.
/// They check `key` against a listing of `assets/` that is kept, and taken
/// again when the listing and the disk disagree about `key`, so a file added
/// while the process runs is found and a removed one leaves [`list`] too.
pub fn get(path: &str) -> Option<Cow<'static, [u8]>> {
    let key = normalize(path)?;
    let data = Assets::get(&key).map(|file| file.data);
    // Debug builds resolve `key` on disk, which also matches `Reveal.js` on a
    // case-insensitive file system (macOS) and reaches files the embedded map
    // would not contain. Release builds look the key up exactly; insist on that
    // here too.
    if cfg!(debug_assertions) {
        return listed(&LISTING, &key, data, walk);
    }
    data
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
/// listed path is accepted by [`get`]. Debug builds report the listing [`get`]
/// keeps: a file added to `assets/` while the process runs is listed once
/// [`get`] has asked for it, or after a restart.
pub fn list(prefix: &str) -> Vec<String> {
    LISTING
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .range::<str, _>((Bound::Included(prefix), Bound::Unbounded))
        .take_while(|path| path.starts_with(prefix))
        .cloned()
        .collect()
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

/// The embedded paths, as [`walk`] last listed them: what [`list`] reports, and
/// in debug builds what [`get`] checks keys against.
static LISTING: LazyLock<RwLock<BTreeSet<String>>> =
    LazyLock::new(|| RwLock::new(walk()));

/// Lists the embedded paths. In debug builds this walks `assets/` and
/// canonicalizes every file, so lookups use [`LISTING`] instead.
fn walk() -> BTreeSet<String> {
    #[cfg(test)]
    WALKS.set(WALKS.get() + 1);
    Assets::iter().map(Cow::into_owned).collect()
}

#[cfg(test)]
thread_local! {
    /// How many times [`walk`] listed the assets on this thread (tests count
    /// them).
    static WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `data`, what the disk holds for `key`, if `key` is in `listing`.
///
/// When the two disagree (a file is read but not listed, or listed but not
/// read), `listing` is replaced by `relist()` and checked again: the file was
/// added or removed since the listing, or was read under another case. A key
/// both listed and read, or neither, costs no listing.
fn listed<T>(
    listing: &RwLock<BTreeSet<String>>, key: &str, data: Option<T>,
    relist: impl FnOnce() -> BTreeSet<String>,
) -> Option<T> {
    let mut found =
        listing.read().unwrap_or_else(PoisonError::into_inner).contains(key);
    if data.is_some() != found {
        let paths = relist();
        found = paths.contains(key);
        *listing.write().unwrap_or_else(PoisonError::into_inner) = paths;
    }
    data.filter(|_| found)
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
    use std::collections::BTreeSet;
    use std::sync::RwLock;

    use super::{WALKS, get, list, listed, normalize};

    /// `paths` as a listing.
    fn listing(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    /// [`listed`] for `key` with `kept` as the kept listing, on a
    /// case-insensitive disk (macOS, Windows) holding `disk`; counts new
    /// listings in `walks`.
    fn lookup(
        kept: &RwLock<BTreeSet<String>>, disk: &[&str], key: &str,
        walks: &mut usize,
    ) -> Option<String> {
        let data = disk
            .iter()
            .find(|path| path.eq_ignore_ascii_case(key))
            .map(|path| path.to_string());
        listed(kept, key, data, || {
            *walks += 1;
            listing(disk)
        })
    }

    #[test]
    fn lookups_keep_the_listing() {
        assert!(get("vendor/VERSIONS.toml").is_some());
        let walks = WALKS.get();
        for _ in 0..20 {
            assert!(get("vendor/VERSIONS.toml").is_some());
            assert!(get("writer/math.js").is_some());
            assert!(get("vendor/katex/no-such-file.js").is_none());
            assert!(get("vendor/katex").is_none());
            assert!(list("vendor/katex/").len() > 20);
        }
        assert_eq!(WALKS.get(), walks, "a lookup listed assets/ again");
    }

    #[test]
    fn listing_is_taken_again_when_it_disagrees_with_the_disk() {
        let kept = RwLock::new(listing(&["a.css", "b.css"]));
        let mut walks = 0;
        let disk = ["a.css", "b.css"];
        assert_eq!(lookup(&kept, &disk, "a.css", &mut walks).unwrap(), "a.css");
        assert_eq!(lookup(&kept, &disk, "c.css", &mut walks), None);
        assert_eq!(walks, 0);

        // Added while the process runs: found, and listed from then on.
        let disk = ["a.css", "b.css", "c.css"];
        for _ in 0..2 {
            let found = lookup(&kept, &disk, "c.css", &mut walks);
            assert_eq!(found.unwrap(), "c.css");
        }
        assert_eq!(walks, 1);
        assert_eq!(*kept.read().unwrap(), listing(&disk));

        // Removed: not found, and no longer listed.
        let disk = ["a.css", "c.css"];
        for _ in 0..2 {
            assert_eq!(lookup(&kept, &disk, "b.css", &mut walks), None);
        }
        assert_eq!(walks, 2);
        assert_eq!(*kept.read().unwrap(), listing(&disk));
    }

    #[test]
    fn files_read_under_another_case_are_refused() {
        let kept = RwLock::new(listing(&["a.css", "dir/b.css"]));
        let mut walks = 0;
        let disk = ["a.css", "dir/b.css"];
        for key in ["A.css", "DIR/b.css", "dir/B.CSS"] {
            assert_eq!(lookup(&kept, &disk, key, &mut walks), None, "{key}");
        }
        assert_eq!(walks, 3);
        assert_eq!(*kept.read().unwrap(), listing(&disk));
    }

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
