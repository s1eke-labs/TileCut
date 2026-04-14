#![cfg(feature = "vips")]

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use image::RgbaImage;
use rayon::ThreadPoolBuilder;
use tempfile::Builder;

use crate::backend::image_backend::{render_tile_image, write_encoded_image};
use crate::backend::{TileBackend, inspect_source, vips_runtime_available};
use crate::error::CliError;
use crate::overview::generate_overview_with_image_crate;
use crate::plan::{CutPlan, SourceInfo};

pub struct VipsBackend {
    input: std::path::PathBuf,
    source: SourceInfo,
}

impl VipsBackend {
    pub fn open(input: &Path) -> Result<Self> {
        if !vips_runtime_available() {
            return Err(CliError::vips_runtime_missing().into());
        }
        Ok(Self {
            input: input.to_path_buf(),
            source: inspect_source(input)?,
        })
    }
}

impl TileBackend for VipsBackend {
    fn source_info(&self) -> &SourceInfo {
        &self.source
    }

    fn write_tiles(
        &self,
        plan: &CutPlan,
        output_root: &Path,
        skip_existing: bool,
        threads: usize,
    ) -> Result<()> {
        let input = self.input.clone();
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .context("failed to build rayon thread pool")?;
        for level in &plan.levels {
            let resized_input = if level.level == 0 {
                None
            } else {
                let temp = Builder::new()
                    .suffix(".png")
                    .tempfile()
                    .context("failed to create temporary resized image")?;
                let status = Command::new("vips")
                    .arg("resize")
                    .arg(&input)
                    .arg(temp.path())
                    .arg(level.scale.to_string())
                    .status()
                    .context("failed to launch `vips resize`")?;
                if !status.success() {
                    bail!("`vips resize` failed for level {}", level.level);
                }
                Some(temp)
            };
            let level_input = resized_input
                .as_ref()
                .map(|temp| temp.path())
                .unwrap_or(input.as_path());
            pool.install(|| {
                use rayon::prelude::*;
                level.tiles.par_iter().try_for_each(|tile| {
                    let output_path = output_root.join(&tile.out_rel_path);
                    if skip_existing && output_path.exists() {
                        return Ok(());
                    }
                    if let Some(parent) = output_path.parent() {
                        fs::create_dir_all(parent)
                            .with_context(|| format!("failed to create {}", parent.display()))?;
                    }
                    let temp = Builder::new()
                        .suffix(".png")
                        .tempfile()
                        .context("failed to create temporary crop")?;
                    let crop_path = temp.path().to_path_buf();
                    let status = Command::new("vips")
                        .arg("crop")
                        .arg(level_input)
                        .arg(&crop_path)
                        .arg(tile.src_rect.x.to_string())
                        .arg(tile.src_rect.y.to_string())
                        .arg(tile.src_rect.w.to_string())
                        .arg(tile.src_rect.h.to_string())
                        .status()
                        .context("failed to launch `vips crop`")?;
                    if !status.success() {
                        bail!(
                            "`vips crop` failed for level {}, tile {},{}",
                            tile.coord.level,
                            tile.coord.x,
                            tile.coord.y
                        );
                    }
                    let cropped: RgbaImage = image::open(&crop_path)
                        .with_context(|| {
                            format!("failed to decode vips crop {}", crop_path.display())
                        })?
                        .to_rgba8();
                    let rendered = render_tile_image(plan, tile, &cropped)?;
                    if let Some(tile_image) = rendered {
                        write_encoded_image(&output_path, &tile_image, plan)?;
                    }
                    Ok(())
                })
            })?;
        }
        Ok(())
    }

    fn generate_overview(&self, max_edge: u32, out_path: &Path) -> Result<()> {
        generate_overview_with_image_crate(&self.input, max_edge, out_path)
    }
}
