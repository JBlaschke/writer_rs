//! T15 theme resolution contracts, lifted from presenter tests/theme_resolve.rs.
//! Writer adds document templates and two-face code themes in place of slides.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::error::{Error, Level};
use writer::theme::{
    REQUIRED_TEMPLATES, Theme, ThemeFile, ThemeSource, builtin_names,
    recording_reads, resolve,
};

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures() -> PathBuf {
    crate_root().join("tests/fixtures/themes")
}

fn child_theme() -> Theme {
    resolve("child-theme", &[fixtures().as_path()]).unwrap()
}

fn chain_names(theme: &Theme) -> Vec<&str> {
    theme.chain.iter().map(|dir| dir.name.as_str()).collect()
}

fn origins(files: &[ThemeFile]) -> Vec<(&str, &str)> {
    files.iter().map(|f| (f.theme.as_str(), f.rel_path.as_str())).collect()
}

fn theme_error(result: writer::Result<Theme>) -> (String, String) {
    match result {
        Err(Error::Theme { name, message }) => (name, message),
        other => panic!("expected Error::Theme, got {other:?}"),
    }
}

#[cfg(unix)]
fn problem_summary(
    found: &[writer::error::Diagnostic], root: &Path,
) -> Vec<(String, String, Level)> {
    let root = root.canonicalize().unwrap();
    found
        .iter()
        .map(|diagnostic| {
            let path = diagnostic.path.as_ref().map_or(String::new(), |path| {
                path.strip_prefix(&root)
                    .unwrap()
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            });
            (diagnostic.code.to_string(), path, diagnostic.level)
        })
        .collect()
}

