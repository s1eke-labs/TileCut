pub mod backend;
pub mod cli;
pub mod cmd;
pub mod coords;
pub mod error;
pub mod manifest;
pub mod naming;
pub mod overview;
pub mod plan;
pub mod validate;

use anyhow::Result;

pub fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    cmd::run(cli)
}
