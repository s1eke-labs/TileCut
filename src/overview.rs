use std::path::Path;

use anyhow::{Context, Result};
use image::{DynamicImage, RgbaImage, imageops::FilterType};

pub fn resize_rgba_for_overview(image: &RgbaImage, max_edge: u32) -> RgbaImage {
    if max_edge == 0 {
        return image.clone();
    }
    let width = image.width();
    let height = image.height();
    let longest = width.max(height);
    if longest <= max_edge {
        return image.clone();
    }
    let scale = max_edge as f64 / longest as f64;
    let out_width = (width as f64 * scale).round().max(1.0) as u32;
    let out_height = (height as f64 * scale).round().max(1.0) as u32;
    DynamicImage::ImageRgba8(image.clone())
        .resize(out_width, out_height, FilterType::Lanczos3)
        .to_rgba8()
}

pub fn generate_overview_with_image_crate(
    input: &Path,
    max_edge: u32,
    out_path: &Path,
) -> Result<()> {
    let image = image::open(input)
        .with_context(|| format!("failed to decode image {}", input.display()))?
        .to_rgba8();
    let resized = resize_rgba_for_overview(&image, max_edge);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    DynamicImage::ImageRgba8(resized)
        .save_with_format(out_path, image::ImageFormat::Png)
        .with_context(|| format!("failed to write {}", out_path.display()))
}
