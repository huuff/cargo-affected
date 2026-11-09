//! Renders a `Report` in each output format.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};

use crate::report::Report;

/// An output format. The CLI mirrors this enum in `cli::Format`, which carries the help
/// text, so the library stays free of clap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// One crate name per line.
    #[default]
    List,
    /// One JSON document: the [`Report`] serialised field by field, pretty-printed.
    Json,
    /// A tree explaining why each crate is affected: Changed Crates, changed non-member
    /// crates and Global Files as roots, Dependents beneath them.
    Tree(Charset),
}

/// The characters drawing the tree's connectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Charset {
    /// Box-drawing characters: `├── `, `└── `, `│   `.
    #[default]
    Unicode,
    /// Plain ASCII: `|-- `, `` `-- ``, `|   `.
    Ascii,
}

/// The four strings a tree line is assembled from.
struct Connectors {
    /// Before a child that has siblings after it.
    tee: &'static str,
    /// Before the last child.
    elbow: &'static str,
    /// Indentation under a child that has siblings after it.
    bar: &'static str,
    /// Indentation under the last child.
    blank: &'static str,
}

impl Charset {
    fn connectors(self) -> Connectors {
        match self {
            Self::Unicode => Connectors {
                tee: "├── ",
                elbow: "└── ",
                bar: "│   ",
                blank: "    ",
            },
            Self::Ascii => Connectors {
                tee: "|-- ",
                elbow: "`-- ",
                bar: "|   ",
                blank: "    ",
            },
        }
    }
}

/// The report in the given format; ends with a newline unless empty.
pub fn render(report: &Report, format: Format) -> Result<String> {
    match format {
        Format::List => Ok(list(report)),
        Format::Json => json(report),
        Format::Tree(charset) => Ok(tree(report, charset)),
    }
}

/// The `-v` diagnostics: one line per file of the Change Set saying where it landed, in
/// the report's order; ends with a newline unless empty.
pub fn diagnostics(report: &Report) -> String {
    report
        .file_outcomes
        .iter()
        .map(|f| format!("{f}\n"))
        .collect()
}

fn list(report: &Report) -> String {
    report
        .affected
        .iter()
        .map(|c| format!("{}\n", c.name))
        .collect()
}

fn json(report: &Report) -> Result<String> {
    let mut text = serde_json::to_string_pretty(report).context("serialising the report")?;
    text.push('\n');
    Ok(text)
}

fn tree(report: &Report, charset: Charset) -> String {
    let mut tree = Tree {
        report,
        connectors: charset.connectors(),
        shown: BTreeSet::new(),
        out: String::new(),
    };
    // A Changed Crate is a root even when a Global File makes its reason `global`: the
    // file must not hide why the crate's Dependents are affected.
    let mut roots: Vec<Root> = report
        .affected
        .iter()
        .filter(|c| c.is_changed())
        .map(|c| Root::Member(&c.name))
        .chain(
            report
                .changed_non_members
                .iter()
                .map(|n| Root::NonMember(n)),
        )
        .chain(report.global_files.iter().map(|p| Root::GlobalFile(p)))
        .collect();
    roots.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    for root in roots {
        tree.root(root);
    }
    tree.out
}

/// A top-level node of the tree.
enum Root<'a> {
    /// A Changed Crate that is a workspace member.
    Member(&'a str),
    /// A changed non-member path crate.
    NonMember(&'a str),
    /// A Global File, relative to the workspace root.
    GlobalFile(&'a Path),
}

impl Root<'_> {
    /// The crate name or the file's path, which the roots are sorted by.
    fn sort_key(&self) -> Cow<'_, str> {
        match self {
            Self::Member(name) | Self::NonMember(name) => Cow::Borrowed(name),
            Self::GlobalFile(path) => path.to_string_lossy(),
        }
    }
}

