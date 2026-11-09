//! Narrows a change to `Cargo.lock` to the crates whose Locked Dependencies changed, by
//! comparing the lockfile at the Base with the inspected one. Pure: no git, no Cargo.
//!
//! A path package (one without a `source`: the workspace members and the path crates they
//! use) is claimed when a third-party package it reaches through third-party packages
//! only was added, bumped, re-checksummed or re-wired, or when its own set of direct
//! third-party dependencies differs. Reaching a change through another path package does
//! not count: that crate is claimed itself and its Dependents follow through the
//! dependency graph. Entries of path packages are otherwise not compared, so bumping a
//! member's own version, which rewrites its entry, claims nobody.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use toml::Value;

/// Path package name -> the Locked Dependencies whose change claims it, as sorted
/// `name version` labels. Crates the lockfile change does not concern are absent.
pub type LockedDependencies = BTreeMap<String, Vec<String>>;

/// The crates claimed by the change from lockfile `old` to lockfile `new`.
///
/// Fails when the change cannot be narrowed: a lockfile does not parse, a dependency
/// reference is ambiguous, or something other than the packages differs (the format
/// `version`, `[metadata]`, unused patches). The caller then treats the lockfile as a
/// Global File.
pub fn changed_dependencies(old: &str, new: &str) -> Result<LockedDependencies> {
    let old = Lock::parse(old).context("the lockfile at the Base")?;
    let new = Lock::parse(new).context("the inspected lockfile")?;
    if old.rest != new.rest {
        bail!("the lockfiles differ outside their packages");
    }

    // Third-party packages that are new or whose contents differ, and those that merely
    // depend on a different set of packages than before.
    let mut replaced = BTreeSet::new();
    let mut rewired = BTreeSet::new();
    for (id, package) in new.third_party() {
        match old.packages.get(id) {
            Some(before) if before.contents == package.contents => {
                if before.dependencies != package.dependencies {
                    rewired.insert(id);
                }
            }
            _ => {
                replaced.insert(id);
            }
        }
    }

    let mut claimed = LockedDependencies::new();
    for (id, package) in new.packages.iter().filter(|(id, _)| id.is_path()) {
        let reached = new.third_party_closure(package);
        let direct = package.third_party_dependencies();
        let direct_before = old.path_package(id).map(Package::third_party_dependencies);
        let direct_before = direct_before.unwrap_or_default();

        let mut labels: BTreeSet<String> = reached
            .intersection(&replaced)
            .chain(direct.difference(&direct_before))
            .map(|id| id.label())
            .collect();
        if labels.is_empty() {
            labels = reached
                .intersection(&rewired)
                .map(|id| id.label())
                .collect();
            labels.extend(
                direct_before
                    .difference(&direct)
                    .map(|id| format!("{} (removed)", id.label())),
            );
        }
        if !labels.is_empty() {
            claimed.entry(id.name.clone()).or_default().extend(labels);
        }
    }
    // Two path packages may share a name at different versions; their labels are merged.
    for labels in claimed.values_mut() {
        labels.sort();
        labels.dedup();
    }
    Ok(claimed)
}

/// What identifies a locked package. `source` is absent for a path package.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PackageId {
    name: String,
    version: String,
    source: Option<String>,
}

impl PackageId {
    fn is_path(&self) -> bool {
        self.source.is_none()
    }

    fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }

    /// Whether a dependency reference (`name`, `name version` or `name version (source)`,
    /// as much as Cargo needed to be unambiguous) can mean this package. A reference
    /// names a git source without the `#<commit>` the package's own `source` carries.
    fn answers(&self, name: &str, version: Option<&str>, source: Option<&str>) -> bool {
        let own_source = self.source.as_deref().map(|s| s.split('#').next().unwrap());
        self.name == name
            && version.is_none_or(|v| v == self.version)
            && source.is_none_or(|s| Some(s) == own_source)
    }
}

#[derive(Debug)]
struct Package {
    /// Everything in the entry but its identity and dependencies: the checksum, `replace`.
    contents: BTreeMap<String, Value>,
    dependencies: BTreeSet<PackageId>,
}

impl Package {
    fn third_party_dependencies(&self) -> BTreeSet<&PackageId> {
        self.dependencies.iter().filter(|d| !d.is_path()).collect()
    }
}

