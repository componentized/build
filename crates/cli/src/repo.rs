//! A repository synced with the componentized build, and what it names of other repositories: the WIT
//! packages it names and the images of the components it pulls.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;
use semver::Version;

use crate::id::{Id, tag_version};

/// A file the build syncs into the repository, the root of the repository holds `build/build.mk`.
const BUILD_MK: &str = "build/build.mk";

/// The files larger than this aren't sources, they're left alone.
const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;

/// The repository holding the directory, the nearest directory with `build/build.mk`, the directory
/// itself or one above it.
pub fn find_root(dir: &Path) -> Result<PathBuf> {
    let dir = dir
        .canonicalize()
        .with_context(|| format!("the directory {} doesn't exist", dir.display()))?;
    match dir.ancestors().find(|dir| dir.join(BUILD_MK).is_file()) {
        Some(root) => Ok(root.to_path_buf()),
        None => bail!(
            "{} isn't in a repository synced with the componentized build, there's no {BUILD_MK} in it \
             or a directory above it\n\
             \n  run it in such a repository, or name one with --directory",
            dir.display()
        ),
    }
}

/// A text file of the repository, by its path relative to the root.
pub struct SourceFile {
    pub path: PathBuf,
    pub content: String,
}

impl SourceFile {
    /// A file that pins the images of a component pulled with wkg, the release build's first and the
    /// debug build's second, e.g. `components/dep-componentized-secrets-gate/dep-componentized-secrets-gate.wkg`.
    pub fn is_wkg(&self) -> bool {
        self.path.extension().is_some_and(|ext| ext == "wkg")
    }

    fn is_wit(&self) -> bool {
        self.path.extension().is_some_and(|ext| ext == "wit")
    }

    /// A file documenting the repository, e.g. a README, what it names is updated, but isn't what
    /// the repository depends on, e.g. a version mentioned in passing.
    pub fn is_doc(&self) -> bool {
        self.path
            .extension()
            .is_some_and(|ext| ["md", "markdown", "txt"].iter().any(|doc| ext.eq_ignore_ascii_case(doc)))
    }
}

/// A repository synced with the componentized build.
pub struct Repo {
    pub root: PathBuf,
    /// The text files of the repository, but those git ignores, the build synced into it, `build/`,
    /// the WIT fetched into `wit/deps` and the locks.
    pub files: Vec<SourceFile>,
    /// The wkg.lock files of the repository, relative to the root.
    pub locks: Vec<PathBuf>,
}

