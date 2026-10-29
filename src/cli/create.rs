//! Private no-clobber file/tree creation for scaffold and resource commands.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use super::{UsageError, display_path};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Directory {
    New,
    EmptyOrNew,
    Merge,
}

pub(super) fn file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    // Preserve existing supplied-path collision messages, including final
    // directories and dangling links, before resolving any parents.
    match fs::symlink_metadata(path) {
        Ok(_) => return Err(existing_path(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let spelling = path.as_os_str().to_string_lossy();
    let directory_spelling = spelling.ends_with(std::path::MAIN_SEPARATOR)
        || cfg!(windows) && spelling.ends_with('/');
    let name =
        path.file_name().filter(|_| !directory_spelling).ok_or_else(|| {
            UsageError(format!(
                "destination {} must name a file",
                display_path(path).display(),
            ))
        })?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Walking the full parent before create_dir_all preserves ordinary ..
    // and linked ancestors, while rejecting missing-component/.. and known
    // non-directory parents without creating an unexpected directory first.
    let (parent, _) = directory_target(parent)?;
    let resolved = parent.join(name);
    match fs::symlink_metadata(&resolved) {
        Ok(_) => return Err(existing_path(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::create_dir_all(&parent)?;
    let mut file =
        match OpenOptions::new().write(true).create_new(true).open(&resolved) {
            Ok(file) => file,
            Err(error)
                if error.kind() == std::io::ErrorKind::AlreadyExists
                    || fs::symlink_metadata(&resolved).is_ok() =>
            {
                return Err(existing_path(path));
            }
            Err(error) => return Err(error.into()),
        };
    file.write_all(bytes)?;
    Ok(())
}

pub(super) fn tree(
    dir: &Path, files: &[(PathBuf, Vec<u8>)], policy: Directory,
) -> anyhow::Result<Vec<PathBuf>> {
    let (root, exists) = directory_target(dir)?;
    if policy == Directory::New && exists {
        return Err(existing_path(dir));
    }
    if policy == Directory::EmptyOrNew && exists {
        check_empty(&root, dir)?;
    }
    preflight_files(&root, files)?;
    fs::create_dir_all(&root)?;
    if policy != Directory::Merge {
        check_empty(&root, dir)?;
    }
    // Recheck expected file/parent conflicts after creating the root. Each
    // actual file is still opened with create_new, including in merge mode.
    preflight_files(&root, files)?;
    let mut written = Vec::with_capacity(files.len());
    for (relative, bytes) in files {
        let path = root.join(relative);
        file(&path, bytes)?;
        written.push(path);
    }
    Ok(written)
}

fn directory_target(dir: &Path) -> anyhow::Result<(PathBuf, bool)> {
    if dir.as_os_str().is_empty() {
        return Err(UsageError("destination directory is empty".into()).into());
    }
    let mut resolved = if dir.is_absolute() {
        PathBuf::new()
    } else {
        fs::canonicalize(".")?
    };
    let mut exists = true;
    for component in dir.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                resolved.push(component)
            }
            Component::CurDir => {}
            Component::ParentDir if exists => {
                resolved.pop();
            }
            Component::ParentDir => {
                return Err(UsageError(format!(
                    "invalid destination {}: `..` follows a missing directory",
                    display_path(dir).display(),
                ))
                .into());
            }
            Component::Normal(name) => {
                resolved.push(name);
                if !exists {
                    continue;
                }
                let metadata = match fs::symlink_metadata(&resolved) {
                    Ok(metadata) => metadata,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::NotFound =>
                    {
                        exists = false;
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                if metadata.file_type().is_symlink() {
                    resolved = fs::canonicalize(&resolved).map_err(|error| {
                        if error.kind() == std::io::ErrorKind::PermissionDenied {
                            anyhow::Error::from(error)
                        } else {
                            UsageError(format!(
                                "destination {} has a broken symbolic link: {error}",
                                display_path(dir).display(),
                            )).into()
                        }
                    })?;
                }
                if !fs::metadata(&resolved)?.is_dir() {
                    return Err(UsageError(format!(
                        "destination {} is not a directory",
                        display_path(dir).display(),
                    ))
                    .into());
                }
            }
        }
    }
    if exists {
        resolved = fs::canonicalize(resolved)?;
    }
    Ok((resolved, exists))
}

fn preflight_files(
    root: &Path, files: &[(PathBuf, Vec<u8>)],
) -> anyhow::Result<()> {
    for (relative, _) in files {
        let mut parent = root.to_path_buf();
        if let Some(directory) = relative.parent() {
            for component in directory.components() {
                parent.push(component);
                match fs::symlink_metadata(&parent) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        let target = fs::canonicalize(&parent).map_err(|error| {
                            if error.kind() == std::io::ErrorKind::PermissionDenied {
                                anyhow::Error::from(error)
                            } else {
                                UsageError(format!(
                                    "scaffold directory has a broken symbolic link: {}",
                                    display_path(&parent).display(),
                                )).into()
                            }
                        })?;
                        if !target.starts_with(root) || !target.is_dir() {
                            return Err(UsageError(format!(
                                "scaffold directory {} does not stay within the project",
                                display_path(&parent).display(),
                            )).into());
                        }
                    }
                    Ok(metadata) if !metadata.is_dir() => {
                        return Err(UsageError(format!(
                            "scaffold parent is not a directory: {}",
                            display_path(&parent).display(),
                        ))
                        .into());
                    }
                    Ok(_) => {}
                    Err(error)
                        if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        let path = root.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(_) => return Err(existing_path(&path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn check_empty(root: &Path, shown: &Path) -> anyhow::Result<()> {
    if fs::read_dir(root)?.next().transpose()?.is_some() {
        return Err(UsageError(format!(
            "destination {} is not empty; use --force to keep unrelated files \
             (existing scaffold files are never replaced)",
            display_path(shown).display(),
        ))
        .into());
    }
    Ok(())
}

fn existing_path(path: &Path) -> anyhow::Error {
    UsageError(format!(
        "destination already exists: {}",
        display_path(path).display(),
    ))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_directory_parent_traversal_is_rejected_before_creation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("new/../project");
        let error = tree(
            &path,
            &[("file".into(), b"bytes".to_vec())],
            Directory::Merge,
        )
        .unwrap_err();
        assert_eq!(super::super::exit_code_for(&error), 2);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn existing_parent_traversal_and_unicode_names_keep_filesystem_semantics() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("existing")).unwrap();
        let path = root.path().join("existing/../My λ project");
        tree(&path, &[("file".into(), b"bytes".to_vec())], Directory::New)
            .unwrap();
        assert_eq!(
            fs::read(root.path().join("My λ project/file")).unwrap(),
            b"bytes"
        );
        assert!(root.path().join("existing").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn ordinary_linked_ancestors_and_safe_selected_parent_links_are_allowed() {
        let root = tempfile::tempdir().unwrap();
        let actual = root.path().join("actual");
        fs::create_dir(&actual).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        tree(
            &alias.join("project"),
            &[("file".into(), b"bytes".to_vec())],
            Directory::New,
        )
        .unwrap();
        assert_eq!(fs::read(actual.join("project/file")).unwrap(), b"bytes");
        fs::create_dir(actual.join("project/local-theme")).unwrap();
        std::os::unix::fs::symlink(
            actual.join("project/local-theme"),
            actual.join("project/theme"),
        )
        .unwrap();
        tree(
            &actual.join("project"),
            &[("theme/theme.toml".into(), b"theme".to_vec())],
            Directory::Merge,
        )
        .unwrap();
        assert_eq!(
            fs::read(actual.join("project/local-theme/theme.toml")).unwrap(),
            b"theme"
        );
    }

    #[cfg(unix)]
    #[test]
    fn planned_parent_links_cannot_redirect_a_merge_outside_the_project() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("theme"))
            .unwrap();
        let error = tree(
            root.path(),
            &[("theme/theme.toml".into(), b"bytes".to_vec())],
            Directory::Merge,
        )
        .unwrap_err();
        assert_eq!(super::super::exit_code_for(&error), 2);
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn file_parent_preflight_rejects_dotdot_collision_without_new_directories()
    {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("existing")).unwrap();
        let owned = root.path().join("existing/result.tex");
        fs::write(&owned, b"user template").unwrap();
        let destination = root.path().join("new/../existing/result.tex");
        let error = file(&destination, b"replacement").unwrap_err();
        assert_eq!(super::super::exit_code_for(&error), 2);
        assert!(!root.path().join("new").exists());
        assert_eq!(fs::read(&owned).unwrap(), b"user template");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn file_parent_type_failure_creates_no_intermediate_directories() {
        let root = tempfile::tempdir().unwrap();
        let blocked = root.path().join("blocked");
        fs::write(&blocked, b"user file").unwrap();
        for destination in [
            root.path().join("new/../blocked/result.tex"),
            root.path().join("blocked/new/result.tex"),
        ] {
            let error = file(&destination, b"bytes").unwrap_err();
            assert!(!root.path().join("new").exists());
            assert_eq!(fs::read(&blocked).unwrap(), b"user file");
            assert!(error.to_string().contains("directory"), "{error}");
        }
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn file_parent_preflight_keeps_existing_dotdot_and_unicode_names() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("existing")).unwrap();
        let path = root.path().join("existing/../My λ template.tex");
        file(&path, b"template").unwrap();
        assert_eq!(
            fs::read(root.path().join("My λ template.tex")).unwrap(),
            b"template"
        );
        let error = file(&path, b"replacement").unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "destination already exists: {}",
                display_path(&path).display(),
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_parent_preflight_keeps_ordinary_linked_ancestors() {
        let root = tempfile::tempdir().unwrap();
        let actual = root.path().join("actual");
        fs::create_dir(&actual).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        file(&alias.join("nested/template.tex"), b"template").unwrap();
        assert_eq!(
            fs::read(actual.join("nested/template.tex")).unwrap(),
            b"template"
        );
        assert_eq!(fs::read_link(&alias).unwrap(), actual);
    }

    #[test]
    fn new_file_destination_with_a_directory_separator_refuses_before_creation()
    {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("new/template.tex/");
        let error = file(&path, b"bytes").unwrap_err();
        assert_eq!(super::super::exit_code_for(&error), 2);
        assert!(!root.path().join("new").exists());
    }
}
