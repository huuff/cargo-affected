//! Maps files of the Change Set to the crates that own them. Pure: no git, no Cargo.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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

/// Names of the Changed Crates: for each file, the deepest crate whose directory
/// contains it. Files under no crate are unmapped and ignored. Sorted, deduplicated.
pub fn changed_crates<'a>(
    crates: &[CrateDir],
    files: impl IntoIterator<Item = &'a Path>,
) -> BTreeSet<String> {
    files
        .into_iter()
        .filter_map(|file| owner(crates, file))
        .map(|c| c.name.clone())
        .collect()
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
