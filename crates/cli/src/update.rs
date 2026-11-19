//! Updating a dependency of a repository: the version of the WIT package it names everywhere, the
//! digest of the package in each wkg.lock, and the tag and digest of each component it pulls.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use semver::Version;

use crate::id::{Id, debug_tag, tag, tag_version};
use crate::registry::Registry;
use crate::repo::{Dependency, Image, Repo};

/// What of a repository an id names, a dependency on another repository's package, its interface
/// and components, or on one of its components.
pub struct Selection<'a> {
    pub dependency: &'a Dependency,
    /// The component an id names, e.g. `gate` for `componentized:secrets-gate`, or all of them for
    /// the id of the package, e.g. `componentized:secrets`.
    pub component: Option<String>,
}

impl Selection<'_> {
    /// Whether the WIT package is updated, the id names the package, which the repository names.
    pub fn interface(&self) -> bool {
        self.component.is_none() && !self.dependency.interface.is_empty()
    }

    /// The components updated, by name, e.g. `gate`.
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.dependency
            .components
            .keys()
            .filter(|c| self.component.as_deref().is_none_or(|selected| selected == c.as_str()))
            .map(String::as_str)
    }

    /// The versions the repository depends on of what's updated.
    pub fn versions(&self) -> BTreeSet<Version> {
        let mut versions = BTreeSet::new();
        if self.interface() {
            versions.extend(self.dependency.interface.iter().cloned());
        }
        for component in self.components() {
            versions.extend(self.dependency.components[component].iter().cloned());
        }
        versions
    }
}

/// What of the repository's dependencies an id names, e.g. `componentized:secrets` or
/// `componentized:secrets-gate`, an error naming those it has when it's not one of them.
pub fn select<'a>(repo: &Repo, dependencies: &'a BTreeMap<Id, Dependency>, id: &Id) -> Result<Selection<'a>> {
    if let Some(dependency) = dependencies.get(id) {
        return Ok(Selection {
            dependency,
            component: None,
        });
    }
    for dependency in dependencies.values() {
        for component in dependency.components.keys() {
            if &dependency.id.component(component) == id {
                return Ok(Selection {
                    dependency,
                    component: Some(component.clone()),
                });
            }
        }
    }
    if repo.own_packages().contains(id) {
        bail!(
            "{id} is this repository's own package, not a dependency\n\
             \n  its version is bumped with build/scripts/bump-version.sh"
        );
    }
    let mut message = format!("{id} isn't a dependency of {}", repo.root.display());
    let known: Vec<String> = dependencies
        .values()
        .flat_map(|d| {
            std::iter::once(d.id.to_string()).chain(d.components.keys().map(|c| d.id.component(c).to_string()))
        })
        .collect();
    if let Some(similar) = known
        .iter()
        .map(|known| (strsim::jaro_winkler(known, &id.to_string()), known))
        .filter(|(similarity, _)| *similarity > 0.8)
        .max_by(|a, b| a.0.total_cmp(&b.0))
    {
        let _ = write!(message, "\n\n  did you mean {}?", similar.1);
    }
    if dependencies.is_empty() {
        message.push_str("\n\n  it has no dependencies");
    } else {
        let names: Vec<_> = dependencies.keys().map(Id::to_string).collect();
        let _ = write!(
            message,
            "\n\n  its dependencies are {}, see `componentized-build deps`",
            names.join(", ")
        );
    }
    bail!(message)
}

/// What's looked up in the registry for an update.
#[derive(Debug)]
pub struct Resolved {
    /// What it is, e.g. `interface`, `gate` or `gate (debug)`.
    pub what: String,
    /// The image and tag, e.g. `ghcr.io/componentized/secrets/gate:0.1.1-dev`.
    pub reference: String,
    /// The digest pinned, of the manifest for a component, of the wasm for the interface.
    pub digest: String,
    /// Why what's pinned isn't what's named, e.g. a debug build identical to the release build.
    pub note: Option<String>,
}

