//! Configuration contract: defaults, discovery, serde forms, and atomic overlays.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::config::*;
use writer::error::Error;
use writer::ir::{DocMeta, SourceKind};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/config/writer.toml")
}

fn meta_with(vars: Value) -> DocMeta {
    DocMeta {
        vars: vars.as_object().expect("front matter mapping").clone(),
        ..DocMeta::default()
    }
}

fn config_message(error: Error) -> String {
    match error {
        Error::Config(message) => message,
        other => panic!("expected Error::Config, got {other:?}"),
    }
}

fn front_message(error: Error) -> String {
    match error {
        Error::FrontMatter(message) => message,
        other => panic!("expected Error::FrontMatter, got {other:?}"),
    }
}

#[test]
fn default_equals_empty_toml() {
    assert_eq!(Config::default(), toml::from_str::<Config>("").unwrap());
    assert_eq!(Config::default(), Config::from_toml_str("").unwrap());
    let empty_tables = "[document]\n[build]\n[markdown]\n[numbering]\n\
        [references]\n[math]\n[html]\n[pdf]\n[docx]\n[latex]\n\
        [typst]\n[notebook]\n[serve]\n";
    assert_eq!(Config::default(), Config::from_toml_str(empty_tables).unwrap());
}

#[test]
fn defaults_match_spec_section_2() {
    assert_eq!(
        serde_json::to_value(Config::default()).unwrap(),
        json!({
            "document": {"lang": "en", "toc": false},
            "build": {"out_dir": "dist", "targets": ["html"], "typst_targets": ["pdf"]},
            "markdown": {
                "smart_punctuation": false, "sourcepos": false,
                "references": true, "superscript": false, "subscript": false
            },
            "numbering": {"headings": false, "figures": "document", "equations": "labelled"},
            "references": {"bibliography": [], "style": "ieee", "locale": ""},
            "math": {"html": "katex", "mathml_font": "embed", "katex_options": {}},
            "html": {"theme": "default", "standalone": true},
            "pdf": {
                "template": "", "paper": "a4", "engine": "typst", "allow_packages": false,
                "system_fonts": true, "font_paths": [], "timestamp": ""
            },
            "docx": {"reference_doc": "", "math": "omml", "update_fields": true},
            "latex": {
                "template": "", "top_level": "section", "cite": "biblatex",
                "emit_bibliography": false, "code": "verbatim", "tables": "longtable",
                "booktabs": true, "cleveref": false, "figure_placement": "htbp",
                "engine": "pdflatex", "unicode": "auto", "cell_prompts": false, "wrap": 0
            },
            "typst": {"strict": false, "package_paths": []},
            "notebook": {
                "mime_priority": [
                    "text/html", "image/svg+xml", "image/png", "image/jpeg", "image/gif",
                    "image/webp", "text/markdown", "text/latex", "application/json", "text/plain"
                ],
                "hide_input_tags": ["hide-input", "remove-input"],
                "hide_output_tags": ["hide-output", "remove-output"],
                "remove_cell_tags": ["remove-cell", "hide-cell", "skip"],
                "cell_options": true, "show_prompts": true
            },
            "serve": {"host": "127.0.0.1", "port": 3000, "open": false, "view": ""}
        })
    );
}

#[test]
fn default_toml_parses_to_default() {
    let text = Config::default_toml();
    assert!(text.ends_with('\n'));
    assert_eq!(Config::default_toml(), text);
    assert_eq!(Config::from_toml_str(&text).unwrap(), Config::default());
}

#[test]
fn default_toml_matches_config_md() {
    let spec = include_str!("../docs/spec/config.md");
    let block =
        spec.split_once("```toml\n").unwrap().1.split_once("```").unwrap().0;
    assert_eq!(Config::default_toml(), block);
}

