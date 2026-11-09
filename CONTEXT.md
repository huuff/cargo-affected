# haf-cargo-affected

Decides which workspace crates CI needs to run for, given what changed in git.

## Language

**Change Set**:
The set of files git reports as different between two states of the repository.
_Avoid_: diff, changes, changed files

**Base**:
The git state the Change Set is computed against: a merge-base with a branch, an explicit ref, or HEAD when inspecting the working tree.
_Avoid_: target, origin

**Changed Crate**:
A workspace member whose own files appear in the Change Set.
_Avoid_: modified crate, touched crate, dirty crate

**Dependent**:
A crate that depends on another, directly or transitively. If A depends on B, A is a dependent of B.
_Avoid_: reverse dependency, downstream, consumer

**Affected Crate**:
A Changed Crate, or any workspace member that is a Dependent of one.
_Avoid_: impacted crate, dirty crate

**Global File**:
A file outside any crate whose change makes every workspace member an Affected Crate, such as the workspace manifest or lockfile.
_Avoid_: root file, shared file

**Extra Path**:
A path outside a crate's directory that the crate declares as its own, so changes there make it a Changed Crate.
_Avoid_: include, external dependency
