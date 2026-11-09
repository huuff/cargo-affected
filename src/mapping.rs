//! Maps files of the Change Set to the crates that own them. Pure: no git, no Cargo.
//!
//! Each file lands as follows. A Global File is claimed by no crate, so a root package
//! never owns the workspace manifest. Otherwise every crate whose Extra Path globs match
//! claims the file, and so does the deepest crate whose directory contains it unless that
//! crate's `ignore` globs exclude it. A file nobody claims is unmapped.
//!
//! All globs are gitignore-style but anchored at their root: the workspace root for Global
//! Files, the crate directory for `extra-paths` and `ignore`. `*` and `?` stay within one
//! path component, `**` spans directories, and a leading `/` or `./` means the same as no
//! prefix. `*.toml` therefore matches `deny.toml` but not `ci/deny.toml`; write
//! `**/*.toml` to match at any depth. See ADR 0002.
//!
//! An `extra-paths` glob may point inside a crate directory, the crate's own included; it
//! then claims the file regardless of that crate's `ignore` globs, which only decide
//! whether the file counts for the crate whose directory contains it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// Anchored gitignore-style globs, matched against paths relative to their root.
#[derive(Debug, Clone, Default)]
pub struct Globs {
    set: GlobSet,
}

impl Globs {
    /// Compiles `patterns`; `origin` names the setting they come from, for error messages.
    pub fn new<'a>(patterns: impl IntoIterator<Item = &'a str>, origin: &str) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = GlobBuilder::new(without_anchor(pattern))
                .literal_separator(true)
                .build()
                .with_context(|| format!("invalid glob `{pattern}` in {origin}"))?;
            builder.add(glob);
        }
        let set = builder
            .build()
            .with_context(|| format!("building the matcher for {origin}"))?;
        Ok(Self { set })
    }

    pub fn matches(&self, path: &Path) -> bool {
        self.set.is_match(path)
    }
}

/// Strips the leading `/` or `./` that anchors a gitignore pattern at its root; our
/// patterns always are, so the prefix carries no information.
fn without_anchor(pattern: &str) -> &str {
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
    pattern.strip_prefix('/').unwrap_or(pattern)
}

/// A crate, the directory it owns relative to the workspace root, and the globs from its
/// `[package.metadata.affected]` table. The root package owns the empty path, i.e. the
/// whole workspace.
#[derive(Debug, Clone)]
pub struct CrateDir {
    pub name: String,
    pub dir: PathBuf,
    /// Extra Paths, rebased so they match paths relative to the workspace root.
    extra_paths: Globs,
    /// Files inside `dir` that do not make the crate a Changed Crate, relative to `dir`.
    ignore: Globs,
}

impl CrateDir {
    /// A crate without an `[package.metadata.affected]` table.
    pub fn new(name: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            dir: dir.into(),
            extra_paths: Globs::default(),
            ignore: Globs::default(),
        }
    }

    /// Declares the crate's `extra-paths`: globs relative to its own directory, which may
    /// start with `../`, that make it a Changed Crate when matched.
    pub fn extra_paths(mut self, globs: &[String]) -> Result<Self> {
        let origin = crate_setting(&self.name, "extra-paths");
        let rebased = globs
            .iter()
            .map(|glob| rebase(&self.dir, glob).with_context(|| origin.clone()))
            .collect::<Result<Vec<_>>>()?;
        self.extra_paths = Globs::new(rebased.iter().map(String::as_str), &origin)?;
        Ok(self)
    }

    /// Declares the crate's `ignore`: globs relative to its own directory for files inside
    /// it that do not make it a Changed Crate.
    pub fn ignore(mut self, globs: &[String]) -> Result<Self> {
        let origin = crate_setting(&self.name, "ignore");
        self.ignore = Globs::new(globs.iter().map(String::as_str), &origin)?;
        Ok(self)
    }

    fn claims_as_extra_path(&self, file: &Path) -> bool {
        self.extra_paths.matches(file)
    }

    fn contains(&self, file: &Path) -> bool {
        file.starts_with(&self.dir)
    }

    /// Whether `file`, which lies inside `dir`, matches the crate's `ignore` globs.
    fn ignores(&self, file: &Path) -> bool {
        file.strip_prefix(&self.dir)
            .is_ok_and(|inside| self.ignore.matches(inside))
    }
}

