use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::backend::{backend_support, choose_backend, open_backend};
use crate::cli::{CutArgs, TileIndexMode};
use crate::error::CliError;
use crate::manifest::{Manifest, TileInventoryEntry};
use crate::plan::{BuildFingerprint, BuildState, CutPlan, SourceInfo, now_unix_secs};

const INTERNAL_DIR: &str = ".tilecut";
const PLAN_PATH: &str = ".tilecut/plan.json";
const STATE_PATH: &str = ".tilecut/state.json";
const MANIFEST_PATH: &str = "manifest.json";
const INDEX_PATH: &str = "tiles.ndjson";

pub fn run(args: CutArgs) -> Result<()> {
    preflight_output_target(&args)?;

    let source = SourceInfo::from_path(&args.input)?;
    let plan = CutPlan::build(&args, source)?;
    let support = backend_support();
    let resolved_backend =
        choose_backend(plan.requested_backend, &plan.source, plan.max_in_memory_mib).with_context(
            || {
                format!(
                    "failed to select backend (vips feature enabled: {}, runtime available: {})",
                    support.vips_feature_enabled, support.vips_runtime_available
                )
            },
        )?;

    if args.dry_run {
        print_dry_run(&plan, resolved_backend);
        return Ok(());
    }

    let thread_count = args.threads.unwrap_or_else(default_thread_count);

    let backend = open_backend(resolved_backend, &args.input)?;
    let fingerprint = plan.fingerprint();

    if args.resume {
        resume_existing_output(&args.out, &fingerprint)?;
        write_plan_file(&args.out, &fingerprint)?;
        write_state_file(&args.out, &BuildState::new(plan.total_tile_slots()))?;
        backend.write_tiles(&plan, &args.out, true, thread_count)?;
        maybe_generate_overview(&plan, backend.as_ref(), &args.out, true)?;
        finalize_output(&plan, &args.out)?;
    } else {
        let staging_dir = prepare_staging_dir(&args.out, args.overwrite)?;
        write_plan_file(&staging_dir, &fingerprint)?;
        write_state_file(&staging_dir, &BuildState::new(plan.total_tile_slots()))?;
        backend.write_tiles(&plan, &staging_dir, false, thread_count)?;
        maybe_generate_overview(&plan, backend.as_ref(), &staging_dir, false)?;
        finalize_output(&plan, &staging_dir)?;
        commit_staging_dir(&staging_dir, &args.out, args.overwrite)?;
    }

    Ok(())
}

fn preflight_output_target(args: &CutArgs) -> Result<()> {
    if args.resume {
        if !args.out.exists() {
            return Err(CliError::resume_output_missing(&args.out).into());
        }
    } else if args.out.exists() && !args.overwrite {
        return Err(CliError::output_directory_exists(&args.out).into());
    }

    Ok(())
}

fn print_dry_run(plan: &CutPlan, backend: crate::backend::ResolvedBackendKind) {
    println!("Input: {}", plan.source.path);
    println!("Size: {}x{}", plan.source.width, plan.source.height);
    println!("Levels: {}", plan.levels.len());
    println!("Grid: {} cols x {} rows", plan.grid.cols, plan.grid.rows);
    println!("Tiles: {}", plan.total_tile_slots());
    for level in &plan.levels {
        println!(
            "  Level {}: scale {:.3}, {}x{}, grid {}x{}, tiles {}",
            level.level,
            level.scale,
            level.width,
            level.height,
            level.grid.cols,
            level.grid.rows,
            level.tiles.len()
        );
    }
    println!("Output Format: {:?}", plan.tile.format);
    println!("Manifest Mode: {:?}", plan.manifest_mode);
    println!("Tile Index: {:?}", plan.tile_index_mode);
    println!("Backend: {:?}", backend);
}

fn maybe_generate_overview(
    plan: &CutPlan,
    backend: &dyn crate::backend::TileBackend,
    out_dir: &Path,
    skip_existing: bool,
) -> Result<()> {
    if let Some(max_edge) = plan.overview {
        let overview_path = out_dir.join("preview/overview.png");
        if skip_existing && overview_path.exists() {
            return Ok(());
        }
        backend.generate_overview(max_edge, &overview_path)?;
    }
    Ok(())
}

