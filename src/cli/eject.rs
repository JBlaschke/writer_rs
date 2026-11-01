//! Built-in theme, template, package and editable reference ejection.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::create::{self, Directory};
use super::{
    EXIT_OK, EjectArgs, EjectCommand, EjectPackageArgs, GlobalArgs, UsageError,
    display_path, input_kind, print_stdout, report,
};

/// Copies built-in resources without replacing existing selected paths.
pub fn run(args: &EjectArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode> {
    match &args.command {
        EjectCommand::Theme(options) => {
            theme(&options.dir, options.design.as_deref(), global)?;
        }
        EjectCommand::TypstPackage(options) => packages(options, global)?,
        EjectCommand::DocxReference(options) => {
            let setting = setting(&options.file, "reference")?;
            let bytes = crate::docx::custom_style::reference_docx()?;
            create::file(&options.file, &bytes)?;
            written(&options.file, global);
            print_stdout(&format!("[docx]\nreference_doc = {setting}"))?;
        }
        EjectCommand::TypstTemplate(options) => {
            let setting = setting(&options.file, "template")?;
            let key = "typst/writer-doc/template.typ";
            let bytes = crate::vendor::get(key).ok_or_else(|| {
                anyhow::anyhow!("embedded asset not found: {key}")
            })?;
            create::file(&options.file, &bytes)?;
            written(&options.file, global);
            print_stdout(&format!("[pdf]\ntemplate = {setting}"))?;
        }
        EjectCommand::LatexTemplate(options) => {
            let setting = setting(&options.file, "template")?;
            create::file(
                &options.file,
                crate::latex::template::SAMPLE.as_bytes(),
            )?;
            written(&options.file, global);
            print_stdout(&format!("[latex]\ntemplate = {setting}"))?;
        }
    }
    Ok(ExitCode::from(EXIT_OK))
}

fn theme(
    dir: &Path, design_input: Option<&Path>, global: &GlobalArgs,
) -> anyhow::Result<()> {
    let setting = setting(dir, "theme")?;
    let prefix = "themes/default/";
    let mut files = Vec::new();
    for key in crate::vendor::list(prefix) {
        let bytes = crate::vendor::get(&key)
            .ok_or_else(|| anyhow::anyhow!("embedded asset not found: {key}"))?
            .into_owned();
        files.push((PathBuf::from(&key[prefix.len()..]), bytes));
    }
    if !files.iter().any(|(path, _)| path == Path::new("theme.toml")) {
        return Err(anyhow::anyhow!(
            "embedded default theme manifest not found"
        ));
    }
    let design = match design_input {
        Some(input) => Some(design_files(input, global, &mut files)?),
        None => None,
    };
    let (_, manifest) = files
        .iter_mut()
        .find(|(path, _)| path == Path::new("theme.toml"))
        .expect("the embedded manifest was checked above");
    manifest.extend_from_slice(b"\n");
    manifest.extend_from_slice(crate::design::TEMPLATE_TOML.as_bytes());
    if let Some(design) = design {
        manifest.extend_from_slice(b"\n");
        manifest.extend_from_slice(
            toml::to_string_pretty(&serde_json::json!({"design": design}))?
                .as_bytes(),
        );
    }
    let paths = create::tree(dir, &files, Directory::New)?;
    if !global.quiet {
        eprintln!(
            "Written {} ({} files)",
            display_path(dir).display(),
            paths.len()
        );
    }
    print_stdout(&format!("[html]\ntheme = {setting}"))?;
    Ok(())
}

fn design_files(
    input: &Path, global: &GlobalArgs, files: &mut Vec<(PathBuf, Vec<u8>)>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut opts = crate::pipeline::BuildOptions::new(input);
    opts.format = Some(input_kind(input)?);
    opts.config_path = global.config.clone();
    let prepared = crate::pipeline::prepare(&opts).map_err(|failure| {
        report::report_build_failure(failure, input, global)
    })?;
    let mut table = crate::pipeline::merged_design_table(&prepared, &opts)?;
    let mut paths = BTreeMap::new();
    let mut names = BTreeSet::new();
    for path in prepared.design.referenced_files() {
        let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(
            || UsageError("design asset names must be UTF-8".into()),
        )?;
        let mut candidate = name.to_string();
        let mut suffix = 1;
        while !names.insert(candidate.clone()) {
            let stem =
                path.file_stem().and_then(|name| name.to_str()).unwrap_or(name);
            candidate = match path.extension().and_then(|ext| ext.to_str()) {
                Some(ext) => format!("{stem}-{suffix}.{ext}"),
                None => format!("{stem}-{suffix}"),
            };
            suffix += 1;
        }
        let relative = PathBuf::from("assets/design").join(candidate);
        files.push((relative.clone(), std::fs::read(&path)?));
        paths.insert(path, relative.to_string_lossy().replace('\\', "/"));
    }
    rewrite_paths(&mut table, &paths);
    Ok(table)
}

fn rewrite_paths(
    table: &mut serde_json::Map<String, serde_json::Value>,
    paths: &BTreeMap<PathBuf, String>,
) {
    use serde_json::Value;
    fn rewrite(value: &mut Value, paths: &BTreeMap<PathBuf, String>) {
        if let Value::String(path) = value
            && let Some(relative) = paths.get(Path::new(path))
        {
            *path = relative.clone();
        }
    }
    fn furniture(value: &mut Value, paths: &BTreeMap<PathBuf, String>) {
        if let Some(table) = value.as_object_mut() {
            if let Some(image) = table.get_mut("image") {
                if let Some(spec) = image.as_object_mut() {
                    if let Some(src) = spec.get_mut("src") {
                        rewrite(src, paths);
                    }
                } else {
                    rewrite(image, paths);
                }
            }
            for variant in ["first", "rest"] {
                if let Some(value) = table.get_mut(variant) {
                    furniture(value, paths);
                }
            }
        }
    }
    if let Some(fonts) = table.get_mut("fonts").and_then(Value::as_object_mut) {
        for role in ["body", "heading", "mono"] {
            if let Some(files) = fonts
                .get_mut(role)
                .and_then(|font| font.get_mut("files"))
                .and_then(Value::as_array_mut)
            {
                for file in files {
                    rewrite(file, paths);
                }
            }
        }
    }
    if let Some(page) = table.get_mut("page").and_then(Value::as_object_mut) {
        for slot in ["header", "footer"] {
            if let Some(value) = page.get_mut(slot) {
                furniture(value, paths);
            }
        }
    }
}

fn packages(
    options: &EjectPackageArgs, global: &GlobalArgs,
) -> anyhow::Result<()> {
    let dir = package_destination(options)?;
    let setting = setting(&dir, "package")?;
    let paths = crate::typst::package::eject(&dir)?;
    if !global.quiet {
        eprintln!(
            "Written {} ({} files)",
            display_path(&dir).display(),
            paths.len()
        );
    }
    print_stdout(&format!("[typst]\npackage_paths = [{setting}]"))?;
    Ok(())
}

fn package_destination(options: &EjectPackageArgs) -> crate::Result<PathBuf> {
    if options.install {
        crate::typst::package::install_dir()
    } else {
        Ok(options.dir.clone())
    }
}

fn setting(path: &Path, kind: &str) -> anyhow::Result<String> {
    let text = path.to_str().ok_or_else(|| {
        UsageError(format!(
            "the {kind} path must be representable in UTF-8 TOML"
        ))
    })?;
    Ok(toml::Value::String(text.into()).to_string())
}

fn written(path: &Path, global: &GlobalArgs) {
    if !global.quiet {
        eprintln!("Written {}", display_path(path).display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_install_selects_the_public_platform_root_without_writing() {
        let ordinary =
            EjectPackageArgs { dir: "packages".into(), install: false };
        assert_eq!(package_destination(&ordinary).unwrap(), ordinary.dir);
        let install =
            EjectPackageArgs { dir: "unused-root".into(), install: true };
        match crate::typst::package::install_dir() {
            Ok(expected) => {
                assert_eq!(package_destination(&install).unwrap(), expected)
            }
            Err(_) => assert!(package_destination(&install).is_err()),
        }
    }
}