/// A file updated.
#[derive(Debug)]
pub struct Edit {
    pub path: PathBuf,
    pub content: String,
    /// What changed, e.g. `componentized:secrets 0.1.0 → 0.1.1, 3 references`.
    pub changes: Vec<String>,
}

/// The update of a dependency, what's looked up and the files that change.
#[derive(Debug)]
pub struct Plan {
    pub id: Id,
    pub from: BTreeSet<Version>,
    pub to: Version,
    pub resolved: Vec<Resolved>,
    pub edits: Vec<Edit>,
}

impl Plan {
    /// Writes the files that change.
    pub fn apply(&self, repo: &Repo) -> Result<()> {
        for edit in &self.edits {
            let path = repo.root.join(&edit.path);
            std::fs::write(&path, &edit.content).with_context(|| format!("couldn't write {}", path.display()))?;
        }
        Ok(())
    }
}

/// The digests of the manifests of a component's builds at a version.
struct Builds {
    release: String,
    /// None when the debug build isn't published, it's identical to the release build, see
    /// build/scripts/publish.sh.
    debug: Option<String>,
}

/// Plans updating the dependency the id names to the version, or to the latest build of the
/// version it's at when there's no version, e.g. a dev version published again. Everything is
/// looked up before anything changes.
pub fn plan(repo: &Repo, registry: &dyn Registry, id: &Id, version: Option<&Version>) -> Result<Plan> {
    let dependencies = repo.dependencies();
    let selection = select(repo, &dependencies, id)?;
    let depended = selection.versions();
    let mut from = depended.clone();
    if selection.interface() {
        from.extend(selection.dependency.documented.iter().cloned());
    }
    let to = match version {
        Some(version) => version.clone(),
        None => match depended.iter().collect::<Vec<_>>().as_slice() {
            [version] => (*version).clone(),
            versions => bail!(
                "{id} is at several versions, {}\n\
                 \n  name the version to update them to, e.g. `componentized-build dep {id} {}`",
                versions.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", "),
                versions.last().map(|v| v.to_string()).unwrap_or_default(),
            ),
        },
    };

    let mut resolved = Vec::new();

    // the interface, at the digest of its content, as wkg looks it up
    let interface = if selection.interface() {
        let package = &selection.dependency.id;
        let versions = registry.package_versions(package)?;
        if !versions.contains(&to) {
            let mut message = format!("the interface {package} isn't published at {to}");
            if let Some(registry) = registry.package_registry(package) {
                let _ = write!(message, " to {registry}");
            }
            bail!(published(message, &format!("{package}"), versions))
        }
        let digest = registry.package_digest(package, &to)?;
        resolved.push(Resolved {
            what: "interface".into(),
            reference: match registry.package_registry(package) {
                Some(registry) => format!("{package} {to} from {registry}"),
                None => format!("{package} {to}"),
            },
            digest: digest.clone(),
            note: None,
        });
        Some(digest)
    } else {
        None
    };

    // each component pulled, at the digests of its manifests
    let components: BTreeSet<&str> = selection.components().collect();
    let images: BTreeSet<Image> = repo
        .pulled_images()
        .into_iter()
        .filter(|image| {
            image.package().is_some_and(|(package, component)| {
                package == selection.dependency.id && component.is_some_and(|c| components.contains(c.as_str()))
            })
        })
        .collect();
    let image_refs = repo.image_refs();
    let mut builds = BTreeMap::new();
    for image in &images {
        let component = image.package().and_then(|(_, c)| c).unwrap_or_default();
        let release = find(registry, image, &to, &selection.dependency.id.component(&component))?;
        resolved.push(Resolved {
            what: component.clone(),
            reference: format!("{image}:{}", tag(&to)),
            digest: release.clone(),
            note: None,
        });
        let debug_named = image_refs
            .iter()
            .any(|r| &r.image == image && tag_version(&r.tag).is_some_and(|(_, debug)| debug));
        let debug = if debug_named {
            let debug = registry.manifest_digest(image, &debug_tag(&to))?;
            resolved.push(match &debug {
                Some(debug) => Resolved {
                    what: format!("{component} (debug)"),
                    reference: format!("{image}:{}", debug_tag(&to)),
                    digest: debug.clone(),
                    note: None,
                },
                None => Resolved {
                    what: format!("{component} (debug)"),
                    reference: format!("{image}:{}", tag(&to)),
                    digest: release.clone(),
                    note: Some(format!(
                        "{}:{} isn't published, it's identical to the release build",
                        image,
                        debug_tag(&to)
                    )),
                },
            });
            debug
        } else {
            None
        };
        builds.insert(image.clone(), Builds { release, debug });
    }

    // the references in each file, the version of the package, the tags and digests of the images
    let package_refs = repo.package_refs();
    let mut replacements: BTreeMap<usize, Vec<(std::ops::Range<usize>, String)>> = BTreeMap::new();
    let mut changes: BTreeMap<usize, BTreeMap<String, usize>> = BTreeMap::new();
    if interface.is_some() {
        for r in package_refs.iter().filter(|r| r.id == selection.dependency.id && r.version != to) {
            replacements.entry(r.file).or_default().push((r.version_range.clone(), to.to_string()));
            *changes
                .entry(r.file)
                .or_default()
                .entry(format!("{} {} → {to}", r.id, r.version))
                .or_default() += 1;
        }
    }
    for r in &image_refs {
        let Some(builds) = builds.get(&r.image) else {
            continue;
        };
        let debug = tag_version(&r.tag).is_some_and(|(_, debug)| debug);
        let (new_tag, manifest) = match (&builds.debug, debug) {
            (Some(manifest), true) => (debug_tag(&to), manifest),
            _ => (tag(&to), &builds.release),
        };
        let digest = r.digest.as_ref().map(|_| manifest.clone());
        if new_tag == r.tag && digest == r.digest {
            continue;
        }
        let mut reference = format!("{}:{new_tag}", r.image);
        if let Some(digest) = &digest {
            reference.push('@');
            reference.push_str(digest);
        }
        replacements.entry(r.file).or_default().push((r.range.clone(), reference));
        let component = r.image.package().and_then(|(_, c)| c).unwrap_or_default();
        let change = match (&r.digest, &digest) {
            (Some(old), Some(new)) => format!("{component} {} {} → {new_tag} {}", r.tag, short(old), short(new)),
            _ => format!("{component} {} → {new_tag}", r.tag),
        };
        *changes.entry(r.file).or_default().entry(change).or_default() += 1;
    }

    let mut edits = Vec::new();
    for (file, mut replacements) in replacements {
        let source = &repo.files[file];
        replacements.sort_by_key(|(range, _)| range.start);
        let mut content = String::with_capacity(source.content.len());
        let mut at = 0;
        for (range, replacement) in replacements {
            if range.start < at {
                continue;
            }
            content.push_str(&source.content[at..range.start]);
            content.push_str(&replacement);
            at = range.end;
        }
        content.push_str(&source.content[at..]);
        edits.push(Edit {
            path: source.path.clone(),
            content,
            changes: changes[&file]
                .iter()
                .map(|(change, count)| match count {
                    1 => change.clone(),
                    count => format!("{change}, {count} references"),
                })
                .collect(),
        });
    }

    // the package in each wkg.lock, at the digest of its wasm
    if let Some(digest) = &interface {
        // the versions the repository depends on, not those its documents name
        let replaced: BTreeSet<Version> = selection.dependency.interface.clone();
        for path in &repo.locks {
            let full = repo.root.join(path);
            let original = std::fs::read_to_string(&full).with_context(|| format!("couldn't read {}", full.display()))?;
            if let Some((content, change)) = update_lock(&original, &selection.dependency.id, &replaced, &to, digest)
                .with_context(|| format!("couldn't update {}", path.display()))?
            {
                edits.push(Edit {
                    path: path.clone(),
                    content,
                    changes: vec![change],
                });
            }
        }
    }
    edits.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(Plan {
        id: id.clone(),
        from,
        to,
        resolved,
        edits,
    })
}

