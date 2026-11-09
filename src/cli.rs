//! Command line surface, shared by `cargo affected` and `cargo-affected`.

use std::ffi::OsString;
use std::path::PathBuf;

use cargo_affected::Options;
use cargo_affected::git::DEFAULT_BASE_BRANCHES;
use cargo_affected::render;
use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "cargo-affected", bin_name = "cargo affected", version, about)]
pub struct Cli {
    /// Branch to compute the Base from: the merge-base of this ref and HEAD.
    #[arg(long, value_name = "REF", conflicts_with = "from", help = base_help())]
    pub base: Option<String>,

    /// Use exactly this ref as the Base instead of a merge-base.
    #[arg(long, value_name = "REF")]
    pub from: Option<String>,

    /// Ref to compare against. Defaults to the working tree, including staged,
    /// unstaged and untracked files that are not gitignored.
    #[arg(long, value_name = "REF")]
    pub to: Option<String>,

    /// Path to the workspace `Cargo.toml`.
    #[arg(long, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// Print only the Changed Crates, leaving out crates affected merely as Dependents.
    #[arg(long)]
    pub changed_only: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t)]
    pub format: Format,

    /// Characters drawing the connectors of `--format tree`.
    #[arg(long, value_enum, default_value_t)]
    pub charset: Charset,

    /// Explain on stderr, for every file in the Change Set, which crate it mapped to and
    /// why: crate directory, extra path, global file, ignored or unmapped.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Format {
    /// One crate name per line.
    #[default]
    List,
    /// One JSON document: base and to commits, global files and, per crate, its manifest
    /// path, reason, changed files and the dependencies it is affected through.
    Json,
    /// Why each crate is affected: changed crates and global files as roots with their
    /// dependents beneath, `cargo tree` style; a crate shown earlier is marked `(*)`.
    Tree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Charset {
    /// Box-drawing characters.
    #[default]
    Unicode,
    /// Plain ASCII, for terminals and logs without UTF-8.
    Ascii,
}

impl From<Charset> for render::Charset {
    fn from(charset: Charset) -> Self {
        match charset {
            Charset::Unicode => Self::Unicode,
            Charset::Ascii => Self::Ascii,
        }
    }
}

impl Cli {
    /// The output format to render, with the charset folded into the tree format.
    pub fn render_format(&self) -> render::Format {
        match self.format {
            Format::List => render::Format::List,
            Format::Json => render::Format::Json,
            Format::Tree => render::Format::Tree(self.charset.into()),
        }
    }

    /// Parses the process arguments, dropping the leading `affected` that Cargo inserts
    /// when dispatching `cargo affected`.
    pub fn parse_from_env() -> Self {
        Self::parse_from(without_cargo_subcommand(std::env::args_os()))
    }
}

impl From<Cli> for Options {
    fn from(cli: Cli) -> Self {
        Self {
            base: cli.base,
            from: cli.from,
            to: cli.to,
            manifest_path: cli.manifest_path,
            changed_only: cli.changed_only,
        }
    }
}

fn base_help() -> String {
    format!(
        "Branch to compute the Base from: the merge-base of this ref and HEAD. Defaults to `{}`, falling back to `{}`",
        DEFAULT_BASE_BRANCHES[0], DEFAULT_BASE_BRANCHES[1]
    )
}

fn without_cargo_subcommand(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut args: Vec<OsString> = args.into_iter().collect();
    if args.get(1).is_some_and(|a| a == "affected") {
        args.remove(1);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn cargo_dispatch_inserts_affected_which_is_dropped() {
        let got =
            without_cargo_subcommand(os_args(&["cargo-affected", "affected", "--to", "HEAD"]));
        assert_eq!(got, os_args(&["cargo-affected", "--to", "HEAD"]));
    }

    #[test]
    fn direct_invocation_is_left_alone() {
        let got = without_cargo_subcommand(os_args(&["cargo-affected", "--to", "HEAD"]));
        assert_eq!(got, os_args(&["cargo-affected", "--to", "HEAD"]));
    }

    #[test]
    fn format_defaults_to_list_and_accepts_json() {
        let cli = Cli::try_parse_from(["cargo-affected"]).unwrap();
        assert_eq!(cli.format, Format::List);
        let cli = Cli::try_parse_from(["cargo-affected", "--format", "json"]).unwrap();
        assert_eq!(cli.format, Format::Json);
        let err = Cli::try_parse_from(["cargo-affected", "--format", "yaml"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn tree_format_takes_the_charset_which_defaults_to_unicode() {
        let cli = Cli::try_parse_from(["cargo-affected", "--format", "tree"]).unwrap();
        assert_eq!(
            cli.render_format(),
            render::Format::Tree(render::Charset::Unicode)
        );
        let cli = Cli::try_parse_from(["cargo-affected", "--format", "tree", "--charset", "ascii"])
            .unwrap();
        assert_eq!(
            cli.render_format(),
            render::Format::Tree(render::Charset::Ascii)
        );
        let err = Cli::try_parse_from(["cargo-affected", "--charset", "utf8"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn verbose_is_off_by_default_and_has_a_short_flag() {
        assert!(!Cli::try_parse_from(["cargo-affected"]).unwrap().verbose);
        assert!(
            Cli::try_parse_from(["cargo-affected", "-v"])
                .unwrap()
                .verbose
        );
        assert!(
            Cli::try_parse_from(["cargo-affected", "--verbose"])
                .unwrap()
                .verbose
        );
    }

    #[test]
    fn base_and_from_are_mutually_exclusive() {
        let err =
            Cli::try_parse_from(["cargo-affected", "--base", "main", "--from", "v1"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }
}
