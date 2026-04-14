use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::backend::{BackendSupport, estimated_rgba_bytes, inspect_source};
use crate::cli::{
    BackendKind, CutArgs, EdgeMode, LayoutMode, ManifestMode, OutputFormat, Point2, TileIndexMode,
};
use crate::coords::WorldMapping;
use crate::naming::{path_template, render_rel_path, zero_pad_width};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub file_size: u64,
    pub modified_unix_secs: u64,
    pub sha256: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectU32 {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileSpec {
    pub width: u32,
    pub height: u32,
    pub edge_mode: EdgeMode,
    pub pad_color: [u8; 4],
    pub format: OutputFormat,
    pub quality: u8,
    pub flatten_alpha: Option<[u8; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridInfo {
    pub cols: u32,
    pub rows: u32,
    pub zero_pad_width: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileCoord {
    pub level: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TilePlan {
    pub coord: TileCoord,
    pub src_rect: RectU32,
    pub content_rect: RectU32,
    pub out_rel_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CutPlan {
    pub source: SourceInfo,
    pub tile: TileSpec,
    pub grid: GridInfo,
    pub layout: LayoutMode,
    pub manifest_mode: ManifestMode,
    pub tile_index_mode: TileIndexMode,
    pub requested_backend: BackendKind,
    pub max_in_memory_mib: u64,
    pub overview: Option<u32>,
    pub skip_empty: bool,
    pub empty_alpha_threshold: u8,
    pub world: Option<WorldMapping>,
    pub naming_template: String,
    pub tiles: Vec<TilePlan>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildFingerprint {
    pub source_sha256: String,
    pub source_width: u32,
    pub source_height: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub edge_mode: EdgeMode,
    pub pad_color: [u8; 4],
    pub format: OutputFormat,
    pub quality: u8,
    pub flatten_alpha: Option<[u8; 4]>,
    pub layout: LayoutMode,
    pub manifest_mode: ManifestMode,
    pub tile_index_mode: TileIndexMode,
    pub skip_empty: bool,
    pub empty_alpha_threshold: u8,
    pub overview: Option<u32>,
    pub world: Option<WorldMapping>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildState {
    pub complete: bool,
    pub total_tiles: usize,
    pub written_tiles: usize,
    pub skipped_tiles: usize,
    pub updated_unix_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InspectReport {
    pub source: SourceInfo,
    pub tile_width: u32,
    pub tile_height: u32,
    pub edge_mode: EdgeMode,
    pub cols: u32,
    pub rows: u32,
    pub tile_count: u64,
    pub estimated_rgba_bytes: u64,
    pub estimated_rgba_mib: f64,
    pub backend: BackendRecommendation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BackendRecommendation {
    pub recommended: BackendKind,
    pub vips_feature_enabled: bool,
    pub vips_runtime_available: bool,
}

impl SourceInfo {
    pub fn from_path(path: &Path) -> Result<Self> {
        inspect_source(path)
    }
}

impl CutPlan {
    pub fn build(args: &CutArgs, source: SourceInfo) -> Result<Self> {
        let tile_width = args.tile.tile_width();
        let tile_height = args.tile.tile_height();
        ensure!(tile_width > 0, "tile width must be greater than 0");
        ensure!(tile_height > 0, "tile height must be greater than 0");
        let grid = compute_grid(
            source.width,
            source.height,
            tile_width,
            tile_height,
            args.edge,
        )?;
        let pad_width = zero_pad_width(grid.cols, grid.rows);
        let tile_index_mode =
            effective_tile_index_mode(args.manifest, args.tile_index, args.skip_empty);
        let tile = TileSpec {
            width: tile_width,
            height: tile_height,
            edge_mode: args.edge,
            pad_color: args.pad_color.0,
            format: args.format,
            quality: args.quality,
            flatten_alpha: args.flatten_alpha.map(|color| color.0),
        };
        let naming_template = path_template(args.layout, args.format);
        let world = build_world(args)?;
        let mut tiles = Vec::with_capacity((grid.cols * grid.rows) as usize);
        for y in 0..grid.rows {
            for x in 0..grid.cols {
                let src_x = x * tile_width;
                let src_y = y * tile_height;
                let src_w = source.width.saturating_sub(src_x).min(tile_width);
                let src_h = source.height.saturating_sub(src_y).min(tile_height);
                let content_rect = RectU32 {
                    x: 0,
                    y: 0,
                    w: src_w,
                    h: src_h,
                };
                let src_rect = RectU32 {
                    x: src_x,
                    y: src_y,
                    w: src_w,
                    h: src_h,
                };
                tiles.push(TilePlan {
                    coord: TileCoord { level: 0, x, y },
                    src_rect,
                    content_rect,
                    out_rel_path: render_rel_path(args.layout, args.format, pad_width, x, y),
                });
            }
        }
        Ok(Self {
            source,
            tile,
            grid,
            layout: args.layout,
            manifest_mode: args.manifest,
            tile_index_mode,
            requested_backend: args.backend,
            max_in_memory_mib: args.max_in_memory_mib,
            overview: args.overview,
            skip_empty: args.skip_empty,
            empty_alpha_threshold: args.empty_alpha_threshold.unwrap_or(0),
            world,
            naming_template,
            tiles,
        })
    }

    pub fn fingerprint(&self) -> BuildFingerprint {
        BuildFingerprint {
            source_sha256: self.source.sha256.clone(),
            source_width: self.source.width,
            source_height: self.source.height,
            tile_width: self.tile.width,
            tile_height: self.tile.height,
            edge_mode: self.tile.edge_mode,
            pad_color: self.tile.pad_color,
            format: self.tile.format,
            quality: self.tile.quality,
            flatten_alpha: self.tile.flatten_alpha,
            layout: self.layout,
            manifest_mode: self.manifest_mode,
            tile_index_mode: self.tile_index_mode,
            skip_empty: self.skip_empty,
            empty_alpha_threshold: self.empty_alpha_threshold,
            overview: self.overview,
            world: self.world.clone(),
        }
    }

    pub fn inspect_report(
        source: SourceInfo,
        tile_width: u32,
        tile_height: u32,
        edge_mode: EdgeMode,
        max_in_memory_mib: u64,
        support: &BackendSupport,
    ) -> Result<InspectReport> {
        let grid = compute_grid(
            source.width,
            source.height,
            tile_width,
            tile_height,
            edge_mode,
        )?;
        let estimated_bytes = estimated_rgba_bytes(source.width, source.height);
        let recommended = if estimated_bytes <= max_in_memory_mib.saturating_mul(1024 * 1024) {
            BackendKind::Image
        } else {
            BackendKind::Vips
        };
        Ok(InspectReport {
            source,
            tile_width,
            tile_height,
            edge_mode,
            cols: grid.cols,
            rows: grid.rows,
            tile_count: u64::from(grid.cols) * u64::from(grid.rows),
            estimated_rgba_bytes: estimated_bytes,
            estimated_rgba_mib: estimated_bytes as f64 / (1024.0 * 1024.0),
            backend: BackendRecommendation {
                recommended,
                vips_feature_enabled: support.vips_feature_enabled,
                vips_runtime_available: support.vips_runtime_available,
            },
        })
    }
}

impl BuildState {
    pub fn new(total_tiles: usize) -> Self {
        Self {
            complete: false,
            total_tiles,
            written_tiles: 0,
            skipped_tiles: 0,
            updated_unix_secs: now_unix_secs(),
        }
    }
}

fn build_world(args: &CutArgs) -> Result<Option<WorldMapping>> {
    match (args.world_origin, args.units_per_pixel) {
        (Some(Point2(origin)), Some(units_per_pixel)) => {
            ensure!(
                units_per_pixel > 0.0,
                "units-per-pixel must be greater than 0"
            );
            Ok(Some(WorldMapping {
                origin,
                units_per_pixel,
                y_axis: args.y_axis,
            }))
        }
        (None, None) => Ok(None),
        _ => bail!("--world-origin and --units-per-pixel must be provided together"),
    }
}

pub fn compute_grid(
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
    edge_mode: EdgeMode,
) -> Result<GridInfo> {
    ensure!(
        tile_width > 0 && tile_height > 0,
        "tile dimensions must be greater than 0"
    );
    let cols = match edge_mode {
        EdgeMode::Skip => width / tile_width,
        EdgeMode::Pad | EdgeMode::Crop => width.div_ceil(tile_width),
    };
    let rows = match edge_mode {
        EdgeMode::Skip => height / tile_height,
        EdgeMode::Pad | EdgeMode::Crop => height.div_ceil(tile_height),
    };
    ensure!(
        cols > 0 && rows > 0,
        "input produces zero tiles with the requested edge mode"
    );
    Ok(GridInfo {
        cols,
        rows,
        zero_pad_width: zero_pad_width(cols, rows),
    })
}

pub fn effective_tile_index_mode(
    manifest_mode: ManifestMode,
    requested: TileIndexMode,
    skip_empty: bool,
) -> TileIndexMode {
    if manifest_mode == ManifestMode::Compact && skip_empty {
        TileIndexMode::Ndjson
    } else {
        requested
    }
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn compute_sha256(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).context("failed to hash input file")?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::{compute_grid, effective_tile_index_mode};
    use crate::cli::{EdgeMode, ManifestMode, TileIndexMode};

    #[test]
    fn computes_grid_for_partial_edges() {
        let grid = compute_grid(513, 512, 256, 256, EdgeMode::Pad).expect("grid");
        assert_eq!(grid.cols, 3);
        assert_eq!(grid.rows, 2);
    }

    #[test]
    fn skips_partial_edges_when_requested() {
        let grid = compute_grid(513, 513, 256, 256, EdgeMode::Skip).expect("grid");
        assert_eq!(grid.cols, 2);
        assert_eq!(grid.rows, 2);
    }

    #[test]
    fn enables_ndjson_for_compact_skip_empty() {
        assert_eq!(
            effective_tile_index_mode(ManifestMode::Compact, TileIndexMode::None, true),
            TileIndexMode::Ndjson
        );
    }

    #[test]
    fn builds_edge_tile_content_rect() {
        let grid = compute_grid(513, 513, 256, 256, EdgeMode::Pad).expect("grid");
        assert_eq!(grid.cols, 3);
        assert_eq!(grid.rows, 3);
    }
}
