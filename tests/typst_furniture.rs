//! Furniture geometry and actual rendered counters, checked on compiler frames.

use std::path::Path;

use typst::layout::{Frame, FrameItem, Point, Size, Transform};
use typst::text::{FontStyle, TextItem};
use typst::visualize::{Geometry, ImageKind, Paint};
use typst_layout::PagedDocument;
use writer::config::Target;
use writer::pipeline::{self, BuildOptions, WorldSlot};
use writer::typst::compile::compile_document;

fn text(frame: &Frame) -> String {
    let mut result = String::new();
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => result.push_str(&text(&group.frame)),
            FrameItem::Text(run) => result.push_str(&run.text),
            _ => {}
        }
    }
    result
}

fn compile(root: &Path, content: &str) -> PagedDocument {
    let input = root.join("document.md");
    std::fs::write(&input, content).unwrap();
    let opts = BuildOptions::new(input);
    let prepared = pipeline::prepare(&opts).unwrap();
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    pipeline::pdf::load_typst_main(&prepared, &opts, world).unwrap();
    compile_document(world).into_result().unwrap().0
}

fn geometry(
    frame: &Frame, transform: Transform, images: &mut Vec<(Point, f64)>,
    footer_bottoms: &mut Vec<f64>,
) {
    for (position, item) in frame.items() {
        let transform =
            transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Group(group) => {
                let transform = transform.pre_concat(group.transform);
                geometry(&group.frame, transform, images, footer_bottoms);
            }
            // The fixture's final footer line uses Typst's default baseline
            // bottom edge, so its baseline is the furniture's bottom edge.
            FrameItem::Text(run)
                if run.text.contains("Company No. 13773507") =>
            {
                footer_bottoms
                    .push(Point::zero().transform(transform).y.to_pt());
            }
            FrameItem::Image(_, size, _) => images
                .push((Point::zero().transform(transform), size.x.to_pt())),
            _ => {}
        }
    }
}

#[test]
fn letter_variants_place_logo_and_footer_at_design_offsets() {
    let directory = tempfile::tempdir().unwrap();
    let theme = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/letter/theme");
    let config = format!(
        "[html]\ntheme = {:?}\n[pdf]\nsystem_fonts = false\n",
        theme.to_str().unwrap(),
    );
    std::fs::write(directory.path().join("writer.toml"), config).unwrap();
    let document = compile(
        directory.path(),
        "Body paragraph one.\n\n:::pagebreak\n:::\n\nBody paragraph two.\n",
    );
    assert_eq!(document.pages().len(), 2);
    let first = text(&document.pages()[0].frame);
    let second = text(&document.pages()[1].frame);
    assert!(first.contains("Generative Biology Institute"));
    assert!(!first.contains("EIT Oxford |"));
    assert!(second.contains("EIT Oxford | Generative Biology Institute"));
    for page in document.pages() {
        assert!(text(&page.frame).contains("Company No. 13773507"));
        assert!(text(&page.frame).contains("oxford@eit.org"));
        let left = find_run(
            &page.frame,
            Transform::identity(),
            "Ellison Institute of Technology, Oxford Limited",
        )
        .unwrap();
        let right =
            find_run(&page.frame, Transform::identity(), "+44 1865 649525")
                .unwrap();
        assert!(
            (left.0.y.to_pt() - right.0.y.to_pt()).abs() < 1.0,
            "footer columns start on different lines: left={:?}, right={:?}",
            left.0,
            right.0
        );
        let mut rules = Vec::new();
        rule_geometry(&page.frame, Transform::identity(), &mut rules);
        assert_eq!(rules.len(), 1);
        assert!((rules[0].1 - 166.0 * 72.0 / 25.4).abs() < 0.01);
        assert!((rules[0].2 - 0.25 * 72.0 / 25.4).abs() < 0.01);
        assert_eq!(rules[0].3, "#a96d4b");
        let final_line = find_run(
            &page.frame,
            Transform::identity(),
            "Company No. 13773507",
        )
        .unwrap();
        assert!(rules[0].0 < final_line.0.y.to_pt());
    }
    let mut images = Vec::new();
    let mut bottoms = Vec::new();
    geometry(
        &document.pages()[0].frame,
        Transform::identity(),
        &mut images,
        &mut bottoms,
    );
    assert_eq!(images.len(), 1);
    assert!(
        (images[0].0.y.to_pt() - 17.0 * 72.0 / 25.4).abs() < 1.0,
        "image top: {:?}",
        images[0]
    );
    assert!((images[0].1 - 50.0 * 72.0 / 25.4).abs() < 0.01);
    // The header is centred and the margins are equal, so the logo's centre
    // is the page's.
    let centre = images[0].0.x.to_pt() + images[0].1 / 2.0;
    let page_centre = document.pages()[0].frame.width().to_pt() / 2.0;
    assert!(
        (centre - page_centre).abs() < 0.5,
        "logo centre {centre}, page centre {page_centre}"
    );
    let expected_bottom =
        document.pages()[0].frame.height().to_pt() - 10.0 * 72.0 / 25.4;
    assert!(
        bottoms.iter().any(|bottom| (bottom - expected_bottom).abs() < 1.0),
        "footer frame bottoms: {bottoms:?}, expected {expected_bottom}"
    );
    images.clear();
    geometry(
        &document.pages()[1].frame,
        Transform::identity(),
        &mut images,
        &mut Vec::new(),
    );
    assert!(images.is_empty());
}

