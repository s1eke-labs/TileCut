use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use anyhow::Error;

use crate::validate::ValidationReport;

#[derive(Clone, Debug)]
pub struct CliError {
    summary: String,
    suggestions: Vec<String>,
    detail: Option<String>,
}

impl CliError {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            suggestions: Vec::new(),
            detail: None,
        }
    }

    pub fn suggestions<I, S>(mut self, suggestions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.suggestions = suggestions.into_iter().map(Into::into).collect();
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn input_not_found(path: &Path, detail: impl Into<String>) -> Self {
        Self::new(format!("Input file was not found: {}", path.display()))
            .suggestions([
                format!(
                    "Check that the path exists and points to an image file: {}",
                    path.display()
                ),
                "Pass a supported image such as .png, .jpg, .jpeg, .webp, or .tiff.".to_string(),
            ])
            .detail(detail)
    }

    pub fn input_unreadable(path: &Path, detail: impl Into<String>) -> Self {
        Self::new(format!("TileCut could not read the input file: {}", path.display()))
            .suggestions([
                "Check file permissions and verify that the file is accessible.".to_string(),
                "If the path is correct, try opening the image in another viewer to confirm it is readable."
                    .to_string(),
            ])
            .detail(detail)
    }

    pub fn unsupported_image(path: &Path, detail: impl Into<String>) -> Self {
        Self::new(format!(
            "Input is not a supported image file: {}",
            path.display()
        ))
        .suggestions([
            "Pass a `.png`, `.jpg`, `.jpeg`, `.webp`, or `.tiff` image.".to_string(),
            "If the file is valid but uses an uncommon extension, re-export it to a supported format."
                .to_string(),
        ])
        .detail(detail)
    }

    pub fn output_directory_exists(path: &Path) -> Self {
        Self::new(format!(
            "Output directory already exists: {}",
            path.display()
        ))
        .suggestions([
            "Use `--overwrite` to rebuild the directory from scratch.".to_string(),
            "Use `--resume` if you want to keep existing tiles and rebuild only missing files."
                .to_string(),
        ])
    }

    pub fn resume_output_missing(path: &Path) -> Self {
        Self::new(format!(
            "Resume requires an existing output directory: {}",
            path.display()
        ))
        .suggestions([
            "Run the same command without `--resume` to create the output directory.".to_string(),
            "If you expected an existing build, check that the output path is correct.".to_string(),
        ])
    }

    pub fn resume_state_missing(path: &Path, detail: impl Into<String>) -> Self {
        Self::new("Resume metadata is missing or unreadable.".to_string())
            .suggestions([
                format!("Check that `{}` exists and is readable.", path.display()),
                "If the previous build directory is incomplete, rebuild with `--overwrite`."
                    .to_string(),
            ])
            .detail(detail)
    }

    pub fn resume_plan_mismatch() -> Self {
        Self::new("Stored resume metadata does not match the requested build.".to_string())
            .suggestions([
                "Re-run with the same input and tile settings that created the existing output."
                    .to_string(),
                "If you changed build options, use `--overwrite` to start a fresh output."
                    .to_string(),
            ])
    }

    pub fn vips_feature_disabled() -> Self {
        Self::new("The `vips` backend is not available in this build.".to_string()).suggestions([
            "Rebuild TileCut with `--features vips`.".to_string(),
            "Or switch to `--backend image` / `--backend auto` for the pure-Rust path.".to_string(),
        ])
    }

    pub fn vips_runtime_missing() -> Self {
        Self::new("The `vips` backend requires a working `vips` installation.".to_string())
            .suggestions([
                "Install `vips` / `libvips` and make sure the `vips` binary is on PATH."
                    .to_string(),
                "If you cannot install it, use `--backend image` or raise the memory budget for `auto`."
                    .to_string(),
            ])
    }

    pub fn jpeg_transparency() -> Self {
        Self::new("JPEG output requires opaque tiles.".to_string()).suggestions([
            "Switch to `--format png` if you need alpha support.".to_string(),
            "Or add `--flatten-alpha R,G,B,A` to choose a background color before encoding JPEG."
                .to_string(),
        ])
    }

    pub fn stitch_output_must_be_png(path: &Path) -> Self {
        Self::new(format!(
            "Stitch output must use a `.png` file extension: {}",
            path.display()
        ))
        .suggestions([
            "Change the output path to end with `.png`.".to_string(),
            "For example: `tilecut stitch build/minimap/manifest.json --out verify.png`."
                .to_string(),
        ])
    }

    pub fn stitch_level_missing(level: u32) -> Self {
        Self::new(format!(
            "Requested stitch level does not exist in the manifest: {level}"
        ))
        .suggestions([
            "Check the manifest `levels` list and pick one of the available level numbers."
                .to_string(),
            "If you need more zoom levels, rebuild with a higher `--max-level`.".to_string(),
        ])
    }

    pub fn stitch_tile_missing(path: &Path) -> Self {
        Self::new(format!(
            "A required tile file is missing for stitch: {}",
            path.display()
        ))
        .suggestions([
            "Re-run `tilecut validate` to inspect the output directory.".to_string(),
            "Rebuild the tileset with `tilecut cut --overwrite` if files were deleted.".to_string(),
        ])
    }

    pub fn stitch_tile_decode_failed(path: &Path, detail: impl Into<String>) -> Self {
        Self::new(format!(
            "TileCut could not decode a stitched tile: {}",
            path.display()
        ))
        .suggestions([
            "Validate the tileset to find damaged files.".to_string(),
            "Rebuild the tileset if the tile file is corrupted.".to_string(),
        ])
        .detail(detail)
    }

    pub fn manifest_read_failed(path: &Path, detail: impl Into<String>) -> Self {
        Self::new(format!(
            "TileCut could not read the manifest: {}",
            path.display()
        ))
        .suggestions([
            "Check that the manifest path exists and is readable.".to_string(),
            "If the output directory was partially deleted, rebuild it before validating."
                .to_string(),
        ])
        .detail(detail)
    }

    pub fn manifest_parse_failed(path: &Path, detail: impl Into<String>) -> Self {
        Self::new(format!("Manifest is not valid JSON: {}", path.display()))
            .suggestions([
                "Check whether the manifest file was edited or truncated.".to_string(),
                "If the file came from TileCut, rebuild the output and validate again.".to_string(),
            ])
            .detail(detail)
    }

    pub fn tile_index_parse_failed(path: &Path, detail: impl Into<String>) -> Self {
        Self::new(format!(
            "Tile index could not be parsed: {}",
            path.display()
        ))
        .suggestions([
            "Check whether `tiles.ndjson` was truncated or manually edited.".to_string(),
            "Rebuild the tileset so TileCut can regenerate the index.".to_string(),
        ])
        .detail(detail)
    }

    pub fn validation_failed(report: &ValidationReport) -> Self {
        let mut detail_lines = Vec::new();
        for issue in &report.errors {
            detail_lines.push(format!("- {issue}"));
        }
        Self::new(format!(
            "Manifest validation failed: {}",
            report.manifest_path.display()
        ))
        .suggestions([
            "Inspect the listed files and paths to see what is missing or inconsistent.".to_string(),
            "Re-run `tilecut cut` with `--overwrite` if you want to regenerate the output directory."
                .to_string(),
        ])
        .detail(detail_lines.join("\n"))
    }

    pub fn generic(detail: impl Into<String>) -> Self {
        Self::new("TileCut failed to complete the command.".to_string())
            .suggestions([
                "Review the command arguments and file paths, then try again.".to_string(),
                "If the problem persists, capture the command and output for debugging."
                    .to_string(),
            ])
            .detail(detail)
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.summary)
    }
}

impl StdError for CliError {}

pub fn render_error(err: &Error) -> String {
    if let Some(cli_error) = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<CliError>())
    {
        return render_cli_error(cli_error);
    }

    let details = err
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    render_cli_error(&CliError::generic(details))
}

fn render_cli_error(err: &CliError) -> String {
    let mut output = format!("Error: {}", err.summary);

    if !err.suggestions.is_empty() {
        output.push_str("\n\nTry:");
        for suggestion in &err.suggestions {
            output.push_str("\n  - ");
            output.push_str(suggestion);
        }
    }

    if let Some(detail) = &err.detail {
        if detail.contains('\n') {
            output.push_str("\n\nDetails:");
            for line in detail.lines() {
                output.push_str("\n  ");
                output.push_str(line);
            }
        } else if !detail.is_empty() {
            output.push_str("\n\nDetails: ");
            output.push_str(detail);
        }
    }

    output
}
