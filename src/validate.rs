use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::cli::{EdgeMode, TileIndexMode};
use crate::error::CliError;
use crate::manifest::{Manifest, TileInventoryEntry};
use crate::naming::render_rel_path;
use crate::plan::{RectU32, level_scale};

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
    let inventory = collect_inventory(manifest, manifest_path)?;

    let mut errors = Vec::new();
    validate_manifest_metadata(manifest, &inventory, &mut errors);
    let levels_by_id = manifest
        .levels
        .iter()
        .map(|level| (level.level, level))
        .collect::<BTreeMap<_, _>>();

    let mut missing_tiles = 0;
    for entry in &inventory {
        let Some(level) = levels_by_id.get(&entry.level) else {
            errors.push(format!(
                "tile {},{} references unknown level {}",
                entry.x, entry.y, entry.level
            ));
            continue;
        };
        validate_tile_geometry(manifest, level.width, level.height, entry, &mut errors);
        if entry.skipped {
            continue;
        }
        let Some(path) = &entry.path else {
            errors.push(format!(
                "tile level {},{},{} is not skipped but has no path",
                entry.level, entry.x, entry.y
            ));
            continue;
        };
        let tile_path = base_dir.join(path);
        if !tile_path.exists() {
            missing_tiles += 1;
            errors.push(format!(
                "missing tile file for level {}: {}",
                entry.level,
                tile_path.display()
            ));
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

pub fn collect_inventory(
    manifest: &Manifest,
    manifest_path: &Path,
) -> Result<Vec<TileInventoryEntry>> {
    let full_slots = derive_inventory_from_compact(manifest);
    let mut inventory = if let Some(tiles) = &manifest.tiles {
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
        let base_dir = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let index_path = base_dir.join(&index.path);
        let raw = std::fs::read_to_string(&index_path)
            .map_err(|err| CliError::manifest_read_failed(&index_path, err.to_string()))?;
        let present = raw
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<TileInventoryEntry>, _>>()
            .map_err(|err| CliError::tile_index_parse_failed(&index_path, err.to_string()))?;
        merge_index_inventory(full_slots, present)
    } else {
        full_slots
    };
    inventory.sort_by_key(|entry| (entry.level, entry.y, entry.x));
    Ok(inventory)
}

fn merge_index_inventory(
    full_slots: Vec<TileInventoryEntry>,
    present: Vec<TileInventoryEntry>,
) -> Vec<TileInventoryEntry> {
    let present_by_coord = present
        .into_iter()
        .map(|entry| ((entry.level, entry.x, entry.y), entry))
        .collect::<BTreeMap<_, _>>();
    full_slots
        .into_iter()
        .map(|slot| {
            present_by_coord
                .get(&(slot.level, slot.x, slot.y))
                .cloned()
                .unwrap_or(TileInventoryEntry {
                    level: slot.level,
                    x: slot.x,
                    y: slot.y,
                    path: None,
                    src_rect: slot.src_rect,
                    content_rect: slot.content_rect,
                    skipped: true,
                })
        })
        .collect()
}

fn validate_manifest_metadata(
    manifest: &Manifest,
    inventory: &[TileInventoryEntry],
    errors: &mut Vec<String>,
) {
    if manifest.levels.is_empty() {
        errors.push("manifest levels must not be empty".to_string());
        return;
    }

    if let Some(level0) = manifest.levels.first() {
        if level0.level != 0 {
            errors.push("manifest levels must start at level 0".to_string());
        }
        if level0.width != manifest.source.width || level0.height != manifest.source.height {
            errors.push("level 0 dimensions must match source dimensions".to_string());
        }
        if manifest.grid.cols != level0.cols || manifest.grid.rows != level0.rows {
            errors.push("manifest grid must match level 0 grid".to_string());
        }
        if manifest.naming.zero_pad_width != level0.zero_pad_width {
            errors.push("manifest naming zero_pad_width must match level 0".to_string());
        }
    }

    if manifest.levels.len() > 1 && !manifest.naming.path_template.contains("{level}") {
        errors.push(
            "multi-level manifests must include `{level}` in naming.path_template".to_string(),
        );
    }

    for (expected_level, level) in manifest.levels.iter().enumerate() {
        if level.level != expected_level as u32 {
            errors.push(format!(
                "manifest levels must be continuous from 0, found level {} at position {}",
                level.level, expected_level
            ));
        }
        let expected_scale = level_scale(level.level);
        if (level.scale - expected_scale).abs() > 1e-9 {
            errors.push(format!(
                "level {} has invalid scale {}, expected {}",
                level.level, level.scale, expected_scale
            ));
        }
        if level.cols == 0 || level.rows == 0 {
            errors.push(format!(
                "level {} grid must be greater than 0x0",
                level.level
            ));
        }
        let level_inventory = inventory
            .iter()
            .filter(|entry| entry.level == level.level)
            .collect::<Vec<_>>();
        let level_total_slots = level_inventory.len();
        let level_tile_count = level_inventory
            .iter()
            .filter(|entry| !entry.skipped)
            .count();
        let level_skipped_count = level_total_slots.saturating_sub(level_tile_count);
        if level.total_slots != level_total_slots {
            errors.push(format!(
                "level {} total_slots mismatch: manifest {}, derived {}",
                level.level, level.total_slots, level_total_slots
            ));
        }
        if level.tile_count != level_tile_count {
            errors.push(format!(
                "level {} tile_count mismatch: manifest {}, derived {}",
                level.level, level.tile_count, level_tile_count
            ));
        }
        if level.skipped_count != level_skipped_count {
            errors.push(format!(
                "level {} skipped_count mismatch: manifest {}, derived {}",
                level.level, level.skipped_count, level_skipped_count
            ));
        }
    }

    let total_slots = inventory.len();
    let tile_count = inventory.iter().filter(|entry| !entry.skipped).count();
    let skipped_count = total_slots.saturating_sub(tile_count);
    if manifest.stats.total_slots != total_slots {
        errors.push(format!(
            "manifest total_slots mismatch: manifest {}, derived {}",
            manifest.stats.total_slots, total_slots
        ));
    }
    if manifest.stats.tile_count != tile_count {
        errors.push(format!(
            "manifest tile_count mismatch: manifest {}, derived {}",
            manifest.stats.tile_count, tile_count
        ));
    }
    if manifest.stats.skipped_count != skipped_count {
        errors.push(format!(
            "manifest skipped_count mismatch: manifest {}, derived {}",
            manifest.stats.skipped_count, skipped_count
        ));
    }
}

fn validate_tile_geometry(
    manifest: &Manifest,
    level_width: u32,
    level_height: u32,
    entry: &TileInventoryEntry,
    errors: &mut Vec<String>,
) {
    if entry.src_rect.w == 0 || entry.src_rect.h == 0 {
        errors.push(format!(
            "tile level {},{},{} has zero-sized src_rect",
            entry.level, entry.x, entry.y
        ));
        return;
    }
    if entry.content_rect.w > manifest.tile.width || entry.content_rect.h > manifest.tile.height {
        errors.push(format!(
            "tile level {},{},{} has content_rect outside tile bounds",
            entry.level, entry.x, entry.y
        ));
    }
    if entry.src_rect.x.saturating_add(entry.src_rect.w) > level_width
        || entry.src_rect.y.saturating_add(entry.src_rect.h) > level_height
    {
        errors.push(format!(
            "tile level {},{},{} src_rect exceeds level bounds",
            entry.level, entry.x, entry.y
        ));
    }
}

fn derive_inventory_from_compact(manifest: &Manifest) -> Vec<TileInventoryEntry> {
    let multi_level =
        manifest.levels.len() > 1 || manifest.naming.path_template.contains("{level}");
    let mut entries = Vec::new();
    for level in &manifest.levels {
        let level_capacity = (level.cols * level.rows) as usize;
        entries.reserve(level_capacity);
        for y in 0..level.rows {
            for x in 0..level.cols {
                let src_x = x * manifest.tile.width;
                let src_y = y * manifest.tile.height;
                let src_w = level.width.saturating_sub(src_x).min(manifest.tile.width);
                let src_h = level.height.saturating_sub(src_y).min(manifest.tile.height);
                let path = Some(
                    render_rel_path(
                        manifest.naming.layout,
                        manifest.tile.format,
                        level.zero_pad_width,
                        level.level,
                        x,
                        y,
                        multi_level,
                    )
                    .to_string_lossy()
                    .to_string(),
                );
                entries.push(TileInventoryEntry {
                    level: level.level,
                    x,
                    y,
                    path,
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
                    skipped: false,
                });
            }
        }
    }
    entries
}