#[test]
fn placeholders_render_actual_current_and_total_page_numbers() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        concat!(
            "[pdf]\nsystem_fonts = false\n[design.page.footer]\n",
            "lines = ['Page {page} / {pages} {literal}']\n",
        ),
    )
    .unwrap();
    let document = compile(
        directory.path(),
        "Body one.\n\n:::pagebreak\n:::\n\nBody two.\n",
    );
    assert_eq!(document.pages().len(), 2);
    assert!(text(&document.pages()[0].frame).contains("Page 1 / 2 {literal}"));
    assert!(text(&document.pages()[1].frame).contains("Page 2 / 2 {literal}"));
}

#[test]
fn empty_footer_removes_default_page_number_and_absence_keeps_it() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("writer.toml");
    std::fs::write(&config, "[pdf]\nsystem_fonts = false\n").unwrap();
    let baseline = compile(directory.path(), "Body.\n");
    assert!(text(&baseline.pages()[0].frame).contains('1'));
    std::fs::write(
        &config,
        "[pdf]\nsystem_fonts = false\n[design.page.footer]\n",
    )
    .unwrap();
    let empty = compile(directory.path(), "Body.\n");
    assert!(!text(&empty.pages()[0].frame).contains('1'));
}

#[test]
fn delegating_template_has_furniture_and_replacement_template_is_reported() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        concat!(
            "[pdf]\nsystem_fonts = false\ntemplate = 'template.typ'\n",
            "[design.page.header]\nlines = ['Design header']\n",
        ),
    )
    .unwrap();
    let template = directory.path().join("template.typ");
    std::fs::write(
        &template,
        format!(
            "#import \"@writer/doc:{}\" as base\n\
         #let writer-doc(..args, body) = base.writer-doc(..args, body)\n",
            writer::typst::WRITER_DOC_VERSION,
        ),
    )
    .unwrap();
    let delegated = compile(directory.path(), "Body.\n");
    assert!(text(&delegated.pages()[0].frame).contains("Design header"));
    std::fs::write(&template, "#let writer-doc(..args, body) = body\n")
        .unwrap();
    let replacement = compile(directory.path(), "Body.\n");
    assert!(!text(&replacement.pages()[0].frame).contains("Design header"));
    let mut opts = BuildOptions::new(directory.path().join("document.md"));
    opts.cli.targets = Some(vec![Target::Pdf]);
    let findings = writer::check::run(&opts, &Default::default()).unwrap();
    assert!(findings.iter().any(|finding| finding.code == "design-shadowed"));
}

#[test]
fn unset_offsets_use_thirty_percent_of_the_matching_margin() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("logo.svg"),
        "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10'/></svg>",
    ).unwrap();
    std::fs::write(directory.path().join("writer.toml"), concat!(
        "[pdf]\nsystem_fonts = false\n[design.page]\nmargin = '40mm'\n",
        "[design.page.header]\nimage = {src = 'logo.svg', width = '10mm'}\n",
        "[design.page.footer]\nimage = {src = 'logo.svg', width = '10mm'}\n",
    )).unwrap();
    let document = compile(directory.path(), "Body.\n");
    let mut images = Vec::new();
    geometry(
        &document.pages()[0].frame,
        Transform::identity(),
        &mut images,
        &mut Vec::new(),
    );
    images
        .sort_by(|(left, _), (right, _)| left.y.partial_cmp(&right.y).unwrap());
    assert_eq!(images.len(), 2);
    let offset = 12.0 * 72.0 / 25.4;
    assert!((images[0].0.y.to_pt() - offset).abs() < 1.0);
    let bottom = images[1].0.y.to_pt() + 10.0 * 72.0 / 25.4;
    let expected = document.pages()[0].frame.height().to_pt() - offset;
    assert!(
        (bottom - expected).abs() < 1.0,
        "footer bottom {bottom}, expected {expected}"
    );
}

