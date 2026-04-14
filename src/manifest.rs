use serde::{Deserialize, Serialize};

use crate::cli::{EdgeMode, LayoutMode, OutputFormat, TileIndexMode, YAxis};
use crate::plan::{CutPlan, RectU32, SourceInfo, TilePlan};

pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: String,
    pub generator: GeneratorInfo,
    pub source: ManifestSource,
    pub tile: ManifestTile,
    pub grid: ManifestGrid,
    pub naming: ManifestNaming,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world: Option<ManifestWorld>,
    pub levels: Vec<ManifestLevel>,
    pub stats: ManifestStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<ManifestIndex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiles: Option<Vec<ManifestTileRecord>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratorInfo {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestSource {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub file_size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestTile {
    pub width: u32,
    pub height: u32,
    pub format: OutputFormat,
    pub quality: u8,
    pub edge_mode: EdgeMode,
    pub pad_color: [u8; 4],
    pub skip_empty: bool,
    pub empty_alpha_threshold: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flatten_alpha: Option<[u8; 4]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestGrid {
    pub origin: String,
    pub zero_based: bool,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestNaming {
    pub coord_space: String,
    pub layout: LayoutMode,
    pub zero_pad_width: usize,
    pub path_template: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestWorld {
    pub enabled: bool,
    pub origin: [f64; 2],
    pub units_per_pixel: f64,
    pub y_axis: YAxis,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestLevel {
    pub level: u32,
    pub scale: f64,
    pub width: u32,
    pub height: u32,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestStats {
    pub tile_count: usize,
    pub skipped_count: usize,
    pub total_slots: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestIndex {
    pub mode: TileIndexMode,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestTileRecord {
    pub level: u32,
    pub x: u32,
    pub y: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub src_rect: RectU32,
    pub content_rect: RectU32,
    pub skipped: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TileInventoryEntry {
    pub level: u32,
    pub x: u32,
    pub y: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub src_rect: RectU32,
    pub content_rect: RectU32,
    pub skipped: bool,
}

impl Manifest {
    pub fn from_plan(plan: &CutPlan, inventory: Vec<TileInventoryEntry>) -> Self {
        let tile_count = inventory.iter().filter(|entry| !entry.skipped).count();
        let skipped_count = inventory.len().saturating_sub(tile_count);
        let tiles = if plan.manifest_mode == crate::cli::ManifestMode::Full {
            Some(
                inventory
                    .iter()
                    .map(|entry| ManifestTileRecord {
                        level: entry.level,
                        x: entry.x,
                        y: entry.y,
                        path: entry.path.clone(),
                        src_rect: entry.src_rect,
                        content_rect: entry.content_rect,
                        skipped: entry.skipped,
                    })
                    .collect(),
            )
        } else {
            None
        };
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            generator: GeneratorInfo {
                name: "tilecut".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            source: ManifestSource::from(&plan.source),
            tile: ManifestTile {
                width: plan.tile.width,
                height: plan.tile.height,
                format: plan.tile.format,
                quality: plan.tile.quality,
                edge_mode: plan.tile.edge_mode,
                pad_color: plan.tile.pad_color,
                skip_empty: plan.skip_empty,
                empty_alpha_threshold: plan.empty_alpha_threshold,
                flatten_alpha: plan.tile.flatten_alpha,
            },
            grid: ManifestGrid {
                origin: "top-left".to_string(),
                zero_based: true,
                cols: plan.grid.cols,
                rows: plan.grid.rows,
            },
            naming: ManifestNaming {
                coord_space: "grid".to_string(),
                layout: plan.layout,
                zero_pad_width: plan.grid.zero_pad_width,
                path_template: plan.naming_template.clone(),
            },
            world: plan.world.as_ref().map(|world| ManifestWorld {
                enabled: true,
                origin: world.origin,
                units_per_pixel: world.units_per_pixel,
                y_axis: world.y_axis,
            }),
            levels: vec![ManifestLevel {
                level: 0,
                scale: 1.0,
                width: plan.source.width,
                height: plan.source.height,
                cols: plan.grid.cols,
                rows: plan.grid.rows,
            }],
            stats: ManifestStats {
                tile_count,
                skipped_count,
                total_slots: inventory.len(),
            },
            index: match plan.tile_index_mode {
                TileIndexMode::None => None,
                mode => Some(ManifestIndex {
                    mode,
                    path: "tiles.ndjson".to_string(),
                }),
            },
            tiles,
        }
    }
}

impl From<&SourceInfo> for ManifestSource {
    fn from(value: &SourceInfo) -> Self {
        Self {
            path: value.path.clone(),
            width: value.width,
            height: value.height,
            format: value.format.clone(),
            file_size: value.file_size,
            sha256: value.sha256.clone(),
        }
    }
}

impl From<(&TilePlan, bool)> for TileInventoryEntry {
    fn from((tile, skipped): (&TilePlan, bool)) -> Self {
        Self {
            level: tile.coord.level,
            x: tile.coord.x,
            y: tile.coord.y,
            path: (!skipped).then(|| tile.out_rel_path.to_string_lossy().to_string()),
            src_rect: tile.src_rect,
            content_rect: tile.content_rect,
            skipped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Manifest, TileInventoryEntry};
    use crate::cli::{
        BackendKind, EdgeMode, LayoutMode, ManifestMode, OutputFormat, TileIndexMode,
    };
    use crate::plan::{CutPlan, GridInfo, RectU32, SourceInfo, TileCoord, TilePlan, TileSpec};

    #[test]
    fn serializes_full_manifest_with_tiles() {
        let plan = CutPlan {
            source: SourceInfo {
                path: "map.png".to_string(),
                width: 256,
                height: 256,
                format: "png".to_string(),
                file_size: 12,
                modified_unix_secs: 0,
                sha256: "abc".to_string(),
            },
            tile: TileSpec {
                width: 128,
                height: 128,
                edge_mode: EdgeMode::Pad,
                pad_color: [0, 0, 0, 0],
                format: OutputFormat::Png,
                quality: 90,
                flatten_alpha: None,
            },
            grid: GridInfo {
                cols: 2,
                rows: 2,
                zero_pad_width: 4,
            },
            layout: LayoutMode::Flat,
            manifest_mode: ManifestMode::Full,
            tile_index_mode: TileIndexMode::None,
            requested_backend: BackendKind::Image,
            max_in_memory_mib: 2048,
            overview: None,
            skip_empty: false,
            empty_alpha_threshold: 0,
            world: None,
            naming_template: "tiles/x{x}_y{y}.png".to_string(),
            tiles: vec![TilePlan {
                coord: TileCoord {
                    level: 0,
                    x: 0,
                    y: 0,
                },
                src_rect: RectU32 {
                    x: 0,
                    y: 0,
                    w: 128,
                    h: 128,
                },
                content_rect: RectU32 {
                    x: 0,
                    y: 0,
                    w: 128,
                    h: 128,
                },
                out_rel_path: "tiles/x0000_y0000.png".into(),
            }],
        };

        let manifest = Manifest::from_plan(
            &plan,
            vec![TileInventoryEntry {
                level: 0,
                x: 0,
                y: 0,
                path: Some("tiles/x0000_y0000.png".to_string()),
                src_rect: RectU32 {
                    x: 0,
                    y: 0,
                    w: 128,
                    h: 128,
                },
                content_rect: RectU32 {
                    x: 0,
                    y: 0,
                    w: 128,
                    h: 128,
                },
                skipped: false,
            }],
        );

        assert!(manifest.tiles.is_some());
        assert_eq!(manifest.stats.tile_count, 1);
    }
}
