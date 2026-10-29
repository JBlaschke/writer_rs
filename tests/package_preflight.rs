//! Public package ejection never resolves a missing-component/.. into owned files.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use writer::error::Error;
use writer::typst::{MITEX_VERSION, WRITER_DOC_VERSION, package};

fn inventory(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_owned();
            if path.is_dir() {
                found.insert(relative, None);
                pending.push(path);
            } else {
                found.insert(relative, Some(fs::read(path).unwrap()));
            }
        }
    }
    found
}

#[test]
fn missing_parent_dotdot_cannot_overwrite_either_existing_package_version() {
    for version in [
        format!("writer/doc/{WRITER_DOC_VERSION}"),
        format!("preview/mitex/{MITEX_VERSION}"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let packages = directory.path().join("packages");
        let owned = packages.join(&version);
        fs::create_dir_all(&owned).unwrap();
        fs::write(owned.join("lib.typ"), b"owned package implementation")
            .unwrap();
        fs::write(owned.join("sentinel"), b"unrelated package bytes").unwrap();
        let before = inventory(directory.path());
        let destination = directory.path().join("new/../packages");
        let error = package::eject(&destination).unwrap_err();
        assert!(matches!(error, Error::Config(_)), "{error:?}");
        assert!(error.to_string().contains("missing directory"), "{error}");
        assert_eq!(inventory(directory.path()), before);
        assert!(!directory.path().join("new").exists());
        assert_eq!(
            fs::read(owned.join("lib.typ")).unwrap(),
            b"owned package implementation"
        );
    }
}

#[test]
fn existing_parent_dotdot_retains_normal_public_ejection_paths() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("existing")).unwrap();
    let destination = directory.path().join("existing/../packages");
    let written = package::eject(&destination).unwrap();
    assert!(!written.is_empty());
    assert!(written.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(written.iter().all(|path| path.starts_with(&destination)));
    assert!(
        directory
            .path()
            .join(format!("packages/writer/doc/{WRITER_DOC_VERSION}/lib.typ",))
            .is_file()
    );
    assert!(
        directory
            .path()
            .join(format!("packages/preview/mitex/{MITEX_VERSION}/lib.typ",))
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn existing_symlink_parent_dotdot_keeps_filesystem_traversal_semantics() {
    let directory = tempfile::tempdir().unwrap();
    let actual = directory.path().join("elsewhere/child");
    fs::create_dir_all(&actual).unwrap();
    let alias = directory.path().join("alias");
    std::os::unix::fs::symlink(&actual, &alias).unwrap();
    let destination = alias.join("../packages");
    let written = package::eject(&destination).unwrap();
    assert!(!written.is_empty());
    assert!(
        directory
            .path()
            .join(format!(
                "elsewhere/packages/writer/doc/{WRITER_DOC_VERSION}/lib.typ",
            ))
            .is_file()
    );
    assert!(!directory.path().join("packages").exists());
    assert_eq!(fs::read_link(&alias).unwrap(), actual);
}
