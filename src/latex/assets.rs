//! Planned LaTeX sidecar bytes, without file writes or new compiler sessions.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::assets::registry::{AssetRegistry, unknown_id};
use crate::error::{Error, Result};
use crate::latex::{FileSource, PlannedFile};
use crate::typst::render::{self, Rendition};
use crate::typst::world::WriterWorld;

/// Only planned image renditions need the supplied publishing-session World.
pub fn needs_world(files: &[PlannedFile]) -> bool {
    files.iter().any(|file| {
        matches!(&file.source, FileSource::Asset { render: true, .. })
    })
}

/// Returns every planned file's bytes; rendering never silently falls back.
///
/// Exact duplicate names may share identical bytes. A differently spelled
/// ASCII-case alias, or different bytes for one name, is a malformed plan.
pub fn materialise(
    files: &[PlannedFile], registry: &AssetRegistry,
    mut world: Option<&mut WriterWorld>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    validate_case(files)?;
    let mut result: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for file in files {
        let bytes = match &file.source {
            FileSource::Bib(path) => {
                fs::read(path).map_err(|error| Error::Resource {
                    what: "bibliography",
                    path: path.clone(),
                    message: error.to_string(),
                })?
            }
            FileSource::Asset { id, render: false } => registry.read(id)?,
            FileSource::Asset { id, render: true } => {
                let asset = registry.get(id).ok_or_else(|| unknown_id(id))?;
                let world = world.as_deref_mut().ok_or_else(|| {
                    image_error(&asset.file_name_hint,
                        "image rendition needs the publishing session's compiler world")
                })?;
                let bytes = registry.read(id)?;
                rendered_pdf(
                    render::image(world, &bytes, &asset.mime, None),
                    &asset.file_name_hint,
                )?
            }
        };
        if let Some(previous) = result.get(&file.name) {
            if previous != &bytes {
                return Err(Error::Other(format!(
                    "LaTeX asset name `{}` has conflicting file contents",
                    file.name,
                )));
            }
        } else {
            result.insert(file.name.clone(), bytes);
        }
    }
    Ok(result)
}

fn validate_case(files: &[PlannedFile]) -> Result<()> {
    let mut names = BTreeMap::new();
    for file in files {
        if let Some(previous) =
            names.insert(file.name.to_ascii_lowercase(), &file.name)
            && previous != &file.name
        {
            return Err(Error::Other(format!(
                "LaTeX asset names `{previous}` and `{}` differ only by ASCII case",
                file.name,
            )));
        }
    }
    Ok(())
}

fn image_error(hint: &str, message: impl Into<String>) -> Error {
    Error::Resource {
        what: "image",
        path: PathBuf::from(hint),
        message: message.into(),
    }
}

