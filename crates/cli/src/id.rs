//! The ids of WIT packages and components, and their versions, as they're named on the command line,
//! in WIT and as OCI tags.

use std::fmt;
use std::str::FromStr;

use semver::Version;

/// The id of a WIT package, `<namespace>:<name>`, e.g. `componentized:secrets`, or of a component
/// published by the repository of a package, `<namespace>:<name>-<component>`, e.g.
/// `componentized:secrets-gate`, as the directory names it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id {
    pub namespace: String,
    pub name: String,
}

impl Id {
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    /// The id of a component of the package's repository, e.g. `componentized:secrets-gate` for
    /// `gate` of `componentized:secrets`.
    pub fn component(&self, component: &str) -> Id {
        Id::new(&self.namespace, format!("{}-{component}", self.name))
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.name)
    }
}

impl FromStr for Id {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((namespace, name)) = s.split_once(':') else {
            return Err("expected `<namespace>:<name>`, e.g. componentized:secrets".into());
        };
        if let Some(at) = name.find(['/', '@']) {
            return Err(format!(
                "name the package alone, without an interface or version, e.g. `{namespace}:{}`",
                &name[..at]
            ));
        }
        label("namespace", namespace)?;
        label("name", name)?;
        Ok(Id::new(namespace, name))
    }
}

/// A WIT label, lowercase words of letters and digits separated by `-`, the first starting with a
/// letter, e.g. `latch-n5`.
fn label(what: &str, label: &str) -> Result<(), String> {
    if label.is_empty() {
        return Err(format!(
            "the {what} is empty, expected `<namespace>:<name>`, e.g. componentized:secrets"
        ));
    }
    if !label.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(format!(
            "the {what} `{label}` may only have lowercase letters, digits and `-`"
        ));
    }
    if label.split('-').any(str::is_empty) {
        return Err(format!(
            "the {what} `{label}` has an empty word, words are separated by a single `-`"
        ));
    }
    if !label.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(format!("the {what} `{label}` must start with a letter"));
    }
    Ok(())
}

/// A version given on the command line, a semantic version with or without a leading `v`, e.g.
/// `0.1.1`, `v0.1.1` or `0.1.1-dev`.
pub fn parse_version(s: &str) -> Result<Version, String> {
    let version = s.strip_prefix('v').unwrap_or(s);
    Version::parse(version).map_err(|err| {
        format!("`{s}` is not a semantic version, {err}, e.g. 0.1.1 or 0.1.1-dev")
    })
}

/// The suffix of the tag of a debug build, e.g. `0.1.1-dev_debug`.
pub const DEBUG_SUFFIX: &str = "_debug";

/// The tag a version is published with, OCI tags have no `+`, it's `_`, e.g. `0.1.1_abc` for
/// `0.1.1+abc`, see build/scripts/publish.sh.
pub fn tag(version: &Version) -> String {
    version.to_string().replace('+', "_")
}

/// The tag a version's debug build is published with, e.g. `0.1.1-dev_debug`.
pub fn debug_tag(version: &Version) -> String {
    format!("{}{DEBUG_SUFFIX}", tag(version))
}

/// The version a tag is published for, and whether it's the debug build's, e.g. `0.1.1-dev` and
/// true for `0.1.1-dev_debug`. None for a tag that isn't a version, e.g. `latest` or the tag cosign
/// attaches a signature with, `sha256-<hex>.sig`.
pub fn tag_version(tag: &str) -> Option<(Version, bool)> {
    let (tag, debug) = match tag.strip_suffix(DEBUG_SUFFIX) {
        Some(tag) => (tag, true),
        None => (tag, false),
    };
    Version::parse(&tag.replace('_', "+")).ok().map(|version| (version, debug))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids() {
        assert_eq!("componentized:secrets".parse(), Ok(Id::new("componentized", "secrets")));
        assert_eq!(
            "componentized:sockets-latch-n5".parse(),
            Ok(Id::new("componentized", "sockets-latch-n5"))
        );
    }

    #[test]
    fn rejects_invalid_ids() {
        for (id, error) in [
            ("componentized", "expected `<namespace>:<name>`"),
            ("componentized:", "the name is empty"),
            (":secrets", "the namespace is empty"),
            ("Componentized:secrets", "may only have lowercase letters"),
            ("componentized:secrets--gate", "has an empty word"),
            ("componentized:secrets-", "has an empty word"),
            ("componentized:1secrets", "must start with a letter"),
            // split, the sources of this cli name no package at a version, see repo::tests::at
            (concat!("componentized:secrets/store", "@0.1.0"), "e.g. `componentized:secrets`"),
            (concat!("componentized:secrets", "@0.1.0"), "e.g. `componentized:secrets`"),
        ] {
            let err = id.parse::<Id>().unwrap_err();
            assert!(err.contains(error), "{id}: {err}");
        }
    }

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("0.1.1-dev"), Ok(Version::parse("0.1.1-dev").unwrap()));
        assert_eq!(parse_version("v0.1.1"), Ok(Version::parse("0.1.1").unwrap()));
        assert!(parse_version("0.1").unwrap_err().contains("not a semantic version"));
        assert!(parse_version("latest").is_err());
    }

    #[test]
    fn maps_versions_to_tags() {
        let version = Version::parse("0.1.1+abc").unwrap();
        assert_eq!(tag(&version), "0.1.1_abc");
        assert_eq!(debug_tag(&version), "0.1.1_abc_debug");
        assert_eq!(tag_version("0.1.1_abc_debug"), Some((version.clone(), true)));
        assert_eq!(tag_version("0.1.1_abc"), Some((version, false)));
        assert_eq!(tag_version("sha256-0123.sig"), None);
        assert_eq!(tag_version("latest"), None);
    }
}