#[test]
fn fixture_sets_every_key_to_a_non_default_value() {
    let configured = Config::load(&fixture()).unwrap();
    let default = serde_json::to_value(Config::default()).unwrap();
    let actual = serde_json::to_value(&configured).unwrap();
    for (table, defaults) in default.as_object().unwrap() {
        for (key, value) in defaults.as_object().unwrap() {
            assert_ne!(
                &actual[table][key], value,
                "fixture leaves {table}.{key} at its default"
            );
        }
    }
    assert_eq!(configured.build.out_dir, PathBuf::from("public"));
    assert_eq!(
        configured.pdf.template,
        PathBuf::from("templates/document.typ")
    );
    assert_eq!(
        configured.references.bibliography,
        [PathBuf::from("refs.bib"), PathBuf::from("more.yaml")]
    );
}

#[test]
fn unknown_table_suggests_build() {
    let message =
        config_message(Config::from_toml_str("[buidl]\n").unwrap_err());
    assert!(message.contains("did you mean 'build'"), "{message}");
    let message = config_message(
        Config::from_toml_str("[build]\nout_dr = 'tmp'\n").unwrap_err(),
    );
    assert!(message.contains("did you mean 'out_dir'"), "{message}");
}

#[test]
fn unknown_variant_gets_a_hint() {
    for (table, key, wrong, expected) in [
        ("math", "html", "katx", "katex"),
        ("math", "mathml_font", "systm", "system"),
        ("serve", "view", "page", "pages"),
        ("pdf", "engine", "chrom", "chrome"),
    ] {
        let text = format!("[{table}]\n{key} = '{wrong}'\n");
        let message = config_message(Config::from_toml_str(&text).unwrap_err());
        assert!(
            message.contains(&format!("did you mean '{expected}'")),
            "{message}"
        );
    }
}

#[test]
fn invalid_variants_and_table_keys_are_rejected() {
    for (table, key) in [
        ("build", "targets"),
        ("numbering", "figures"),
        ("numbering", "equations"),
        ("math", "html"),
        ("math", "mathml_font"),
        ("pdf", "engine"),
        ("docx", "math"),
        ("latex", "top_level"),
        ("latex", "cite"),
        ("latex", "code"),
        ("latex", "tables"),
        ("latex", "engine"),
        ("latex", "unicode"),
        ("serve", "view"),
    ] {
        let value = if key == "targets" {
            "['bogus']"
        } else {
            "'bogus'"
        };
        let message = config_message(
            Config::from_toml_str(&format!("[{table}]\n{key} = {value}\n"))
                .unwrap_err(),
        );
        assert!(
            message.contains("unknown variant `bogus`"),
            "{table}.{key}: {message}"
        );
    }
    for table in [
        "document", "build", "markdown", "numbering", "references", "math",
        "html", "pdf", "docx", "latex", "typst", "notebook", "serve",
    ] {
        assert!(matches!(
            Config::from_toml_str(&format!("[{table}]\nunknown = 1\n")),
            Err(Error::Config(_))
        ));
    }
}

#[test]
fn toc_and_heading_numbering_forms() {
    for (value, expected, canonical) in [
        (json!(false), Toc::Off, json!(false)),
        (json!(true), Toc::Depth(3), json!(3)),
        (json!(1), Toc::Depth(1), json!(1)),
        (json!(6), Toc::Depth(6), json!(6)),
    ] {
        assert_eq!(
            serde_json::from_value::<Toc>(value.clone()).unwrap(),
            expected
        );
        let text = format!("[document]\ntoc = {value}\n");
        assert_eq!(
            Config::from_toml_str(&text).unwrap().document.toc,
            expected
        );
        assert_eq!(serde_json::to_value(expected).unwrap(), canonical);
    }
    for value in [
        json!(0),
        json!(7),
        json!(-1),
        json!(1.5),
        json!("true"),
        json!([]),
        json!({}),
        Value::Null,
    ] {
        let error = serde_json::from_value::<Toc>(value.clone())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("false, true") && error.contains("1..=6"),
            "{value}: {error}"
        );
        if value != Value::Null {
            assert!(
                Config::from_toml_str(&format!("[document]\ntoc = {value}\n"))
                    .is_err(),
                "{value}"
            );
        }
    }
    for (value, expected, canonical) in [
        (json!(false), HeadingNumbering::Off, json!(false)),
        (json!(true), HeadingNumbering::Pattern("1.1".into()), json!("1.1")),
        (json!("A.1"), HeadingNumbering::Pattern("A.1".into()), json!("A.1")),
        (json!(""), HeadingNumbering::Pattern("".into()), json!("")),
    ] {
        assert_eq!(
            serde_json::from_value::<HeadingNumbering>(value.clone()).unwrap(),
            expected
        );
        assert_eq!(
            Config::from_toml_str(&format!(
                "[numbering]\nheadings = {value}\n"
            ))
            .unwrap()
            .numbering
            .headings,
            expected
        );
        assert_eq!(serde_json::to_value(&expected).unwrap(), canonical);
    }
    for value in [json!(2), json!(1.5), json!([]), json!({}), Value::Null] {
        assert!(
            serde_json::from_value::<HeadingNumbering>(value.clone()).is_err(),
            "{value}"
        );
        if value != Value::Null {
            assert!(
                Config::from_toml_str(&format!(
                    "[numbering]\nheadings = {value}\n"
                ))
                .is_err()
            );
        }
    }
}

