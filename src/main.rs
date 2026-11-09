mod cli;

use std::process::ExitCode;

use cargo_affected::{Options, changed_crates};
use cli::Cli;

fn main() -> ExitCode {
    let opts = Options::from(Cli::parse_from_env());
    match changed_crates(&opts) {
        Ok(names) => {
            for name in names {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