#[derive(Debug)]
struct Lock {
    packages: BTreeMap<PackageId, Package>,
    /// Every top-level key but `package`.
    rest: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RawLock {
    #[serde(default)]
    package: Vec<RawPackage>,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RawPackage {
    name: String,
    version: String,
    source: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(flatten)]
    contents: BTreeMap<String, Value>,
}

impl Lock {
    fn parse(text: &str) -> Result<Self> {
        let raw: RawLock = toml::from_str(text).context("parsing the lockfile")?;
        let ids: Vec<PackageId> = raw
            .package
            .iter()
            .map(|p| PackageId {
                name: p.name.clone(),
                version: p.version.clone(),
                source: p.source.clone(),
            })
            .collect();
        let mut packages = BTreeMap::new();
        for (id, package) in ids.iter().zip(raw.package) {
            let dependencies = package
                .dependencies
                .iter()
                .map(|reference| resolve(reference, &ids).cloned())
                .collect::<Result<_>>()?;
            let package = Package {
                contents: package.contents,
                dependencies,
            };
            if packages.insert(id.clone(), package).is_some() {
                bail!("package `{}` is locked twice", id.label());
            }
        }
        Ok(Self {
            packages,
            rest: raw.rest,
        })
    }

    fn third_party(&self) -> impl Iterator<Item = (&PackageId, &Package)> {
        self.packages.iter().filter(|(id, _)| !id.is_path())
    }

    /// The path package this lockfile holds for `id`: the one of that name, or of that
    /// name and version when several share the name.
    fn path_package(&self, id: &PackageId) -> Option<&Package> {
        let mut named = self
            .packages
            .iter()
            .filter(|(other, _)| other.is_path() && other.name == id.name);
        match (named.next(), named.next()) {
            (Some((_, only)), None) => Some(only),
            _ => self.packages.get(id),
        }
    }