/// The digest of the manifest of an image at a version, an error naming the versions published
/// when it's not one of them.
fn find(registry: &dyn Registry, image: &Image, version: &Version, id: &Id) -> Result<String> {
    if let Some(digest) = registry.manifest_digest(image, &tag(version))? {
        return Ok(digest);
    }
    let versions = registry
        .tags(image)?
        .iter()
        .filter_map(|tag| tag_version(tag))
        .filter(|(_, debug)| !debug)
        .map(|(version, _)| version)
        .collect();
    bail!(published(
        format!("the component {id} isn't published at {version}, there's no {image}:{}", tag(version)),
        &image.to_string(),
        versions
    ))
}

/// The message with the versions published of what it names, the most recent first.
fn published(mut message: String, what: &str, mut versions: Vec<Version>) -> String {
    versions.sort();
    versions.dedup();
    if versions.is_empty() {
        let _ = write!(message, "\n\n  {what} has no versions published");
    } else {
        let recent: Vec<String> = versions.iter().rev().take(10).map(Version::to_string).collect();
        let _ = write!(
            message,
            "\n\n  the versions published are {}{}",
            recent.join(", "),
            if versions.len() > recent.len() { ", ..." } else { "" }
        );
    }
    message
}

/// The lock with the package at the version and digest, in place of the versions replaced, None
/// when the lock has none of them, or already has the version at the digest.
fn update_lock(
    original: &str,
    id: &Id,
    replaced: &BTreeSet<Version>,
    to: &Version,
    digest: &str,
) -> Result<Option<(String, String)>> {
    let mut lock: toml_edit::DocumentMut = original.parse()?;
    let Some(packages) = lock.get_mut("packages").and_then(|p| p.as_array_of_tables_mut()) else {
        return Ok(None);
    };
    let name = id.to_string();
    let Some(package) = packages
        .iter_mut()
        .find(|package| package.get("name").and_then(|n| n.as_str()) == Some(name.as_str()))
    else {
        return Ok(None);
    };
    let Some(versions) = package.get_mut("versions").and_then(|v| v.as_array_of_tables_mut()) else {
        return Ok(None);
    };
    let version_of = |entry: &toml_edit::Table| {
        entry
            .get("version")
            .and_then(|v| v.as_str())
            .and_then(|v| Version::parse(v).ok())
    };
    let matching: Vec<usize> = versions
        .iter()
        .enumerate()
        .filter(|(_, entry)| version_of(entry).is_some_and(|v| replaced.contains(&v) || &v == to))
        .map(|(index, _)| index)
        .collect();
    let Some(&first) = matching.first() else {
        return Ok(None);
    };
    let entry = versions.get(first).unwrap();
    let from = version_of(entry).map(|v| v.to_string()).unwrap_or_default();
    let from_digest = entry.get("digest").and_then(|d| d.as_str()).unwrap_or_default().to_string();
    // the first in place, as wkg wrote it, the others removed
    for index in matching.iter().skip(1).rev() {
        versions.remove(*index);
    }
    let entry = versions.get_mut(first).unwrap();
    entry["requirement"] = toml_edit::value(format!("={to}"));
    entry["version"] = toml_edit::value(to.to_string());
    entry["digest"] = toml_edit::value(digest);
    let content = lock.to_string();
    if content == original {
        return Ok(None);
    }
    let change = if from == to.to_string() {
        format!("{id} {to} {} → {}", short(&from_digest), short(digest))
    } else {
        format!("{id} {from} → {to} {}", short(digest))
    };
    Ok(Some((content, change)))
}

