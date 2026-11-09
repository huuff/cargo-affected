//! Renders a `Report` in each output format.

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
}

/// The report in the given format; ends with a newline unless empty.
pub fn render(report: &Report, format: Format) -> Result<String> {
    match format {
        Format::List => Ok(list(report)),
        Format::Json => json(report),
    }
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::affected::Reason;
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
        }
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

    #[test]
    fn json_to_is_the_commit_hash_when_a_commit_was_inspected() {
        let text = render(&report(To::Commit("89abcdef".to_string())), Format::Json).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["to"], "89abcdef");
    }
}