#[test]
fn serve_view_empty_is_none() {
    for (name, expected) in [
        ("", None),
        ("web", Some(ServeView::Web)),
        ("pages", Some(ServeView::Pages)),
    ] {
        let config =
            Config::from_toml_str(&format!("[serve]\nview = '{name}'\n"))
                .unwrap();
        assert_eq!(config.serve.view, expected);
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["serve"]["view"], json!(name));
        assert_eq!(serde_json::from_value::<Config>(value).unwrap(), config);
    }
    assert_eq!(Config::default().serve.view, None);
    assert!(Config::from_toml_str("[serve]\nview = 'Web'\n").is_err());
    assert!(Config::from_toml_str("[serve]\nview = false\n").is_err());
}

#[test]
fn load_error_names_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("writer.toml");
    let message = config_message(Config::load(&path).unwrap_err());
    assert!(message.contains(&path.display().to_string()), "{message}");
    std::fs::write(&path, "[buidl]\n").unwrap();
    let message = config_message(Config::load(&path).unwrap_err());
    assert!(message.starts_with(&format!("{}: ", path.display())), "{message}");
    assert!(message.contains("did you mean 'build'"), "{message}");
}

#[test]
fn discover_walks_up_and_stops_at_the_first_file() {
    let root = tempfile::tempdir().unwrap();
    let top = root.path().join("project");
    let nested = top.join("documents/2026/report");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(top.join(CONFIG_FILE_NAME), "[serve]\nport = 4000\n")
        .unwrap();
    let (path, config) = Config::discover(&nested).unwrap().expect("found");
    assert_eq!(path, top.join(CONFIG_FILE_NAME));
    assert_eq!(config.serve.port, 4000);
    let middle = top.join("documents");
    std::fs::write(middle.join(CONFIG_FILE_NAME), "[serve]\nport = 5000\n")
        .unwrap();
    let (path, config) = Config::discover(&nested).unwrap().expect("found");
    assert_eq!(path, middle.join(CONFIG_FILE_NAME));
    assert_eq!(config.serve.port, 5000);
    let dotted = nested.join("../../2026/report/.");
    assert_eq!(Config::discover(&dotted).unwrap().unwrap().0, path);
    std::fs::write(middle.join(CONFIG_FILE_NAME), "[buidl]\n").unwrap();
    let message = config_message(Config::discover(&nested).unwrap_err());
    assert!(message.contains("did you mean 'build'"), "{message}");
}

#[test]
fn discover_without_a_file_is_none_or_outside() {
    let root = tempfile::tempdir().unwrap();
    if let Some((path, _)) = Config::discover(root.path()).unwrap() {
        assert!(!path.starts_with(root.path()), "{}", path.display());
    }
}

#[test]
fn fixture_inputs_do_not_discover_a_repo_config() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    for dir in ["tests/fixtures/md", "tests/fixtures/ir", "tests/fixtures"] {
        if let Some((path, _)) = Config::discover(&repo.join(dir)).unwrap() {
            assert!(
                !path.starts_with(repo),
                "{dir} discovers {}",
                path.display()
            );
        }
    }
}