impl Repo {
    pub fn open(root: &Path) -> Result<Self> {
        let mut files = Vec::new();
        let mut locks = Vec::new();
        let walk = ignore::WalkBuilder::new(root)
            // .gitignore applies to a copy of a repository too, without its .git
            .require_git(false)
            .filter_entry({
                let root = root.to_path_buf();
                move |entry| {
                    let path = entry.path();
                    let dir = entry.file_type().is_some_and(|t| t.is_dir());
                    !(dir
                        && (path == root.join("build")
                            || path == root.join("target")
                            || (path.ends_with("deps")
                                && path.parent().is_some_and(|p| p.ends_with("wit")))))
                }
            })
            .build();
        for entry in walk {
            let entry = entry.context("couldn't list the files of the repository")?;
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
            if relative.file_name().is_some_and(|name| name == "wkg.lock") {
                locks.push(relative);
                continue;
            }
            if relative.file_name().is_some_and(|name| name == "Cargo.lock") {
                continue;
            }
            if entry.metadata().map(|m| m.len()).unwrap_or(0) > MAX_FILE_SIZE {
                continue;
            }
            let bytes = std::fs::read(path).with_context(|| format!("couldn't read {}", path.display()))?;
            // not a text file, e.g. an image
            let Ok(content) = String::from_utf8(bytes) else {
                continue;
            };
            files.push(SourceFile { path: relative, content });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        locks.sort();
        Ok(Self {
            root: root.to_path_buf(),
            files,
            locks,
        })
    }

    /// The WIT packages the repository declares, `package <id>@<version>;`, e.g. `componentized:valkey`.
    pub fn own_packages(&self) -> BTreeSet<Id> {
        static PACKAGE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(&format!(r"(?m)^\s*package\s+{}(?:@|\s*;|\s*\{{)", *ID)).unwrap()
        });
        self.files
            .iter()
            .filter(|file| file.is_wit())
            .flat_map(|file| PACKAGE.captures_iter(&file.content))
            .map(|captures| Id::new(&captures["namespace"], &captures["name"]))
            .collect()
    }

    /// Each WIT package the repository names at a version, e.g. `componentized:secrets/store@<version>`,
    /// in any of its files, WIT, WAC, Rust, Markdown, ..., but its own packages.
    pub fn package_refs(&self) -> Vec<PackageRef> {
        let own = self.own_packages();
        self.files
            .iter()
            .enumerate()
            .flat_map(|(file, source)| package_refs(&source.content).map(move |r| PackageRef { file, ..r }))
            .filter(|r| !own.contains(&r.id))
            .collect()
    }

    /// Each image the repository names, `<registry>/<repository>:<tag>`, with or without a digest,
    /// e.g. `ghcr.io/componentized/secrets/gate:0.1.1-dev@sha256:...`, in any of its files.
    pub fn image_refs(&self) -> Vec<ImageRef> {
        self.files
            .iter()
            .enumerate()
            .flat_map(|(file, source)| image_refs(&source.content).map(move |r| ImageRef { file, ..r }))
            .collect()
    }

    /// The images of the components the repository pulls, those its .wkg files name.
    pub fn pulled_images(&self) -> BTreeSet<Image> {
        self.image_refs()
            .into_iter()
            .filter(|r| self.files[r.file].is_wkg())
            .map(|r| r.image)
            .collect()
    }

    /// What the repository depends on, the WIT packages it names, other than its own, and the
    /// components it pulls, by the id of the package of the repository they're from.
    pub fn dependencies(&self) -> BTreeMap<Id, Dependency> {
        let mut dependencies: BTreeMap<Id, Dependency> = BTreeMap::new();
        let dependency = |dependencies: &mut BTreeMap<Id, Dependency>, id: &Id| {
            dependencies.entry(id.clone()).or_insert_with(|| Dependency {
                id: id.clone(),
                interface: BTreeSet::new(),
                documented: BTreeSet::new(),
                components: BTreeMap::new(),
            });
        };
        // a package only a document names isn't a dependency, e.g. one mentioned in passing
        let refs = self.package_refs();
        for r in refs.iter().filter(|r| !self.files[r.file].is_doc()) {
            dependency(&mut dependencies, &r.id);
            dependencies.get_mut(&r.id).unwrap().interface.insert(r.version.clone());
        }
        for r in refs.iter().filter(|r| self.files[r.file].is_doc()) {
            if let Some(dependency) = dependencies.get_mut(&r.id) {
                dependency.documented.insert(r.version.clone());
            }
        }
        let pulled = self.pulled_images();
        for r in self.image_refs() {
            if !pulled.contains(&r.image) {
                continue;
            }
            let Some((id, Some(component))) = r.image.package() else {
                continue;
            };
            let Some((version, _)) = tag_version(&r.tag) else {
                continue;
            };
            dependency(&mut dependencies, &id);
            dependencies
                .get_mut(&id)
                .unwrap()
                .components
                .entry(component)
                .or_default()
                .insert(version);
        }
        dependencies
    }
}

/// A dependency of the repository on another repository, on its interface, a WIT package, and on
/// the components it publishes.
#[derive(Debug)]
pub struct Dependency {
    pub id: Id,
    /// The versions of the WIT package the repository names, none when it only pulls components.
    pub interface: BTreeSet<Version>,
    /// The versions of the WIT package only its documents name, e.g. a README.
    pub documented: BTreeSet<Version>,
    /// The components the repository pulls, e.g. `gate` of `ghcr.io/componentized/secrets/gate`, by
    /// the versions it pulls.
    pub components: BTreeMap<String, BTreeSet<Version>>,
}

impl Dependency {
    /// Every version the repository depends on, of the interface and the components, not those
    /// only its documents name.
    pub fn versions(&self) -> BTreeSet<Version> {
        self.interface
            .iter()
            .chain(self.components.values().flatten())
            .cloned()
            .collect()
    }
}

/// An OCI image, without a tag, e.g. `ghcr.io/componentized/secrets/gate`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Image {
    /// The registry, e.g. `ghcr.io`.
    pub registry: String,
    /// The repository in the registry, e.g. `componentized/secrets/gate`.
    pub repository: String,
}

