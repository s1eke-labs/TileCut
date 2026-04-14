use anyhow::Result;

use crate::backend::backend_support;
use crate::cli::InspectArgs;
use crate::plan::{CutPlan, SourceInfo};

pub fn run(args: InspectArgs) -> Result<()> {
    let source = SourceInfo::from_path(&args.input)?;
    let report = CutPlan::inspect_report(&args, source, &backend_support())?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Source: {}", report.source.path);
        println!("Size: {}x{}", report.source.width, report.source.height);
        println!(
            "Tile: {}x{} ({:?})",
            report.tile_width, report.tile_height, report.edge_mode
        );
        println!(
            "Levels: {} ({} total tile slots)",
            report.total_levels, report.total_tile_count
        );
        println!("Grid: {} cols x {} rows", report.cols, report.rows);
        println!("Tile Count: {}", report.tile_count);
        for level in &report.levels {
            println!(
                "  Level {}: scale {:.3}, {}x{}, grid {}x{}, tiles {}",
                level.level,
                level.scale,
                level.width,
                level.height,
                level.cols,
                level.rows,
                level.tile_count
            );
        }
        println!(
            "Estimated RGBA Memory: {:.2} MiB",
            report.estimated_rgba_mib
        );
        println!(
            "Recommended Backend: {:?} (vips feature: {}, runtime: {})",
            report.backend.recommended,
            report.backend.vips_feature_enabled,
            report.backend.vips_runtime_available
        );
    }
    Ok(())
}