#[test]
fn html_theme_precedence_covers_every_pair() {
    for mask in 0u8..8 {
        let mut config = if mask & 1 != 0 {
            Config::from_toml_str("[html]\ntheme = 'file'\n").unwrap()
        } else {
            Config::default()
        };
        if mask & 2 != 0 {
            config
                .merge_front_matter(&meta_with(
                    json!({"theme": "front matter"}),
                ))
                .unwrap();
        }
        if mask & 4 != 0 {
            config.apply_cli(&CliOverrides {
                theme: Some("cli".into()),
                ..CliOverrides::default()
            });
        }
        let expected = if mask & 4 != 0 {
            "cli"
        } else if mask & 2 != 0 {
            "front matter"
        } else if mask & 1 != 0 {
            "file"
        } else {
            "default"
        };
        assert_eq!(config.html.theme, expected, "sources {mask:03b}");
    }
}

#[test]
fn front_matter_keys_map_to_tables() {
    let meta = meta_with(json!({
        "toc": true,
        "markdown": {"sourcepos": true, "references": false},
        "math": {"html": "image", "mathml_font": "system", "katex_options": {"throwOnError": false}},
        "numbering": {"headings": "1.1.a", "figures": "section", "equations": "none"},
        "references": {"style": "apa", "locale": "de"},
        "html": {"standalone": false},
        "pdf": {"allow_packages": true, "system_fonts": false},
        "docx": {"math": "image", "update_fields": false},
        "latex": {"engine": "lualatex", "unicode": "map"},
        "typst": {"strict": true, "package_paths": ["packages"]},
        "notebook": {"show_prompts": false, "remove_cell_tags": ["gone"]},
        "bibliography": ["refs.bib", "more.yaml"], "cite_style": "chicago", "cite_locale": "fr",
        "theme": "front-theme", "template": "front.typ", "paper": "us-letter",
        "reference_doc": "front.docx", "latex_template": "front.tex",
        "venue": "PyData", "serve": {"port": 1}, "build": {"out_dir": "ignored"}
    }));
    let before = meta.clone();
    let mut config = Config::default();
    config.merge_front_matter(&meta).unwrap();
    assert_eq!(meta, before);
    assert_eq!(config.document.toc, Toc::Depth(3));
    assert!(config.markdown.sourcepos && !config.markdown.references);
    assert_eq!(config.math.html, HtmlMath::Image);
    assert_eq!(config.math.mathml_font, MathmlFont::System);
    assert_eq!(config.math.katex_options["throwOnError"], json!(false));
    assert_eq!(
        config.numbering.headings,
        HeadingNumbering::Pattern("1.1.a".into())
    );
    assert_eq!(config.numbering.figures, FigureNumbering::Section);
    assert_eq!(config.numbering.equations, EquationNumbering::None);
    assert_eq!(config.references.style, "chicago");
    assert_eq!(config.references.locale, "fr");
    assert_eq!(
        config.references.bibliography,
        [PathBuf::from("refs.bib"), PathBuf::from("more.yaml")]
    );
    assert_eq!(config.html.theme, "front-theme");
    assert!(!config.html.standalone);
    assert_eq!(config.pdf.template, PathBuf::from("front.typ"));
    assert_eq!(config.pdf.paper, "us-letter");
    assert!(config.pdf.allow_packages && !config.pdf.system_fonts);
    assert_eq!(config.docx.reference_doc, PathBuf::from("front.docx"));
    assert_eq!(config.docx.math, DocxMath::Image);
    assert!(!config.docx.update_fields);
    assert_eq!(config.latex.template, PathBuf::from("front.tex"));
    assert_eq!(config.latex.engine, LatexEngine::Lualatex);
    assert_eq!(config.latex.unicode, LatexUnicode::Map);
    assert!(config.typst.strict);
    assert_eq!(config.typst.package_paths, [PathBuf::from("packages")]);
    assert!(!config.notebook.show_prompts);
    assert_eq!(config.notebook.remove_cell_tags, ["gone"]);
    assert_eq!(
        config.notebook.hide_input_tags,
        NotebookConfig::default().hide_input_tags
    );
    assert_eq!(config.serve, ServeConfig::default());
    assert_eq!(config.build, BuildConfig::default());
    config
        .merge_front_matter(&meta_with(
            json!({"csl": "ieee", "bibliography": "single.bib"}),
        ))
        .unwrap();
    assert_eq!(config.references.style, "ieee");
    assert_eq!(config.references.bibliography, [PathBuf::from("single.bib")]);
}