#[test]
fn image_gap_separates_the_image_from_the_first_line_cap_height() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("logo.svg"),
        "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10'/></svg>",
    ).unwrap();
    let header = "[design.page.header]\nsize = '9pt'\nlines = ['GAPLINE']\n";
    for (design, expected) in [
        // Unset: writer-doc's own paragraph spacing, 1.2em of the furniture
        // text, as HTML's 1em is.
        (
            format!("{header}image = {{src = 'logo.svg', width = '10mm'}}\n"),
            1.2 * 9.0,
        ),
        // The design's paragraph spacing.
        (
            format!(
                "[design.text]\nparagraph_spacing = '3.6mm'\n{header}\
                 image = {{src = 'logo.svg', width = '10mm'}}\n"
            ),
            3.6 * 72.0 / 25.4,
        ),
        // An explicit gap.
        (
            format!(
                "[design.text]\nparagraph_spacing = '3.6mm'\n{header}\
                 image = {{src = 'logo.svg', width = '10mm', gap = '3mm'}}\n"
            ),
            3.0 * 72.0 / 25.4,
        ),
    ] {
        std::fs::write(
            directory.path().join("writer.toml"),
            format!("[pdf]\nsystem_fonts = false\n{design}"),
        )
        .unwrap();
        let document = compile(directory.path(), "Body.\n");
        let frame = &document.pages()[0].frame;
        let mut images = Vec::new();
        geometry(frame, Transform::identity(), &mut images, &mut Vec::new());
        assert_eq!(images.len(), 1);
        // The logo is square, so its height is its width.
        let bottom = images[0].0.y.to_pt() + images[0].1;
        let (origin, run) =
            find_run(frame, Transform::identity(), "GAPLINE").unwrap();
        // Typst's default top edge is the cap height, so the gap ends there.
        let top = origin.y.to_pt()
            - run.font.metrics().cap_height.at(run.size).to_pt();
        let gap = top - bottom;
        assert!(
            (gap - expected).abs() < 0.3,
            "{design}\ngap {gap}, expected {expected}"
        );
    }
}

fn font_size(frame: &Frame, marker: &str) -> Option<f64> {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                if let Some(size) = font_size(&group.frame, marker) {
                    return Some(size);
                }
            }
            FrameItem::Text(run) if run.text.contains(marker) => {
                return Some(run.size.to_pt());
            }
            _ => {}
        }
    }
    None
}

#[test]
fn unset_footer_size_is_three_quarters_of_the_backend_body_size() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("writer.toml"), concat!(
        "[pdf]\nsystem_fonts = false\n[design.page.header]\nlines = ['HEADERSIZE']\n",
        "[design.page.footer]\nlines = ['FOOTERSIZE']\n",
    )).unwrap();
    let document = compile(directory.path(), "Body.\n");
    let frame = &document.pages()[0].frame;
    assert_eq!(font_size(frame, "HEADERSIZE"), Some(11.0));
    assert_eq!(font_size(frame, "FOOTERSIZE"), Some(8.25));
}

fn rule_geometry(
    frame: &Frame, transform: Transform,
    result: &mut Vec<(f64, f64, f64, String)>,
) {
    for (position, item) in frame.items() {
        let transform =
            transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Group(group) => rule_geometry(
                &group.frame,
                transform.pre_concat(group.transform),
                result,
            ),
            FrameItem::Shape(shape, _) => {
                if let Geometry::Line(end) = &shape.geometry
                    && let Some(stroke) = &shape.stroke
                    && let Paint::Solid(color) = &stroke.paint
                {
                    result.push((
                        Point::zero().transform(transform).y.to_pt(),
                        end.x.to_pt(),
                        stroke.thickness.to_pt(),
                        color.to_hex().to_string(),
                    ));
                }
            }
            _ => {}
        }
    }
}

