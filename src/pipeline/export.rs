//! Shared source-export destination checks, before any directory or write.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{InputRole, find_clash};
use crate::error::{Error, Result};

/// Validates every file/directory and their input/output aliases without writes.
pub(super) fn preflight(
    files: &[&Path], directory: Option<&Path>, inputs: &[(PathBuf, InputRole)],
    target: &str,
) -> Result<()> {
    let outputs = files.iter().copied().chain(directory);
    if let Some(clash) = find_clash(outputs, inputs) {
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            clash,
        )));
    }
    for (index, file) in files.iter().enumerate() {
        check_file(file, target)?;
        for previous in &files[..index] {
            let resolved = super::path_after_creating_dirs(file);
            let other = super::path_after_creating_dirs(previous);
            if super::same_file(file, previous)
                || resolved.is_some() && resolved == other
            {
                return Err(Error::Config(format!(
                    "{target} export outputs {} and {} name the same file",
                    previous.display(),
                    file.display(),
                )));
            }
        }
        if let Some(parent) = file.parent() {
            check_directory(parent)?;
        }
    }
    if let Some(directory) = directory {
        check_directory(directory)?;
    }
    Ok(())
}

fn check_file(path: &Path, target: &str) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => Err(Error::Config(format!(
            "{target} export output {} is not a regular file",
            path.display(),
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A dangling link cannot be checked for planned-output aliases.
            if fs::symlink_metadata(path).is_ok() {
                return Err(Error::Config(format!(
                    "{target} export output {} is a dangling symlink",
                    path.display(),
                )));
            }
            Ok(())
        }
        Err(error) => Err(super::io_error(error, "cannot write", path)),
    }
}

fn check_directory(path: &Path) -> Result<()> {
    for directory in
        path.ancestors().filter(|path| !path.as_os_str().is_empty())
    {
        match fs::metadata(directory) {
            Ok(metadata) if metadata.is_dir() => return Ok(()),
            Ok(_) => {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!(
                        "cannot create directory {}: not a directory",
                        directory.display(),
                    ),
                )));
            }
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    && fs::symlink_metadata(directory).is_err() => {}
            Err(error) => {
                return Err(super::io_error(
                    error, "cannot create directory", directory,
                ));
            }
        }
    }
    Ok(())
}