#[test]
fn front_matter_dedicated_key_beats_table_map() {
    // Dedicated keys replace conflicting map values before type validation.
    let meta = meta_with(json!({
        "references": {"style": 4, "bibliography": false, "locale": []},
        "html": {"theme": false}, "pdf": {"template": 8, "paper": null},
        "docx": {"reference_doc": 2}, "latex": {"template": false},
        "theme": "dedicated", "template": "dedicated.typ", "paper": "a5",
        "reference_doc": "dedicated.docx", "latex_template": "dedicated.tex",
        "bibliography": "dedicated.bib", "cite_style": "apa", "cite_locale": "de"
    }));
    let mut config = Config::default();
    config.merge_front_matter(&meta).unwrap();
    assert_eq!(config.html.theme, "dedicated");
    assert_eq!(config.pdf.template, PathBuf::from("dedicated.typ"));
    assert_eq!(config.pdf.paper, "a5");
    assert_eq!(config.docx.reference_doc, PathBuf::from("dedicated.docx"));
    assert_eq!(config.latex.template, PathBuf::from("dedicated.tex"));
    assert_eq!(
        config.references.bibliography,
        [PathBuf::from("dedicated.bib")]
    );
    assert_eq!(config.references.style, "apa");
    assert_eq!(config.references.locale, "de");
    let mut alias = Config::default();
    alias
        .merge_front_matter(&meta_with(
            json!({"references": {"style": "map"}, "csl": "alias"}),
        ))
        .unwrap();
    assert_eq!(alias.references.style, "alias");
}

#[test]
fn front_matter_numbering_bool_sets_headings() {
    let mut config = Config::load(&fixture()).unwrap();
    for flag in [false, true] {
        config
            .merge_front_matter(&meta_with(json!({"numbering": flag})))
            .unwrap();
        let expected = if flag {
            HeadingNumbering::Pattern("1.1".into())
        } else {
            HeadingNumbering::Off
        };
        assert_eq!(config.numbering.headings, expected);
        assert_eq!(config.numbering.figures, FigureNumbering::Section);
        assert_eq!(config.numbering.equations, EquationNumbering::All);
    }
}

#[test]
fn front_matter_non_mapping_table_is_an_error() {
    let before = Config::load(&fixture()).unwrap();
    for table in [
        "markdown", "math", "numbering", "references", "html", "pdf", "docx",
        "latex", "typst", "notebook",
    ] {
        for value in [json!(false), json!(1), json!("no"), json!([])] {
            if table == "numbering" && value == json!(false) {
                continue;
            }
            let meta = meta_with(json!({table: value}));
            let mut config = before.clone();
            assert_eq!(
                front_message(config.merge_front_matter(&meta).unwrap_err()),
                format!("{table}: expected a mapping")
            );
            assert_eq!(config, before);
        }
        let mut config = before.clone();
        config.merge_front_matter(&meta_with(json!({table: null}))).unwrap();
        assert_eq!(config, before, "null {table} is an empty overlay");
    }
}

