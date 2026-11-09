//! The dependency graph between crates and the closure from Changed Crates to their
//! Dependents. Pure: no git, no Cargo.

use std::collections::{BTreeMap, BTreeSet};

/// Who depends on whom, by crate name. Every dependency written in a `Cargo.toml` is an
/// edge, whatever its kind, optionality or target.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DependencyGraph {
    /// Dependency name -> names of the crates that depend on it directly.
    dependents: BTreeMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    /// Records that `dependent` depends on `dependency`.
    pub fn add_dependency(&mut self, dependent: &str, dependency: &str) {
        self.dependents
            .entry(dependency.to_string())
            .or_default()
            .insert(dependent.to_string());
    }

    /// The Affected Crates: the Changed Crates plus every transitive Dependent of one.
    pub fn affected(&self, changed: impl IntoIterator<Item = String>) -> BTreeSet<String> {
        let mut affected = BTreeSet::new();
        let mut pending: Vec<String> = changed.into_iter().collect();
        while let Some(name) = pending.pop() {
            let dependents = self.dependents.get(&name);
            if affected.insert(name)
                && let Some(dependents) = dependents
            {
                pending.extend(dependents.iter().cloned());
            }
        }
        affected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(&str, &str)]) -> DependencyGraph {
        let mut g = DependencyGraph::default();
        for (dependent, dependency) in edges {
            g.add_dependency(dependent, dependency);
        }
        g
    }

    fn names(set: BTreeSet<String>) -> Vec<String> {
        set.into_iter().collect()
    }

    #[test]
    fn a_direct_dependent_of_a_changed_crate_is_affected() {
        let g = graph(&[("app", "core")]);
        assert_eq!(names(g.affected(["core".to_string()])), ["app", "core"]);
    }

    #[test]
    fn affectedness_propagates_transitively() {
        let g = graph(&[("app", "mid"), ("mid", "core")]);
        assert_eq!(
            names(g.affected(["core".to_string()])),
            ["app", "core", "mid"]
        );
    }

    #[test]
    fn dependencies_of_a_changed_crate_are_not_affected() {
        let g = graph(&[("app", "mid"), ("mid", "core")]);
        assert_eq!(names(g.affected(["mid".to_string()])), ["app", "mid"]);
    }

    #[test]
    fn a_diamond_reports_each_crate_once() {
        let g = graph(&[
            ("top", "left"),
            ("top", "right"),
            ("left", "base"),
            ("right", "base"),
        ]);
        assert_eq!(
            names(g.affected(["base".to_string()])),
            ["base", "left", "right", "top"]
        );
    }

    #[test]
    fn a_changed_crate_unknown_to_the_graph_is_still_affected() {
        let g = graph(&[("app", "core")]);
        assert_eq!(names(g.affected(["lonely".to_string()])), ["lonely"]);
    }

    #[test]
    fn no_changed_crates_means_nothing_is_affected() {
        let g = graph(&[("app", "core")]);
        assert!(g.affected([]).is_empty());
    }
}
