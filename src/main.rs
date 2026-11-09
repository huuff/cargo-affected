mod cli;

use std::process::ExitCode;

use cargo_affected::render::render;
use cargo_affected::{Options, report};
use cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse_from_env();
    let format = cli.format.into();
    let opts = Options::from(cli);
    match report(&opts).and_then(|report| render(&report, format)) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