#[test]
fn front_matter_unknown_key_is_an_error_with_hint_and_no_change() {
    for (table, typo, value, expected) in [
        ("markdown", "sourceps", json!(true), "sourcepos"),
        ("math", "htm", json!("image"), "html"),
        ("numbering", "headngs", json!(true), "headings"),
        ("references", "stle", json!("apa"), "style"),
        ("html", "them", json!("default"), "theme"),
        ("pdf", "papr", json!("a4"), "paper"),
        ("docx", "update_fileds", json!(true), "update_fields"),
        ("latex", "top_levl", json!("part"), "top_level"),
        ("typst", "strct", json!(true), "strict"),
        ("notebook", "cell_optins", json!(true), "cell_options"),
    ] {
        let mut vars =
            json!({"markdown": {"sourcepos": true}, "theme": "changed"});
        vars[table] = json!({typo: value});
        let meta = meta_with(vars);
        let original_meta = meta.clone();
        let mut config = Config::default();
        let message =
            front_message(config.merge_front_matter(&meta).unwrap_err());
        assert!(message.starts_with(&format!("{table}:")), "{message}");
        assert!(
            message.contains(&format!("did you mean '{expected}'")),
            "{message}"
        );
        assert_eq!(
            config,
            Config::default(),
            "failure must roll back all earlier tables"
        );
        assert_eq!(meta, original_meta);
    }
}

#[test]
fn front_matter_invalid_values_and_alias_conflicts_roll_back() {
    let before = Config::load(&fixture()).unwrap();
    for vars in [
        json!({"toc": 0}),
        json!({"toc": "yes"}),
        json!({"bibliography": false}),
        json!({"bibliography": ["refs.bib", 2]}),
        json!({"cite_style": "apa", "csl": "ieee"}),
        json!({"markdown": {"sourcepos": false}, "latex_template": 2}),
        json!({"markdown": {"sourcepos": false}, "notebook": {"cell_options": "yes"}}),
        json!({"references": {"style": false}}),
        json!({"theme": null}),
    ] {
        let meta = meta_with(vars);
        let original_meta = meta.clone();
        let mut config = before.clone();
        let message =
            front_message(config.merge_front_matter(&meta).unwrap_err());
        if meta.vars.contains_key("cite_style") && meta.vars.contains_key("csl")
        {
            assert_eq!(message, "use either cite_style or csl");
        }
        assert_eq!(config, before, "{message}");
        assert_eq!(meta, original_meta);
    }
}

#[test]
fn front_matter_merge_is_idempotent() {
    let meta = meta_with(json!({
        "markdown": {"sourcepos": false}, "numbering": true,
        "math": {"html": "image", "katex_options": {"strict": false}},
        "bibliography": ["one.bib"], "theme": "front",
        "notebook": {"remove_cell_tags": ["gone"], "show_prompts": true}
    }));
    let mut once = Config::load(&fixture()).unwrap();
    once.merge_front_matter(&meta).unwrap();
    let mut twice = once.clone();
    twice.merge_front_matter(&meta).unwrap();
    assert_eq!(once, twice);
    assert_eq!(once.references.bibliography, [PathBuf::from("one.bib")]);
    assert_eq!(once.notebook.remove_cell_tags, ["gone"]);
    assert_eq!(
        once.math.katex_options,
        json!({"strict": false}).as_object().unwrap().clone()
    );
}