fn write_files(dir: &Path, files: &[(&str, &str)]) {
    for (path, contents) in files {
        let path = dir.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

fn write_root_theme(dir: &Path, manifest: &str, extra: &[(&str, &str)]) {
    write_files(dir, &[("theme.toml", manifest)]);
    for template in REQUIRED_TEMPLATES {
        write_files(dir, &[(&format!("templates/{template}"), template)]);
    }
    write_files(dir, extra);
}

fn disk_chain(tmp: &Path) -> Theme {
    write_root_theme(
        &tmp.join("root"),
        r#"
name = "root"
[styles]
entries = ["styles/a.css", "styles/b.css"]
code_theme = "Nord"
[scripts]
entries = ["scripts/r.js"]
[vars]
accent = "red"
font = "serif"
nested = { a = 1, b = 2 }
"#,
        &[
            ("styles/a.css", "a"),
            ("styles/b.css", "b"),
            ("scripts/r.js", "r"),
            ("templates/layouts/section.html", "root section"),
            ("assets/logo.svg", "root logo"),
            ("assets/fonts/x.woff2", "root font"),
        ],
    );
    write_files(
        &tmp.join("child"),
        &[
            (
                "theme.toml",
                r#"
name = "child"
extends = "../root"
[styles]
entries = ["styles/c.css"]
[scripts]
entries = ["scripts/c.js"]
[vars]
accent = "blue"
nested = { b = 3, c = 4 }
"#,
            ),
            ("styles/c.css", "c"),
            ("scripts/c.js", "cjs"),
            ("templates/layouts/two-up.html", "child two-up"),
            ("assets/logo.svg", "child logo"),
        ],
    );
    resolve("child", &[tmp]).unwrap()
}

#[test]
fn builtin_default_resolves_without_search_roots() {
    assert_eq!(builtin_names(), ["default"]);
    let theme = resolve("default", &[]).unwrap();
    assert_eq!(chain_names(&theme), ["default"]);
    assert_eq!(theme.name(), "default");
    assert_eq!(
        theme.chain[0].source,
        ThemeSource::Embedded { prefix: "themes/default/".into() }
    );
    assert!(theme.watch_dirs().is_empty());
    for template in REQUIRED_TEMPLATES {
        let (from, source) = theme.read_template(template).unwrap().unwrap();
        assert_eq!(from, "default");
        assert!(!source.is_empty());
    }
    let (light, dark) = theme.code_themes().unwrap();
    assert_eq!(light.unwrap().label, "InspiredGitHub");
    assert_eq!(dark.unwrap().label, "OneHalfDark");
    assert_eq!(
        origins(&theme.style_entries().unwrap()),
        [
            ("default", "styles/theme.css"),
            ("default", "styles/notebook.css"),
            ("default", "styles/print.css"),
        ]
    );
    assert!(theme.script_entries().unwrap().is_empty());
    assert!(theme.problems().is_empty());
}

#[test]
fn child_theme_chain_and_lookup() {
    let theme = child_theme();
    assert_eq!(chain_names(&theme), ["child-theme", "default"]);
    for (path, expected) in [
        ("partials/head.html", "child-theme"),
        ("document.html", "default"),
        ("@default/partials/head.html", "default"),
    ] {
        assert_eq!(theme.read_template(path).unwrap().unwrap().0, expected);
    }
    assert_eq!(
        origins(&theme.style_entries().unwrap()),
        [
            ("default", "styles/theme.css"),
            ("default", "styles/notebook.css"),
            ("default", "styles/print.css"),
            ("child-theme", "styles/child.css"),
        ]
    );
    assert_eq!(
        origins(&theme.script_entries().unwrap()),
        [("child-theme", "scripts/child.js")]
    );
    assert_eq!(theme.vars()["accent"], "#aa3300");
    let (light, dark) = theme.code_themes().unwrap();
    assert_eq!(light.unwrap().label, "Nord");
    assert_eq!(dark.unwrap().label, "OneHalfDark");
    assert_eq!(
        theme.watch_dirs(),
        [fs::canonicalize(fixtures().join("child-theme")).unwrap()]
    );
    assert!(theme.problems().is_empty());
}

#[test]
fn resolution_errors_are_theme_errors() {
    let tmp = tempfile::tempdir().unwrap();
    for (dir, manifest) in [
        ("a", "name = 'cycle-a'\nextends = '../b'"),
        ("b", "name = 'cycle-b'\nextends = '../a'"),
    ] {
        write_files(&tmp.path().join(dir), &[("theme.toml", manifest)]);
    }
    let (_, message) = theme_error(resolve("a", &[tmp.path()]));
    assert!(message.contains("cycle"), "{message}");
    assert!(message.contains("cycle-a") && message.contains("cycle-b"));
    write_files(&tmp.path().join("root"), &[("theme.toml", "name = 'root'")]);
    let (_, message) = theme_error(resolve("root", &[tmp.path()]));
    assert!(message.contains("templates/document.html"), "{message}");
    write_files(
        &tmp.path().join("child"),
        &[
            ("theme.toml", "name = 'child'\nextends = '../root'"),
            ("templates/document.html", "A child cannot repair its root"),
        ],
    );
    let (_, message) = theme_error(resolve("child", &[tmp.path()]));
    assert!(message.contains("templates/document.html"), "{message}");
}

#[test]
fn disk_chain_merges_and_concatenates_parent_first() {
    let tmp = tempfile::tempdir().unwrap();
    let theme = disk_chain(tmp.path());
    assert_eq!(chain_names(&theme), ["child", "root"]);
    assert_eq!(
        Value::Object(theme.vars()),
        json!({
            "accent": "blue", "font": "serif", "nested": {"a": 1, "b": 3, "c": 4},
        })
    );
    let styles = theme.style_entries().unwrap();
    assert_eq!(
        origins(&styles),
        [
            ("root", "styles/a.css"),
            ("root", "styles/b.css"),
            ("child", "styles/c.css"),
        ]
    );
    assert_eq!(
        styles.iter().map(|f| f.bytes.as_slice()).collect::<Vec<_>>(),
        [b"a".as_slice(), b"b", b"c"]
    );
    assert_eq!(
        origins(&theme.script_entries().unwrap()),
        [("root", "scripts/r.js"), ("child", "scripts/c.js")]
    );
    let (light, dark) = theme.code_themes().unwrap();
    assert_eq!(light.unwrap().label, "Nord");
    assert!(dark.is_none());
}

#[test]
fn presenter_keys_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    for (entry, expected) in [
        ("[styles]\nhighlight_style = 'monokai'", "code_theme"),
        ("[styles]\nreveal_theme = 'white'", "has no meaning in writer"),
        ("[reveal]\ncontrols = true", "has no meaning in writer"),
        ("[layouts]\nblank = 'Blank'", "has no meaning in writer"),
    ] {
        write_root_theme(
            &tmp.path().join("t"),
            &format!("name = 't'\n{entry}"),
            &[],
        );
        let (_, message) = theme_error(resolve("t", &[tmp.path()]));
        assert!(message.contains(expected), "{entry}: {message}");
    }
}

const TM_THEME: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>name</key><string>Tiny</string>
<key>settings</key><array><dict><key>settings</key><dict>
<key>background</key><string>#FFFFFF</string>
<key>foreground</key><string>#111111</string>
</dict></dict><dict><key>scope</key><string>comment</string>
<key>settings</key><dict><key>foreground</key><string>#888888</string>
</dict></dict></array></dict></plist>"#;

fn code_theme(dir: &Path, value: &str) -> Theme {
    child_of_default(
        dir,
        &format!("[styles]\ncode_theme = {}\n", json!(value)),
        &[],
    )
}

#[test]
fn code_theme_values_follow_the_rules() {
    let tmp = tempfile::tempdir().unwrap();
    for extension in ["tmTheme", "TMTHEME"] {
        // A fresh directory preserves the requested case on macOS as well.
        let tmp = tempfile::tempdir().unwrap();
        let path = format!("styles/code.{extension}");
        let theme = code_theme(tmp.path(), &path);
        write_files(&tmp.path().join("t"), &[(&path, TM_THEME)]);
        let (light, dark) = theme.code_themes().unwrap();
        assert_eq!(light.unwrap().label, format!("@t/{path}"));
        assert_eq!(dark.unwrap().label, "OneHalfDark");
    }
    let theme = code_theme(tmp.path(), "none");
    assert!(theme.code_themes().unwrap().0.is_none());
    for (value, expected) in [
        ("Nrod", "did you mean 'Nord'?"),
        ("ansi", "terminal-only theme"),
        ("base16", "terminal-only theme"),
        ("base16-256", "terminal-only theme"),
        ("missing.tmTheme", "not found"),
        ("../styles/code.tmTheme", "must stay inside"),
    ] {
        let error = code_theme(tmp.path(), value).code_themes().unwrap_err();
        let Error::Theme { message, .. } = error else {
            panic!("{error:?}")
        };
        assert!(message.contains(expected), "{value}: {message}");
    }
    write_files(&tmp.path().join("t"), &[("broken.tmTheme", "garbage")]);
    assert!(matches!(
        code_theme(tmp.path(), "broken.tmTheme").code_themes(),
        Err(Error::Theme { .. })
    ));
}

#[test]
fn problems_find_what_a_build_fails_on() {
    let tmp = tempfile::tempdir().unwrap();
    let theme = child_of_default(
        tmp.path(),
        r#"
[styles]
entries = ["styles/missing.css", "styles/theme.scss"]
code_theme = "Nrod"
[scripts]
entries = ["scripts/missing.js"]
"#,
        &[
            ("styles/theme.scss", "body {}"),
            ("templates/document.html", "{% broken %}"),
            ("templates/unused.html", "{% also_broken %}"),
        ],
    );
    let problems = theme.problems();
    for (code, suffix, level) in [
        ("theme-style-entry", "theme.toml", Level::Error),
        ("theme-script-entry", "theme.toml", Level::Error),
        ("theme-code-theme", "theme.toml", Level::Error),
        ("theme-template", "document.html", Level::Error),
        ("theme-template", "unused.html", Level::Warning),
    ] {
        assert!(
            problems.iter().any(|d| d.code == code
                && d.level == level
                && d.path.as_ref().is_some_and(|path| path.ends_with(suffix))
                && d.span.as_ref().is_some_and(|span| span.start_line >= 1)),
            "{code}/{suffix}: {problems:?}"
        );
    }
    assert_eq!(
        problems.iter().filter(|d| d.code == "theme-style-entry").count(),
        2
    );
    assert!(
        problems
            .iter()
            .any(|d| d.message.contains("SCSS themes arrive with T79"))
    );
    assert!(
        problems
            .iter()
            .any(|d| d.message.contains("the build does not load it"))
    );
}

#[test]
fn disk_themes_are_read_fresh_and_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let theme = child_of_default(
        tmp.path(),
        "[styles]\nentries = ['styles/a.css']",
        &[
            ("styles/a.css", "before"),
            ("assets/logo.svg", "logo"),
            ("templates/partial.html", "partial"),
        ],
    );
    let (_, files) = recording_reads(|| {
        theme.read_template("partial.html").unwrap();
        theme.style_entries().unwrap();
        theme.read_asset("logo.svg").unwrap();
    });
    for path in ["templates/partial.html", "styles/a.css", "assets/logo.svg"] {
        assert!(files.contains(
            &fs::canonicalize(tmp.path().join("t").join(path)).unwrap()
        ));
    }
    write_files(&tmp.path().join("t"), &[("styles/a.css", "after")]);
    let entries = theme.style_entries().unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.theme == "t" && entry.rel_path == "styles/a.css")
        .unwrap();
    assert_eq!(entry.bytes, b"after");
    assert!(recording_reads(|| ()).1.is_empty());
}