fn find_run<'a>(
    frame: &'a Frame, transform: Transform, marker: &str,
) -> Option<(Point, &'a TextItem)> {
    for (position, item) in frame.items() {
        let transform =
            transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Group(group) => {
                if let Some(found) = find_run(
                    &group.frame,
                    transform.pre_concat(group.transform),
                    marker,
                ) {
                    return Some(found);
                }
            }
            FrameItem::Text(run) if run.text.contains(marker) => {
                return Some((Point::zero().transform(transform), run));
            }
            _ => {}
        }
    }
    None
}

#[test]
fn line_overrides_and_the_right_column_keep_their_furniture_properties() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("writer.toml"), concat!(
        "[pdf]\nsystem_fonts = false\n[design.text]\njustify = true\n",
        "[design.page]\nmargin = '20mm'\n[design.page.header]\n",
        "align = 'center'\nsize = '9pt'\ncolor = '#111111'\n",
        "weight = 'normal'\nitalic = true\n",
        "lines = ['INHERIT', {text = 'OVERRIDE', size = '13pt', ",
        "color = '#aabbcc', weight = 'bold', italic = false}]\n",
        "right_lines = ['RIGHT']\n",
        "rule = {position = 'below', color = '#123456', width = '0.75pt'}\n",
    )).unwrap();
    let document = compile(directory.path(), "Body.\n");
    let frame = &document.pages()[0].frame;
    let inherited = find_run(frame, Transform::identity(), "INHERIT").unwrap();
    let overridden =
        find_run(frame, Transform::identity(), "OVERRIDE").unwrap();
    let right = find_run(frame, Transform::identity(), "RIGHT").unwrap();
    assert_eq!(inherited.1.size.to_pt(), 9.0);
    assert_eq!(overridden.1.size.to_pt(), 13.0);
    assert_eq!(inherited.1.font.info().variant.style, FontStyle::Italic);
    assert_eq!(overridden.1.font.info().variant.style, FontStyle::Normal);
    assert_eq!(overridden.1.font.info().variant.weight.to_number(), 700);
    let Paint::Solid(inherited_color) = &inherited.1.fill else {
        panic!("expected inherited solid text color")
    };
    let Paint::Solid(overridden_color) = &overridden.1.fill else {
        panic!("expected overridden solid text color")
    };
    assert_eq!(inherited_color.to_hex().as_str(), "#111111");
    assert_eq!(overridden_color.to_hex().as_str(), "#aabbcc");
    let margin = 20.0 * 72.0 / 25.4;
    assert!((inherited.0.x.to_pt() - margin).abs() < 0.01);
    let right_edge = right.0.x.to_pt() + right.1.width().to_pt();
    assert!((right_edge - frame.width().to_pt() + margin).abs() < 0.01);
    let mut rules = Vec::new();
    rule_geometry(frame, Transform::identity(), &mut rules);
    assert_eq!(rules.len(), 1);
    assert!((rules[0].2 - 0.75).abs() < 0.01);
    assert_eq!(rules[0].3, "#123456");
    assert!(
        rules[0].0 > overridden.0.y.to_pt(),
        "rule y={}, text baseline={}",
        rules[0].0,
        overridden.0.y.to_pt()
    );
}

fn svg_bytes(frame: &Frame) -> Option<Vec<u8>> {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                if let Some(bytes) = svg_bytes(&group.frame) {
                    return Some(bytes);
                }
            }
            FrameItem::Image(image, _, _) => {
                if let ImageKind::Svg(svg) = image.kind() {
                    return Some(svg.data().to_vec());
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn retained_world_refreshes_a_changed_design_image_without_rebuilding_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::write(
        root.join("writer.toml"),
        concat!(
            "[pdf]\nsystem_fonts = false\n[design.page.header]\n",
            "image = {src = 'logo.svg', width = '10mm'}\n",
        ),
    )
    .unwrap();
    let input = root.join("document.md");
    std::fs::write(&input, "Body.\n").unwrap();
    let opts = BuildOptions::new(input);
    let mut worlds = WorldSlot::new();
    let mut images = Vec::new();
    let mut builds = Vec::new();
    for color in ["#ff0000", "#0000ff"] {
        let svg = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10' fill='{color}'/></svg>",
        );
        std::fs::write(root.join("logo.svg"), &svg).unwrap();
        let prepared = pipeline::prepare(&opts).unwrap();
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        world.reset();
        pipeline::pdf::load_typst_main(&prepared, &opts, world).unwrap();
        let document = compile_document(world).into_result().unwrap().0;
        images.push(svg_bytes(&document.pages()[0].frame).unwrap());
        builds.push(world.library_builds());
    }
    assert_ne!(images[0], images[1]);
    assert_eq!(builds[0], builds[1]);
    assert!(String::from_utf8_lossy(&images[1]).contains("#0000ff"));
}

