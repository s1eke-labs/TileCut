use std::path::PathBuf;
use std::str::FromStr;

use clap::{Args, Parser, Subcommand, ValueEnum, value_parser};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(
    name = "tilecut",
    version,
    about = "Cut large minimap images into fixed-size tiles and write a reusable manifest.",
    long_about = "TileCut is an offline tile builder for large minimap images.\n\nUse `inspect` to preview the grid and backend recommendation, `cut` to generate tiles plus manifests, and `validate` to verify an existing output before shipping it.",
    after_long_help = "Examples:\n  tilecut inspect map.png --tile-size 256\n  tilecut cut map.png --out out --tile-size 256\n  tilecut cut map.png --out out --skip-empty --manifest full --tile-index ndjson\n  tilecut validate out/manifest.json",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(
        about = "Read image dimensions and preview the cut plan.",
        long_about = "Inspect an input image without writing tiles.\n\nTileCut reports the source dimensions, tile grid size, estimated in-memory RGBA size, and which backend `auto` would choose for the requested tile settings.",
        after_long_help = "Examples:\n  tilecut inspect world_map.png --tile-size 256\n  tilecut inspect world_map.png --tile-width 512 --tile-height 256 --edge crop --json\n\nNotes:\n  - `pad` keeps a full tile at the image edges.\n  - `skip` ignores partial edge tiles entirely."
    )]
    Inspect(InspectArgs),
    #[command(
        about = "Cut tiles, write manifests, and optionally generate previews.",
        long_about = "Cut an input image into tiles and write a build directory containing `manifest.json`, tile files, optional `tiles.ndjson`, optional `preview/overview.png`, and internal `.tilecut` resume metadata.",
        after_long_help = "Examples:\n  tilecut cut map.png --out out --tile-size 256\n  tilecut cut map.png --out out --overview 1024\n  tilecut cut map.png --out out --world-origin=0,0 --units-per-pixel 1\n  tilecut cut map.png --out out --manifest full --tile-index ndjson --skip-empty\n  tilecut cut map.png --out out --format jpeg --flatten-alpha 0,0,0,255\n\nNotes:\n  - `compact` manifest mode stores tile rules and statistics, not every tile record.\n  - `full` manifest mode expands every tile and is easier to debug on smaller maps.\n  - `backend auto` chooses `image` for smaller inputs and prefers `vips` once the estimated RGBA size exceeds the memory budget."
    )]
    Cut(CutArgs),
    #[command(
        about = "Check a manifest and its tile output for consistency.",
        long_about = "Validate an existing TileCut output by checking the manifest schema, derived tile count, optional `tiles.ndjson`, on-disk files, and tile image dimensions.",
        after_long_help = "Examples:\n  tilecut validate build/minimap/manifest.json\n  tilecut validate build/minimap/manifest.json --json\n\nNotes:\n  - Validation does not rebuild missing tiles.\n  - Use this before publishing a generated tileset or after a partial resume."
    )]
    Validate(ValidateArgs),
}

#[derive(Args, Debug, Clone)]
pub struct TileSizingArgs {
    #[arg(
        long,
        value_name = "PX",
        value_parser = value_parser!(u32).range(1..),
        help = "Fallback tile size for both width and height.",
        long_help = "Fallback tile size for both width and height.\n\nIf you also pass `--tile-width` or `--tile-height`, those values override this shared default."
    )]
    pub tile_size: Option<u32>,
    #[arg(
        long,
        value_name = "PX",
        value_parser = value_parser!(u32).range(1..),
        help = "Explicit tile width in pixels."
    )]
    pub tile_width: Option<u32>,
    #[arg(
        long,
        value_name = "PX",
        value_parser = value_parser!(u32).range(1..),
        help = "Explicit tile height in pixels."
    )]
    pub tile_height: Option<u32>,
}

impl TileSizingArgs {
    pub fn tile_width(&self) -> u32 {
        self.tile_width.or(self.tile_size).unwrap_or(256)
    }

    pub fn tile_height(&self) -> u32 {
        self.tile_height.or(self.tile_size).unwrap_or(256)
    }
}

