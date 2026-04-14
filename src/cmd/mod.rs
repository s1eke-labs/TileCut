pub mod cut;
pub mod inspect;
pub mod stitch;
pub mod validate;

use anyhow::Result;

use crate::cli::{Cli, Command};

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Inspect(args) => inspect::run(args),
        Command::Cut(args) => cut::run(args),
        Command::Stitch(args) => stitch::run(args),
        Command::Validate(args) => validate::run(args),
    }
}