#[test]
fn empty_first_variant_suppresses_only_first_page_furniture() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        concat!(
            "[pdf]\nsystem_fonts = false\n[design.page.footer]\n",
            "lines = ['RESTFOOTER']\n[design.page.footer.first]\n",
        ),
    )
    .unwrap();
    let document = compile(
        directory.path(),
        "Body one.\n\n:::pagebreak\n:::\n\nBody two.\n",
    );
    assert_eq!(document.pages().len(), 2);
    let first = text(&document.pages()[0].frame);
    let rest = text(&document.pages()[1].frame);
    assert!(!first.contains("RESTFOOTER"));
    assert!(rest.contains("RESTFOOTER"));
    assert!(!first.contains('1'));
    assert!(!rest.contains('2'));
}

fn image_boxes(
    frame: &Frame, transform: Transform, found: &mut Vec<(Point, Size)>,
) {
    for (position, item) in frame.items() {
        let transform =
            transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Group(group) => image_boxes(
                &group.frame,
                transform.pre_concat(group.transform),
                found,
            ),
            FrameItem::Image(_, size, _) => {
                found.push((Point::zero().transform(transform), *size));
            }
            _ => {}
        }
    }
}

#[test]
fn page_background_fills_the_first_page_or_every_page() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::write(
        root.join("cover.svg"),
        "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='100'><rect width='200' height='100' fill='#3366cc'/></svg>",
    )
    .unwrap();
    // A4 in points; the picture is twice as wide as it is high.
    let (width, height) = (595.2756, 841.8898);
    let cover = (2.0 * height, height);
    for (image, pages, origin, size) in [
        (
            "{src = 'cover.svg', fit = 'stretch'}",
            "first",
            (0.0, 0.0),
            (width, height),
        ),
        (
            "{src = 'cover.svg', fit = 'stretch'}",
            "all",
            (0.0, 0.0),
            (width, height),
        ),
        ("'cover.svg'", "first", ((width - cover.0) / 2.0, 0.0), cover),
        ("{src = 'cover.svg', align = 'left'}", "all", (0.0, 0.0), cover),
        (
            "{src = 'cover.svg', align = 'right top'}",
            "first",
            (width - cover.0, 0.0),
            cover,
        ),
        (
            "{src = 'cover.svg', fit = 'contain', align = 'bottom'}",
            "first",
            (0.0, height - width / 2.0),
            (width, width / 2.0),
        ),
    ] {
        std::fs::write(
            root.join("writer.toml"),
            format!(
                "[pdf]\nsystem_fonts = false\n[design.page.background]\nimage = {image}\npages = '{pages}'\n"
            ),
        )
        .unwrap();
        let document =
            compile(root, "Body one.\n\n:::pagebreak\n:::\n\nBody two.\n");
        assert_eq!(document.pages().len(), 2);
        for (index, page) in document.pages().iter().enumerate() {
            let mut found = Vec::new();
            image_boxes(&page.frame, Transform::identity(), &mut found);
            if index == 0 || pages == "all" {
                assert_eq!(found.len(), 1, "{image} {pages} page {index}");
                let (point, actual) = found[0];
                for (got, expected) in [
                    (point.x.to_pt(), origin.0),
                    (point.y.to_pt(), origin.1),
                    (actual.x.to_pt(), size.0),
                    (actual.y.to_pt(), size.1),
                ] {
                    assert!(
                        (got - expected).abs() < 0.01,
                        "{image} {pages} page {index}: {point:?} {actual:?}"
                    );
                }
            } else {
                assert!(found.is_empty(), "{image} {pages} page {index}");
            }
            // A background declares no slot, so the built-in number stays.
            assert!(
                text(&page.frame).contains(&(index + 1).to_string()),
                "{image} {pages} page {index}"
            );
        }
    }
}