#[derive(Args, Debug, Clone)]
pub struct InspectArgs {
    #[arg(value_name = "INPUT", help = "Input image to inspect.")]
    pub input: PathBuf,
    #[command(flatten, next_help_heading = "Tile sizing")]
    pub tile: TileSizingArgs,
    #[arg(
        long,
        value_enum,
        default_value_t = EdgeMode::Pad,
        help_heading = "Planning",
        help = "How to handle partial edge tiles.",
        long_help = "How to handle partial edge tiles.\n\n`pad` keeps the output tile size fixed and fills the unused area with the pad color.\n`crop` writes a smaller image for partial edge tiles.\n`skip` ignores partial edge tiles entirely."
    )]
    pub edge: EdgeMode,
    #[arg(
        long,
        value_name = "MIB",
        value_parser = value_parser!(u64).range(1..),
        default_value_t = 2048,
        help_heading = "Planning",
        help = "Memory budget used by `backend auto`."
    )]
    pub max_in_memory_mib: u64,
    #[arg(
        long,
        help_heading = "Output",
        help = "Print the inspection report as JSON."
    )]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct CutArgs {
    #[arg(value_name = "INPUT", help = "Source image file to cut into tiles.")]
    pub input: PathBuf,
    #[arg(
        long,
        value_name = "DIR",
        help_heading = "Output & layout",
        help = "Output directory for `manifest.json`, `tiles/`, and optional previews."
    )]
    pub out: PathBuf,
    #[command(flatten, next_help_heading = "Tile sizing")]
    pub tile: TileSizingArgs,
    #[arg(
        long,
        value_enum,
        default_value_t = OutputFormat::Png,
        help_heading = "Output & layout",
        help = "Image format used for generated tiles.",
        long_help = "Image format used for generated tiles.\n\nUse `png` for lossless output and alpha support, `jpeg` for opaque tiles with smaller files, or `webp` when your asset pipeline supports it."
    )]
    pub format: OutputFormat,
    #[arg(
        long,
        value_name = "1-100",
        value_parser = value_parser!(u8).range(1..=100),
        default_value_t = 90,
        help_heading = "Output & layout",
        help = "Compression quality for `jpeg` and `webp` output."
    )]
    pub quality: u8,
    #[arg(
        long,
        value_enum,
        default_value_t = EdgeMode::Pad,
        help_heading = "Output & layout",
        help = "How to handle partial edge tiles.",
        long_help = "How to handle partial edge tiles.\n\n`pad` writes full-size edge tiles.\n`crop` writes smaller edge images.\n`skip` omits partial edge tiles completely."
    )]
    pub edge: EdgeMode,
    #[arg(
        long,
        value_name = "R,G,B,A",
        default_value = "0,0,0,0",
        help_heading = "Output & layout",
        help = "RGBA color used when `--edge pad` needs to fill unused pixels."
    )]
    pub pad_color: RgbaColor,
    #[arg(
        long,
        value_enum,
        default_value_t = LayoutMode::Flat,
        help_heading = "Output & layout",
        help = "Directory layout for generated tiles."
    )]
    pub layout: LayoutMode,
    #[arg(
        long,
        value_name = "PX",
        value_parser = value_parser!(u32).range(1..),
        help_heading = "Output & layout",
        help = "Write `preview/overview.png` with this maximum edge length."
    )]
    pub overview: Option<u32>,
    #[arg(
        long,
        value_name = "R,G,B,A",
        help_heading = "Output & layout",
        help = "Background color for alpha flattening when `--format jpeg` is used.",
        long_help = "Background color for alpha flattening when `--format jpeg` is used.\n\nOnly used for JPEG output. If omitted, transparent pixels cause the build to fail instead of silently flattening."
    )]
    pub flatten_alpha: Option<RgbaColor>,
    #[arg(
        long,
        value_enum,
        default_value_t = ManifestMode::Compact,
        help_heading = "Manifest & indexing",
        help = "Manifest verbosity.",
        long_help = "Manifest verbosity.\n\n`compact` stores tile rules and statistics.\n`full` expands every tile record with coordinates, source rectangles, content rectangles, and skip state."
    )]
    pub manifest: ManifestMode,
    #[arg(
        long,
        value_enum,
        default_value_t = TileIndexMode::None,
        help_heading = "Manifest & indexing",
        help = "Optional sidecar tile index format.",
        long_help = "Optional sidecar tile index format.\n\nUse `ndjson` for streaming or very large outputs. When `compact` manifest mode is combined with `--skip-empty`, TileCut automatically enables `ndjson` to record which tiles were actually written."
    )]
    pub tile_index: TileIndexMode,
    #[arg(
        long,
        help_heading = "Manifest & indexing",
        help = "Skip tiles whose every pixel alpha is at or below the threshold.",
        long_help = "Skip tiles whose every pixel alpha is at or below the threshold.\n\nUseful for sparse maps with large transparent regions. In `compact` manifest mode, TileCut automatically writes `tiles.ndjson` so skipped coordinates remain discoverable."
    )]
    pub skip_empty: bool,
    #[arg(
        long,
        value_name = "0-255",
        requires = "skip_empty",
        help_heading = "Manifest & indexing",
        help = "Alpha threshold used by `--skip-empty`.",
        long_help = "Alpha threshold used by `--skip-empty`.\n\nOnly used when `--skip-empty` is enabled. The default threshold is `0`, which means fully transparent tiles are skipped."
    )]
    pub empty_alpha_threshold: Option<u8>,
    #[arg(
        long,
        allow_hyphen_values = true,
        value_name = "X,Y",
        requires = "units_per_pixel",
        help_heading = "World mapping",
        help = "World-space coordinate for the top-left pixel of the source image."
    )]
    pub world_origin: Option<Point2>,
    #[arg(
        long,
        value_name = "VALUE",
        value_parser = parse_positive_f64,
        requires = "world_origin",
        help_heading = "World mapping",
        help = "World-space units represented by one source pixel."
    )]
    pub units_per_pixel: Option<f64>,
    #[arg(
        long,
        value_enum,
        default_value_t = YAxis::Down,
        help_heading = "World mapping",
        help = "Whether world-space Y grows down or up."
    )]
    pub y_axis: YAxis,
    #[arg(
        long,
        conflicts_with = "resume",
        help_heading = "Resuming & overwrite",
        help = "Replace an existing output directory with a fresh build."
    )]
    pub overwrite: bool,
    #[arg(
        long,
        conflicts_with = "overwrite",
        help_heading = "Resuming & overwrite",
        help = "Reuse an existing output directory and only rebuild missing files."
    )]
    pub resume: bool,
    #[arg(
        long,
        value_enum,
        default_value_t = BackendKind::Auto,
        help_heading = "Backend & performance",
        help = "Tile backend to use.",
        long_help = "Tile backend to use.\n\n`auto` chooses based on the estimated in-memory RGBA size and the memory budget.\n`image` decodes the source into memory and is the default pure-Rust path.\n`vips` is intended for larger inputs and requires both the Cargo feature and a working system `vips` installation."
    )]
    pub backend: BackendKind,
    #[arg(
        long,
        value_name = "N",
        value_parser = parse_positive_usize,
        help_heading = "Backend & performance",
        help = "Number of worker threads used while writing tiles."
    )]
    pub threads: Option<usize>,
    #[arg(
        long,
        value_name = "MIB",
        value_parser = value_parser!(u64).range(1..),
        default_value_t = 2048,
        help_heading = "Backend & performance",
        help = "Memory budget used when `--backend auto` chooses between `image` and `vips`."
    )]
    pub max_in_memory_mib: u64,
    #[arg(
        long,
        help_heading = "Advanced & debugging",
        help = "Print the resolved plan and selected backend without writing files."
    )]
    pub dry_run: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ValidateArgs {
    #[arg(
        value_name = "MANIFEST",
        help = "Path to the manifest file to validate."
    )]
    pub manifest: PathBuf,
    #[arg(
        long,
        help_heading = "Output",
        help = "Print the validation report as JSON."
    )]
    pub json: bool,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum EdgeMode {
    #[default]
    Pad,
    Crop,
    Skip,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    #[default]
    Flat,
    Sharded,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ManifestMode {
    #[default]
    Compact,
    Full,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TileIndexMode {
    #[default]
    None,
    Ndjson,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    #[default]
    Auto,
    Image,
    Vips,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum YAxis {
    #[default]
    Down,
    Up,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RgbaColor(pub [u8; 4]);

impl FromStr for RgbaColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s
            .split(',')
            .map(|part| part.trim().parse::<u8>().map_err(|err| err.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        if parts.len() != 4 {
            return Err("expected four comma-separated u8 values".to_string());
        }
        Ok(Self([parts[0], parts[1], parts[2], parts[3]]))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point2(pub [f64; 2]);

impl FromStr for Point2 {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s
            .split(',')
            .map(|part| part.trim().parse::<f64>().map_err(|err| err.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        if parts.len() != 2 {
            return Err("expected two comma-separated numeric values".to_string());
        }
        Ok(Self([parts[0], parts[1]]))
    }
}

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value.parse::<usize>().map_err(|err| err.to_string())?;
    if parsed == 0 {
        return Err("expected a positive integer greater than 0".to_string());
    }
    Ok(parsed)
}

fn parse_positive_f64(value: &str) -> Result<f64, String> {
    let parsed = value.parse::<f64>().map_err(|err| err.to_string())?;
    if parsed <= 0.0 {
        return Err("expected a positive number greater than 0".to_string());
    }
    Ok(parsed)
}
