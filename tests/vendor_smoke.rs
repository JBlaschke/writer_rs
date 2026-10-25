//! Contract tests for embedded assets and notices (vendored-assets.md §5.4).

use std::fmt::Write as _;
use std::path::Path;

use writer::vendor;

/// The 20 KaTeX faces shipped as WOFF2, in byte order.
#[rustfmt::skip]
const KATEX_FONTS: [&str; 20] = [
    "AMS-Regular",
    "Caligraphic-Bold", "Caligraphic-Regular",
    "Fraktur-Bold", "Fraktur-Regular",
    "Main-Bold", "Main-BoldItalic", "Main-Italic", "Main-Regular",
    "Math-BoldItalic", "Math-Italic",
    "SansSerif-Bold", "SansSerif-Italic", "SansSerif-Regular",
    "Script-Regular",
    "Size1-Regular", "Size2-Regular", "Size3-Regular", "Size4-Regular",
    "Typewriter-Regular",
];

/// The 43 downloaded files of vendored-assets.md §2; VERSIONS.toml is additional.
const VENDOR_FILES: [&str; 43] = [
    "vendor/crates/tylax/LICENSE",
    "vendor/crates/typst-assets/LICENSE",
    "vendor/crates/typst-assets/NOTICE",
    "vendor/katex/LICENSE",
    "vendor/katex/fonts/KaTeX_AMS-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Caligraphic-Bold.woff2",
    "vendor/katex/fonts/KaTeX_Caligraphic-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Fraktur-Bold.woff2",
    "vendor/katex/fonts/KaTeX_Fraktur-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Main-Bold.woff2",
    "vendor/katex/fonts/KaTeX_Main-BoldItalic.woff2",
    "vendor/katex/fonts/KaTeX_Main-Italic.woff2",
    "vendor/katex/fonts/KaTeX_Main-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Math-BoldItalic.woff2",
    "vendor/katex/fonts/KaTeX_Math-Italic.woff2",
    "vendor/katex/fonts/KaTeX_SansSerif-Bold.woff2",
    "vendor/katex/fonts/KaTeX_SansSerif-Italic.woff2",
    "vendor/katex/fonts/KaTeX_SansSerif-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Script-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Size1-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Size2-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Size3-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Size4-Regular.woff2",
    "vendor/katex/fonts/KaTeX_Typewriter-Regular.woff2",
    "vendor/katex/katex.min.css",
    "vendor/katex/katex.min.js",
    "vendor/latin-modern-math/GUST-FONT-LICENSE.txt",
    "vendor/latin-modern-math/MANIFEST-Latin-Modern-Math.txt",
    "vendor/latin-modern-math/README-Latin-Modern-Math.txt",
    "vendor/latin-modern-math/latinmodern-math.woff2",
    "vendor/morphdom/LICENSE",
    "vendor/morphdom/morphdom-umd.min.js",
    "vendor/typst-packages/preview/mitex/0.2.7/CHANGELOG.md",
    "vendor/typst-packages/preview/mitex/0.2.7/LICENSE",
    "vendor/typst-packages/preview/mitex/0.2.7/README.md",
    "vendor/typst-packages/preview/mitex/0.2.7/lib.typ",
    "vendor/typst-packages/preview/mitex/0.2.7/mitex.typ",
    "vendor/typst-packages/preview/mitex/0.2.7/mitex.wasm",
    "vendor/typst-packages/preview/mitex/0.2.7/specs/README.md",
    "vendor/typst-packages/preview/mitex/0.2.7/specs/latex/standard.typ",
    "vendor/typst-packages/preview/mitex/0.2.7/specs/mod.typ",
    "vendor/typst-packages/preview/mitex/0.2.7/specs/prelude.typ",
    "vendor/typst-packages/preview/mitex/0.2.7/typst.toml",
];

