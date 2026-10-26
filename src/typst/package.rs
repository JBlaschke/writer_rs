//! Embedded and downloaded Typst packages (typst-dialect.md §6.5).
//! Owner: T03; T31 adds ejection and installation. Stub created by T00.

use std::any::Any;
use std::io::{self, Read};
use std::path::PathBuf;

use typst::diag::{FileError, FileResult, PackageError};
use typst::foundations::Bytes;
use typst::syntax::VirtualPath;
use typst::syntax::package::{PackageSpec, PackageVersion};
use typst_kit::downloader::Downloader;
use typst_kit::files::FsRoot;
use typst_kit::packages::{FsPackages, SystemPackages, UniversePackages};

use super::WRITER_DOC_VERSION;
use crate::vendor;

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
    use crate::typst::MITEX_VERSION;

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