#[test]
fn unknown_theme_lists_the_search_roots() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let (name, message) =
        theme_error(resolve("nope", &[first.path(), second.path()]));
    assert_eq!(name, "nope");
    assert!(message.contains("not found"), "{message}");
    for root in [first.path(), second.path()] {
        let tried = root.join("nope").display().to_string();
        assert!(message.contains(&tried), "{tried} missing in: {message}");
    }
    assert!(message.contains("built-in themes: default"), "{message}");

    let (_, message) = theme_error(resolve("nope", &[]));
    assert!(message.contains("no search directories"), "{message}");
}

#[test]
fn coinciding_search_roots_are_tried_and_reported_once() {
    // The pipeline passes config dir, source dir and CWD, which are often the same.
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("nope")).unwrap();
    let roots = [tmp.path(), tmp.path(), tmp.path()];
    let (_, message) = theme_error(resolve("nope", &roots));
    let tried = tmp.path().join("nope").display().to_string();
    assert_eq!(message.matches(&tried).count(), 2, "listed + note: {message}");
    assert_eq!(
        message.matches("exists but has no theme.toml").count(),
        1,
        "{message}"
    );

    // Relative roots are reported as absolute directories, once.
    let spec = "no-such-theme-dir";
    let (_, message) =
        theme_error(resolve(spec, &[Path::new(""), Path::new(".")]));
    let absolute = std::env::current_dir().unwrap().join(spec);
    assert!(
        message.contains(&format!("in: {}; built-in", absolute.display())),
        "{message}"
    );
}