    /// The third-party packages `package` reaches without passing through a path package.
    fn third_party_closure<'a>(&'a self, package: &'a Package) -> BTreeSet<&'a PackageId> {
        let mut reached = BTreeSet::new();
        let mut pending: Vec<&PackageId> = package.third_party_dependencies().into_iter().collect();
        while let Some(id) = pending.pop() {
            if reached.insert(id)
                && let Some(package) = self.packages.get(id)
            {
                pending.extend(package.third_party_dependencies());
            }
        }
        reached
    }
}

/// The one package a dependency reference names.
fn resolve<'a>(reference: &str, ids: &'a [PackageId]) -> Result<&'a PackageId> {
    let mut parts = reference.splitn(3, ' ');
    let name = parts.next().unwrap_or_default();
    let version = parts.next();
    let source = parts.next().map(|s| {
        s.strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(s)
    });
    let mut candidates = ids.iter().filter(|id| id.answers(name, version, source));
    match (candidates.next(), candidates.next()) {
        (Some(id), None) => Ok(id),
        (None, _) => Err(anyhow!("dependency `{reference}` names no locked package")),
        (Some(_), Some(_)) => Err(anyhow!("dependency `{reference}` is ambiguous")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

    /// A lockfile from `(name, version, third-party?, dependency references)` entries.
    fn lock(packages: &[(&str, &str, bool, &[&str])]) -> String {
        let mut text = String::from("version = 4\n");
        for (name, version, third_party, dependencies) in packages {
            text += &format!("\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n");
            if *third_party {
                text += &format!("source = \"{REGISTRY}\"\nchecksum = \"{name}-{version}\"\n");
            }
            if !dependencies.is_empty() {
                text += &format!("dependencies = {dependencies:?}\n");
            }
        }
        text
    }

    fn claimed(old: &str, new: &str) -> Vec<(String, Vec<String>)> {
        changed_dependencies(old, new)
            .unwrap()
            .into_iter()
            .collect()
    }

    fn entry(name: &str, labels: &[&str]) -> (String, Vec<String>) {
        (
            name.to_string(),
            labels.iter().map(|l| l.to_string()).collect(),
        )
    }

    #[test]
    fn a_bumped_dependency_claims_the_crates_using_it_directly_or_through_third_parties() {
        // `core` uses `serde`, `net` uses `tokio` which uses `mio`, `app` uses only `core`.
        let packages = |serde: &'static str, mio: &'static str| {
            lock(&[
                ("app", "0.1.0", false, &["core"]),
                ("core", "0.1.0", false, &["serde"]),
                ("net", "0.1.0", false, &["tokio"]),
                ("lonely", "0.1.0", false, &["itoa"]),
                ("serde", serde, true, &[]),
                ("tokio", "1.40.0", true, &["mio"]),
                ("mio", mio, true, &[]),
                ("itoa", "1.0.0", true, &[]),
            ])
        };
        let old = packages("1.0.1", "1.0.2");
        assert_eq!(
            claimed(&old, &packages("1.0.2", "1.0.2")),
            [entry("core", &["serde 1.0.2"])]
        );
        assert_eq!(
            claimed(&old, &packages("1.0.1", "1.0.3")),
            [entry("net", &["mio 1.0.3"])]
        );
    }

    #[test]
    fn bumping_a_path_package_version_claims_nobody() {
        let packages = |core: &'static str| {
            lock(&[
                ("app", "0.1.0", false, &["core", "serde"]),
                ("core", core, false, &["serde"]),
                ("serde", "1.0.1", true, &[]),
            ])
        };
        assert_eq!(claimed(&packages("0.1.0"), &packages("0.2.0")), []);
    }

    #[test]
    fn a_comment_or_reordering_claims_nobody() {
        let old = lock(&[
            ("core", "0.1.0", false, &["serde"]),
            ("serde", "1.0.1", true, &[]),
        ]);
        let new = lock(&[
            ("serde", "1.0.1", true, &[]),
            ("core", "0.1.0", false, &["serde"]),
        ]) + "\n# touched\n";
        assert_eq!(claimed(&old, &new), []);
    }

    #[test]
    fn a_changed_checksum_claims_the_crates_using_the_package() {
        let old = lock(&[
            ("core", "0.1.0", false, &["serde"]),
            ("serde", "1.0.1", true, &[]),
        ]);
        let new = old.replace("checksum = \"serde-1.0.1\"", "checksum = \"other\"");
        assert_eq!(claimed(&old, &new), [entry("core", &["serde 1.0.1"])]);
    }

    #[test]
    fn a_second_version_of_a_dependency_claims_only_the_crate_that_moved_to_it() {
        // Adding `rand 0.9.0` makes Cargo spell every reference to `rand` with a version;
        // `old-user` still means the package it meant before.
        let old = lock(&[
            ("new-user", "0.1.0", false, &["rand"]),
            ("old-user", "0.1.0", false, &["rand"]),
            ("rand", "0.8.0", true, &[]),
        ]);
        let new = lock(&[
            ("new-user", "0.1.0", false, &["rand 0.9.0"]),
            ("old-user", "0.1.0", false, &["rand 0.8.0"]),
            ("rand", "0.8.0", true, &[]),
            ("rand", "0.9.0", true, &[]),
        ]);
        assert_eq!(claimed(&old, &new), [entry("new-user", &["rand 0.9.0"])]);
    }

    #[test]
    fn a_dependency_dropped_by_a_third_party_or_by_the_crate_itself_claims_the_crate() {
        let old = lock(&[
            ("core", "0.1.0", false, &["serde", "itoa"]),
            ("net", "0.1.0", false, &["tokio"]),
            ("serde", "1.0.1", true, &[]),
            ("itoa", "1.0.0", true, &[]),
            ("tokio", "1.40.0", true, &["mio"]),
            ("mio", "1.0.2", true, &[]),
        ]);
        let new = lock(&[
            ("core", "0.1.0", false, &["serde"]),
            ("net", "0.1.0", false, &["tokio"]),
            ("serde", "1.0.1", true, &[]),
            ("tokio", "1.40.0", true, &[]),
        ]);
        assert_eq!(
            claimed(&old, &new),
            [
                entry("core", &["itoa 1.0.0 (removed)"]),
                entry("net", &["tokio 1.40.0"])
            ]
        );
    }

    #[test]
    fn a_reference_with_a_source_resolves_against_a_git_package_pinned_to_a_commit() {
        let git = |commit: &str| {
            format!(
                "version = 4\n\n[[package]]\nname = \"core\"\nversion = \"0.1.0\"\n\
                 dependencies = [\"dep 1.0.0 (git+https://example.com/dep)\"]\n\n\
                 [[package]]\nname = \"dep\"\nversion = \"1.0.0\"\n\
                 source = \"git+https://example.com/dep#{commit}\"\n\n\
                 [[package]]\nname = \"dep\"\nversion = \"1.0.0\"\n\
                 source = \"{REGISTRY}\"\nchecksum = \"dep\"\n"
            )
        };
        assert_eq!(
            claimed(&git("aaaa"), &git("bbbb")),
            [entry("core", &["dep 1.0.0"])]
        );
    }

    #[test]
    fn a_change_outside_the_packages_or_an_unreadable_lockfile_cannot_be_narrowed() {
        let old = lock(&[("core", "0.1.0", false, &[])]);
        let downgraded = old.replace("version = 4", "version = 3");
        assert!(changed_dependencies(&old, &downgraded).is_err());
        assert!(changed_dependencies(&old, "not a lockfile [").is_err());
        let dangling = lock(&[("core", "0.1.0", false, &["serde"])]);
        assert!(changed_dependencies(&old, &dangling).is_err());
    }
}