#[test]
fn cli_overrides_map_every_field() {
    let before = Config::load(&fixture()).unwrap();
    let mut config = before.clone();
    #[rustfmt::skip] // Group output, server, and compiler overrides.
    config.apply_cli(&CliOverrides {
        targets: Some(vec![Target::Pdf, Target::Docx]),
        out: Some("ignored.html".into()), theme: Some("cli".into()),
        template: Some("cli.typ".into()), latex_template: Some("cli.tex".into()),
        reference_doc: Some("cli.docx".into()),
        standalone: Some(true),
        port: Some(0), host: Some("localhost".into()), open: Some(false),
        view: Some(ServeView::Web), strict: Some(false),
        engine: Some(PdfEngine::Typst), allow_packages: Some(false),
    });
    assert_eq!(config.build.targets, [Target::Pdf, Target::Docx]);
    assert_eq!(config.build.typst_targets, [Target::Pdf, Target::Docx]);
    assert_eq!(
        config.build.out_dir, before.build.out_dir,
        "out is not build.out_dir"
    );
    assert_eq!(config.html.theme, "cli");
    assert!(config.html.standalone);
    assert_eq!(config.pdf.template, PathBuf::from("cli.typ"));
    assert_eq!(config.docx.reference_doc, PathBuf::from("cli.docx"));
    assert_eq!(config.latex.template, PathBuf::from("cli.tex"));
    assert_eq!(
        (config.serve.host.as_str(), config.serve.port),
        ("localhost", 0)
    );
    assert!(!config.serve.open);
    assert_eq!(config.serve.view, Some(ServeView::Web));
    assert!(!config.typst.strict && !config.pdf.allow_packages);
    assert_eq!(config.pdf.engine, PdfEngine::Typst);
    config.apply_cli(&CliOverrides::default());
    let preserved = config.clone();
    config.apply_cli(&CliOverrides::default());
    assert_eq!(config, preserved);
    let mut no_overrides = before.clone();
    no_overrides.apply_cli(&CliOverrides::default());
    assert_eq!(no_overrides, before, "strict None must keep true");
    config.apply_cli(&CliOverrides {
        strict: Some(true),
        standalone: Some(false),
        ..CliOverrides::default()
    });
    assert!(config.typst.strict && !config.html.standalone);
}

#[test]
fn targets_for_kinds() {
    let config = Config::default();
    assert_eq!(config.targets_for(SourceKind::Markdown), [Target::Html]);
    assert_eq!(config.targets_for(SourceKind::Notebook), [Target::Html]);
    assert_eq!(config.targets_for(SourceKind::Typst), [Target::Pdf]);
    let config = Config::load(&fixture()).unwrap();
    assert_eq!(
        config.targets_for(SourceKind::Markdown),
        [Target::Docx, Target::Latex]
    );
    assert_eq!(
        config.targets_for(SourceKind::Notebook),
        [Target::Docx, Target::Latex]
    );
    assert_eq!(
        config.targets_for(SourceKind::Typst),
        [Target::Html, Target::Typst]
    );
}

#[test]
fn toml_dates_in_free_form_maps_become_strings() {
    let config = Config::from_toml_str(
        "[math.katex_options]\nreleased = 2026-10-08\n\
         nested = { at = [07:32:00, { when = 1979-05-27T07:32:00Z }] }\n",
    )
    .unwrap();
    assert_eq!(config.math.katex_options["released"], json!("2026-10-08"));
    assert_eq!(
        config.math.katex_options["nested"],
        json!({"at": ["07:32:00", {"when": "1979-05-27T07:32:00Z"}]})
    );
    let macros =
        json!({"macros": {"\\R": "\\mathbb{R}"}, "d": {"x": "2026-10-08"}});
    let mut config = Config::from_toml_str(
        "[math.katex_options]\nmacros = { \"\\\\R\" = \"\\\\mathbb{R}\" }\nd = { x = \"2026-10-08\" }\n",
    ).unwrap();
    assert_eq!(
        serde_json::to_value(&config.math.katex_options).unwrap(),
        macros
    );
    config.math.katex_options.clear();
    config
        .merge_front_matter(&meta_with(
            json!({"math": {"katex_options": macros}}),
        ))
        .unwrap();
    assert_eq!(
        serde_json::to_value(&config.math.katex_options).unwrap(),
        macros
    );
}

#[test]
fn config_serializes_for_the_template_context() {
    let config = Config::load(&fixture()).unwrap();
    let value = serde_json::to_value(&config).unwrap();
    assert_eq!(value["document"]["toc"], json!(4));
    assert_eq!(value["math"]["html"], json!("mathml"));
    assert_eq!(value["build"]["out_dir"], json!("public"));
    assert_eq!(value["serve"]["view"], json!("pages"));
    assert_eq!(serde_json::from_value::<Config>(value).unwrap(), config);
    let toml = toml::to_string(&config).unwrap();
    assert_eq!(Config::from_toml_str(&toml).unwrap(), config);
    let defaults = toml::to_string(&Config::default()).unwrap();
    assert_eq!(Config::from_toml_str(&defaults).unwrap(), Config::default());
}