#[test]
fn directory_without_manifest_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("mytheme")).unwrap();
    let (_, message) = theme_error(resolve("mytheme", &[tmp.path()]));
    assert!(message.contains("exists but has no theme.toml"), "{message}");
}

#[test]
fn wrong_case_manifest_is_not_a_manifest_on_any_platform() {
    // A complete theme except that its manifest is `Theme.toml`: on macOS `is_file` finds
    // it case-insensitively, but theme files are matched exactly (as on Linux), so this is
    // the regular "no manifest" error with a hint, never a failed read.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("t");
    write_root_theme(&dir, "name = \"t\"", &[]);
    fs::rename(dir.join("theme.toml"), dir.join("Theme.toml")).unwrap();
    let (name, message) = theme_error(resolve("t", &[tmp.path()]));
    assert_eq!(name, "t");
    assert!(message.starts_with("not found;"), "{message}");
    assert!(
        message.contains(&format!(
            "{} exists but has no theme.toml; it has Theme.toml, but the name must be \
             theme.toml exactly",
            dir.display()
        )),
        "{message}"
    );
    assert!(!message.contains("disappeared"), "{message}");

    // A wrong-case directory named like a built-in theme falls back to the built-in one.
    let shadow = tmp.path().join("default");
    write_root_theme(&shadow, "name = \"default\"", &[]);
    fs::rename(shadow.join("theme.toml"), shadow.join("THEME.TOML")).unwrap();
    let theme = resolve("default", &[tmp.path()]).unwrap();
    assert_eq!(
        theme.chain[0].source,
        ThemeSource::Embedded { prefix: "themes/default/".into() }
    );
}