impl Image {
    pub fn new(registry: impl Into<String>, repository: impl Into<String>) -> Self {
        Self {
            registry: registry.into(),
            repository: repository.into(),
        }
    }

    /// The package of the repository the image is published from, and the component, as the build
    /// publishes them, e.g. `componentized:secrets` and `gate` for `ghcr.io/componentized/secrets/gate`,
    /// `componentized:secrets` alone for its interface, `ghcr.io/componentized/secrets`.
    pub fn package(&self) -> Option<(Id, Option<String>)> {
        let mut segments = self.repository.split('/');
        let namespace = segments.next()?;
        let name = segments.next()?;
        let component = segments.collect::<Vec<_>>().join("-");
        let id: Id = format!("{namespace}:{name}").parse().ok()?;
        Some((id, (!component.is_empty()).then_some(component)))
    }
}

impl fmt::Display for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.registry, self.repository)
    }
}

/// A WIT package named at a version, e.g. `componentized:secrets/store@<version>`.
#[derive(Debug)]
pub struct PackageRef {
    /// The index of the file in [`Repo::files`].
    pub file: usize,
    pub id: Id,
    pub version: Version,
    /// Where the version is in the file.
    pub version_range: Range<usize>,
}

/// An image named at a tag, with or without a digest, e.g. `ghcr.io/componentized/secrets/gate:0.1.1-dev`.
#[derive(Debug)]
pub struct ImageRef {
    /// The index of the file in [`Repo::files`].
    pub file: usize,
    pub image: Image,
    pub tag: String,
    pub digest: Option<String>,
    /// Where the reference is in the file, the image, its tag and its digest.
    pub range: Range<usize>,
}

/// A WIT label, see [`crate::id`].
const LABEL: &str = r"[a-z][a-z0-9]*(?:-[a-z0-9]+)*";
/// The namespace and name of a WIT package, e.g. `componentized:secrets`.
static ID: LazyLock<String> =
    LazyLock::new(|| format!(r"(?P<namespace>{LABEL}):(?P<name>{LABEL})"));
/// A semantic version, without what can't be part of it but follows it in a text, e.g. the period
/// ending a sentence.
const VERSION: &str = r"(?P<version>[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?)";

/// Whether a match starting after the text is in the middle of a word, e.g. `xcomponentized:secrets`.
fn continues_word(text: &str) -> bool {
    text.chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

/// The WIT packages named at a version in a text, e.g. `componentized:secrets@<version>`,
/// `componentized:secrets/store@<version>` or `use componentized:secrets/types@<version>.{secret}`.
fn package_refs(text: &str) -> impl Iterator<Item = PackageRef> + '_ {
    static PACKAGE_REF: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(&format!(r"{}(?:/{LABEL})*@{VERSION}", *ID)).unwrap()
    });
    PACKAGE_REF.captures_iter(text).filter_map(|captures| {
        let all = captures.get(0).unwrap();
        if continues_word(&text[..all.start()]) {
            return None;
        }
        // a package's declaration isn't a dependency, e.g. `package componentized:secrets@<version>;`
        if text[..all.start()].trim_end().ends_with("package") && text[..all.start()].ends_with(char::is_whitespace) {
            return None;
        }
        let version = captures.name("version").unwrap();
        // what follows a version can't continue it, e.g. `0.1.0x`
        if text[version.end()..].starts_with(|c: char| c.is_ascii_alphanumeric()) {
            return None;
        }
        Some(PackageRef {
            file: 0,
            id: Id::new(&captures["namespace"], &captures["name"]),
            version: Version::parse(version.as_str()).ok()?,
            version_range: version.range(),
        })
    })
}

