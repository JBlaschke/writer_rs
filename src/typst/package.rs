//! Embedded and downloaded Typst packages (typst-dialect.md §6.5).
//! Owner: T03 (loading), T31 (ejection and installation). Stub created by T00.

use std::any::Any;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use typst::diag::{FileError, FileResult, PackageError};
use typst::foundations::Bytes;
use typst::syntax::VirtualPath;
use typst::syntax::package::{PackageSpec, PackageVersion};
use typst_kit::downloader::Downloader;
use typst_kit::files::FsRoot;
use typst_kit::packages::{FsPackages, SystemPackages, UniversePackages};

use super::{MITEX_VERSION, WRITER_DOC_VERSION};
use crate::error::{Error, Result};
use crate::vendor;

/// Writes the bundled writer and mitex packages under a Typst package root.
///
/// Returns the written file paths in lexicographic order. Both version
/// destinations must be absent, including dangling symlinks. Their existence
/// is checked before creating directories or writing either package, and all
/// embedded bytes are loaded before writing. Unrelated files are preserved.
pub fn eject(dir: &Path) -> Result<Vec<PathBuf>> {
    let packages = [
        (
            "typst/writer-doc/".to_owned(),
            dir.join("writer/doc").join(WRITER_DOC_VERSION),
        ),
        (
            format!("vendor/typst-packages/preview/mitex/{MITEX_VERSION}/"),
            dir.join("preview/mitex").join(MITEX_VERSION),
        ),
    ];
    for (_, destination) in &packages {
        preflight_destination(destination)?;
    }

    let mut files = Vec::new();
    for (prefix, destination) in packages {
        let paths = vendor::list(&prefix);
        let manifest = format!("{prefix}typst.toml");
        if !paths.contains(&manifest) {
            return Err(Error::Other(format!(
                "embedded package manifest not found: {manifest}"
            )));
        }
        for path in paths {
            let bytes = vendor::get(&path).ok_or_else(|| {
                Error::Other(format!("embedded asset not found: {path}"))
            })?;
            files.push((destination.join(&path[prefix.len()..]), bytes));
        }
    }
    files.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (path, bytes) in &files {
        // Each output is a file beneath a non-empty package version path.
        let parent = path.parent().expect("package file has a parent");
        fs::create_dir_all(parent)?;
        fs::write(path, bytes)?;
    }
    Ok(files.into_iter().map(|(path, _)| path).collect())
}

/// Returns Typst's system data package root without creating it.
///
/// Uses the same platform-specific lookup as Typst's package loader. Fails
/// with [`Error::Config`] if the operating system provides no data directory.
pub fn install_dir() -> Result<PathBuf> {
    FsPackages::system_data()
        .map(|packages| packages.path().to_owned())
        .ok_or_else(|| {
            Error::Config(
                "could not determine Typst's system data package directory"
                    .into(),
            )
        })
}

