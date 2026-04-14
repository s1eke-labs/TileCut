use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::backend::{BackendSupport, estimated_rgba_bytes, inspect_source};
use crate::cli::{
    BackendKind, CutArgs, EdgeMode, InspectArgs, LayoutMode, ManifestMode, OutputFormat, Point2,
    TileIndexMode,
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
pub struct LevelPlan {
    pub level: u32,
    pub scale: f64,
    pub width: u32,
    pub height: u32,
    pub grid: GridInfo,
    pub naming_template: String,
    pub tiles: Vec<TilePlan>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CutPlan {
    pub source: SourceInfo,
    pub tile: TileSpec,
    pub grid: GridInfo,
    pub levels: Vec<LevelPlan>,
    pub layout: LayoutMode,
    pub manifest_mode: ManifestMode,
    pub tile_index_mode: TileIndexMode,
    pub requested_backend: BackendKind,
    pub max_in_memory_mib: u64,
    pub max_level: u32,
    pub overview: Option<u32>,
    pub skip_empty: bool,
    pub empty_alpha_threshold: u8,
    pub world: Option<WorldMapping>,
    pub naming_template: String,
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
    pub max_level: u32,
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
pub struct InspectLevelReport {
    pub level: u32,
    pub scale: f64,
    pub width: u32,
    pub height: u32,
    pub cols: u32,
    pub rows: u32,
    pub tile_count: u64,
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
    pub total_levels: u32,
    pub total_tile_count: u64,
    pub estimated_rgba_bytes: u64,
    pub estimated_rgba_mib: f64,
    pub levels: Vec<InspectLevelReport>,
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
        let naming_template = path_template(args.layout, args.format, args.max_level > 0);
        let world = build_world(args)?;
        let levels = build_level_plans(
            source.width,
            source.height,
            &tile,
            args.max_level,
            args.layout,
            &naming_template,
        )?;
        let grid = levels
            .first()
            .map(|level| level.grid.clone())
            .expect("CutPlan always contains at least level 0");

        Ok(Self {
            source,
            tile,
            grid,
            levels,
            layout: args.layout,
            manifest_mode: args.manifest,
            tile_index_mode,
            requested_backend: args.backend,
            max_in_memory_mib: args.max_in_memory_mib,
            max_level: args.max_level,
            overview: args.overview,
            skip_empty: args.skip_empty,
            empty_alpha_threshold: args.empty_alpha_threshold.unwrap_or(0),
            world,
            naming_template,
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
            max_level: self.max_level,
            skip_empty: self.skip_empty,
            empty_alpha_threshold: self.empty_alpha_threshold,
            overview: self.overview,
            world: self.world.clone(),
        }
    }

    pub fn total_tile_slots(&self) -> usize {
        self.levels.iter().map(|level| level.tiles.len()).sum()
    }

    pub fn level(&self, level: u32) -> Option<&LevelPlan> {
        self.levels
            .iter()
            .find(|candidate| candidate.level == level)
    }

    pub fn inspect_report(
        args: &InspectArgs,
        source: SourceInfo,
        support: &BackendSupport,
    ) -> Result<InspectReport> {
        let tile_width = args.tile.tile_width();
        let tile_height = args.tile.tile_height();
        let levels = build_inspect_levels(
            source.width,
            source.height,
            tile_width,
            tile_height,
            args.edge,
            args.max_level,
        )?;
        let level0 = levels
            .first()
            .expect("Inspect report always contains at least level 0");
        let estimated_bytes = estimated_rgba_bytes(source.width, source.height);
        let recommended = if estimated_bytes <= args.max_in_memory_mib.saturating_mul(1024 * 1024) {
            BackendKind::Image
        } else {
            BackendKind::Vips
        };

        Ok(InspectReport {
            source,
            tile_width,
            tile_height,
            edge_mode: args.edge,
            cols: level0.cols,
            rows: level0.rows,
            tile_count: level0.tile_count,
            total_levels: levels.len() as u32,
            total_tile_count: levels.iter().map(|level| level.tile_count).sum(),
            estimated_rgba_bytes: estimated_bytes,
            estimated_rgba_mib: estimated_bytes as f64 / (1024.0 * 1024.0),
            levels,
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

fn build_level_plans(
    source_width: u32,
    source_height: u32,
    tile: &TileSpec,
    max_level: u32,
    layout: LayoutMode,
    naming_template: &str,
) -> Result<Vec<LevelPlan>> {
    let multi_level = max_level > 0;
    let mut levels = Vec::with_capacity(max_level.saturating_add(1) as usize);
    for level in 0..=max_level {
        let scale = level_scale(level);
        let (width, height) = scaled_dimensions_for_level(source_width, source_height, level);
        let grid = compute_grid(width, height, tile.width, tile.height, tile.edge_mode)?;
        let mut tiles = Vec::with_capacity((grid.cols * grid.rows) as usize);
        for y in 0..grid.rows {
            for x in 0..grid.cols {
                let src_x = x * tile.width;
                let src_y = y * tile.height;
                let src_w = width.saturating_sub(src_x).min(tile.width);
                let src_h = height.saturating_sub(src_y).min(tile.height);
                tiles.push(TilePlan {
                    coord: TileCoord { level, x, y },
                    src_rect: RectU32 {
                        x: src_x,
                        y: src_y,
                        w: src_w,
                        h: src_h,
                    },
                    content_rect: RectU32 {
                        x: 0,
                        y: 0,
                        w: src_w,
                        h: src_h,
                    },
                    out_rel_path: render_rel_path(
                        layout,
                        tile.format,
                        grid.zero_pad_width,
                        level,
                        x,
                        y,
                        multi_level,
                    ),
                });
            }
        }
        levels.push(LevelPlan {
            level,
            scale,
            width,
            height,
            grid,
            naming_template: naming_template.to_string(),
            tiles,
        });
    }
    Ok(levels)
}

fn build_inspect_levels(
    source_width: u32,
    source_height: u32,
    tile_width: u32,
    tile_height: u32,
    edge_mode: EdgeMode,
    max_level: u32,
) -> Result<Vec<InspectLevelReport>> {
    let mut levels = Vec::with_capacity(max_level.saturating_add(1) as usize);
    for level in 0..=max_level {
        let (width, height) = scaled_dimensions_for_level(source_width, source_height, level);
        let grid = compute_grid(width, height, tile_width, tile_height, edge_mode)?;
        levels.push(InspectLevelReport {
            level,
            scale: level_scale(level),
            width,
            height,
            cols: grid.cols,
            rows: grid.rows,
            tile_count: u64::from(grid.cols) * u64::from(grid.rows),
        });
    }
    Ok(levels)
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

pub fn level_scale(level: u32) -> f64 {
    1.0 / (2_u64.pow(level) as f64)
}

pub fn scaled_dimensions_for_level(width: u32, height: u32, level: u32) -> (u32, u32) {
    if level == 0 {
        return (width, height);
    }
    let scale = level_scale(level);
    let scaled_width = ((width as f64) * scale).round().max(1.0) as u32;
    let scaled_height = ((height as f64) * scale).round().max(1.0) as u32;
    (scaled_width, scaled_height)
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
    use super::{
        compute_grid, effective_tile_index_mode, level_scale, scaled_dimensions_for_level,
    };
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
    fn scales_dimensions_from_original_size() {
        assert_eq!(scaled_dimensions_for_level(513, 257, 1), (257, 129));
        assert_eq!(scaled_dimensions_for_level(513, 257, 2), (128, 64));
    }

    #[test]
    fn computes_standard_binary_level_scale() {
        assert_eq!(level_scale(0), 1.0);
        assert_eq!(level_scale(1), 0.5);
        assert_eq!(level_scale(2), 0.25);
    }
}
