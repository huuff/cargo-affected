mod cli;

use std::process::ExitCode;

use cargo_affected::render::{diagnostics, render};
use cargo_affected::{Options, report};
use cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse_from_env();
    let format = cli.render_format();
    let verbose = cli.verbose;
    let opts = Options::from(cli);
    let rendered = report(&opts).and_then(|report| {
        let text = render(&report, format)?;
        Ok((diagnostics(&report), text))
    });
    match rendered {
        Ok((explanation, text)) => {
            if verbose {
                eprint!("{explanation}");
            }
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
