//! Completions of the dependencies of the repository and the versions published, run by the shell
//! as `COMPLETE=<shell> componentized-build -- <words>`, see `componentized-build completions`.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use clap_complete::engine::CompletionCandidate;
use semver::Version;

use crate::id::{Id, tag_version};
use crate::registry::{Registries, Registry};
use crate::repo::{self, Repo};
use crate::update;

/// The words being completed, those after `--`.
fn words() -> Vec<String> {
    std::env::args()
        .skip_while(|arg| arg != "--")
        .skip(1)
        .collect()
}

/// The repository the words name with `-C <dir>` or `--directory <dir>`, or the current directory's.
fn repo(words: &[String]) -> Option<Repo> {
    let mut directory = None;
    let mut words = words.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "-C" | "--directory" => directory = words.next().map(PathBuf::from),
            word => {
                if let Some(dir) = word.strip_prefix("--directory=") {
                    directory = Some(PathBuf::from(dir));
                }
            }
        }
    }
    let directory = directory.or_else(|| std::env::current_dir().ok())?;
    Repo::open(&repo::find_root(&directory).ok()?).ok()
}

/// The dependencies of the repository, the packages and their components.
pub fn ids() -> Vec<CompletionCandidate> {
    let Some(repo) = repo(&words()) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for dependency in repo.dependencies().values() {
        let versions: Vec<String> = dependency.versions().iter().map(Version::to_string).collect();
        let mut what = Vec::new();
        if !dependency.interface.is_empty() {
            what.push("interface".to_string());
        }
        if !dependency.components.is_empty() {
            what.push(format!(
                "components {}",
                dependency.components.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        candidates.push(
            CompletionCandidate::new(dependency.id.to_string())
                .help(Some(format!("{} at {}", what.join(", "), versions.join(", ")).into()))
                .display_order(Some(0)),
        );
        for (component, versions) in &dependency.components {
            let versions: Vec<String> = versions.iter().map(Version::to_string).collect();
            candidates.push(
                CompletionCandidate::new(dependency.id.component(component).to_string())
                    .help(Some(format!("the {component} component alone, at {}", versions.join(", ")).into()))
                    .display_order(Some(1)),
            );
        }
    }
    candidates
}

/// The versions published of the dependency the words name, newest first, the one the repository
/// depends on marked.
pub fn versions(current: &OsStr) -> Vec<CompletionCandidate> {
    let words = words();
    let Some(repo) = repo(&words) else {
        return Vec::new();
    };
    // the dependency, the word following `dep` that's not an option
    let Some(id) = words
        .iter()
        .skip_while(|word| *word != "dep")
        .skip(1)
        .filter(|word| !word.starts_with('-'))
        .find_map(|word| word.parse::<Id>().ok())
    else {
        return Vec::new();
    };
    let dependencies = repo.dependencies();
    let Ok(selection) = update::select(&repo, &dependencies, &id) else {
        return Vec::new();
    };
    let Ok(registry) = Registries::new(&repo.root, Duration::from_secs(5)) else {
        return Vec::new();
    };
    // the interface's versions, or its first component's
    let versions: BTreeSet<Version> = if selection.interface() {
        let Ok(versions) = registry.package_versions(&selection.dependency.id) else {
            return Vec::new();
        };
        versions.into_iter().collect()
    } else {
        let component = selection.components().next().map(str::to_string);
        let image = repo.pulled_images().into_iter().find(|image| {
            image
                .package()
                .is_some_and(|(package, c)| package == selection.dependency.id && c == component)
        });
        let Some(tags) = image.and_then(|image| registry.tags(&image).ok()) else {
            return Vec::new();
        };
        tags.iter()
            .filter_map(|tag| tag_version(tag))
            .filter(|(_, debug)| !debug)
            .map(|(version, _)| version)
            .collect()
    };
    let current_versions = selection.versions();
    let current = current.to_string_lossy();
    versions
        .iter()
        .rev()
        .filter(|version| version.to_string().starts_with(current.trim_start_matches('v')))
        .enumerate()
        .map(|(order, version)| {
            CompletionCandidate::new(version.to_string())
                .help(current_versions.contains(version).then(|| "the version it's at".into()))
                .display_order(Some(order))
        })
        .collect()
}