// ---------------------------------------------------------------------------------------
// The child-theme fixture

#[test]
fn unknown_parent_and_bad_manifests_are_theme_errors() {
    let tmp = tempfile::tempdir().unwrap();
    write_files(
        &tmp.path().join("orphan"),
        &[("theme.toml", "name = \"orphan\"\nextends = \"no-such-parent\"")],
    );
    let (name, message) = theme_error(resolve("orphan", &[tmp.path()]));
    assert_eq!(name, "no-such-parent");
    assert!(message.contains("extended by theme 'orphan'"), "{message}");

    write_files(
        &tmp.path().join("typo"),
        &[("theme.toml", "name = \"typo\"\n\n[styles]\nentrys = []\n")],
    );
    let (name, message) = theme_error(resolve("typo", &[tmp.path()]));
    assert_eq!(name, "typo");
    assert!(message.contains("theme.toml"), "{message}");
    assert!(message.contains("line 4"), "{message}");
    assert!(message.contains("did you mean 'entries'"), "{message}");
}

#[test]
fn duplicate_names_in_a_chain_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    write_files(
        &tmp.path().join("mine"),
        &[("theme.toml", "name = \"default\"\nextends = \"default\"")],
    );
    let (_, message) = theme_error(resolve("mine", &[tmp.path()]));
    assert!(
        message.contains("two themes in the chain are named 'default'"),
        "{message}"
    );
}

#[test]
fn theme_names_must_work_in_at_template_names() {
    let tmp = tempfile::tempdir().unwrap();
    for (dir, name) in
        [("slash", "acme/dark"), ("at", "@acme"), ("space", " acme")]
    {
        write_root_theme(
            &tmp.path().join(dir),
            &format!("name = {}", json!(name)),
            &[],
        );
        let (_, message) = theme_error(resolve(dir, &[tmp.path()]));
        assert!(message.contains("invalid theme name"), "{name}: {message}");
        assert!(message.contains("theme.toml"), "{message}");
    }
}

#[test]
fn chains_longer_than_eight_themes_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    write_root_theme(&tmp.path().join("t0"), r#"name = "t0""#, &[]);
    for i in 1..=8 {
        let manifest = format!("name = \"t{i}\"\nextends = \"../t{}\"", i - 1);
        write_files(
            &tmp.path().join(format!("t{i}")),
            &[("theme.toml", &manifest)],
        );
    }
    // t7 → … → t0 is eight themes: allowed.
    let theme = resolve("t7", &[tmp.path()]).unwrap();
    assert_eq!(theme.chain.len(), 8);
    assert_eq!(theme.watch_dirs().len(), 8);
    // t8 → … → t0 is nine.
    let (_, message) = theme_error(resolve("t8", &[tmp.path()]));
    assert!(message.contains("longer than 8"), "{message}");
}

// ---------------------------------------------------------------------------------------
// Disk chains

#[test]
fn assets_are_looked_up_child_first() {
    let tmp = tempfile::tempdir().unwrap();
    let theme = disk_chain(tmp.path());
    assert_eq!(
        theme.read_asset("logo.svg").unwrap(),
        Some(b"child logo".to_vec())
    );
    assert_eq!(
        theme.read_asset("./fonts/x.woff2").unwrap(),
        Some(b"root font".to_vec())
    );
    assert_eq!(theme.read_asset("nope.png").unwrap(), None);
    assert_eq!(
        theme.read_asset("../theme.toml").unwrap(),
        None,
        "stays in assets/"
    );
    assert_eq!(theme.read_asset("../../root/theme.toml").unwrap(), None);
}

#[test]
fn missing_style_entry_is_a_theme_error() {
    let tmp = tempfile::tempdir().unwrap();
    disk_chain(tmp.path());
    fs::remove_file(tmp.path().join("child/styles/c.css")).unwrap();
    let theme = resolve("child", &[tmp.path()]).unwrap();
    match theme.style_entries() {
        Err(Error::Theme { name, message }) => {
            assert_eq!(name, "child");
            assert!(message.contains("styles/c.css"), "{message}");
        }
        other => panic!("expected Error::Theme, got {other:?}"),
    }
}

