//! `igris`: the IGRIS command line. M0 ships only `--version` and `--help`.

use clap::Parser;

/// Agent orchestration: Hermes takes your request, a decision layer picks,
/// agents build and review in Herdr, and the shell decides when it is done.
#[derive(Parser)]
#[command(name = "igris", version)]
struct Cli {}

fn main() {
    Cli::parse();
}