/// Rebases a glob written relative to `dir` onto `dir`'s own root, lexically: `.` and
/// `..` are resolved against the leading components, and glob metacharacters in `dir`
/// itself are escaped. A glob that climbs above the root is an error, since only files
/// under the workspace root are ever mapped.
fn rebase(dir: &Path, glob: &str) -> Result<String> {
    let mut parts: Vec<String> = dir
        .components()
        .map(|c| globset::escape(&c.as_os_str().to_string_lossy()))
        .collect();
    for part in without_anchor(glob).split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts
                    .pop()
                    .ok_or_else(|| anyhow!("glob `{glob}` escapes the workspace root"))?;
            }
            part => parts.push(part.to_string()),
        }
    }
    Ok(parts.join("/"))
}

/// The files whose change makes every workspace member an Affected Crate: the built-in
/// set plus the user's `[workspace.metadata.affected].global` globs, all matched against
/// paths relative to the workspace root.
#[derive(Debug)]
pub struct GlobalFiles {
    globs: Globs,
}

/// Names a crate's setting in error messages, the same way for a wrong type and a bad glob.
pub fn crate_setting(crate_name: &str, key: &str) -> String {
    format!("`[package.metadata.affected].{key}` of crate `{crate_name}`")
}

impl GlobalFiles {
    /// The setting holding the user's Global File globs, as named in error messages.
    pub const SETTING: &str = "`[workspace.metadata.affected].global`";

    /// Globs every workspace treats as Global Files.
    pub const BUILT_IN: [&str; 5] = [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain",
        "rust-toolchain.toml",
        ".cargo/**",
    ];

    /// The built-in Global Files extended with the user's globs.
    pub fn new(user_globs: &[String]) -> Result<Self> {
        let patterns = Self::BUILT_IN
            .iter()
            .copied()
            .chain(user_globs.iter().map(String::as_str));
        let globs = Globs::new(patterns, Self::SETTING)?;
        Ok(Self { globs })
    }

    /// Whether `file`, relative to the workspace root, is a Global File.
    pub fn matches(&self, file: &Path) -> bool {
        self.globs.matches(file)
    }
}

/// Where one file of the Change Set landed; each variant names how it was claimed. A file
/// claimed by several crates has one outcome per crate. Variants are declared in mapping
/// order, which `Ord` turns into the order of a file's outcomes within a `Mapping`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    /// Matches a Global File glob; no crate claims it.
    GlobalFile,
    /// Matches an `extra-paths` glob of the named crate.
    ExtraPath(String),
    /// Lies inside the named crate's directory, the deepest one containing it.
    CrateDirectory(String),
    /// Lies inside the named crate's directory but matches its `ignore` globs.
    Ignored(String),
    /// Under no crate and matching no glob.
    Unmapped,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileOutcome {
    pub file: PathBuf,
    pub outcome: Outcome,
}

/// Where the files of the Change Set landed, sorted by file and deduplicated.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Mapping {
    pub outcomes: Vec<FileOutcome>,
}

impl Mapping {
    /// The Global Files in the Change Set.
    pub fn global_files(&self) -> Vec<&Path> {
        self.outcomes
            .iter()
            .filter(|o| o.outcome == Outcome::GlobalFile)
            .map(|o| o.file.as_path())
            .collect()
    }

    /// Names of the Changed Crates.
    pub fn changed(&self) -> BTreeSet<String> {
        self.outcomes
            .iter()
            .filter_map(|o| match &o.outcome {
                Outcome::ExtraPath(name) | Outcome::CrateDirectory(name) => Some(name.clone()),
                Outcome::GlobalFile | Outcome::Ignored(_) | Outcome::Unmapped => None,
            })
            .collect()
    }
}

/// Maps each file as described in the module documentation.
pub fn map_files<'a>(
    crates: &[CrateDir],
    globals: &GlobalFiles,
    files: impl IntoIterator<Item = &'a Path>,
) -> Mapping {
    let mut outcomes: Vec<FileOutcome> = files
        .into_iter()
        .flat_map(|file| {
            map_file(crates, globals, file)
                .into_iter()
                .map(|outcome| FileOutcome {
                    file: file.to_path_buf(),
                    outcome,
                })
        })
        .collect();
    outcomes.sort();
    outcomes.dedup();
    Mapping { outcomes }
}