/// The ordered notice set of vendored-assets.md §8.1.
const LICENSE_PATHS: [&str; 15] = [
    "licenses/README.md",
    "licenses/hayagriva-styles.md",
    "licenses/katex-fonts-OFL.txt",
    "licenses/two-face-0.4.md",
    "licenses/two-face.md",
    "vendor/VERSIONS.toml",
    "vendor/crates/tylax/LICENSE",
    "vendor/crates/typst-assets/LICENSE",
    "vendor/crates/typst-assets/NOTICE",
    "vendor/katex/LICENSE",
    "vendor/latin-modern-math/GUST-FONT-LICENSE.txt",
    "vendor/latin-modern-math/MANIFEST-Latin-Modern-Math.txt",
    "vendor/latin-modern-math/README-Latin-Modern-Math.txt",
    "vendor/morphdom/LICENSE",
    "vendor/typst-packages/preview/mitex/0.2.7/LICENSE",
];

// Keep the related field types aligned (AGENTS.md).
#[rustfmt::skip]
struct PackagePin {
    name:    &'static str,
    version: &'static str,
    license: &'static str,
    role:    &'static str,
    path:    &'static str,
    note:    Option<&'static str>,
    sources: &'static [(&'static str, &'static str)],
}

/// Complete pins, including source order and hashes, from vendored-assets.md §1.
#[rustfmt::skip]
const PACKAGE_PINS: [PackagePin; 6] = [
    PackagePin {
        name: "katex", version: "0.19.1", license: "MIT AND OFL-1.1",
        role: "web", path: "katex", note: None,
        sources: &[(
            "https://registry.npmjs.org/katex/-/katex-0.19.1.tgz",
            "24da5a599eb2fcede2e270b99a6ff48bca816beecc95feeb84e8cccbd1bb34fd",
        )],
    },
    PackagePin {
        name: "morphdom", version: "2.7.8", license: "MIT",
        role: "web", path: "morphdom", note: None,
        sources: &[(
            "https://registry.npmjs.org/morphdom/-/morphdom-2.7.8.tgz",
            "06bfcd3edc6dcb0fcaa4e6f2aab28656830fd29816bfc96553c6d221bfab4bf3",
        )],
    },
    PackagePin {
        name: "latin-modern-math", version: "1.959",
        license: "GUST Font License", role: "font",
        path: "latin-modern-math", note: None,
        sources: &[
            (
                "https://raw.githubusercontent.com/fred-wang/MathFonts/9182b146cb64ac1a5e9cb92a09cd79612ca95d1c/LatinModern/latinmodern-math.woff2",
                "811ec18a1f3dca9e118c4f26bd3d39a677bcc73e009b83c10f940c950b170c13",
            ),
            (
                "https://raw.githubusercontent.com/fred-wang/MathFonts/9182b146cb64ac1a5e9cb92a09cd79612ca95d1c/LatinModern/GUST-FONT-LICENSE.txt",
                "2bd69affc3da00715116f713f57eab9707e96daf3562ad0215987b15b9c16f73",
            ),
            (
                "https://raw.githubusercontent.com/fred-wang/MathFonts/9182b146cb64ac1a5e9cb92a09cd79612ca95d1c/LatinModern/README-Latin-Modern-Math.txt",
                "e6fdd5fb44b656146fa988d191f58901c09a347d3f4fbaa3b45b24a6ad7ec30b",
            ),
            (
                "https://raw.githubusercontent.com/fred-wang/MathFonts/9182b146cb64ac1a5e9cb92a09cd79612ca95d1c/LatinModern/MANIFEST-Latin-Modern-Math.txt",
                "1daa729f7922c42ba5ce8da7dfe4ecdc9e5c878ad86217fd6f32c4671505b007",
            ),
        ],
    },
    PackagePin {
        name: "mitex", version: "0.2.7", license: "Apache-2.0",
        role: "typst-package", path: "typst-packages/preview/mitex/0.2.7",
        note: None,
        sources: &[(
            "https://packages.typst.org/preview/mitex-0.2.7.tar.gz",
            "0159e214845e49cbdc332d9d572da112dae5ad248072e0a7680d38c8307c2e15",
        )],
    },
    PackagePin {
        name: "typst-assets", version: "0.15.1", license: "Apache-2.0",
        role: "license-only", path: "crates/typst-assets",
        note: Some(
            "linked through typst-kit embedded-fonts; only LICENSE and NOTICE are copied",
        ),
        sources: &[(
            "https://static.crates.io/crates/typst-assets/typst-assets-0.15.1.crate",
            "bcee505dac6702dd1c5e65aa2e94a6179d19ee09e2e5637d7313db91765dc4e0",
        )],
    },
    PackagePin {
        name: "tylax", version: "0.3.8", license: "Apache-2.0",
        role: "license-only", path: "crates/tylax",
        note: Some(
            "assets/math/unicode-latex.toml is derived from it; only LICENSE is copied",
        ),
        sources: &[(
            "https://static.crates.io/crates/tylax/tylax-0.3.8.crate",
            "145d65de1d49c044ae64c664edf034f4956c17dbddaa0cf2fad3afea85400672",
        )],
    },
];

