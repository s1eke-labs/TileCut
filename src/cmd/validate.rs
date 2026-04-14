use anyhow::Result;

use crate::cli::ValidateArgs;
use crate::error::CliError;
use crate::validate::validate_manifest_path;

pub fn run(args: ValidateArgs) -> Result<()> {
    let report = validate_manifest_path(&args.manifest)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if report.is_valid() {
        println!("Manifest: {}", report.manifest_path.display());
        println!("Checked Tiles: {}", report.checked_tiles);
        println!("Missing Tiles: {}", report.missing_tiles);
        println!("Status: ok");
    }
    if report.is_valid() {
        Ok(())
    } else {
        Err(CliError::validation_failed(&report).into())
    }
}
