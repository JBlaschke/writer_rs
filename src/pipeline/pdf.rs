//! Generated Typst compilation and PDF output for Markdown and notebooks.

use std::path::{Path, PathBuf};

use super::{BuildFailure, BuildOptions, Prepared};
use crate::error::{Diagnostics, Error, Result};
use crate::typst::compile::{self, Compiled, PdfOptions};
use crate::typst::emit::{self, AssetPaths, EmitOptions, Emitted};
use crate::typst::world::{WriterWorld, world_inputs};

type Detailed<T> = std::result::Result<T, BuildFailure>;

/// Builds a PDF through the caller's retained compiler session.
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    build_detailed(prepared, opts, out, world).map_err(|failure| *failure.error)
}

/// Keeps emitter and compiler findings available even when compilation fails.
pub(crate) fn build_detailed(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Detailed<(Vec<PathBuf>, Diagnostics)> {
    let mut diagnostics = load_with_diagnostics(prepared, opts, world)?;
    let options =
        PdfOptions { timestamp: world.session_timestamp(), ident: None };
    let Compiled { output, diagnostics: found } =
        compile::compile_pdf(world, &options);
    diagnostics.extend(found.into_iter().map(|mut diagnostic| {
        diagnostic.path = diagnostic
            .span
            .as_ref()
            .and_then(|span| span.file.clone())
            .or(diagnostic.path)
            .or_else(|| Some(prepared.document.source.path.clone()));
        diagnostic
    }));
    let result =
        Compiled { output, diagnostics: diagnostics.clone() }.into_result();
    let (pdf, diagnostics) = result.map_err(|mut error| {
        if let Error::Compile { path, line, .. } = &mut error
            && path == Path::new("/__writer-main.typ")
        {
            *path = prepared.document.source.path.clone();
            *line = None;
        }
        BuildFailure { error: Box::new(error), diagnostics }
    })?;
    super::write_file(out, &pdf.bytes).map_err(|error| BuildFailure {
        error: Box::new(error),
        diagnostics: diagnostics.clone(),
    })?;
    Ok((vec![out.to_path_buf()], diagnostics))
}

/// Generates the same source as a PDF build, without compiling or writing it.
pub fn dump(
    prepared: &Prepared, _opts: &BuildOptions,
) -> Result<(String, Diagnostics)> {
    let (emitted, _) = generate(prepared)?;
    Ok((emitted.text, emitted.diagnostics))
}

/// Loads generated source and virtual resources without resetting the session.
pub fn load_typst_main(
    prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld,
) -> Detailed<()> {
    load_with_diagnostics(prepared, opts, world).map(|_| ())
}

/// The pages stage can preserve successful emission findings through this seam.
pub(crate) fn load_with_diagnostics(
    prepared: &Prepared, _opts: &BuildOptions, world: &mut WriterWorld,
) -> Detailed<Diagnostics> {
    let (emitted, mounts) = generate(prepared)?;
    for prefix in ["/__writer/template", "/__writer/bib", "/__writer/csl"] {
        world.unmount(prefix);
    }
    for (prefix, directory) in mounts {
        world.mount(&prefix, &directory);
    }
    world.set_inputs(world_inputs(&prepared.config));
    world.set_assets(prepared.assets.clone());
    world.set_main_text(&emitted.text, emitted.map);
    Ok(emitted.diagnostics)
}

fn generate(prepared: &Prepared) -> Result<(Emitted, Vec<(String, PathBuf)>)> {
    let config = &prepared.config;
    let mut mounts = Vec::new();
    let template = if config.pdf.template.as_os_str().is_empty() {
        None
    } else {
        Some(resource(&config.pdf.template, "/__writer/template", &mut mounts)?)
    };
    let bibliography = config
        .references
        .bibliography
        .iter()
        .enumerate()
        .map(|(index, path)| {
            resource(path, &format!("/__writer/bib/{index}"), &mut mounts)
        })
        .collect::<Result<Vec<_>>>()?;
    let style = if super::is_csl(&config.references.style) {
        resource(
            Path::new(&config.references.style),
            "/__writer/csl",
            &mut mounts,
        )?
    } else {
        config.references.style.clone()
    };
    let options = EmitOptions {
        config,
        assets: AssetPaths::Virtual,
        template,
        bibliography,
        style,
        footnote_prefix: String::new(),
        fragment: false,
    };
    let emitted = emit::emit(&prepared.document, &prepared.assets, &options)?;
    Ok((emitted, mounts))
}

fn resource(
    path: &Path, prefix: &str, mounts: &mut Vec<(String, PathBuf)>,
) -> Result<String> {
    let filename =
        path.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
            Error::Config(format!(
                "resource path {} needs a UTF-8 filename",
                path.display()
            ))
        })?;
    mounts
        .push((prefix.into(), path.parent().unwrap_or(Path::new(".")).into()));
    Ok(format!("{prefix}/{filename}"))
}

/// Reads the uncompressed XMP page count written by the pinned Typst exporter.
/// Returns `None` when the expected metadata is absent or malformed.
pub fn page_count(pdf: &[u8]) -> Option<usize> {
    if !pdf.starts_with(b"%PDF-") || !pdf.trim_ascii_end().ends_with(b"%%EOF") {
        return None;
    }
    // Match xmp-writer's complete opening. An ASCII PDFInfo title can contain
    // a fake xpacket; the real packet has this UTF-8 BOM and fixed wrapper.
    let opening = concat!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>",
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"xmp-writer\">",
    )
    .as_bytes();
    let start = find(pdf, opening)?;
    let metadata =
        pdf.get(start..start.saturating_add(8 * 1024 * 1024).min(pdf.len()))?;
    let end = find(
        metadata,
        b"</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"r\"?>",
    )?;
    let metadata = &metadata[..end];
    let tag = b"<xmpTPg:NPages>";
    let count = metadata.get(find(metadata, tag)? + tag.len()..)?;
    let count = count.get(..find(count, b"</xmpTPg:NPages>")?)?.trim_ascii();
    if count.is_empty() || !count.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let count = std::str::from_utf8(count).ok()?.parse::<usize>().ok()?;
    (count > 0).then_some(count)
}

fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes.windows(needle.len()).position(|part| part == needle)
}
