//! HTML build and serve pipeline (html-output.md §6).

use std::path::{Path, PathBuf};

use crate::assets::emit::OutputMode;
use crate::assets::registry::AssetRegistry;
use crate::config::Config;
use crate::error::{Diagnostics, Error, Result};
use crate::html::emit::MathHook;
use crate::html::{BuildInfo, RenderInput, RenderedDocument};
use crate::theme::Theme;
use crate::typst::world::WriterWorld;

use super::{
    BuildOptions, InputRole, Prepared, find_clash, input_files, write_file,
};

/// Name of the HTML document in a site directory.
pub const SITE_INDEX: &str = "index.html";
/// Directory holding a site's emitted assets.
pub const SITE_ASSETS_DIR: &str = "assets";

fn math_hook<'a>(
    config: &'a Config, world: &'a mut WriterWorld,
) -> Box<dyn MathHook + 'a> {
    Box::new(crate::html::math::MathRenderer::new(config, world))
}

/// Renders using a cloned asset registry, retaining newly generated resources.
pub fn render(
    prepared: &Prepared, theme: &Theme, output: OutputMode, build: BuildInfo,
    world: &mut WriterWorld,
) -> Result<(RenderedDocument, AssetRegistry)> {
    let mut assets = prepared.assets.clone();
    let mut math = math_hook(&prepared.config, world);
    let rendered = crate::html::render(RenderInput {
        document: &prepared.document,
        assets: &mut assets,
        theme,
        config: &prepared.config,
        output,
        build,
        math: math.as_mut(),
    })?;
    Ok((rendered, assets))
}

/// Renders and writes one HTML file, or a site document and asset directory.
/// Every written file is checked against all known source and theme inputs.
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    let standalone = prepared.config.html.standalone;
    let theme = super::resolve_theme(prepared, opts)?;
    let (rendered, _) = render(
        prepared,
        &theme,
        output_mode(standalone, out),
        BuildInfo::build(standalone),
        world,
    )?;
    let document = document_path(out, standalone);
    let mut inputs = input_files(prepared);
    inputs.extend(
        rendered.theme_files.iter().cloned().map(|p| (p, InputRole::Theme)),
    );
    let outputs = std::iter::once(document.as_path())
        .chain(rendered.files.iter().map(|file| file.path.as_path()));
    if let Some(clash) = find_clash(outputs, &inputs) {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            clash,
        )));
    }
    write_file(&document, rendered.html.as_bytes())?;
    let mut written = vec![document];
    if !standalone {
        let dir = out.join(SITE_ASSETS_DIR);
        std::fs::create_dir_all(&dir)
            .map_err(|e| super::io_error(e, "cannot create", &dir))?;
        for file in &rendered.files {
            write_file(&file.path, &file.bytes)?;
        }
        written.push(dir);
    }
    Ok((written, rendered.diagnostics))
}

/// Builds the ordered JSON context used by `dump --context`.
pub fn context(
    prepared: &Prepared, world: &mut WriterWorld,
) -> Result<serde_json::Value> {
    let opts = BuildOptions::new(&prepared.document.source.path);
    let theme = super::resolve_theme(prepared, &opts)?;
    let mut assets = prepared.assets.clone();
    let mut math = math_hook(&prepared.config, world);
    crate::html::context::context_json(&mut RenderInput {
        document: &prepared.document,
        assets: &mut assets,
        theme: &theme,
        config: &prepared.config,
        output: OutputMode::Standalone,
        build: BuildInfo::build(prepared.config.html.standalone),
        math: math.as_mut(),
    })
}

/// Asset URL mode for an HTML output path.
pub fn output_mode(standalone: bool, out: &Path) -> OutputMode {
    if standalone {
        OutputMode::Standalone
    } else {
        OutputMode::Site {
            assets_dir: out.join(SITE_ASSETS_DIR),
            url_prefix: format!("{SITE_ASSETS_DIR}/"),
        }
    }
}

/// Main HTML file for a standalone file or site directory.
pub fn document_path(out: &Path, standalone: bool) -> PathBuf {
    if standalone {
        out.to_path_buf()
    } else {
        out.join(SITE_INDEX)
    }
}