#[test]
fn entries_outside_the_theme_directory_are_reported_as_such() {
    let tmp = tempfile::tempdir().unwrap();
    disk_chain(tmp.path());
    // The file exists, but entries must stay inside their theme's directory.
    write_files(tmp.path(), &[("shared/base.css", "shared")]);
    let child = tmp.path().join("child/theme.toml");
    let manifest = fs::read_to_string(&child).unwrap();
    let absolute = tmp.path().join("shared/base.css").display().to_string();
    for (entry, problem) in [
        ("../shared/base.css", "must stay inside the theme directory"),
        (absolute.as_str(), "must be a relative path"),
        ("styles/..", "does not name a file"),
    ] {
        let edited =
            manifest.replace("\"styles/c.css\"", &json!(entry).to_string());
        fs::write(&child, edited).unwrap();
        let theme = resolve("child", &[tmp.path()]).unwrap();
        match theme.style_entries() {
            Err(Error::Theme { name, message }) => {
                assert_eq!(name, "child");
                assert!(message.contains(entry), "{message}");
                assert!(message.contains(problem), "{entry}: {message}");
                assert!(!message.contains("not found"), "{message}");
            }
            other => panic!("expected Error::Theme for {entry}, got {other:?}"),
        }
    }
}

#[test]
fn disk_lookups_match_file_names_exactly() {
    // On a case-insensitive file system (macOS) `is_file` would accept these; the embedded
    // tree and Linux do not, so disk themes must not either.
    let tmp = tempfile::tempdir().unwrap();
    let theme = disk_chain(tmp.path());
    assert!(theme.has_template("layouts/two-up.html"));
    assert!(!theme.has_template("layouts/TWO-UP.html"));
    assert!(!theme.has_template("Layouts/two-up.html"));
    assert_eq!(theme.read_template("layouts/Two-Up.html").unwrap(), None);
    assert!(!theme.has_template("@child/layouts/TWO-UP.html"));
    assert_eq!(theme.read_asset("LOGO.svg").unwrap(), None);
    assert_eq!(
        theme.read_asset("Fonts/x.woff2").unwrap(),
        None,
        "directories too"
    );
    assert!(theme.chain[0].list("TEMPLATES").is_empty());
    assert!(!theme.chain[0].exists("STYLES/c.css"));

    let child = child_theme();
    assert!(!child.has_template("layouts/TITLE.html"));
    assert!(!child.has_template("@default/layouts/TITLE.html"));
}

