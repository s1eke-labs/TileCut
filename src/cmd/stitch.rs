use std::path::{Path, PathBuf};

use anyhow::Result;
use image::{DynamicImage, GenericImageView, RgbaImage};

use crate::cli::StitchArgs;
use crate::error::CliError;
use crate::manifest::Manifest;
use crate::validate::collect_inventory;

pub fn run(args: StitchArgs) -> Result<()> {
    ensure_png_output(&args.out)?;

    let raw = std::fs::read_to_string(&args.manifest)
        .map_err(|err| CliError::manifest_read_failed(&args.manifest, err.to_string()))?;
    let manifest: Manifest = serde_json::from_str(&raw)
        .map_err(|err| CliError::manifest_parse_failed(&args.manifest, err.to_string()))?;
    let level = manifest
        .levels
        .iter()
        .find(|level| level.level == args.level)
        .ok_or_else(|| CliError::stitch_level_missing(args.level))?;
    let inventory = collect_inventory(&manifest, &args.manifest)?;
    let base_dir = args
        .manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut output = RgbaImage::new(level.width, level.height);

    for entry in inventory
        .iter()
        .filter(|entry| entry.level == args.level && !entry.skipped)
    {
        let Some(rel_path) = &entry.path else {
            return Err(CliError::generic(format!(
                "tile level {},{},{} is not skipped but has no path",
                entry.level, entry.x, entry.y
            ))
            .into());
        };
        let tile_path = base_dir.join(rel_path);
        if !tile_path.exists() {
            return Err(CliError::stitch_tile_missing(&tile_path).into());
        }
        let tile = image::open(&tile_path)
            .map_err(|err| CliError::stitch_tile_decode_failed(&tile_path, err.to_string()))?
            .to_rgba8();
        if entry.content_rect.x.saturating_add(entry.content_rect.w) > tile.width()
            || entry.content_rect.y.saturating_add(entry.content_rect.h) > tile.height()
        {
            return Err(CliError::generic(format!(
                "tile {} content_rect exceeds image bounds",
                tile_path.display()
            ))
            .into());
        }
        let cropped = tile
            .view(
                entry.content_rect.x,
                entry.content_rect.y,
                entry.content_rect.w,
                entry.content_rect.h,
            )
            .to_image();
        for (x, y, pixel) in cropped.enumerate_pixels() {
            output.put_pixel(entry.src_rect.x + x, entry.src_rect.y + y, *pixel);
        }
    }

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    DynamicImage::ImageRgba8(output)
        .save_with_format(&args.out, image::ImageFormat::Png)
        .map_err(|err| {
            CliError::generic(format!(
                "failed to write stitched png {}: {err}",
                args.out.display()
            ))
        })?;
    Ok(())
}

fn ensure_png_output(path: &Path) -> Result<()> {
    let is_png = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("png"))
        .unwrap_or(false);
    if !is_png {
        return Err(CliError::stitch_output_must_be_png(path).into());
    }
    Ok(())
}