fn prepare_staging_dir(out_dir: &Path, overwrite: bool) -> Result<PathBuf> {
    if out_dir.exists() && !overwrite {
        return Err(CliError::output_directory_exists(out_dir).into());
    }
    let parent = out_dir.parent().unwrap_or_else(|| Path::new("."));
    let name = out_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tilecut-out");
    let staging_dir = parent.join(format!(".tilecut-tmp-{name}-{}", now_unix_secs()));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)
            .with_context(|| format!("failed to remove stale {}", staging_dir.display()))?;
    }
    fs::create_dir_all(staging_dir.join(INTERNAL_DIR))
        .with_context(|| format!("failed to create {}", staging_dir.display()))?;
    Ok(staging_dir)
}

fn resume_existing_output(out_dir: &Path, fingerprint: &BuildFingerprint) -> Result<()> {
    let plan_path = out_dir.join(PLAN_PATH);
    let raw = fs::read_to_string(&plan_path)
        .map_err(|err| CliError::resume_state_missing(&plan_path, err.to_string()))?;
    let current: BuildFingerprint = serde_json::from_str(&raw)
        .map_err(|err| CliError::resume_state_missing(&plan_path, err.to_string()))?;
    if &current != fingerprint {
        return Err(CliError::resume_plan_mismatch().into());
    }
    fs::create_dir_all(out_dir.join(INTERNAL_DIR))
        .with_context(|| format!("failed to create {}", out_dir.join(INTERNAL_DIR).display()))?;
    Ok(())
}

fn finalize_output(plan: &CutPlan, out_dir: &Path) -> Result<()> {
    let inventory = scan_inventory(plan, out_dir)?;
    if !plan.skip_empty {
        let missing = inventory.iter().filter(|entry| entry.skipped).count();
        ensure!(
            missing == 0,
            "expected all tiles to be present, but {missing} were missing"
        );
    }

    if plan.tile_index_mode == TileIndexMode::Ndjson {
        write_index_file(out_dir, &inventory)?;
    }

    let manifest = Manifest::from_plan(plan, inventory.clone());
    write_manifest_file(out_dir, &manifest)?;
    write_state_file(
        out_dir,
        &BuildState {
            complete: true,
            total_tiles: inventory.len(),
            written_tiles: inventory.iter().filter(|entry| !entry.skipped).count(),
            skipped_tiles: inventory.iter().filter(|entry| entry.skipped).count(),
            updated_unix_secs: now_unix_secs(),
        },
    )?;
    Ok(())
}

fn scan_inventory(plan: &CutPlan, out_dir: &Path) -> Result<Vec<TileInventoryEntry>> {
    let mut inventory = Vec::with_capacity(plan.total_tile_slots());
    for level in &plan.levels {
        for tile in &level.tiles {
            let path = out_dir.join(&tile.out_rel_path);
            let skipped = !path.exists();
            inventory.push(TileInventoryEntry {
                level: tile.coord.level,
                x: tile.coord.x,
                y: tile.coord.y,
                path: (!skipped).then(|| tile.out_rel_path.to_string_lossy().to_string()),
                src_rect: tile.src_rect,
                content_rect: tile.content_rect,
                skipped,
            });
        }
    }
    Ok(inventory)
}

fn write_manifest_file(out_dir: &Path, manifest: &Manifest) -> Result<()> {
    let path = out_dir.join(MANIFEST_PATH);
    fs::write(&path, serde_json::to_vec_pretty(manifest)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn write_index_file(out_dir: &Path, inventory: &[TileInventoryEntry]) -> Result<()> {
    let mut contents = String::new();
    for entry in inventory.iter().filter(|entry| !entry.skipped) {
        contents.push_str(&serde_json::to_string(entry)?);
        contents.push('\n');
    }
    fs::write(out_dir.join(INDEX_PATH), contents)
        .with_context(|| format!("failed to write {}", out_dir.join(INDEX_PATH).display()))
}

fn write_plan_file(out_dir: &Path, fingerprint: &BuildFingerprint) -> Result<()> {
    let path = out_dir.join(PLAN_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, serde_json::to_vec_pretty(fingerprint)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn write_state_file(out_dir: &Path, state: &BuildState) -> Result<()> {
    let path = out_dir.join(STATE_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, serde_json::to_vec_pretty(state)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn commit_staging_dir(staging_dir: &Path, out_dir: &Path, overwrite: bool) -> Result<()> {
    if out_dir.exists() {
        if overwrite {
            fs::remove_dir_all(out_dir)
                .with_context(|| format!("failed to remove {}", out_dir.display()))?;
        } else {
            return Err(CliError::output_directory_exists(out_dir).into());
        }
    }
    fs::rename(staging_dir, out_dir).with_context(|| {
        format!(
            "failed to move {} to {}",
            staging_dir.display(),
            out_dir.display()
        )
    })
}

fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
}
