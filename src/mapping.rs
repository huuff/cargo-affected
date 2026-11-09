//! Maps files of the Change Set to the crates that own them. Pure: no git, no Cargo.
//!
//! A file is first checked against the Global Files; only files that are not Global Files
//! are mapped to a crate, so a root package never claims the workspace manifest.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// A crate and the directory it owns, relative to the workspace root.
/// The root package owns the empty path, i.e. the whole workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateDir {
    pub name: String,
    pub dir: PathBuf,
}

impl CrateDir {
    pub fn new(name: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            dir: dir.into(),
        }
    }
}

/// The files whose change makes every workspace member an Affected Crate: the built-in
/// set plus the user's `[workspace.metadata.affected].global` globs, all matched against
/// paths relative to the workspace root.
#[derive(Debug)]
pub struct GlobalFiles {
    set: GlobSet,
}

impl GlobalFiles {
    /// Globs every workspace treats as Global Files.
    pub const BUILT_IN: [&str; 5] = [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain",
        "rust-toolchain.toml",
        ".cargo/**",
    ];

    /// The built-in Global Files extended with the user's gitignore-style globs: `*` and
    /// `?` stay within one path component, `**` spans directories.
    pub fn new(user_globs: &[String]) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        for pattern in Self::BUILT_IN
            .iter()
            .copied()
            .chain(user_globs.iter().map(String::as_str))
        {
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .with_context(|| {
                    format!("invalid glob `{pattern}` in `[workspace.metadata.affected].global`")
                })?;
            builder.add(glob);
        }
        let set = builder
            .build()
            .context("building the Global File matcher")?;
        Ok(Self { set })
    }

    /// Whether `file`, relative to the workspace root, is a Global File.
    pub fn matches(&self, file: &Path) -> bool {
        self.set.is_match(file)
    }
}

/// Where the files of the Change Set landed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// Files matching a Global File glob, sorted and deduplicated.
    pub global_files: Vec<PathBuf>,
    /// Names of the Changed Crates, sorted and deduplicated.
    pub changed: BTreeSet<String>,
}

/// Maps each file: Global Files first, otherwise the deepest crate whose directory
/// contains it. Files under no crate are unmapped and ignored.
pub fn map_files<'a>(
    crates: &[CrateDir],
    globals: &GlobalFiles,
    files: impl IntoIterator<Item = &'a Path>,
) -> Mapping {
    let mut mapping = Mapping::default();
    for file in files {
        if globals.matches(file) {
            mapping.global_files.push(file.to_path_buf());
        } else if let Some(owner) = owner(crates, file) {
            mapping.changed.insert(owner.name.clone());
        }
    }
    mapping.global_files.sort();
    mapping.global_files.dedup();
    mapping
}

