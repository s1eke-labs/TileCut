use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::{
    DynamicImage, GenericImageView, ImageBuffer, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage,
};
use rayon::ThreadPoolBuilder;

use crate::backend::{TileBackend, inspect_source};
use crate::error::CliError;
use crate::overview::resize_rgba_for_overview;
use crate::plan::{CutPlan, SourceInfo, TilePlan};

pub struct ImageBackend {
    source: SourceInfo,
    image: Arc<RgbaImage>,
}

impl ImageBackend {
    pub fn open(input: &Path) -> Result<Self> {
        let source = inspect_source(input)?;
        let image = image::open(input)
            .map_err(|err| CliError::unsupported_image(input, err.to_string()))?
            .to_rgba8();
        Ok(Self {
            source,
            image: Arc::new(image),
        })
    }
}

impl TileBackend for ImageBackend {
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
        let image = Arc::clone(&self.image);
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .context("failed to build rayon thread pool")?;
        pool.install(|| {
            use rayon::prelude::*;
            plan.tiles.par_iter().try_for_each(|tile| {
                let output_path = output_root.join(&tile.out_rel_path);
                if skip_existing && output_path.exists() {
                    return Ok(());
                }
                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                let cropped = image
                    .view(
                        tile.src_rect.x,
                        tile.src_rect.y,
                        tile.src_rect.w,
                        tile.src_rect.h,
                    )
                    .to_image();
                let rendered = render_tile_image(plan, tile, &cropped)?;
                match rendered {
                    Some(tile_image) => write_encoded_image(&output_path, &tile_image, plan),
                    None => Ok(()),
                }
            })
        })
    }

    fn generate_overview(&self, max_edge: u32, out_path: &Path) -> Result<()> {
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let resized = resize_rgba_for_overview(&self.image, max_edge);
        DynamicImage::ImageRgba8(resized)
            .save_with_format(out_path, ImageFormat::Png)
            .with_context(|| format!("failed to write {}", out_path.display()))
    }
}

pub fn render_tile_image(
    plan: &CutPlan,
    _tile: &TilePlan,
    cropped: &RgbaImage,
) -> Result<Option<RgbaImage>> {
    if plan.skip_empty
        && cropped
            .pixels()
            .all(|pixel| pixel.0[3] <= plan.empty_alpha_threshold)
    {
        return Ok(None);
    }

    let rendered = match plan.tile.edge_mode {
        crate::cli::EdgeMode::Crop => cropped.clone(),
        crate::cli::EdgeMode::Pad | crate::cli::EdgeMode::Skip => {
            let mut image = ImageBuffer::from_pixel(
                plan.tile.width,
                plan.tile.height,
                Rgba(plan.tile.pad_color),
            );
            for (x, y, pixel) in cropped.enumerate_pixels() {
                image.put_pixel(x, y, *pixel);
            }
            image
        }
    };
    Ok(Some(rendered))
}

pub fn write_encoded_image(path: &Path, image: &RgbaImage, plan: &CutPlan) -> Result<()> {
    let file =
        fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    match plan.tile.format {
        crate::cli::OutputFormat::Png => DynamicImage::ImageRgba8(image.clone())
            .write_to(&mut writer, ImageFormat::Png)
            .with_context(|| format!("failed to write png {}", path.display())),
        crate::cli::OutputFormat::Webp => DynamicImage::ImageRgba8(image.clone())
            .write_to(&mut writer, ImageFormat::WebP)
            .with_context(|| format!("failed to write webp {}", path.display())),
        crate::cli::OutputFormat::Jpeg => {
            let flattened = flatten_for_jpeg(image, plan.tile.flatten_alpha)?;
            let mut encoder = JpegEncoder::new_with_quality(&mut writer, plan.tile.quality);
            encoder
                .encode(
                    flattened.as_raw(),
                    flattened.width(),
                    flattened.height(),
                    image::ColorType::Rgb8.into(),
                )
                .with_context(|| format!("failed to write jpeg {}", path.display()))
        }
    }
}

fn flatten_for_jpeg(image: &RgbaImage, flatten_alpha: Option<[u8; 4]>) -> Result<RgbImage> {
    let mut rgb = RgbImage::new(image.width(), image.height());
    let background = flatten_alpha.unwrap_or([0, 0, 0, 255]);
    for (x, y, pixel) in image.enumerate_pixels() {
        let alpha = f32::from(pixel.0[3]) / 255.0;
        if alpha < 1.0 && flatten_alpha.is_none() {
            return Err(CliError::jpeg_transparency().into());
        }
        let bg_alpha = f32::from(background[3]) / 255.0;
        let effective_alpha = alpha + bg_alpha * (1.0 - alpha);
        let blend = |foreground: u8, background_channel: u8| -> u8 {
            if effective_alpha == 0.0 {
                0
            } else {
                (((f32::from(foreground) * alpha)
                    + (f32::from(background_channel) * bg_alpha * (1.0 - alpha)))
                    / effective_alpha)
                    .round()
                    .clamp(0.0, 255.0) as u8
            }
        };
        rgb.put_pixel(
            x,
            y,
            Rgb([
                blend(pixel.0[0], background[0]),
                blend(pixel.0[1], background[1]),
                blend(pixel.0[2], background[2]),
            ]),
        );
    }
    Ok(rgb)
}
