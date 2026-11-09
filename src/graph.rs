//! The dependency graph between crates and the closure from Changed Crates to their
//! Dependents. Pure: no git, no Cargo.

use std::collections::{BTreeMap, BTreeSet};

/// Who depends on whom, by crate name. Every dependency written in a `Cargo.toml` is an
/// edge, whatever its kind, optionality or target.
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// Dependency name -> names of the crates that depend on it directly.
    dependents: BTreeMap<String, BTreeSet<String>>,
    /// Dependent name -> names of the crates it depends on directly.
    dependencies: BTreeMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    /// Records that `dependent` depends on `dependency`.
    pub fn add_dependency(&mut self, dependent: &str, dependency: &str) {
        self.dependents
            .entry(dependency.to_string())
            .or_default()
            .insert(dependent.to_string());
        self.dependencies
            .entry(dependent.to_string())
            .or_default()
            .insert(dependency.to_string());
    }

    /// The crates `dependent` depends on directly, sorted by name.
    pub fn dependencies<'a>(&'a self, dependent: &str) -> impl Iterator<Item = &'a str> + 'a {
        self.dependencies
            .get(dependent)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    /// The Changed Crates plus every transitive Dependent of one.
    pub fn with_dependents(&self, changed: impl IntoIterator<Item = String>) -> BTreeSet<String> {
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

    /// A graph from `(dependent, dependency)` pairs, for tests.
    #[cfg(test)]
    pub(crate) fn from_edges(edges: &[(&str, &str)]) -> Self {
        let mut g = Self::default();
        for (dependent, dependency) in edges {
            g.add_dependency(dependent, dependency);
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(&str, &str)]) -> DependencyGraph {
        DependencyGraph::from_edges(edges)
    }

    fn names(set: BTreeSet<String>) -> Vec<String> {
        set.into_iter().collect()
    }

    #[test]
    fn a_direct_dependent_of_a_changed_crate_is_affected() {
        let g = graph(&[("app", "core")]);
        assert_eq!(
            names(g.with_dependents(["core".to_string()])),
            ["app", "core"]
        );
    }

    #[test]
    fn affectedness_propagates_transitively() {
        let g = graph(&[("app", "mid"), ("mid", "core")]);
        assert_eq!(
            names(g.with_dependents(["core".to_string()])),
            ["app", "core", "mid"]
        );
    }

    #[test]
    fn dependencies_of_a_changed_crate_are_not_affected() {
        let g = graph(&[("app", "mid"), ("mid", "core")]);
        assert_eq!(
            names(g.with_dependents(["mid".to_string()])),
            ["app", "mid"]
        );
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
            names(g.with_dependents(["base".to_string()])),
            ["base", "left", "right", "top"]
        );
    }

    #[test]
    fn a_cycle_through_a_dev_dependency_terminates_and_reports_each_crate_once() {
        // `core-tests` depends on `core`, and `core` dev-depends on `core-tests`.
        let g = graph(&[
            ("core-tests", "core"),
            ("core", "core-tests"),
            ("app", "core"),
        ]);
        assert_eq!(
            names(g.with_dependents(["core".to_string()])),
            ["app", "core", "core-tests"]
        );
    }

    #[test]
    fn a_changed_crate_unknown_to_the_graph_is_still_affected() {
        let g = graph(&[("app", "core")]);
        assert_eq!(names(g.with_dependents(["lonely".to_string()])), ["lonely"]);
    }

    #[test]
    fn direct_dependencies_are_listed_sorted_and_transitive_ones_are_not() {
        let g = graph(&[("top", "right"), ("top", "left"), ("left", "base")]);
        assert_eq!(g.dependencies("top").collect::<Vec<_>>(), ["left", "right"]);
        assert_eq!(g.dependencies("base").count(), 0);
        assert_eq!(g.dependencies("unknown").count(), 0);
    }

    #[test]
    fn no_changed_crates_means_nothing_is_affected() {
        let g = graph(&[("app", "core")]);
        assert!(g.with_dependents([]).is_empty());
    }
}
