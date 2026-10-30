//! Built-in theme, template, package and editable reference ejection.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::create::{self, Directory};
use super::{
    EXIT_OK, EjectArgs, EjectCommand, EjectPackageArgs, GlobalArgs, UsageError,
    display_path, print_stdout,
};

/// Copies built-in resources without replacing existing selected paths.
pub fn run(args: &EjectArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode> {
    match &args.command {
        EjectCommand::Theme(options) => theme(&options.dir, global)?,
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

fn theme(dir: &Path, global: &GlobalArgs) -> anyhow::Result<()> {
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
