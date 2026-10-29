//! Built-in reference ejection; the remaining resources belong to T72.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use super::{
    EXIT_OK, EjectArgs, EjectCommand, GlobalArgs, UsageError, display_path,
    print_stdout,
};

/// Copies an editable Word reference without replacing an existing path.
pub fn run(args: &EjectArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode> {
    let EjectCommand::DocxReference(options) = &args.command else {
        return Err(crate::pipeline::not_implemented("eject", "T72").into());
    };
    let path = &options.file;
    let path_text = path.to_str().ok_or_else(|| {
        UsageError(
            "the reference path must be representable in UTF-8 TOML".into(),
        )
    })?;
    let setting = toml::Value::String(path_text.into()).to_string();
    let bytes = crate::docx::custom_style::reference_docx()?;
    if fs::symlink_metadata(path).is_ok() {
        return Err(existing_path(path));
    }
    if let Some(parent) =
        path.parent().filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut file =
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => file,
            Err(error)
                if error.kind() == std::io::ErrorKind::AlreadyExists
                    || fs::symlink_metadata(path).is_ok() =>
            {
                return Err(existing_path(path));
            }
            Err(error) => return Err(error.into()),
        };
    file.write_all(&bytes)?;
    if !global.quiet {
        eprintln!("Written {}", display_path(path).display());
    }
    print_stdout(&format!("[docx]\nreference_doc = {setting}"))?;
    Ok(ExitCode::from(EXIT_OK))
}

fn existing_path(path: &Path) -> anyhow::Error {
    UsageError(format!(
        "destination already exists: {}",
        display_path(path).display()
    ))
    .into()
}