fn expected_vendor_files() -> Vec<String> {
    let mut files: Vec<String> =
        VENDOR_FILES.iter().map(|path| (*path).to_owned()).collect();
    files.push("vendor/VERSIONS.toml".to_owned());
    files.sort();
    files
}

#[test]
fn vendor_file_list_matches_spec() {
    assert_eq!(vendor::list("vendor/"), expected_vendor_files());
    let bytes: usize = VENDOR_FILES
        .iter()
        .map(|path| {
            vendor::get(path)
                .unwrap_or_else(|| panic!("{path} is vendored"))
                .len()
        })
        .sum();
    assert_eq!(bytes, 1_425_604, "downloaded bytes, excluding VERSIONS.toml");
}

#[test]
fn versions_toml_records_pins() {
    let text = vendor::get_str("vendor/VERSIONS.toml").unwrap();
    assert!(text.starts_with(
        "# Generated by scripts/vendor.sh; do not edit by hand.\n\
         # See docs/spec/vendored-assets.md for what is copied and why.\n"
    ));
    let versions: toml::Table =
        text.parse().expect("VERSIONS.toml is valid TOML");
    assert_eq!(versions.len(), 1);
    let packages = versions["package"].as_array().expect("[[package]] array");
    assert_eq!(packages.len(), PACKAGE_PINS.len());
    for (package, pin) in packages.iter().zip(&PACKAGE_PINS) {
        let package = package.as_table().expect("a package table");
        #[rustfmt::skip]
        assert_eq!(
            package.len(), 7 + usize::from(pin.note.is_some()),
            "{}", pin.name
        );
        for (key, expected) in [
            ("name", pin.name),
            ("version", pin.version),
            ("license", pin.license),
            ("role", pin.role),
            ("date", "2026-10-25"),
            ("path", pin.path),
        ] {
            #[rustfmt::skip]
            assert_eq!(
                package[key].as_str(), Some(expected),
                "{}: {key}", pin.name
            );
        }
        assert_eq!(package.get("note").and_then(toml::Value::as_str), pin.note);
        let sources =
            package["source"].as_array().expect("[[package.source]] array");
        assert_eq!(sources.len(), pin.sources.len(), "{}", pin.name);
        for (source, &(url, sha256)) in sources.iter().zip(pin.sources) {
            let source = source.as_table().expect("source table");
            assert_eq!(source.len(), 2);
            assert_eq!(source["url"].as_str(), Some(url));
            assert_eq!(source["sha256"].as_str(), Some(sha256));
        }
    }
    assert_eq!(PACKAGE_PINS[0].version, vendor::KATEX_VERSION);
    assert_eq!(PACKAGE_PINS[1].version, vendor::MORPHDOM_VERSION);
    assert_eq!(PACKAGE_PINS[3].version, vendor::MITEX_VERSION);

    let lock: toml::Table = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"),
    )
    .unwrap()
    .parse()
    .expect("Cargo.lock is valid TOML");
    let locked_packages =
        lock["package"].as_array().expect("locked package array");
    let locked_versions = |name: &str| {
        let mut versions: Vec<&str> = locked_packages
            .iter()
            .filter(|package| package["name"].as_str() == Some(name))
            .map(|package| {
                package["version"].as_str().expect("package version")
            })
            .collect();
        versions.sort();
        versions
    };
    assert_eq!(locked_versions("typst-assets"), [PACKAGE_PINS[4].version]);
    assert_eq!(locked_versions("two-face"), ["0.4.5", "0.5.2+bat-0.26.1"]);
}

