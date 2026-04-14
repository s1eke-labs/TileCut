use std::path::{Path, PathBuf};

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::cli::{EdgeMode, TileIndexMode};
use crate::error::CliError;
use crate::manifest::{Manifest, TileInventoryEntry};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub manifest_path: PathBuf,
    pub checked_tiles: usize,
    pub missing_tiles: usize,
    pub errors: Vec<String>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate_manifest_path(path: &Path) -> Result<ValidationReport> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| CliError::manifest_read_failed(path, err.to_string()))?;
    let manifest: Manifest = serde_json::from_str(&content)
        .map_err(|err| CliError::manifest_parse_failed(path, err.to_string()))?;
    validate_manifest(&manifest, path)
}

pub fn validate_manifest(manifest: &Manifest, manifest_path: &Path) -> Result<ValidationReport> {
    ensure!(
        manifest.grid.cols > 0,
        "manifest grid cols must be greater than 0"
    );
    ensure!(
        manifest.grid.rows > 0,
        "manifest grid rows must be greater than 0"
    );
    ensure!(
        manifest.tile.width > 0,
        "manifest tile width must be greater than 0"
    );
    ensure!(
        manifest.tile.height > 0,
        "manifest tile height must be greater than 0"
    );

    let base_dir = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let inventory = if let Some(tiles) = &manifest.tiles {
        tiles
            .iter()
            .map(|tile| TileInventoryEntry {
                level: tile.level,
                x: tile.x,
                y: tile.y,
                path: tile.path.clone(),
                src_rect: tile.src_rect,
                content_rect: tile.content_rect,
                skipped: tile.skipped,
            })
            .collect::<Vec<_>>()
    } else if let Some(index) = &manifest.index {
        if index.mode != TileIndexMode::Ndjson {
            bail!("unsupported tile index mode {:?}", index.mode);
        }
        let index_path = base_dir.join(&index.path);
        let raw = std::fs::read_to_string(&index_path)
            .map_err(|err| CliError::manifest_read_failed(&index_path, err.to_string()))?;
        raw.lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<TileInventoryEntry>, _>>()
            .map_err(|err| CliError::tile_index_parse_failed(&index_path, err.to_string()))?
    } else {
        derive_inventory_from_compact(manifest)
    };

    let mut errors = Vec::new();
    let mut missing_tiles = 0;
    for entry in &inventory {
        if entry.src_rect.w == 0 || entry.src_rect.h == 0 {
            errors.push(format!(
                "tile {},{} has zero-sized src_rect",
                entry.x, entry.y
            ));
            continue;
        }
        if entry.content_rect.w > manifest.tile.width || entry.content_rect.h > manifest.tile.height
        {
            errors.push(format!(
                "tile {},{} has content_rect outside tile bounds",
                entry.x, entry.y
            ));
        }
        if entry.skipped {
            continue;
        }
        let Some(path) = &entry.path else {
            errors.push(format!(
                "tile {},{} is not skipped but has no path",
                entry.x, entry.y
            ));
            continue;
        };
        let tile_path = base_dir.join(path);
        if !tile_path.exists() {
            missing_tiles += 1;
            errors.push(format!("missing tile file {}", tile_path.display()));
            continue;
        }
        match image::image_dimensions(&tile_path) {
            Ok((width, height)) => {
                let expected = match manifest.tile.edge_mode {
                    EdgeMode::Pad => (manifest.tile.width, manifest.tile.height),
                    EdgeMode::Crop | EdgeMode::Skip => (entry.content_rect.w, entry.content_rect.h),
                };
                if (width, height) != expected {
                    errors.push(format!(
                        "tile {} dimensions mismatch: expected {}x{}, got {}x{}",
                        tile_path.display(),
                        expected.0,
                        expected.1,
                        width,
                        height
                    ));
                }
            }
            Err(err) => errors.push(format!("failed to inspect {}: {err}", tile_path.display())),
        }
    }

    Ok(ValidationReport {
        manifest_path: manifest_path.to_path_buf(),
        checked_tiles: inventory.len(),
        missing_tiles,
        errors,
    })
}

fn derive_inventory_from_compact(manifest: &Manifest) -> Vec<TileInventoryEntry> {
    let mut entries = Vec::with_capacity((manifest.grid.cols * manifest.grid.rows) as usize);
    let pad_width = manifest.naming.zero_pad_width;
    for y in 0..manifest.grid.rows {
        for x in 0..manifest.grid.cols {
            let src_x = x * manifest.tile.width;
            let src_y = y * manifest.tile.height;
            let src_w = manifest
                .source
                .width
                .saturating_sub(src_x)
                .min(manifest.tile.width);
            let src_h = manifest
                .source
                .height
                .saturating_sub(src_y)
                .min(manifest.tile.height);
            let path = match manifest.naming.layout {
                crate::cli::LayoutMode::Flat => Some(format!(
                    "tiles/x{x:0pad_width$}_y{y:0pad_width$}.{}",
                    manifest.tile.format.extension()
                )),
                crate::cli::LayoutMode::Sharded => Some(format!(
                    "tiles/y{y:0pad_width$}/x{x:0pad_width$}.{}",
                    manifest.tile.format.extension()
                )),
            };
            entries.push(TileInventoryEntry {
                level: 0,
                x,
                y,
                path,
                src_rect: crate::plan::RectU32 {
                    x: src_x,
                    y: src_y,
                    w: src_w,
                    h: src_h,
                },
                content_rect: crate::plan::RectU32 {
                    x: 0,
                    y: 0,
                    w: src_w,
                    h: src_h,
                },
                skipped: false,
            });
        }
    }
    entries
}
