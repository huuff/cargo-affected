//! Combines Global Files, Changed Crates and the dependency closure into Affected Crates,
//! each carrying the single reason of highest precedence. Pure: no git, no Cargo.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::graph::DependencyGraph;

/// Why a crate is an Affected Crate. Variants are declared in ascending precedence, so
/// the greatest applicable reason is the one reported: `global` > `changed` > `dependent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reason {
    /// The crate is a Dependent of a Changed Crate.
    Dependent,
    /// The crate's own files are in the Change Set.
    Changed,
    /// A Global File is in the Change Set.
    Global,
}

/// The Affected Crates and their reasons, sorted by name.
///
/// `global` names the crates affected because a Global File changed (every workspace
/// member, or nothing); `changed` the Changed Crates. Dependents come from the closure
/// of `changed` over `graph`.
pub fn affected_crates(
    global: impl IntoIterator<Item = String>,
    changed: impl IntoIterator<Item = String>,
    graph: &DependencyGraph,
) -> BTreeMap<String, Reason> {
    let changed: Vec<String> = changed.into_iter().collect();
    let dependents = graph.with_dependents(changed.iter().cloned());
    let mut affected = BTreeMap::new();
    let layers = [
        (
            Reason::Dependent,
            dependents.into_iter().collect::<Vec<_>>(),
        ),
        (Reason::Changed, changed),
        (Reason::Global, global.into_iter().collect()),
    ];
    for (reason, names) in layers {
        for name in names {
            affected
                .entry(name)
                .and_modify(|r| *r = reason.max(*r))
                .or_insert(reason);
        }
    }
    affected
}

/// The direct dependencies of `name` through which affectedness arrived: those that are
/// Affected Crates themselves, sorted by name. Empty unless `name` is affected as a
/// Dependent, since a Changed Crate or a crate affected by a Global File needs no
/// dependency to explain it.
pub fn via(
    name: &str,
    affected: &BTreeMap<String, Reason>,
    graph: &DependencyGraph,
) -> Vec<String> {
    if affected.get(name) != Some(&Reason::Dependent) {
        return Vec::new();
    }
    graph
        .dependencies(name)
        .filter(|dependency| affected.contains_key(*dependency))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DependencyGraph;

    fn graph(edges: &[(&str, &str)]) -> DependencyGraph {
        DependencyGraph::from_edges(edges)
    }

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_crate_that_is_global_changed_and_dependent_at_once_is_reported_as_global() {
        // `app` depends on `core`, both have their own files in the Change Set, and a
        // Global File changed: `app` qualifies under all three reasons.
        let g = graph(&[("app", "core")]);
        let got = affected_crates(strings(&["app", "core"]), strings(&["app", "core"]), &g);
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            [
                ("app".to_string(), Reason::Global),
                ("core".to_string(), Reason::Global)
            ]
        );
    }

    #[test]
    fn a_crate_that_is_changed_and_dependent_at_once_is_reported_as_changed() {
        let g = graph(&[("app", "core")]);
        let got = affected_crates([], strings(&["app", "core"]), &g);
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            [
                ("app".to_string(), Reason::Changed),
                ("core".to_string(), Reason::Changed)
            ]
        );
    }

    #[test]
    fn a_dependent_of_a_changed_crate_is_affected_as_dependent() {
        let g = graph(&[("app", "mid"), ("mid", "core")]);
        let got = affected_crates([], strings(&["core"]), &g);
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            [
                ("app".to_string(), Reason::Dependent),
                ("core".to_string(), Reason::Changed),
                ("mid".to_string(), Reason::Dependent)
            ]
        );
    }

    #[test]
    fn global_crates_outside_the_closure_are_affected_too() {
        let g = graph(&[("app", "core")]);
        let got = affected_crates(strings(&["lonely", "app", "core"]), [], &g);
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            [
                ("app".to_string(), Reason::Global),
                ("core".to_string(), Reason::Global),
                ("lonely".to_string(), Reason::Global)
            ]
        );
    }

    #[test]
    fn via_names_the_affected_direct_dependencies_of_a_dependent_only() {
        // `app` depends on `mid` and on `util`; only `mid` leads back to the Changed Crate.
        let g = graph(&[("app", "mid"), ("app", "util"), ("mid", "core")]);
        let affected = affected_crates([], strings(&["core"]), &g);
        assert_eq!(via("app", &affected, &g), ["mid"]);
        assert_eq!(via("mid", &affected, &g), ["core"]);
        assert!(via("core", &affected, &g).is_empty());
        assert!(via("util", &affected, &g).is_empty());
    }

    #[test]
    fn via_of_a_diamond_lists_both_sides_sorted_and_is_empty_for_a_changed_crate() {
        let g = graph(&[
            ("top", "right"),
            ("top", "left"),
            ("left", "base"),
            ("right", "base"),
        ]);
        let affected = affected_crates([], strings(&["left", "right"]), &g);
        assert_eq!(via("top", &affected, &g), ["left", "right"]);
        assert!(via("left", &affected, &g).is_empty());

        // A Changed Crate whose dependencies are affected too explains itself.
        let affected = affected_crates([], strings(&["base", "top"]), &g);
        assert!(via("top", &affected, &g).is_empty());
    }

    #[test]
    fn via_is_empty_when_a_global_file_made_the_crate_affected() {
        let g = graph(&[("app", "core")]);
        let affected = affected_crates(strings(&["app", "core"]), strings(&["core"]), &g);
        assert!(via("app", &affected, &g).is_empty());
    }

    #[test]
    fn reason_precedence_is_global_then_changed_then_dependent() {
        assert!(Reason::Global > Reason::Changed);
        assert!(Reason::Changed > Reason::Dependent);
    }
}