#[test]
fn katex_is_complete() {
    let script = vendor::get_str("vendor/katex/katex.min.js").unwrap();
    assert!(script.contains(&format!("version:\"{}\"", vendor::KATEX_VERSION)));
    let css = vendor::get_str("vendor/katex/katex.min.css").unwrap();
    assert_eq!(vendor::list("vendor/katex/fonts/").len(), KATEX_FONTS.len());
    for face in KATEX_FONTS {
        assert!(
            css.contains(&format!("url(fonts/KaTeX_{face}.woff2)")),
            "{face}"
        );
    }
    for path in vendor::list("vendor/")
        .into_iter()
        .filter(|path| path.ends_with(".woff2"))
    {
        assert!(vendor::get(&path).unwrap().starts_with(b"wOF2"), "{path}");
    }
}

#[test]
fn morphdom_defines_global() {
    let script =
        vendor::get_str("vendor/morphdom/morphdom-umd.min.js").unwrap();
    assert!(script.contains("global.morphdom=factory()"));
}

#[test]
fn mitex_package_is_served() {
    let dir =
        vendor::typst_package_dir("preview", "mitex", vendor::MITEX_VERSION)
            .expect("the mitex package is embedded");
    assert_eq!(dir, "vendor/typst-packages/preview/mitex/0.2.7/");
    assert_eq!(vendor::list(&dir).len(), 11);
    let manifest: toml::Table = vendor::get_str(&format!("{dir}typst.toml"))
        .unwrap()
        .parse()
        .expect("valid typst package manifest");
    for (key, expected) in [
        ("name", "mitex"),
        ("version", vendor::MITEX_VERSION),
        ("entrypoint", "lib.typ"),
    ] {
        assert_eq!(manifest["package"][key].as_str(), Some(expected));
    }
    assert!(
        vendor::get(&format!("{dir}mitex.wasm")).unwrap().starts_with(b"\0asm")
    );
    assert!(vendor::typst_package_dir("preview", "cetz", "0.4.0").is_none());
    assert!(vendor::typst_package_dir("preview", "mitex", "0.2.6").is_none());
}

fn value_or_dash(value: &str) -> &str {
    if value.is_empty() { "—" } else { value }
}

/// A reproducible notice for the dependency's compiled CSL archive (§3.4).
fn hayagriva_styles_md() -> String {
    let styles = hayagriva::archive::ArchivedStyle::all();
    assert_eq!(styles.len(), 80);
    assert_eq!(hayagriva::archive::locales().len(), 63);
    let mut markdown = String::from(
        "# hayagriva styles and locales\n\n\
         This file is generated; do not edit. Regenerate with WRITER_BLESS=1 cargo test --test vendor_smoke.\n\n\
         hayagriva 0.10.1 embeds 80 CSL styles and 63 locales.\n\
         The archive's styles and locales carry CC BY-SA 3.0\n\
         (https://creativecommons.org/licenses/by-sa/3.0/).\n\
         Each style's recorded rights, including exceptions, appear below.\n\n",
    );
    for style in styles {
        let parsed = style.get();
        let info = parsed.info();
        writeln!(markdown, "## {}\n", info.title.value).unwrap();
        writeln!(
            markdown,
            "Names: {}",
            value_or_dash(&style.names().join(", "))
        )
        .unwrap();
        writeln!(markdown, "Id: {}", value_or_dash(&info.id)).unwrap();
        for (label, attributions) in
            [("Authors", &info.authors), ("Contributors", &info.contributors)]
        {
            let names = attributions
                .iter()
                .map(|person| person.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(markdown, "{label}: {}", value_or_dash(&names)).unwrap();
        }
        let rights = info.rights.as_ref().map(|rights| {
            format!(
                "{} ({})",
                value_or_dash(&rights.name),
                value_or_dash(rights.license.as_deref().unwrap_or(""))
            )
        });
        writeln!(markdown, "Rights: {}\n", rights.as_deref().unwrap_or("—"))
            .unwrap();
    }
    markdown
}

#[test]
fn generated_license_files_are_current() {
    let generated = [
        ("licenses/two-face.md", two_face::acknowledgement::listing().to_md()),
        (
            "licenses/two-face-0.4.md",
            two_face_04::acknowledgement::listing().to_md(),
        ),
        ("licenses/hayagriva-styles.md", hayagriva_styles_md()),
    ];
    let bless =
        std::env::var_os("WRITER_BLESS").is_some_and(|value| value == "1");
    for (path, expected) in generated {
        if bless {
            let file =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, expected).unwrap();
        } else {
            let actual = vendor::get_str(path).unwrap();
            #[rustfmt::skip]
            assert!(
                actual == expected,
                "{path} is stale (embedded {} bytes, generated {} bytes); run WRITER_BLESS=1 cargo test --test vendor_smoke generated_license_files_are_current",
                actual.len(), expected.len()
            );
        }
    }
}