/// A digest shortened for display, e.g. `sha256:65bf990900b5`.
pub fn short(digest: &str) -> String {
    match digest.split_once(':') {
        Some((algorithm, hex)) => format!("{algorithm}:{}", &hex[..hex.len().min(12)]),
        None => digest.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::Path;

    use regex::Regex;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::registry::WKG_CONFIG;

    /// The text with each `sha256:<name>` a digest, the SHA-256 of the name, as a registry has
    /// them, e.g. `sha256:gate` for the digest of the gate, and `%` for `@`, the sources of this cli
    /// name no package at a version, e.g. its tests, which `componentized-build dep` in this
    /// repository would update.
    fn d(text: &str) -> String {
        let text = text.replace('%', "@");
        Regex::new(r"sha256:([A-Za-z0-9.-]+)")
            .unwrap()
            .replace_all(&text, |captures: &regex::Captures| {
                let hex: String = Sha256::digest(&captures[1]).iter().map(|byte| format!("{byte:02x}")).collect();
                format!("sha256:{hex}")
            })
            .into_owned()
    }

    /// A registry of the images published, by image and tag, the digests of their manifests and
    /// their content. A WIT package is published as `ghcr.io/<namespace>/<name>`.
    #[derive(Default)]
    struct Fake {
        manifests: HashMap<(String, String), (String, String)>,
        lookups: RefCell<Vec<String>>,
    }

    impl Fake {
        fn package_image(id: &Id) -> String {
            format!("ghcr.io/{}/{}", id.namespace, id.name)
        }
    }

    impl Fake {
        /// The image published at the tag, its manifest's digest `sha256:<name>`, its wasm's
        /// `sha256:<name>-wasm`.
        fn publish(mut self, image: &str, tag: &str, name: &str) -> Self {
            self.manifests
                .insert((image.into(), tag.into()), (d(name), d(&format!("{name}-wasm"))));
            self
        }
    }

    impl Registry for Fake {
        fn package_registry(&self, _id: &Id) -> Option<String> {
            Some("componentized.dev".into())
        }

        fn package_versions(&self, id: &Id) -> Result<Vec<Version>> {
            let image = Fake::package_image(id);
            Ok(self
                .manifests
                .keys()
                .filter(|(i, _)| *i == image)
                .filter_map(|(_, tag)| Version::parse(tag).ok())
                .collect())
        }

        fn package_digest(&self, id: &Id, version: &Version) -> Result<String> {
            self.lookups.borrow_mut().push(format!("{id} {version}"));
            let key = (Fake::package_image(id), version.to_string());
            Ok(self.manifests.get(&key).context("not published")?.1.clone())
        }

        fn manifest_digest(&self, image: &Image, tag: &str) -> Result<Option<String>> {
            self.lookups.borrow_mut().push(format!("{image}:{tag}"));
            Ok(self.manifests.get(&(image.to_string(), tag.to_string())).map(|(digest, _)| digest.clone()))
        }

        fn tags(&self, image: &Image) -> Result<Vec<String>> {
            Ok(self
                .manifests
                .keys()
                .filter(|(i, _)| *i == image.to_string())
                .map(|(_, tag)| tag.clone())
                .collect())
        }
    }

    const LOCK: &str = r#"# This file is automatically generated.
# It is not intended for manual editing.
version = 1

[[packages]]
name = "componentized:secrets"
registry = "componentized.dev"

[[packages.versions]]
requirement = "=0.1.0"
version = "0.1.0"
digest = "sha256:old-0.1.0"

[[packages.versions]]
requirement = "=0.1.1-dev"
version = "0.1.1-dev"
digest = "sha256:old-dev"

[[packages]]
name = "wasi:logging"
registry = "wasi.dev"

[[packages.versions]]
requirement = "=0.1.0-draft"
version = "0.1.0-draft"
digest = "sha256:logging"
"#;

    /// A repository like componentized/valkey, depending on componentized:secrets, its interface
    /// and components, and on componentized:logging's to-stderr component.
    fn repo() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().unwrap();
        let files: &[(&str, &str)] = &[
            ("build/build.mk", "# componentized:secrets%0.1.1-dev ghcr.io/componentized/secrets/gate:0.1.1-dev\n"),
            (".gitignore", "/target\n**/wit/deps/\n"),
            (WKG_CONFIG, "[namespace_registries]\ncomponentized = \"componentized.dev\"\n"),
            ("wit/worlds.wit", "package componentized:valkey%0.3.0-dev;\n"),
            (
                "components/wit/worlds.wit",
                "package componentized:valkey-components%0.3.0-dev;\n\
                 world client {\n    import componentized:secrets/store%0.1.1-dev;\n    import componentized:valkey/client%0.3.0-dev;\n}\n",
            ),
            ("components/wit/deps/componentized-secrets-0.1.1-dev/package.wit", "package componentized:secrets%0.1.1-dev;\n"),
            ("components/wkg.lock", LOCK),
            (
                "components/cli/cli.wac",
                "\"componentized:secrets/store%0.1.1-dev\": secrets[\"componentized:secrets/store%0.1.1-dev\"],\n",
            ),
            (
                "components/dep-componentized-secrets-gate/dep-componentized-secrets-gate.wkg",
                "ghcr.io/componentized/secrets/gate:0.1.1-dev@sha256:gate\nghcr.io/componentized/secrets/gate:0.1.1-dev_debug@sha256:gate-debug\n",
            ),
            (
                "components/dep-componentized-secrets-gate/README.md",
                "# dep-componentized-secrets-gate\n\nPulled from `ghcr.io/componentized/secrets/gate:0.1.1-dev`.\n",
            ),
            ("components/cli/README.md", "Configured with `componentized:secrets%0.1.0`.\n"),
            (
                "components/dep-componentized-secrets-latch-key/dep-componentized-secrets-latch-key.wkg",
                "ghcr.io/componentized/secrets/latch-key:0.1.1-dev@sha256:latch-key\nghcr.io/componentized/secrets/latch-key:0.1.1-dev_debug@sha256:latch-key-debug\n",
            ),
            (
                "components/dep-componentized-logging-to-stderr/dep-componentized-logging-to-stderr.wkg",
                "ghcr.io/componentized/logging/to-stderr:0.3.1@sha256:to-stderr\nghcr.io/componentized/logging/to-stderr:0.3.1_debug@sha256:to-stderr-debug\n",
            ),
            ("target/notes.md", "componentized:secrets%0.1.1-dev\n"),
        ];
        for (path, content) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, d(content)).unwrap();
        }
        let repo = Repo::open(dir.path()).unwrap();
        (dir, repo)
    }

    /// The registry with componentized:secrets published at 0.1.2, its gate's debug build
    /// identical to its release build, and 0.1.1-dev built again.
    fn registry() -> Fake {
        Fake::default()
            .publish("ghcr.io/componentized/secrets", "0.1.0", "sha256:secrets-0.1.0")
            .publish("ghcr.io/componentized/secrets", "0.1.1-dev", "sha256:secrets-dev")
            .publish("ghcr.io/componentized/secrets", "0.1.2", "sha256:secrets-0.1.2")
            .publish("ghcr.io/componentized/secrets/gate", "0.1.1-dev", "sha256:gate-new")
            .publish("ghcr.io/componentized/secrets/gate", "0.1.1-dev_debug", "sha256:gate-debug-new")
            .publish("ghcr.io/componentized/secrets/gate", "0.1.2", "sha256:gate-0.1.2")
            .publish("ghcr.io/componentized/secrets/latch-key", "0.1.1-dev", "sha256:latch-key-new")
            .publish("ghcr.io/componentized/secrets/latch-key", "0.1.1-dev_debug", "sha256:latch-key-debug-new")
            .publish("ghcr.io/componentized/secrets/latch-key", "0.1.2", "sha256:latch-key-0.1.2")
            .publish("ghcr.io/componentized/secrets/latch-key", "0.1.2_debug", "sha256:latch-key-0.1.2-debug")
            .publish("ghcr.io/componentized/logging/to-stderr", "0.3.1", "sha256:to-stderr")
            .publish("ghcr.io/componentized/logging/to-stderr", "0.3.1_debug", "sha256:to-stderr-debug")
    }

    fn edited<'a>(plan: &'a Plan, path: &str) -> Option<&'a str> {
        plan.edits
            .iter()
            .find(|edit| edit.path == Path::new(path))
            .map(|edit| edit.content.as_str())
    }

    fn version(version: &str) -> Version {
        Version::parse(version).unwrap()
    }

    #[test]
    fn lists_dependencies() {
        let (_dir, repo) = repo();
        let dependencies = repo.dependencies();
        let ids: Vec<_> = dependencies.keys().map(Id::to_string).collect();
        assert_eq!(ids, ["componentized:logging", "componentized:secrets"]);
        let secrets = &dependencies[&Id::new("componentized", "secrets")];
        assert_eq!(secrets.interface, BTreeSet::from([version("0.1.1-dev")]));
        assert_eq!(secrets.components.keys().collect::<Vec<_>>(), ["gate", "latch-key"]);
        let logging = &dependencies[&Id::new("componentized", "logging")];
        assert!(logging.interface.is_empty());
    }

    #[test]
    fn updates_a_dependency_to_a_version() {
        let (_dir, repo) = repo();
        let plan = plan(&repo, &registry(), &Id::new("componentized", "secrets"), Some(&version("0.1.2"))).unwrap();
        // the version a document names is updated too
        assert_eq!(plan.from, BTreeSet::from([version("0.1.0"), version("0.1.1-dev")]));
        assert_eq!(edited(&plan, "components/cli/README.md").unwrap(), d("Configured with `componentized:secrets%0.1.2`.\n"));

        let paths: Vec<_> = plan.edits.iter().map(|edit| edit.path.to_string_lossy().into_owned()).collect();
        assert_eq!(
            paths,
            [
                "components/cli/README.md",
                "components/cli/cli.wac",
                "components/dep-componentized-secrets-gate/README.md",
                "components/dep-componentized-secrets-gate/dep-componentized-secrets-gate.wkg",
                "components/dep-componentized-secrets-latch-key/dep-componentized-secrets-latch-key.wkg",
                "components/wit/worlds.wit",
                "components/wkg.lock",
            ]
        );
        assert_eq!(
            edited(&plan, "components/wit/worlds.wit").unwrap(),
            d("package componentized:valkey-components%0.3.0-dev;\n\
             world client {\n    import componentized:secrets/store%0.1.2;\n    import componentized:valkey/client%0.3.0-dev;\n}\n")
        );
        assert_eq!(
            edited(&plan, "components/cli/cli.wac").unwrap(),
            d("\"componentized:secrets/store%0.1.2\": secrets[\"componentized:secrets/store%0.1.2\"],\n")
        );
        // the debug build of the gate isn't published, it's identical to the release build
        assert_eq!(
            edited(&plan, "components/dep-componentized-secrets-gate/dep-componentized-secrets-gate.wkg").unwrap(),
            d("ghcr.io/componentized/secrets/gate:0.1.2@sha256:gate-0.1.2\nghcr.io/componentized/secrets/gate:0.1.2@sha256:gate-0.1.2\n")
        );
        assert!(plan.resolved.iter().any(|r| r.what == "gate (debug)" && r.note.is_some()));
        assert_eq!(
            edited(&plan, "components/dep-componentized-secrets-latch-key/dep-componentized-secrets-latch-key.wkg").unwrap(),
            d("ghcr.io/componentized/secrets/latch-key:0.1.2@sha256:latch-key-0.1.2\nghcr.io/componentized/secrets/latch-key:0.1.2_debug@sha256:latch-key-0.1.2-debug\n")
        );
        assert_eq!(
            edited(&plan, "components/dep-componentized-secrets-gate/README.md").unwrap(),
            "# dep-componentized-secrets-gate\n\nPulled from `ghcr.io/componentized/secrets/gate:0.1.2`.\n"
        );
        // the version replaced, at the digest of the wasm of the interface, the stale 0.1.0 kept
        assert_eq!(
            edited(&plan, "components/wkg.lock").unwrap(),
            d(&LOCK.replace(
                "requirement = \"=0.1.1-dev\"\nversion = \"0.1.1-dev\"\ndigest = \"sha256:old-dev\"",
                "requirement = \"=0.1.2\"\nversion = \"0.1.2\"\ndigest = \"sha256:secrets-0.1.2-wasm\""
            ))
        );
    }

    #[test]
    fn refreshes_the_digests_of_the_version_its_at() {
        let (_dir, repo) = repo();
        let plan = plan(&repo, &registry(), &Id::new("componentized", "secrets"), None).unwrap();
        // the version it depends on, not the one a document names in passing
        assert_eq!(plan.to, version("0.1.1-dev"));
        assert_eq!(edited(&plan, "components/wit/worlds.wit"), None);
        assert_eq!(edited(&plan, "components/dep-componentized-secrets-gate/README.md"), None);
        assert_eq!(
            edited(&plan, "components/dep-componentized-secrets-gate/dep-componentized-secrets-gate.wkg").unwrap(),
            d("ghcr.io/componentized/secrets/gate:0.1.1-dev@sha256:gate-new\nghcr.io/componentized/secrets/gate:0.1.1-dev_debug@sha256:gate-debug-new\n")
        );
        assert!(edited(&plan, "components/wkg.lock").unwrap().contains(&d("digest = \"sha256:secrets-dev-wasm\"")));
    }

    #[test]
    fn updates_a_component_alone() {
        let (_dir, repo) = repo();
        let plan = plan(&repo, &registry(), &Id::new("componentized", "secrets-latch-key"), Some(&version("0.1.2"))).unwrap();
        let paths: Vec<_> = plan.edits.iter().map(|edit| edit.path.to_string_lossy().into_owned()).collect();
        assert_eq!(
            paths,
            ["components/dep-componentized-secrets-latch-key/dep-componentized-secrets-latch-key.wkg"]
        );
    }

    #[test]
    fn changes_nothing_when_up_to_date() {
        let (_dir, repo) = repo();
        let plan = plan(&repo, &registry(), &Id::new("componentized", "logging"), Some(&version("0.3.1"))).unwrap();
        assert!(plan.edits.is_empty(), "{:?}", plan.edits);
    }

    #[test]
    fn names_the_versions_published_for_a_version_that_isnt() {
        let (_dir, repo) = repo();
        let err = plan(&repo, &registry(), &Id::new("componentized", "secrets"), Some(&version("0.2.0"))).unwrap_err();
        let err = err.to_string();
        assert!(err.contains("the interface componentized:secrets isn't published at 0.2.0 to componentized.dev"), "{err}");
        assert!(err.contains("the versions published are 0.1.2, 0.1.1-dev, 0.1.0"), "{err}");
    }

    #[test]
    fn names_the_dependencies_for_an_id_that_isnt_one() {
        let (_dir, repo) = repo();
        let err = plan(&repo, &registry(), &Id::new("componentized", "secret"), None).unwrap_err().to_string();
        assert!(err.contains("componentized:secret isn't a dependency"), "{err}");
        assert!(err.contains("did you mean componentized:secrets?"), "{err}");
        assert!(err.contains("its dependencies are componentized:logging, componentized:secrets"), "{err}");

        let err = plan(&repo, &registry(), &Id::new("componentized", "valkey"), None).unwrap_err().to_string();
        assert!(err.contains("this repository's own package"), "{err}");
    }

    #[test]
    fn looks_everything_up_before_changing_anything() {
        let (_dir, repo) = repo();
        let registry = Fake::default().publish("ghcr.io/componentized/secrets", "0.1.2", "sha256:secrets-0.1.2");
        let err = plan(&repo, &registry, &Id::new("componentized", "secrets"), Some(&version("0.1.2"))).unwrap_err();
        assert!(err.to_string().contains("the component componentized:secrets-gate isn't published at 0.1.2"), "{err}");
    }
}