fn owner<'c>(crates: &'c [CrateDir], file: &Path) -> Option<&'c CrateDir> {
    crates
        .iter()
        .filter(|c| file.starts_with(&c.dir))
        .max_by_key(|c| c.dir.components().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths<'a>(files: &'a [&str]) -> impl Iterator<Item = &'a Path> {
        files.iter().map(Path::new)
    }

    /// Changed Crates with only the built-in Global Files in play.
    fn changed_crates<'a>(
        crates: &[CrateDir],
        files: impl IntoIterator<Item = &'a Path>,
    ) -> BTreeSet<String> {
        map_files(crates, &GlobalFiles::new(&[]).unwrap(), files).changed
    }

    #[test]
    fn file_inside_a_crate_directory_marks_that_crate_changed() {
        let crates = [
            CrateDir::new("a", "crates/a"),
            CrateDir::new("b", "crates/b"),
        ];
        let got = changed_crates(&crates, paths(&["crates/b/src/lib.rs"]));
        assert_eq!(got.into_iter().collect::<Vec<_>>(), ["b"]);
    }

    #[test]
    fn nested_crate_claims_its_own_files_and_the_outer_crate_does_not() {
        let crates = [
            CrateDir::new("outer", "outer"),
            CrateDir::new("inner", "outer/inner"),
        ];
        let got = changed_crates(&crates, paths(&["outer/inner/src/lib.rs"]));
        assert_eq!(got.into_iter().collect::<Vec<_>>(), ["inner"]);
    }

    #[test]
    fn root_package_owns_files_not_claimed_by_a_deeper_crate() {
        let crates = [CrateDir::new("root", ""), CrateDir::new("a", "crates/a")];
        let got = changed_crates(&crates, paths(&["src/main.rs", "crates/a/build.rs"]));
        assert_eq!(got.into_iter().collect::<Vec<_>>(), ["a", "root"]);
    }

    #[test]
    fn files_outside_every_crate_are_ignored() {
        let crates = [CrateDir::new("a", "crates/a")];
        let got = changed_crates(&crates, paths(&["README.md", "crates/about.txt"]));
        assert!(got.is_empty());
    }

    #[test]
    fn a_directory_whose_name_merely_starts_with_a_crate_dir_is_not_inside_it() {
        let crates = [CrateDir::new("a", "crates/a")];
        let got = changed_crates(&crates, paths(&["crates/ab/src/lib.rs"]));
        assert!(got.is_empty());
    }

    #[test]
    fn a_built_in_global_file_is_reported_as_global_and_claimed_by_no_crate() {
        let crates = [CrateDir::new("root", ""), CrateDir::new("a", "crates/a")];
        let globals = GlobalFiles::new(&[]).unwrap();
        let got = map_files(&crates, &globals, paths(&["Cargo.lock"]));
        assert_eq!(got.global_files, [PathBuf::from("Cargo.lock")]);
        assert!(got.changed.is_empty());
    }

    #[test]
    fn every_built_in_global_file_matches_at_the_root_only() {
        let globals = GlobalFiles::new(&[]).unwrap();
        for file in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            ".cargo/audit/config.toml",
        ] {
            assert!(globals.matches(Path::new(file)), "{file} should be global");
        }
        for file in [
            "crates/a/Cargo.toml",
            "crates/a/.cargo/config.toml",
            "Cargo.lock.bak",
        ] {
            assert!(
                !globals.matches(Path::new(file)),
                "{file} should not be global"
            );
        }
    }

    #[test]
    fn user_globs_extend_the_built_in_global_files() {
        let crates = [CrateDir::new("a", "crates/a")];
        let globals = GlobalFiles::new(&["ci/**".to_string(), "deny.toml".to_string()]).unwrap();
        let files = [
            "ci/test.sh",
            "ci/nested/run.sh",
            "deny.toml",
            "Cargo.lock",
            "crates/a/lib.rs",
        ];
        let got = map_files(&crates, &globals, paths(&files));
        let want: Vec<PathBuf> = ["Cargo.lock", "ci/nested/run.sh", "ci/test.sh", "deny.toml"]
            .into_iter()
            .map(PathBuf::from)
            .collect();
        assert_eq!(got.global_files, want);
        assert_eq!(got.changed.into_iter().collect::<Vec<_>>(), ["a"]);
    }

    #[test]
    fn a_single_star_stays_within_one_directory_while_a_double_star_spans_them() {
        let globals = GlobalFiles::new(&["*.toml".to_string(), "ci/*".to_string()]).unwrap();
        assert!(globals.matches(Path::new("deny.toml")));
        assert!(globals.matches(Path::new("ci/test.sh")));
        assert!(!globals.matches(Path::new("crates/a/Cargo.toml")));
        assert!(!globals.matches(Path::new("ci/nested/run.sh")));
    }

    #[test]
    fn an_invalid_user_glob_is_an_error_naming_the_pattern() {
        let err = GlobalFiles::new(&["ci/[".to_string()]).unwrap_err();
        assert!(format!("{err:#}").contains("ci/["), "{err:#}");
    }

    #[test]
    fn result_is_sorted_and_deduplicated() {
        let crates = [
            CrateDir::new("zeta", "zeta"),
            CrateDir::new("alpha", "alpha"),
        ];
        let files = ["zeta/a.rs", "alpha/b.rs", "zeta/c.rs"];
        let got = changed_crates(&crates, paths(&files));
        assert_eq!(got.into_iter().collect::<Vec<_>>(), ["alpha", "zeta"]);
    }
}