/// The tree being written: the report it reads from and the text so far.
struct Tree<'a> {
    report: &'a Report,
    connectors: Connectors,
    /// Crates already expanded; a repeat is marked `(*)` and not expanded again.
    shown: BTreeSet<String>,
    out: String,
}

impl Tree<'_> {
    fn root(&mut self, root: Root) {
        match root {
            Root::Member(name) => self.crate_node(name, "", ""),
            Root::NonMember(name) => self.crate_node(name, " (non-member)", ""),
            Root::GlobalFile(path) => self.global_file(path),
        }
    }

    /// Writes `name` and its label `suffix` on the current line, then its Dependents
    /// indented by `prefix`.
    fn crate_node(&mut self, name: &str, suffix: &str, prefix: &str) {
        self.out.push_str(name);
        self.out.push_str(suffix);
        if !self.shown.insert(name.to_string()) {
            self.out.push_str(" (*)\n");
            return;
        }
        self.out.push('\n');
        let children = self
            .report
            .dependents
            .get(name)
            .map_or(&[][..], Vec::as_slice);
        for (i, child) in children.iter().enumerate() {
            let last = i + 1 == children.len();
            self.connector(prefix, last);
            let indent = if last {
                self.connectors.blank
            } else {
                self.connectors.bar
            };
            self.crate_node(child, "", &format!("{prefix}{indent}"));
        }
    }

    /// Writes a Global File root with every member as a leaf: the file affects them all
    /// alike, so there is nothing to walk and the leaves do not count as shown.
    fn global_file(&mut self, path: &Path) {
        self.out.push_str(&path.to_string_lossy());
        self.out.push('\n');
        let members = &self.report.affected;
        for (i, member) in members.iter().enumerate() {
            self.connector("", i + 1 == members.len());
            self.out.push_str(&member.name);
            self.out.push('\n');
        }
    }

    /// Writes the indentation and the connector that start a child's line.
    fn connector(&mut self, prefix: &str, last: bool) {
        self.out.push_str(prefix);
        self.out.push_str(if last {
            self.connectors.elbow
        } else {
            self.connectors.tee
        });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::affected::Reason;
    use crate::mapping::{FileOutcome, Outcome};
    use crate::report::{AffectedCrate, To};

    fn report(to: To) -> Report {
        Report {
            base: "0123abcd".to_string(),
            to,
            global_files: vec![PathBuf::from("Cargo.lock")],
            affected: vec![
                AffectedCrate {
                    name: "api".to_string(),
                    manifest_path: PathBuf::from("/ws/crates/api/Cargo.toml"),
                    reason: Reason::Dependent,
                    changed_files: Vec::new(),
                    via: vec!["core".to_string()],
                },
                AffectedCrate {
                    name: "core".to_string(),
                    manifest_path: PathBuf::from("/ws/crates/core/Cargo.toml"),
                    reason: Reason::Changed,
                    changed_files: vec![PathBuf::from("crates/core/src/lib.rs")],
                    via: Vec::new(),
                },
            ],
            changed_non_members: Vec::new(),
            dependents: BTreeMap::new(),
            file_outcomes: vec![
                FileOutcome {
                    file: PathBuf::from("Cargo.lock"),
                    outcome: Outcome::GlobalFile,
                },
                FileOutcome {
                    file: PathBuf::from("crates/core/src/lib.rs"),
                    outcome: Outcome::CrateDirectory("core".to_string()),
                },
            ],
        }
    }

    #[test]
    fn diagnostics_print_one_line_per_file_outcome_and_nothing_for_no_files() {
        assert_eq!(
            diagnostics(&report(To::Worktree)),
            "Cargo.lock -> global file\ncrates/core/src/lib.rs -> core (crate directory)\n"
        );
        let none = Report {
            file_outcomes: Vec::new(),
            ..report(To::Worktree)
        };
        assert_eq!(diagnostics(&none), "");
    }

    #[test]
    fn list_prints_one_name_per_line_and_nothing_for_an_empty_report() {
        assert_eq!(
            render(&report(To::Worktree), Format::List).unwrap(),
            "api\ncore\n"
        );
        let empty = Report {
            affected: Vec::new(),
            ..report(To::Worktree)
        };
        assert_eq!(render(&empty, Format::List).unwrap(), "");
    }

    #[test]
    fn json_has_the_spec_shape_with_lowercase_reasons_and_worktree_as_a_string() {
        let text = render(&report(To::Worktree), Format::Json).unwrap();
        assert!(text.ends_with('\n'));
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            doc,
            json!({
                "base": "0123abcd",
                "to": "worktree",
                "global_files": ["Cargo.lock"],
                "affected": [
                    {
                        "name": "api",
                        "manifest_path": "/ws/crates/api/Cargo.toml",
                        "reason": "dependent",
                        "changed_files": [],
                        "via": ["core"]
                    },
                    {
                        "name": "core",
                        "manifest_path": "/ws/crates/core/Cargo.toml",
                        "reason": "changed",
                        "changed_files": ["crates/core/src/lib.rs"],
                        "via": []
                    }
                ]
            })
        );
    }

    fn affected_crate(name: &str, reason: Reason, changed_files: &[&str]) -> AffectedCrate {
        AffectedCrate {
            name: name.to_string(),
            manifest_path: PathBuf::from(format!("/ws/crates/{name}/Cargo.toml")),
            reason,
            changed_files: changed_files.iter().map(PathBuf::from).collect(),
            via: Vec::new(),
        }
    }

    fn edges(edges: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        edges
            .iter()
            .map(|(name, dependents)| {
                (
                    name.to_string(),
                    dependents.iter().map(|d| d.to_string()).collect(),
                )
            })
            .collect()
    }

    /// `core` -> `mid` -> `app`, with `core` Changed.
    fn transitive() -> Report {
        Report {
            base: "0123abcd".to_string(),
            to: To::Worktree,
            global_files: Vec::new(),
            affected: vec![
                affected_crate("app", Reason::Dependent, &[]),
                affected_crate("core", Reason::Changed, &["crates/core/src/lib.rs"]),
                affected_crate("mid", Reason::Dependent, &[]),
            ],
            changed_non_members: Vec::new(),
            dependents: edges(&[("core", &["mid"]), ("mid", &["app"])]),
            file_outcomes: Vec::new(),
        }
    }

    #[test]
    fn tree_hangs_each_dependent_under_the_crate_it_depends_on() {
        assert_eq!(
            render(&transitive(), Format::Tree(Charset::Unicode)).unwrap(),
            "core\n\
             └── mid\n\
             \x20   └── app\n"
        );
    }

    /// `base` -> `left` -> `top` and `base` -> `right` -> `top`, with `base` Changed.
    fn diamond() -> Report {
        Report {
            affected: vec![
                affected_crate("base", Reason::Changed, &["crates/base/src/lib.rs"]),
                affected_crate("left", Reason::Dependent, &[]),
                affected_crate("right", Reason::Dependent, &[]),
                affected_crate("top", Reason::Dependent, &[]),
            ],
            dependents: edges(&[
                ("base", &["left", "right"]),
                ("left", &["top"]),
                ("right", &["top"]),
            ]),
            ..transitive()
        }
    }

    #[test]
    fn tree_marks_a_crate_already_shown_and_does_not_expand_it_again() {
        assert_eq!(
            render(&diamond(), Format::Tree(Charset::Unicode)).unwrap(),
            "base\n\
             ├── left\n\
             │   └── top\n\
             └── right\n\
             \x20   └── top (*)\n"
        );
    }

    #[test]
    fn tree_shows_a_changed_non_member_as_a_labelled_root_with_its_member_dependents() {
        let report = Report {
            affected: vec![
                affected_crate("app", Reason::Dependent, &[]),
                affected_crate("uses-foo", Reason::Dependent, &[]),
            ],
            changed_non_members: vec!["foo".to_string()],
            dependents: edges(&[("foo", &["uses-foo"]), ("uses-foo", &["app"])]),
            ..transitive()
        };
        assert_eq!(
            render(&report, Format::Tree(Charset::Unicode)).unwrap(),
            "foo (non-member)\n\
             └── uses-foo\n\
             \x20   └── app\n"
        );
    }

    #[test]
    fn tree_lists_every_member_as_a_leaf_under_a_global_file() {
        let report = Report {
            global_files: vec![PathBuf::from("Cargo.lock")],
            affected: vec![
                affected_crate("app", Reason::Global, &[]),
                affected_crate("core", Reason::Global, &[]),
                affected_crate("mid", Reason::Global, &[]),
            ],
            dependents: edges(&[("core", &["mid"]), ("mid", &["app"])]),
            ..transitive()
        };
        assert_eq!(
            render(&report, Format::Tree(Charset::Unicode)).unwrap(),
            "Cargo.lock\n\
             ├── app\n\
             ├── core\n\
             └── mid\n"
        );
    }

    #[test]
    fn tree_still_explains_a_crate_whose_own_files_changed_when_a_global_file_did_too() {
        // Reason precedence reports every member as `global`, yet `core`'s own files are
        // in the Change Set, so it stays a root and its Dependents are still explained.
        let report = Report {
            global_files: vec![PathBuf::from("Cargo.lock")],
            affected: vec![
                affected_crate("app", Reason::Global, &[]),
                affected_crate("core", Reason::Global, &["crates/core/src/lib.rs"]),
                affected_crate("mid", Reason::Global, &[]),
            ],
            ..transitive()
        };
        assert_eq!(
            render(&report, Format::Tree(Charset::Unicode)).unwrap(),
            "Cargo.lock\n\
             ├── app\n\
             ├── core\n\
             └── mid\n\
             core\n\
             └── mid\n\
             \x20   └── app\n"
        );
    }

    #[test]
    fn tree_in_ascii_uses_ascii_connectors_only() {
        let text = render(&diamond(), Format::Tree(Charset::Ascii)).unwrap();
        assert_eq!(
            text,
            "base\n\
             |-- left\n\
             |   `-- top\n\
             `-- right\n\
             \x20   `-- top (*)\n"
        );
        assert!(text.is_ascii());
    }

    #[test]
    fn tree_marks_a_root_already_shown_under_an_earlier_root() {
        let report = Report {
            affected: vec![
                affected_crate("app", Reason::Dependent, &[]),
                affected_crate("core", Reason::Changed, &["crates/core/src/lib.rs"]),
                affected_crate("mid", Reason::Changed, &["crates/mid/src/lib.rs"]),
            ],
            ..transitive()
        };
        assert_eq!(
            render(&report, Format::Tree(Charset::Unicode)).unwrap(),
            "core\n\
             └── mid\n\
             \x20   └── app\n\
             mid (*)\n"
        );
    }

    #[test]
    fn tree_prints_only_the_roots_when_the_report_has_no_dependents_to_walk() {
        let report = Report {
            affected: vec![affected_crate(
                "core",
                Reason::Changed,
                &["crates/core/src/lib.rs"],
            )],
            changed_non_members: vec!["foo".to_string()],
            dependents: BTreeMap::new(),
            ..transitive()
        };
        assert_eq!(
            render(&report, Format::Tree(Charset::Unicode)).unwrap(),
            "core\nfoo (non-member)\n"
        );
    }

    #[test]
    fn tree_of_an_empty_report_is_empty() {
        let empty = Report {
            affected: Vec::new(),
            dependents: BTreeMap::new(),
            ..transitive()
        };
        assert_eq!(render(&empty, Format::Tree(Charset::Unicode)).unwrap(), "");
    }

    #[test]
    fn json_to_is_the_commit_hash_when_a_commit_was_inspected() {
        let text = render(&report(To::Commit("89abcdef".to_string())), Format::Json).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["to"], "89abcdef");
    }
}
