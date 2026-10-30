//! Query and fragment suffixes do not bypass selected-target MIME diagnostics.

use writer::check::{CheckOptions, run};
use writer::config::{Config, Target};
use writer::error::Level;
use writer::ir::{self, Inline};
use writer::pipeline::{self, BuildOptions};

#[test]
fn original_and_suffixed_local_assets_share_the_same_mime_support_check() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let input = root.join("document.md");
    std::fs::write(
        &input,
        concat!(
            "![Plain](plot.tiff)\n\n",
            "![Query](plot.tiff?download=1)\n\n",
            "![Fragment](plot.tiff#page)\n\n",
            "![Both](plot.tiff?download=1#page)\n\n",
            "![Supported](plot.png?cache=1#page)\n",
        ),
    )
    .unwrap();
    // Classification reads MIME metadata only. Invalid encoded bytes must not
    // start decoding or a compiler just to identify unsupported TIFF support.
    std::fs::write(root.join("plot.tiff"), b"not decoded TIFF bytes").unwrap();
    std::fs::write(root.join("plot.png"), b"not decoded PNG bytes").unwrap();
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.timestamp = "compiler setup must remain unused".into();
    let config_path = root.join("writer.toml");
    std::fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
    let mut options = BuildOptions::new(&input);
    options.config_path = Some(config_path);
    options.cli.targets = Some(vec![Target::Pdf, Target::Typst]);

    let prepared = pipeline::prepare(&options).unwrap();
    let mut urls = Vec::new();
    ir::walk_inlines(&prepared.document, &mut |inline| {
        if let Inline::Image { url, .. } = inline {
            urls.push(url.clone());
        }
    });
    assert_eq!(urls.len(), 5);
    assert!(urls.iter().all(|url| url.starts_with("asset:")));
    assert!(urls[1].ends_with("?download=1"));
    assert!(urls[2].ends_with("#page"));
    assert!(urls[3].ends_with("?download=1#page"));

    let diagnostics = run(&options, &CheckOptions::default()).unwrap();
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let unsupported = diagnostics
        .iter()
        .filter(|finding| finding.code == "unsupported-construct")
        .collect::<Vec<_>>();
    assert_eq!(unsupported.len(), 8, "{diagnostics:?}");
    for target in ["pdf:", "typst:"] {
        let selected = unsupported
            .iter()
            .filter(|finding| finding.message.starts_with(target))
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 4, "{diagnostics:?}");
        for line in [1, 3, 5, 7] {
            assert_eq!(
                selected
                    .iter()
                    .filter(|finding| {
                        finding.level == Level::Warning
                            && finding.path.as_deref() == Some(input.as_path())
                            && finding
                                .span
                                .as_ref()
                                .is_some_and(|span| span.start_line == line)
                    })
                    .count(),
                1,
                "{diagnostics:?}"
            );
        }
        assert!(selected.iter().all(|finding| {
            finding.message.contains("unsupported image MIME type: image/tiff")
        }));
    }
    assert!(!root.join("dist").exists());
    assert_eq!(
        std::fs::read(root.join("plot.tiff")).unwrap(),
        b"not decoded TIFF bytes"
    );
    assert_eq!(
        std::fs::read(root.join("plot.png")).unwrap(),
        b"not decoded PNG bytes"
    );
}

#[test]
fn a_literal_unregistered_placeholder_is_a_target_warning_without_a_fake_file()
{
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let input = root.join("document.md");
    let url = "asset:ffffffffffff?cache=1#panel";
    std::fs::write(&input, format!("![Unknown]({url})\n")).unwrap();
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.timestamp = "compiler setup must remain unused".into();
    let config_path = root.join("writer.toml");
    std::fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
    let mut options = BuildOptions::new(&input);
    options.config_path = Some(config_path);
    options.cli.targets = Some(vec![Target::Typst, Target::Pdf, Target::Typst]);

    let prepared = pipeline::prepare(&options).unwrap();
    assert!(prepared.diagnostics.is_empty(), "{:?}", prepared.diagnostics);
    assert_eq!(prepared.assets.iter().count(), 0);
    let mut parsed_urls = Vec::new();
    ir::walk_inlines(&prepared.document, &mut |inline| {
        if let Inline::Image { url, .. } = inline {
            parsed_urls.push(url.clone());
        }
    });
    assert_eq!(parsed_urls, [url]);

    for strict in [false, true] {
        options.cli.strict = Some(strict);
        let diagnostics = run(&options, &CheckOptions::default()).unwrap();
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        for target in ["typst", "pdf"] {
            let matching = diagnostics
                .iter()
                .filter(|finding| {
                    finding.message
                        == format!(
                            "{target}: unregistered asset placeholder `{url}`"
                        )
                })
                .collect::<Vec<_>>();
            assert_eq!(matching.len(), 1, "{diagnostics:?}");
            let finding = matching[0];
            assert_eq!(finding.code, "unsupported-construct");
            assert_eq!(
                finding.level,
                if strict { Level::Error } else { Level::Warning }
            );
            assert_eq!(finding.path.as_deref(), Some(input.as_path()));
            let span = finding.span.as_ref().unwrap();
            assert_eq!(
                (span.start_line, span.end_line, span.cell),
                (1, 1, None)
            );
        }
    }
    assert!(!root.join("dist").exists());
}
