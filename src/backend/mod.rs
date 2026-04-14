pub mod image_backend;
#[cfg(feature = "vips")]
pub mod vips_backend;

use std::path::Path;
use std::process::Command;

use anyhow::{Error, Result};
use image::ImageReader;

use crate::cli::BackendKind;
use crate::error::CliError;
use crate::plan::{SourceInfo, compute_sha256};

pub trait TileBackend {
    fn source_info(&self) -> &SourceInfo;
    fn write_tiles(
        &self,
        plan: &crate::plan::CutPlan,
        output_root: &Path,
        skip_existing: bool,
        threads: usize,
    ) -> Result<()>;
    fn generate_overview(&self, max_edge: u32, out_path: &Path) -> Result<()>;
}

#[derive(Clone, Debug)]
pub struct BackendSupport {
    pub vips_feature_enabled: bool,
    pub vips_runtime_available: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ResolvedBackendKind {
    Image,
    Vips,
}

pub fn inspect_source(path: &Path) -> Result<SourceInfo> {
    let metadata = std::fs::metadata(path).map_err(|err| -> Error {
        match err.kind() {
            std::io::ErrorKind::NotFound => CliError::input_not_found(path, err.to_string()).into(),
            _ => CliError::input_unreadable(path, err.to_string()).into(),
        }
    })?;
    let reader = ImageReader::open(path)
        .map_err(|err| CliError::input_unreadable(path, err.to_string()))?
        .with_guessed_format()
        .map_err(|err| CliError::unsupported_image(path, err.to_string()))?;
    let format = reader
        .format()
        .map(|format| {
            format
                .extensions_str()
                .first()
                .copied()
                .unwrap_or("unknown")
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string());
    let (width, height) = reader
        .into_dimensions()
        .map_err(|err| CliError::unsupported_image(path, err.to_string()))?;
    let modified_unix_secs = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    Ok(SourceInfo {
        path: path.to_string_lossy().to_string(),
        width,
        height,
        format,
        file_size: metadata.len(),
        modified_unix_secs,
        sha256: compute_sha256(path)
            .map_err(|err| CliError::input_unreadable(path, err.to_string()))?,
    })
}

pub fn estimated_rgba_bytes(width: u32, height: u32) -> u64 {
    u64::from(width) * u64::from(height) * 4
}

pub fn backend_support() -> BackendSupport {
    BackendSupport {
        vips_feature_enabled: cfg!(feature = "vips"),
        vips_runtime_available: vips_runtime_available(),
    }
}

pub fn choose_backend(
    requested: BackendKind,
    source: &SourceInfo,
    max_in_memory_mib: u64,
) -> Result<ResolvedBackendKind> {
    match requested {
        BackendKind::Image => Ok(ResolvedBackendKind::Image),
        BackendKind::Vips => resolve_vips_backend(),
        BackendKind::Auto => {
            let estimated = estimated_rgba_bytes(source.width, source.height);
            let budget = max_in_memory_mib.saturating_mul(1024 * 1024);
            if estimated <= budget {
                Ok(ResolvedBackendKind::Image)
            } else {
                resolve_vips_backend()
            }
        }
    }
}

pub fn open_backend(kind: ResolvedBackendKind, input: &Path) -> Result<Box<dyn TileBackend>> {
    match kind {
        ResolvedBackendKind::Image => Ok(Box::new(image_backend::ImageBackend::open(input)?)),
        ResolvedBackendKind::Vips => {
            #[cfg(feature = "vips")]
            {
                Ok(Box::new(vips_backend::VipsBackend::open(input)?))
            }
            #[cfg(not(feature = "vips"))]
            {
                let _ = input;
                Err(CliError::vips_feature_disabled().into())
            }
        }
    }
}

fn resolve_vips_backend() -> Result<ResolvedBackendKind> {
    if !cfg!(feature = "vips") {
        return Err(CliError::vips_feature_disabled().into());
    }
    if !vips_runtime_available() {
        return Err(CliError::vips_runtime_missing().into());
    }
    Ok(ResolvedBackendKind::Vips)
}

pub fn vips_runtime_available() -> bool {
    Command::new("vips")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{choose_backend, estimated_rgba_bytes};
    use crate::cli::BackendKind;
    use crate::plan::SourceInfo;

    fn fake_source(width: u32, height: u32) -> SourceInfo {
        SourceInfo {
            path: "fake.png".to_string(),
            width,
            height,
            format: "png".to_string(),
            file_size: 0,
            modified_unix_secs: 0,
            sha256: "abc".to_string(),
        }
    }

    #[test]
    fn estimates_rgba_bytes() {
        assert_eq!(estimated_rgba_bytes(10, 20), 800);
    }

    #[test]
    fn auto_selects_image_when_within_budget() {
        let source = fake_source(256, 256);
        let backend = choose_backend(BackendKind::Auto, &source, 1).expect("backend");
        assert_eq!(backend, super::ResolvedBackendKind::Image);
    }

    #[cfg(not(feature = "vips"))]
    #[test]
    fn auto_errors_for_large_images_without_vips_feature() {
        let source = fake_source(100_000, 100_000);
        let error = choose_backend(BackendKind::Auto, &source, 1).expect_err("should fail");
        assert!(error.to_string().contains("vips"));
    }
}