#[test]
fn license_paths_and_text() {
    assert_eq!(vendor::license_paths(), LICENSE_PATHS);
    let text = vendor::licenses();
    let mut expected = format!(
        "writer {}: third-party notices for the files and data embedded in this binary.\n\
         Licenses of the linked Rust crates: THIRD-PARTY-LICENSES/ in the release archive, or each crate's source on crates.io.\n\n",
        env!("CARGO_PKG_VERSION")
    );
    for path in LICENSE_PATHS {
        let bytes =
            vendor::get(path).unwrap_or_else(|| panic!("{path} is embedded"));
        let notice = String::from_utf8_lossy(&bytes);
        writeln!(expected, "==> {path} <==").unwrap();
        expected.push_str(&notice);
        if !notice.ends_with('\n') {
            expected.push('\n');
        }
        expected.push('\n');
    }
    assert!(
        text == expected,
        "licenses() must print every complete notice in order"
    );
}

#[test]
fn os_junk_is_never_listed_or_served() {
    let is_junk = |path: &str| {
        let name = path.rsplit('/').next().unwrap_or(path);
        name == ".DS_Store"
            || name.starts_with("._")
            || name == "Thumbs.db"
            || name.ends_with(".swp")
            || name.ends_with('~')
            || name.ends_with(".map")
    };
    let junk: Vec<String> =
        vendor::list("").into_iter().filter(|path| is_junk(path)).collect();
    assert!(junk.is_empty(), "OS/editor junk embedded: {junk:?}");
    for path in [
        "vendor/.DS_Store", "vendor/katex/._katex.min.js",
        "vendor/katex/katex.min.js.map", "themes/default/.DS_Store",
        "themes/default/Thumbs.db", "writer/.math.js.swp", "writer/math.js~",
    ] {
        assert!(vendor::get(path).is_none(), "{path} must not be served");
    }
}

/// Inspect normal, build, development, target and workspace dependency tables,
/// including aliases.
fn check_dependencies(value: &toml::Value) {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                if matches!(
                    key.as_str(),
                    "dependencies" | "dev-dependencies" | "build-dependencies"
                ) {
                    for (alias, dependency) in
                        value.as_table().expect("dependency table")
                    {
                        let package = dependency
                            .get("package")
                            .and_then(toml::Value::as_str);
                        for name in
                            std::iter::once(alias.as_str()).chain(package)
                        {
                            assert!(
                                ![
                                    "image", "resvg", "usvg", "svg2pdf",
                                    "tylax", "lol_html", "grass"
                                ]
                                .contains(&name),
                                "forbidden dependency {name:?} in {key} (key {alias:?})"
                            );
                        }
                    }
                }
                check_dependencies(value);
            }
        }
        toml::Value::Array(array) => {
            for value in array {
                check_dependencies(value);
            }
        }
        _ => {}
    }
}

#[test]
fn no_forbidden_dependencies() {
    let manifest = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .unwrap();
    let manifest = manifest.parse::<toml::Table>().expect("valid Cargo.toml");
    check_dependencies(&toml::Value::Table(manifest));
}

