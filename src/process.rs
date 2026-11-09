//! Reads the output of a finished child process, for the modules that shell out to
//! `git` and `cargo`.

use std::process::Output;

use anyhow::{Context, Result};

/// The trimmed stderr of a finished process, for an error in the program's own words.
pub(crate) fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

/// The stdout of a finished `program`, which must be UTF-8.
pub(crate) fn stdout_of(program: &str, out: Output) -> Result<String> {
    String::from_utf8(out.stdout).with_context(|| format!("{program} output is not UTF-8"))
}