#[test]
fn disk_themes_are_read_fresh_on_every_call() {
    let tmp = tempfile::tempdir().unwrap();
    let theme = disk_chain(tmp.path());
    let section = tmp.path().join("root/templates/layouts/section.html");
    assert_eq!(
        theme.read_template("layouts/section.html").unwrap(),
        Some(("root".to_string(), "root section".to_string()))
    );
    let env = theme.environment(false).unwrap();
    assert_eq!(
        env.render_str(r#"{% include "layouts/section.html" %}"#, ()).unwrap(),
        "root section"
    );

    fs::write(&section, "edited section").unwrap();
    assert_eq!(
        theme.read_template("layouts/section.html").unwrap().unwrap().1,
        "edited section"
    );
    let env = theme.environment(false).unwrap();
    assert_eq!(
        env.render_str(r#"{% include "layouts/section.html" %}"#, ()).unwrap(),
        "edited section"
    );

    // A file added to the child now shadows the parent's.
    write_files(
        &tmp.path().join("child"),
        &[("templates/layouts/section.html", "child section")],
    );
    assert_eq!(
        theme.read_template("layouts/section.html").unwrap().unwrap(),
        ("child".to_string(), "child section".to_string())
    );
    fs::write(tmp.path().join("root/styles/a.css"), "a2").unwrap();
    assert_eq!(theme.style_entries().unwrap()[0].bytes, b"a2");
}

#[test]
fn child_templates_can_extend_the_parent_version_by_theme_name() {
    let tmp = tempfile::tempdir().unwrap();
    disk_chain(tmp.path());
    write_files(
        &tmp.path().join("root"),
        &[(
            "templates/layouts/default.html",
            "<section>{% block content %}root content{% endblock %}</section>",
        )],
    );
    write_files(
        &tmp.path().join("child"),
        &[(
            "templates/layouts/default.html",
            r#"{% extends "@root/layouts/default.html" %}{% block content %}child: {{ super() }} {{ x }}{% endblock %}"#,
        )],
    );
    let theme = resolve("child", &[tmp.path()]).unwrap();
    let env = theme.environment(false).unwrap();
    let html = env
        .get_template("layouts/default.html")
        .unwrap()
        .render(json!({ "x": "<&>" }))
        .unwrap();
    assert_eq!(html, "<section>child: root content &lt;&amp;&gt;</section>");
}

// ---------------------------------------------------------------------------------------
// Audit fixes (A4): symbolic links, `Theme::problems`, "did you mean"

/// A child theme `t` of `default` in `dir/t` with `manifest_extra` appended to its manifest.
fn child_of_default(
    dir: &Path, manifest_extra: &str, files: &[(&str, &str)],
) -> Theme {
    let manifest =
        format!("name = \"t\"\nextends = \"default\"\n{manifest_extra}");
    write_files(&dir.join("t"), &[("theme.toml", manifest.as_str())]);
    write_files(&dir.join("t"), files);
    resolve("t", &[dir]).unwrap()
}

/// F08: a symbolic link inside a disk theme whose target is outside it finds nothing (read,
/// exists, list, assets); links that stay inside the theme work.
#[cfg(unix)]
#[test]
fn symbolic_links_out_of_a_theme_are_not_followed() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    write_files(
        tmp.path(),
        &[("secret/hosts", "SECRET"), ("secret/b.css", "x{}")],
    );
    let theme = child_of_default(
        tmp.path(),
        "[styles]\nentries = [\"styles/theme.css\"]\n",
        &[
            (
                "styles/theme.css",
                "body { background: url(../assets/logo.png); }",
            ),
            ("assets/real.png", "REAL"),
        ],
    );
    let root = tmp.path().join("t");
    symlink(tmp.path().join("secret/hosts"), root.join("assets/logo.png"))
        .unwrap();
    symlink(tmp.path().join("secret"), root.join("assets/shared")).unwrap();
    symlink(root.join("assets/real.png"), root.join("assets/alias.png"))
        .unwrap();
    symlink(tmp.path().join("secret/b.css"), root.join("styles/linked.css"))
        .unwrap();
    let dir = &theme.chain[0];
    for rel in ["assets/logo.png", "assets/shared/hosts", "styles/linked.css"] {
        assert_eq!(dir.read(rel).unwrap(), None, "{rel}");
        assert!(!dir.exists(rel), "{rel}");
    }
    assert_eq!(theme.read_asset("logo.png").unwrap(), None);
    assert_eq!(theme.read_asset("shared/hosts").unwrap(), None);
    // A link to a file of the theme itself is fine.
    assert_eq!(theme.read_asset("alias.png").unwrap(), Some(b"REAL".to_vec()));
    assert_eq!(
        dir.list("assets"),
        ["assets/alias.png", "assets/real.png"],
        "list() lists what read() accepts"
    );
    // A theme reached through a linked directory is that directory, and works.
    symlink(&root, tmp.path().join("t-link")).unwrap();
    let linked = resolve("t-link", &[tmp.path()]).unwrap();
    assert_eq!(linked.read_asset("real.png").unwrap(), Some(b"REAL".to_vec()));
}

/// F10 (b, review): in a helper's argument, `@<theme>/<path>` stays inside that theme:
/// `..` cannot climb out of it into the chain's `assets/` (`@t/../x.css` named
/// `assets/x.css`) or into another theme. Inside the theme, `..` works.
#[test]
fn theme_helper_paths_stay_inside_their_theme() {
    use writer::html::paths::{normalize_helper_path, resolve_helper_path};
    for (path, expected) in [
        ("@t/../x.css", None),
        ("@t/../@default/styles/theme.css", None),
        ("./@t/a/../../x.css", None),
        ("@t\\..\\x.css", None),
        ("@t/a/../b.css", Some("@t/b.css")),
        ("@t/assets/../assets/x.css", Some("@t/assets/x.css")),
        ("./@t//x.css", Some("@t/x.css")),
        ("img/../x.css", Some("x.css")),
        ("vendor/../x.css", Some("x.css")),
    ] {
        assert_eq!(normalize_helper_path(path).as_deref(), expected, "{path}");
    }
    let tmp = tempfile::tempdir().unwrap();
    write_files(tmp.path(), &[("x.css", "/* intended */")]);
    let theme =
        child_of_default(tmp.path(), "", &[("assets/x.css", "/* assets */")]);
    assert_eq!(resolve_helper_path(&theme, "@t/../x.css").unwrap(), None);
    assert_eq!(
        resolve_helper_path(&theme, "@t/assets/../assets/x.css")
            .unwrap()
            .map(|(bytes, _)| bytes),
        Some(b"/* assets */".to_vec())
    );
    assert_eq!(
        resolve_helper_path(&theme, "x.css").unwrap().map(|(bytes, _)| bytes),
        Some(b"/* assets */".to_vec())
    );
}

/// F08 (review): a file that a symbolic link takes out of its theme is reported as such:
/// `theme-link-outside` warnings for each such link (not for links inside the theme), and
/// the messages about an entry or a manifest name the link instead of saying "not found".
#[cfg(unix)]
#[test]
fn links_out_of_a_theme_are_named() {
    use std::os::unix::fs::symlink;
    use writer::error::Level::Warning as W;
    let tmp = tempfile::tempdir().unwrap();
    write_files(
        tmp.path(),
        &[("secret/hosts", "SECRET"), ("secret/b.css", "x{}"), ("t2/x", "")],
    );
    let theme = child_of_default(
        tmp.path(),
        "[styles]\nentries = [\"styles/linked.css\"]\n",
        &[("assets/real.png", "REAL")],
    );
    let root = tmp.path().join("t");
    fs::create_dir_all(root.join("styles")).unwrap();
    symlink(tmp.path().join("secret/hosts"), root.join("assets/logo.png"))
        .unwrap();
    symlink(tmp.path().join("secret"), root.join("assets/shared")).unwrap();
    symlink(root.join("assets/real.png"), root.join("assets/alias.png"))
        .unwrap();
    symlink(tmp.path().join("secret/b.css"), root.join("styles/linked.css"))
        .unwrap();
    symlink(tmp.path().join("nowhere"), root.join("assets/dangling.png"))
        .unwrap();

    let dir = &theme.chain[0];
    assert!(dir.linked_outside("assets/logo.png"));
    assert!(dir.linked_outside("assets/shared/hosts"));
    assert!(!dir.linked_outside("assets/alias.png"));
    assert!(!dir.linked_outside("assets/nope.png"));
    assert!(!dir.linked_outside("assets/dangling.png"));

    let found = theme.problems();
    let style = ("theme-style-entry".to_string(), "t/theme.toml".to_string());
    let link =
        |path: &str| ("theme-link-outside".to_string(), path.to_string());
    assert_eq!(
        problem_summary(&found, tmp.path())
            .into_iter()
            .map(|(code, path, level)| ((code, path), level))
            .collect::<Vec<_>>(),
        [
            (style, writer::error::Level::Error),
            (link("t/assets/logo.png"), W),
            (link("t/assets/shared"), W),
            (link("t/styles/linked.css"), W),
        ]
    );
    assert!(
        found[0].message.contains(
            "entry 'styles/linked.css' is a symbolic link (or below one) to a file outside \
             the theme directory"
        ),
        "{}",
        found[0].message
    );
    assert!(
        found[2]
            .message
            .starts_with("theme 't': assets/shared is a symbolic link to ")
            && found[2]
                .message
                .contains("writer ignores it and everything below it, so it counts as missing"),
        "{}",
        found[2].message
    );
    let Err(Error::Theme { message, .. }) = theme.style_entries() else {
        panic!("expected Error::Theme");
    };
    assert!(message.contains("is a symbolic link"), "{message}");

    // A manifest that is a link out of its directory.
    symlink(root.join("theme.toml"), tmp.path().join("t2/theme.toml")).unwrap();
    let (_, message) = theme_error(resolve("t2", &[tmp.path()]));
    assert!(
        message.contains("t2 has a theme.toml that is a symbolic link (or below one) to a file outside the theme directory"),
        "{message}"
    );
}