#[test]
fn rust_embed_only_in_vendor_rs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.expect("readable source directory");
        if entry.file_type().is_file() && entry.path() != root.join("vendor.rs")
        {
            let source =
                std::fs::read_to_string(entry.path()).expect("UTF-8 source");
            assert!(
                !source.contains("rust_embed"),
                "{} embeds assets outside src/vendor.rs",
                entry.path().display()
            );
        }
    }
}

#[test]
fn list_is_sorted_and_prefix_filtered() {
    let all = vendor::list("");
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted);
    for prefix in
        ["vendor/katex/", "vendor/katex/katex", "licenses/", "writer/"]
    {
        let expected: Vec<_> = all
            .iter()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect();
        assert_eq!(
            vendor::list(prefix),
            expected,
            "plain string prefix {prefix:?}"
        );
    }
    assert!(vendor::list("no-such-prefix/").is_empty());
}

/// The path contract is identical for debug disk reads and release embedded reads.
#[test]
fn non_normalized_paths_resolve_like_clean_ones() {
    for (messy, clean) in [
        (
            "vendor/katex/./fonts/KaTeX_Main-Regular.woff2",
            "vendor/katex/fonts/KaTeX_Main-Regular.woff2",
        ),
        ("vendor/katex/fonts/../katex.min.css", "vendor/katex/katex.min.css"),
        ("vendor//VERSIONS.toml", "vendor/VERSIONS.toml"),
        ("vendor/../vendor/katex/katex.min.js", "vendor/katex/katex.min.js"),
        ("writer/../vendor/VERSIONS.toml", "vendor/VERSIONS.toml"),
        ("./writer/math.js", "writer/math.js"),
        ("/writer/math.js", "writer/math.js"),
        ("vendor/katex/katex.min.js/", "vendor/katex/katex.min.js"),
        ("vendor\\katex\\katex.min.js", "vendor/katex/katex.min.js"),
    ] {
        let expected =
            vendor::get(clean).unwrap_or_else(|| panic!("{clean} is embedded"));
        let actual = vendor::get(messy);
        #[rustfmt::skip]
        assert!(
            actual.as_deref() == Some(&*expected),
            "{messy:?}: got {:?} bytes, expected the {} bytes of {clean}",
            actual.map(|bytes| bytes.len()), expected.len()
        );
    }
    assert_eq!(
        vendor::get_str("vendor/./VERSIONS.toml").unwrap(),
        vendor::get_str("vendor/VERSIONS.toml").unwrap()
    );
}

#[test]
fn wrong_case_directories_and_escaping_paths_are_none() {
    for path in [
        "vendor/katex/KaTeX.min.js", "vendor/katex/KATEX.MIN.JS",
        "Vendor/katex/katex.min.js", "vendor/katex", "vendor/katex/",
        "../assets/vendor/VERSIONS.toml", "vendor/../../Cargo.toml",
        "../Cargo.toml", "", "/",
    ] {
        assert!(vendor::get(path).is_none(), "{path:?} must be None");
    }
}

#[test]
fn every_listed_path_is_gettable() {
    let all = vendor::list("");
    assert!(all.len() >= expected_vendor_files().len() + 6, "{all:?}");
    for path in &all {
        assert!(
            vendor::get(path).is_some(),
            "{path} is listed but not gettable"
        );
    }
}

#[test]
fn get_str_reads_utf8_and_reports_missing() {
    let versions = vendor::get_str("vendor/VERSIONS.toml").unwrap();
    assert!(
        versions.contains(&format!("version = \"{}\"", vendor::KATEX_VERSION))
    );
    assert!(
        versions
            .contains(&format!("version = \"{}\"", vendor::MORPHDOM_VERSION))
    );
    assert!(!vendor::get_str("writer/math.js").unwrap().is_empty());
    let error = vendor::get_str("writer/does-not-exist.js").unwrap_err();
    assert!(error.to_string().contains("writer/does-not-exist.js"), "{error}");
    assert!(
        vendor::get_str("vendor/katex/fonts/KaTeX_Main-Regular.woff2").is_err()
    );
}

#[test]
fn disk_dir_only_in_debug() {
    let expected = if cfg!(debug_assertions) {
        Some(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")))
    } else {
        None
    };
    assert_eq!(vendor::disk_dir(), expected);
}