/// The images named at a tag in a text, with or without a digest, e.g.
/// `ghcr.io/componentized/secrets/gate:0.1.1-dev@sha256:...` or `localhost:5000/greeter:0.1.0`.
fn image_refs(text: &str) -> impl Iterator<Item = ImageRef> + '_ {
    static IMAGE_REF: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
            // a registry has a domain or a port, e.g. ghcr.io or localhost:5000
            r"(?P<registry>(?:[a-z0-9-]+\.)+[a-z0-9-]+(?::[0-9]+)?|localhost(?::[0-9]+)?)",
            r"/(?P<repository>[a-z0-9]+(?:[._-][a-z0-9]+)*(?:/[a-z0-9]+(?:[._-][a-z0-9]+)*)*)",
            r":(?P<tag>[A-Za-z0-9_][A-Za-z0-9_.-]{0,127})",
            r"(?:@(?P<digest>sha256:[0-9a-f]{64}))?",
        ))
        .unwrap()
    });
    IMAGE_REF.captures_iter(text).filter_map(|captures| {
        let all = captures.get(0).unwrap();
        let before = &text[..all.start()];
        if continues_word(before) || (before.ends_with('/') && !before.ends_with("//")) {
            return None;
        }
        let digest = captures.name("digest").map(|d| d.as_str().to_string());
        let mut tag = captures["tag"].to_string();
        let mut end = all.end();
        if digest.is_none() {
            // what ends a sentence isn't part of the tag, e.g. "pulled from `...:0.1.1-dev`."
            let trimmed = tag.trim_end_matches(['.', '-']).len();
            end -= tag.len() - trimmed;
            tag.truncate(trimmed);
        }
        Some(ImageRef {
            file: 0,
            image: Image::new(&captures["registry"], &captures["repository"]),
            tag,
            digest,
            range: all.start()..end,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text with `%` for `@`, the sources of this cli name no package at a version, e.g. its
    /// tests, which `componentized-build dep` in this repository would update.
    fn at(text: &str) -> String {
        text.replace('%', "@")
    }

    #[test]
    fn finds_package_refs() {
        let text = &at("import componentized:secrets/store%0.1.1-dev;\n\
                    use componentized:secrets/types%0.1.1-dev.{secret};\n\
                    \"componentized:secrets/store%0.1.1-dev\": secrets[\"wasi:cli/run%0.3.0\"],\n\
                    see componentized:secrets%0.1.0.\n\
                    not a.componentized:secrets%0.1.0 nor componentized:secrets%0.1.0x\n\
                    package componentized:valkey%0.3.0-dev;\n");
        let refs: Vec<_> = package_refs(text)
            .map(|r| (r.id.to_string(), r.version.to_string(), &text[r.version_range]))
            .collect();
        assert_eq!(
            refs,
            [
                ("componentized:secrets".into(), "0.1.1-dev".into(), "0.1.1-dev"),
                ("componentized:secrets".into(), "0.1.1-dev".into(), "0.1.1-dev"),
                ("componentized:secrets".into(), "0.1.1-dev".into(), "0.1.1-dev"),
                ("wasi:cli".into(), "0.3.0".into(), "0.3.0"),
                ("componentized:secrets".into(), "0.1.0".into(), "0.1.0"),
            ]
        );
    }

    #[test]
    fn finds_image_refs() {
        let digest = format!("sha256:{}", "a".repeat(64));
        let text = format!(
            "ghcr.io/componentized/secrets/gate:0.1.1-dev@{digest}\n\
             pulled from `ghcr.io/componentized/secrets/gate:0.1.1-dev`.\n\
             oci://localhost:5000/greeter:0.1.0_debug.\n\
             https://github.com/componentized/secrets/tree/main/components/gate\n"
        );
        let refs: Vec<_> = image_refs(&text)
            .map(|r| (r.image.to_string(), r.tag, r.digest, &text[r.range]))
            .collect();
        assert_eq!(
            refs,
            [
                (
                    "ghcr.io/componentized/secrets/gate".into(),
                    "0.1.1-dev".into(),
                    Some(digest.clone()),
                    &*format!("ghcr.io/componentized/secrets/gate:0.1.1-dev@{digest}")
                ),
                (
                    "ghcr.io/componentized/secrets/gate".into(),
                    "0.1.1-dev".into(),
                    None,
                    "ghcr.io/componentized/secrets/gate:0.1.1-dev"
                ),
                ("localhost:5000/greeter".into(), "0.1.0_debug".into(), None, "localhost:5000/greeter:0.1.0_debug"),
            ]
        );
    }

    #[test]
    fn maps_images_to_packages() {
        let gate = Image::new("ghcr.io", "componentized/secrets/gate");
        assert_eq!(gate.package(), Some((Id::new("componentized", "secrets"), Some("gate".into()))));
        let interface = Image::new("ghcr.io", "componentized/secrets");
        assert_eq!(interface.package(), Some((Id::new("componentized", "secrets"), None)));
        assert_eq!(Image::new("localhost:5000", "greeter").package(), None);
    }
}