fn map_file(crates: &[CrateDir], globals: &GlobalFiles, file: &Path) -> Vec<Outcome> {
    if globals.matches(file) {
        return vec![Outcome::GlobalFile];
    }
    let mut outcomes: Vec<Outcome> = crates
        .iter()
        .filter(|c| c.claims_as_extra_path(file))
        .map(|c| Outcome::ExtraPath(c.name.clone()))
        .collect();
    if let Some(owner) = owner(crates, file) {
        outcomes.push(if owner.ignores(file) {
            Outcome::Ignored(owner.name.clone())
        } else {
            Outcome::CrateDirectory(owner.name.clone())
        });
    }
    if outcomes.is_empty() {
        outcomes.push(Outcome::Unmapped);
    }
    outcomes
}

fn owner<'c>(crates: &'c [CrateDir], file: &Path) -> Option<&'c CrateDir> {
    crates
        .iter()
        .filter(|c| c.contains(file))
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
        map_files(crates, &GlobalFiles::new(&[]).unwrap(), files).changed()
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
        assert_eq!(got.global_files(), [Path::new("Cargo.lock")]);
        assert!(got.changed().is_empty());
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
        let want: Vec<&Path> = ["Cargo.lock", "ci/nested/run.sh", "ci/test.sh", "deny.toml"]
            .into_iter()
            .map(Path::new)
            .collect();
        assert_eq!(got.global_files(), want);
        assert_eq!(got.changed().into_iter().collect::<Vec<_>>(), ["a"]);
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
    fn an_extra_path_outside_the_crate_directory_marks_the_crate_changed() {
        let crates = [
            CrateDir::new("codegen", "codegen")
                .extra_paths(&["../proto/**".to_string()])
                .unwrap(),
            CrateDir::new("lib", "lib"),
        ];
        let globals = GlobalFiles::new(&[]).unwrap();
        let got = map_files(&crates, &globals, paths(&["proto/service.proto"]));
        assert_eq!(got.changed().into_iter().collect::<Vec<_>>(), ["codegen"]);
        assert_eq!(
            got.outcomes,
            [FileOutcome {
                file: PathBuf::from("proto/service.proto"),
                outcome: Outcome::ExtraPath("codegen".to_string()),
            }]
        );
    }

    #[test]
    fn a_file_matching_the_owning_crates_ignore_globs_is_ignored_not_changed() {
        let crates = [CrateDir::new("lib", "lib")
            .ignore(&["README.md".to_string(), "docs/**".to_string()])
            .unwrap()];
        let globals = GlobalFiles::new(&[]).unwrap();
        // `README.md` is anchored at the crate directory, so the nested readme still counts
        // and is the only file making `lib` a Changed Crate.
        let files = ["lib/README.md", "lib/docs/guide.md", "lib/src/README.md"];
        let got = map_files(&crates, &globals, paths(&files));
        assert_eq!(got.changed().into_iter().collect::<Vec<_>>(), ["lib"]);
        assert_eq!(
            got.outcomes,
            [
                FileOutcome {
                    file: PathBuf::from("lib/README.md"),
                    outcome: Outcome::Ignored("lib".to_string()),
                },
                FileOutcome {
                    file: PathBuf::from("lib/docs/guide.md"),
                    outcome: Outcome::Ignored("lib".to_string()),
                },
                FileOutcome {
                    file: PathBuf::from("lib/src/README.md"),
                    outcome: Outcome::CrateDirectory("lib".to_string()),
                },
            ]
        );
    }

    #[test]
    fn an_extra_path_inside_another_crates_directory_marks_both_crates_changed() {
        let crates = [
            CrateDir::new("codegen", "codegen")
                .extra_paths(&["../schema/src/*.rs".to_string()])
                .unwrap(),
            CrateDir::new("schema", "schema")
                .ignore(&["src/generated.rs".to_string()])
                .unwrap(),
        ];
        let globals = GlobalFiles::new(&[]).unwrap();
        let got = map_files(&crates, &globals, paths(&["schema/src/types.rs"]));
        assert_eq!(
            got.changed().into_iter().collect::<Vec<_>>(),
            ["codegen", "schema"]
        );
        assert_eq!(
            got.outcomes,
            [
                FileOutcome {
                    file: PathBuf::from("schema/src/types.rs"),
                    outcome: Outcome::ExtraPath("codegen".to_string()),
                },
                FileOutcome {
                    file: PathBuf::from("schema/src/types.rs"),
                    outcome: Outcome::CrateDirectory("schema".to_string()),
                },
            ]
        );

        // The owner's `ignore` keeps the file away from the owner only; the Extra Path
        // still makes `codegen` Changed.
        let got = map_files(&crates, &globals, paths(&["schema/src/generated.rs"]));
        assert_eq!(got.changed().into_iter().collect::<Vec<_>>(), ["codegen"]);
        assert_eq!(
            got.outcomes,
            [
                FileOutcome {
                    file: PathBuf::from("schema/src/generated.rs"),
                    outcome: Outcome::ExtraPath("codegen".to_string()),
                },
                FileOutcome {
                    file: PathBuf::from("schema/src/generated.rs"),
                    outcome: Outcome::Ignored("schema".to_string()),
                },
            ]
        );
    }

    #[test]
    fn a_leading_slash_or_dot_slash_anchors_at_the_root_like_no_prefix() {
        let globals = GlobalFiles::new(&["/deny.toml".to_string(), "./ci/**".to_string()]).unwrap();
        assert!(globals.matches(Path::new("deny.toml")));
        assert!(globals.matches(Path::new("ci/test.sh")));
        assert!(!globals.matches(Path::new("crates/a/deny.toml")));

        let crates = [CrateDir::new("codegen", "codegen")
            .extra_paths(&["/../proto/**".to_string()])
            .unwrap()
            .ignore(&["./README.md".to_string()])
            .unwrap()];
        let files = ["proto/service.proto", "codegen/README.md"];
        let got = map_files(&crates, &GlobalFiles::new(&[]).unwrap(), paths(&files));
        assert_eq!(
            got.outcomes,
            [
                FileOutcome {
                    file: PathBuf::from("codegen/README.md"),
                    outcome: Outcome::Ignored("codegen".to_string()),
                },
                FileOutcome {
                    file: PathBuf::from("proto/service.proto"),
                    outcome: Outcome::ExtraPath("codegen".to_string()),
                },
            ]
        );
    }

    #[test]
    fn an_extra_path_is_relative_to_the_crate_directory_not_the_workspace_root() {
        let crates = [CrateDir::new("a", "crates/a")
            .extra_paths(&["assets/**".to_string(), "../shared/*.json".to_string()])
            .unwrap()];
        let files = [
            "crates/a/assets/logo.png",
            "crates/shared/config.json",
            "assets/logo.png",
            "shared/config.json",
        ];
        let got = map_files(&crates, &GlobalFiles::new(&[]).unwrap(), paths(&files));
        assert_eq!(
            got.outcomes,
            [
                FileOutcome {
                    file: PathBuf::from("assets/logo.png"),
                    outcome: Outcome::Unmapped,
                },
                FileOutcome {
                    file: PathBuf::from("crates/a/assets/logo.png"),
                    outcome: Outcome::ExtraPath("a".to_string()),
                },
                FileOutcome {
                    file: PathBuf::from("crates/a/assets/logo.png"),
                    outcome: Outcome::CrateDirectory("a".to_string()),
                },
                FileOutcome {
                    file: PathBuf::from("crates/shared/config.json"),
                    outcome: Outcome::ExtraPath("a".to_string()),
                },
                FileOutcome {
                    file: PathBuf::from("shared/config.json"),
                    outcome: Outcome::Unmapped,
                },
            ]
        );
    }

    #[test]
    fn an_extra_path_climbing_above_the_workspace_root_is_an_error_naming_the_crate() {
        let err = CrateDir::new("codegen", "codegen")
            .extra_paths(&["../../proto/**".to_string()])
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("`codegen`"), "{msg}");
        assert!(msg.contains("../../proto/**"), "{msg}");
        assert!(msg.contains("workspace root"), "{msg}");
    }

    #[test]
    fn an_invalid_crate_glob_is_an_error_naming_the_crate_and_the_setting() {
        let err = CrateDir::new("lib", "lib")
            .ignore(&["docs/[".to_string()])
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("`lib`"), "{msg}");
        assert!(msg.contains("[package.metadata.affected].ignore"), "{msg}");
        assert!(msg.contains("docs/["), "{msg}");
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