fn rendered_pdf(rendered: Result<Rendition>, hint: &str) -> Result<Vec<u8>> {
    rendered
        .map_err(|error| image_error(hint, error.to_string()))?
        .pdf
        .ok_or_else(|| image_error(hint, "image rendition produced no PDF"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::typst::sourcemap::SourceMap;
    use crate::typst::world::MainFile;
    use std::path::Path;
    use typst::World;
    use typst::foundations::Datetime;

    const PNG: &[u8] =
        include_bytes!("../../tests/fixtures/md/images/plot.png");
    const SVG: &[u8] =
        include_bytes!("../../tests/fixtures/md/images/diagram.svg");
    const GIF: &[u8] =
        include_bytes!("../../tests/fixtures/docx/images/anim.gif");
    const WEBP: &[u8] =
        include_bytes!("../../tests/fixtures/docx/images/pic.webp");

    fn asset(id: &str, name: &str, render: bool) -> PlannedFile {
        PlannedFile {
            name: name.into(),
            source: FileSource::Asset { id: id.into(), render },
        }
    }

    fn world(root: &Path) -> WriterWorld {
        let mut config = Config::default();
        config.pdf.system_fonts = false;
        config.pdf.allow_packages = false;
        let mut world = WriterWorld::from_config(
            &config,
            root,
            MainFile::Virtual,
            Datetime::from_ymd(2026, 10, 29).unwrap(),
        )
        .unwrap();
        world.set_main_text("Original native main", SourceMap::default());
        world
    }

    #[test]
    fn copied_assets_and_bibliography_need_no_world_and_keep_exact_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let bibliography = directory.path().join("source.bib");
        let bib = b"@book{test, title={Exact bytes}}\r\n";
        fs::write(&bibliography, bib).unwrap();
        let disk = directory.path().join("disk.png");
        fs::write(&disk, PNG).unwrap();
        let mut registry = AssetRegistry::new();
        let bytes =
            registry.register_bytes(PNG.to_vec(), "image/png", "memory.png");
        let file = registry.register_file(&disk);
        let files = vec![
            asset(&bytes, "memory.png", false),
            asset(&file, "disk.png", false),
            PlannedFile {
                name: "copied.bib".into(),
                source: FileSource::Bib(bibliography.clone()),
            },
        ];
        assert!(!needs_world(&files));
        assert!(!needs_world(&[]));
        let output = materialise(&files, &registry, None).unwrap();
        assert_eq!(output["memory.png"], PNG);
        assert_eq!(output["disk.png"], PNG);
        assert_eq!(output["copied.bib"], bib);
        assert_eq!(fs::read(&bibliography).unwrap(), bib);
        assert_eq!(fs::read(&disk).unwrap(), PNG);
        assert!(!directory.path().join("copied.bib").exists());
        assert!(!directory.path().join("memory.png").exists());
    }

    #[test]
    fn actual_svg_and_raster_renditions_are_deterministic_and_restore_the_main()
    {
        let directory = tempfile::tempdir().unwrap();
        let mut registry = AssetRegistry::new();
        let mut files = Vec::new();
        for (bytes, mime, hint) in [
            (SVG, "image/svg+xml", "diagram.svg"),
            (GIF, "image/gif", "anim.gif"),
            (WEBP, "image/webp", "pic.webp"),
        ] {
            let id = registry.register_bytes(bytes.to_vec(), mime, hint);
            files.push(asset(&id, &format!("{hint}.pdf"), true));
        }
        assert!(needs_world(&files));
        let mut retained = world(directory.path());
        let main = retained.main();
        let first =
            materialise(&files, &registry, Some(&mut retained)).unwrap();
        assert_eq!(retained.main(), main);
        assert_eq!(
            retained.source(main).unwrap().text(),
            "Original native main"
        );
        let second =
            materialise(&files, &registry, Some(&mut retained)).unwrap();
        let fresh =
            materialise(&files, &registry, Some(&mut world(directory.path())))
                .unwrap();
        assert_eq!(first, second);
        assert_eq!(first, fresh);
        assert_eq!(first.len(), 3);
        for (name, bytes) in &first {
            assert!(bytes.starts_with(b"%PDF-"), "{name}");
            assert_eq!(
                crate::pipeline::pdf::page_count(bytes),
                Some(1),
                "{name}"
            );
            assert!(!directory.path().join(name).exists());
        }
        assert_eq!(retained.main(), main);
        assert_eq!(
            retained.source(main).unwrap().text(),
            "Original native main"
        );
    }

    #[test]
    fn requested_renditions_require_a_supplied_world_and_never_copy_original_bytes()
     {
        let mut registry = AssetRegistry::new();
        let id = registry.register_bytes(
            SVG.to_vec(),
            "image/svg+xml",
            "diagram.svg",
        );
        let files = [asset(&id, "diagram.svg.pdf", true)];
        assert!(needs_world(&files));
        let error = materialise(&files, &registry, None).unwrap_err();
        let Error::Resource { what, path, message } = error else {
            panic!("expected an image resource failure");
        };
        assert_eq!(what, "image");
        assert_eq!(path, Path::new("diagram.svg"));
        assert!(message.contains("compiler world"));
    }

    #[test]
    fn invalid_image_rendering_uses_the_original_hint_and_returns_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let mut registry = AssetRegistry::new();
        let id = registry.register_bytes(
            b"not SVG".to_vec(),
            "image/svg+xml",
            "broken.svg",
        );
        let mut world = world(directory.path());
        let main = world.main();
        let error = materialise(
            &[asset(&id, "broken.svg.pdf", true)],
            &registry,
            Some(&mut world),
        )
        .unwrap_err();
        let Error::Resource { what, path, message } = error else {
            panic!("expected an image resource failure");
        };
        assert_eq!(what, "image");
        assert_eq!(path, Path::new("broken.svg"));
        assert!(!message.is_empty());
        assert_eq!(world.main(), main);
        assert_eq!(world.source(main).unwrap().text(), "Original native main");
        assert!(!directory.path().join("broken.svg.pdf").exists());
    }

    #[test]
    fn a_missing_pdf_or_renderer_error_never_uses_another_rendition() {
        let rendition = Rendition {
            svg: b"SVG".to_vec(),
            png: b"PNG".to_vec(),
            pdf: None,
            width_pt: 1.0,
            height_pt: 1.0,
            baseline_pt: 1.0,
        };
        for result in
            [Ok(rendition), Err(Error::Other("renderer failed".into()))]
        {
            let error = rendered_pdf(result, "source.gif").unwrap_err();
            let Error::Resource { what, path, message } = error else {
                panic!("expected an image resource failure");
            };
            assert_eq!(what, "image");
            assert_eq!(path, Path::new("source.gif"));
            assert!(
                message.contains("no PDF")
                    || message.contains("renderer failed")
            );
        }
    }

    #[test]
    fn exact_duplicate_names_require_identical_bytes_and_case_aliases_are_rejected()
     {
        let mut registry = AssetRegistry::new();
        let first = registry.register_bytes(vec![1], "image/png", "one.png");
        let other = registry.register_bytes(vec![2], "image/png", "two.png");
        let exact = [
            asset(&first, "shared.png", false),
            asset(&first, "shared.png", false),
        ];
        assert_eq!(
            materialise(&exact, &registry, None).unwrap(),
            BTreeMap::from([("shared.png".into(), vec![1])])
        );
        let conflict = [
            asset(&first, "shared.png", false),
            asset(&other, "shared.png", false),
        ];
        let error = materialise(&conflict, &registry, None).unwrap_err();
        assert!(
            matches!(error, Error::Other(message) if message.contains("shared.png") && message.contains("conflicting"))
        );
        for id in [&first, &other] {
            let aliases = [
                asset(&first, "shared.png", false),
                asset(id, "SHARED.png", false),
            ];
            let error = materialise(&aliases, &registry, None).unwrap_err();
            assert!(
                matches!(error, Error::Other(message) if message.contains("ASCII case"))
            );
        }
    }

    #[test]
    fn bibliography_read_errors_retain_the_source_resource_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.bib");
        let file = PlannedFile {
            name: "copied.bib".into(),
            source: FileSource::Bib(path.clone()),
        };
        let error =
            materialise(&[file], &AssetRegistry::new(), None).unwrap_err();
        assert!(
            matches!(error, Error::Resource { what: "bibliography", path: found, .. } if found == path)
        );
        assert!(!directory.path().join("copied.bib").exists());
    }
}