fn preflight_destination(destination: &Path) -> Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(Error::Config(format!(
                "package version destination already exists: {}",
                destination.display()
            )));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    for parent in destination.ancestors().skip(1) {
        if parent.as_os_str().is_empty() {
            break;
        }
        match fs::symlink_metadata(parent) {
            Ok(_) => {
                if fs::metadata(parent)?.is_dir() {
                    return Ok(());
                }
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!(
                        "package parent is not a directory: {}",
                        parent.display()
                    ),
                )
                .into());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Resolves embedded packages before configured and system package roots.
pub(crate) struct PackageStore {
    paths: Vec<FsPackages>,
    system: SystemPackages,
}

impl PackageStore {
    pub(crate) fn new(paths: Vec<PathBuf>, allow_packages: bool) -> Self {
        let universe = if allow_packages {
            UniversePackages::new(WriterDownloader::new())
        } else {
            UniversePackages::new(NoDownloader)
        };
        Self {
            paths: paths.into_iter().map(FsPackages::new).collect(),
            system: SystemPackages::from_parts(
                FsPackages::system_data(),
                FsPackages::system_cache(),
                universe,
            ),
        }
    }

    /// Returns a real path only for packages read from the filesystem.
    pub(crate) fn load(
        &self, spec: &PackageSpec, path: &VirtualPath,
    ) -> FileResult<(Bytes, Option<PathBuf>)> {
        if let Some(dir) = embedded_dir(spec) {
            let key = format!("{dir}{}", path.get_without_slash());
            let bytes = vendor::get(&key)
                .ok_or_else(|| FileError::NotFound(PathBuf::from(key)))?;
            return Ok((Bytes::new(bytes.into_owned()), None));
        }
        for packages in &self.paths {
            if let Some(root) = packages.obtain(spec) {
                return load_disk(root, path);
            }
        }
        match self.system.obtain(spec) {
            Ok(root) => load_disk(root, path),
            Err(PackageError::NotFound(_)) if is_writer_doc(spec) => {
                // The embedded package's version is a valid constant.
                let latest = WRITER_DOC_VERSION
                    .parse::<PackageVersion>()
                    .expect("writer package version is valid");
                if spec.version == latest {
                    Err(PackageError::NotFound(spec.clone()).into())
                } else {
                    Err(PackageError::VersionNotFound(spec.clone(), latest)
                        .into())
                }
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn embedded_dir(spec: &PackageSpec) -> Option<String> {
    if is_writer_doc(spec) && spec.version.to_string() == WRITER_DOC_VERSION {
        let dir = "typst/writer-doc/";
        return vendor::get(&format!("{dir}typst.toml"))
            .map(|_| dir.to_owned());
    }
    if spec.namespace == "preview" {
        vendor::typst_package_dir(
            &spec.namespace,
            &spec.name,
            &spec.version.to_string(),
        )
    } else {
        None
    }
}

fn is_writer_doc(spec: &PackageSpec) -> bool {
    spec.namespace == "writer" && spec.name == "doc"
}

fn load_disk(
    root: FsRoot, path: &VirtualPath,
) -> FileResult<(Bytes, Option<PathBuf>)> {
    let real = root.resolve(path)?;
    let bytes = root.load(path)?;
    let real = std::fs::canonicalize(&real).unwrap_or(real);
    Ok((bytes, Some(real)))
}

#[derive(Debug)]
struct NoDownloader;

impl Downloader for NoDownloader {
    fn stream(
        &self, _: &dyn Any, _: &str,
    ) -> io::Result<(Option<usize>, Box<dyn Read>)> {
        Err(io::ErrorKind::NotFound.into())
    }
}

#[derive(Debug)]
struct WriterDownloader {
    agent: ureq::Agent,
}

impl WriterDownloader {
    fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(format!("writer/{}", env!("CARGO_PKG_VERSION")))
            .build();
        Self { agent: config.into() }
    }
}

impl Downloader for WriterDownloader {
    fn stream(
        &self, _: &dyn Any, url: &str,
    ) -> io::Result<(Option<usize>, Box<dyn Read>)> {
        let response = self.agent.get(url).call().map_err(|error| {
            if matches!(error, ureq::Error::StatusCode(404)) {
                io::Error::new(io::ErrorKind::NotFound, error)
            } else {
                io::Error::other(error)
            }
        })?;
        let body = response.into_body();
        let length = body.content_length().and_then(|n| n.try_into().ok());
        Ok((length, Box::new(body.into_reader())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_destinations(root: &Path) -> [PathBuf; 2] {
        [
            root.join("writer/doc").join(WRITER_DOC_VERSION),
            root.join("preview/mitex").join(MITEX_VERSION),
        ]
    }

    #[test]
    fn ejection_writes_every_embedded_byte_and_preserves_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("packages");
        let unrelated = root.join("writer/doc/9.9.9/lib.typ");
        fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
        fs::write(&unrelated, "keep").unwrap();
        let prefixes = [
            "typst/writer-doc/".to_owned(),
            format!("vendor/typst-packages/preview/mitex/{MITEX_VERSION}/"),
        ];
        let mut expected = Vec::new();
        for (prefix, destination) in
            prefixes.into_iter().zip(package_destinations(&root))
        {
            for asset in vendor::list(&prefix) {
                let path = destination.join(&asset[prefix.len()..]);
                expected.push((path, vendor::get(&asset).unwrap()));
            }
        }
        expected.sort_by(|(left, _), (right, _)| left.cmp(right));

        let paths = eject(&root).unwrap();
        assert_eq!(
            paths,
            expected.iter().map(|(path, _)| path.clone()).collect::<Vec<_>>()
        );
        assert!(!paths.is_empty());
        for (path, bytes) in expected {
            assert_eq!(fs::read(&path).unwrap(), bytes.as_ref(), "{path:?}");
        }
        assert_eq!(fs::read(&unrelated).unwrap(), b"keep");
    }

    #[test]
    fn either_existing_version_file_or_directory_prevents_all_writes() {
        for blocked_index in 0..2 {
            for directory in [true, false] {
                let dir = tempfile::tempdir().unwrap();
                let destinations = package_destinations(dir.path());
                let blocked = &destinations[blocked_index];
                fs::create_dir_all(blocked.parent().unwrap()).unwrap();
                if directory {
                    fs::create_dir(blocked).unwrap();
                    fs::write(blocked.join("keep.txt"), "keep").unwrap();
                } else {
                    fs::write(blocked, "keep").unwrap();
                }

                let error = eject(dir.path()).unwrap_err();
                assert!(matches!(error, Error::Config(_)), "{error:?}");
                assert!(error.to_string().contains("already exists"));
                assert!(!destinations[1 - blocked_index].exists());
                let marker = if directory {
                    blocked.join("keep.txt")
                } else {
                    blocked.clone()
                };
                assert_eq!(fs::read(marker).unwrap(), b"keep");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn either_version_symlink_including_dangling_prevents_all_writes() {
        for blocked_index in 0..2 {
            for target_kind in ["file", "directory", "dangling"] {
                let dir = tempfile::tempdir().unwrap();
                let destinations = package_destinations(dir.path());
                let blocked = &destinations[blocked_index];
                let target = dir.path().join("target");
                match target_kind {
                    "file" => fs::write(&target, "keep").unwrap(),
                    "directory" => fs::create_dir(&target).unwrap(),
                    _ => {}
                }
                fs::create_dir_all(blocked.parent().unwrap()).unwrap();
                std::os::unix::fs::symlink(&target, blocked).unwrap();

                let error = eject(dir.path()).unwrap_err();
                assert!(matches!(error, Error::Config(_)), "{error:?}");
                assert!(!destinations[1 - blocked_index].exists());
                assert_eq!(fs::read_link(blocked).unwrap(), target);
            }
        }
    }

    #[test]
    fn parent_files_prevent_writes_to_either_package() {
        for blocked_index in 0..2 {
            let dir = tempfile::tempdir().unwrap();
            let destinations = package_destinations(dir.path());
            let blocked = destinations[blocked_index].parent().unwrap();
            fs::create_dir_all(blocked.parent().unwrap()).unwrap();
            fs::write(blocked, "keep").unwrap();

            let error = eject(dir.path()).unwrap_err();
            assert!(matches!(error, Error::Io(_)), "{error:?}");
            assert!(!destinations[1 - blocked_index].exists());
            assert_eq!(fs::read(blocked).unwrap(), b"keep");
        }
    }

    #[test]
    fn output_root_file_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("packages");
        fs::write(&root, "keep").unwrap();
        assert!(matches!(eject(&root), Err(Error::Io(_))));
        assert_eq!(fs::read(root).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn dangling_parent_symlink_prevents_writes_to_either_package() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("absent");
        let parent = dir.path().join("preview");
        std::os::unix::fs::symlink(&target, &parent).unwrap();

        assert!(matches!(eject(dir.path()), Err(Error::Io(_))));
        assert!(!dir.path().join("writer").exists());
        assert!(!target.exists());
        assert_eq!(fs::read_link(parent).unwrap(), target);
    }

    #[test]
    fn installation_path_matches_typst_system_data() {
        match FsPackages::system_data() {
            Some(packages) => {
                let path = packages.path();
                let existed = path.exists();
                assert_eq!(install_dir().unwrap(), path);
                assert_eq!(path.exists(), existed);
            }
            None => assert!(matches!(install_dir(), Err(Error::Config(_)))),
        }
    }

    #[test]
    fn no_downloader_never_opens_a_connection() {
        let error = NoDownloader.stream(&(), "https://example.invalid");
        assert!(matches!(error, Err(e) if e.kind() == io::ErrorKind::NotFound));
    }

    #[test]
    fn embedded_packages_precede_configured_paths() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("preview/mitex").join(MITEX_VERSION);
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join("typst.toml"), "override").unwrap();
        let store = PackageStore::new(vec![dir.path().to_owned()], false);
        let spec = format!("@preview/mitex:{MITEX_VERSION}").parse().unwrap();
        let (bytes, path) = store
            .load(&spec, &VirtualPath::new("typst.toml").unwrap())
            .unwrap();
        assert!(path.is_none());
        assert_ne!(bytes.as_slice(), b"override");
    }

    #[test]
    fn configured_roots_keep_their_order_and_disk_origin() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for (dir, text) in [(first.path(), "first"), (second.path(), "second")]
        {
            let package = dir.join("local/hello/0.1.0");
            std::fs::create_dir_all(&package).unwrap();
            std::fs::write(package.join("lib.typ"), text).unwrap();
        }
        let store = PackageStore::new(
            vec![first.path().to_owned(), second.path().to_owned()],
            false,
        );
        let spec = "@local/hello:0.1.0".parse().unwrap();
        let (bytes, path) =
            store.load(&spec, &VirtualPath::new("lib.typ").unwrap()).unwrap();
        assert_eq!(bytes.as_slice(), b"first");
        assert_eq!(
            path,
            Some(
                std::fs::canonicalize(
                    first.path().join("local/hello/0.1.0/lib.typ"),
                )
                .unwrap()
            ),
        );
    }

    #[test]
    fn missing_file_in_embedded_package_does_not_fall_through() {
        let store = PackageStore::new(Vec::new(), false);
        let spec = format!("@preview/mitex:{MITEX_VERSION}").parse().unwrap();
        assert!(matches!(
            store.load(&spec, &VirtualPath::new("missing.typ").unwrap()),
            Err(FileError::NotFound(_))
        ));
    }

    #[test]
    fn another_writer_version_can_come_from_a_configured_root() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("writer/doc/9.9.9");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join("lib.typ"), "local").unwrap();
        let store = PackageStore::new(vec![dir.path().to_owned()], false);
        let spec = "@writer/doc:9.9.9".parse().unwrap();
        let (bytes, path) =
            store.load(&spec, &VirtualPath::new("lib.typ").unwrap()).unwrap();
        assert_eq!(bytes.as_slice(), b"local");
        assert!(path.is_some());
    }
}
