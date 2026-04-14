use std::fs;
use std::path::Path;

use assert_cmd::Command;
use image::{Rgba, RgbaImage};
use predicates::prelude::*;
use tempfile::TempDir;
use tilecut::overview::resize_rgba_to_dimensions;

fn write_rgba_fixture<F>(path: &Path, width: u32, height: u32, painter: F)
where
    F: Fn(u32, u32) -> [u8; 4],
{
    let image = RgbaImage::from_fn(width, height, |x, y| Rgba(painter(x, y)));
    image.save(path).expect("fixture image should save");
}

fn read_manifest(out_dir: &Path) -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(out_dir.join("manifest.json")).expect("manifest exists"),
    )
    .expect("manifest json")
}

fn read_rgba(path: &Path) -> RgbaImage {
    image::open(path).expect("image exists").to_rgba8()
}

#[test]
fn root_help_lists_commands_and_examples() {
    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains(
                "TileCut is an offline tile builder for large minimap images.",
            )
            .and(predicate::str::contains(
                "Read image dimensions and preview the cut plan.",
            ))
            .and(predicate::str::contains("Examples:"))
            .and(predicate::str::contains(
                "tilecut cut map.png --out out --tile-size 256",
            )),
        );
}

#[test]
fn cut_help_groups_options_and_examples() {
    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Output & layout:")
                .and(predicate::str::contains("Tile sizing:"))
                .and(predicate::str::contains("--max-level"))
                .and(predicate::str::contains("Manifest & indexing:"))
                .and(predicate::str::contains("World mapping:"))
                .and(predicate::str::contains("Resuming & overwrite:"))
                .and(predicate::str::contains("Backend & performance:"))
                .and(predicate::str::contains("Advanced & debugging:"))
                .and(predicate::str::contains("tilecut cut map.png --out out --manifest full --tile-index ndjson --skip-empty")),
        );
}

#[test]
fn subcommand_help_lists_key_flags() {
    for (command, expected_flags) in [
        ("inspect", vec!["--max-level", "--json"]),
        ("stitch", vec!["--out", "--level"]),
        ("validate", vec!["--json"]),
    ] {
        let output = Command::cargo_bin("tilecut")
            .expect("binary")
            .arg(command)
            .arg("--help")
            .output()
            .expect("help command should run");

        assert!(output.status.success(), "{command} --help should succeed");

        let stdout = String::from_utf8(output.stdout).expect("help output should be utf-8");
        for flag in expected_flags {
            assert!(
                stdout.contains(flag),
                "{command} help should mention {flag}"
            );
        }
    }
}

#[test]
fn inspect_reports_expected_grid_as_json() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("inspect.png");
    write_rgba_fixture(&input, 513, 512, |_, _| [255, 0, 0, 255]);

    let mut cmd = Command::cargo_bin("tilecut").expect("binary");
    let assert = cmd
        .arg("inspect")
        .arg(&input)
        .arg("--tile-size")
        .arg("256")
        .arg("--json")
        .assert()
        .success();

    let report: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("inspect json");
    assert_eq!(report["cols"], 3);
    assert_eq!(report["rows"], 2);
    assert_eq!(report["backend"]["recommended"], "image");
}

#[test]
fn inspect_reports_pyramid_levels_as_json() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("inspect-pyramid.png");
    write_rgba_fixture(&input, 513, 513, |x, y| {
        [(x % 255) as u8, (y % 255) as u8, 32, 255]
    });

    let mut cmd = Command::cargo_bin("tilecut").expect("binary");
    let assert = cmd
        .arg("inspect")
        .arg(&input)
        .arg("--tile-size")
        .arg("256")
        .arg("--max-level")
        .arg("2")
        .arg("--json")
        .assert()
        .success();

    let report: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("inspect json");
    assert_eq!(report["total_levels"], 3);
    assert_eq!(report["total_tile_count"], 14);
    assert_eq!(report["levels"][0]["scale"], 1.0);
    assert_eq!(report["levels"][1]["scale"], 0.5);
    assert_eq!(report["levels"][2]["scale"], 0.25);
    assert_eq!(report["levels"][1]["width"], 257);
    assert_eq!(report["levels"][1]["height"], 257);
    assert_eq!(report["levels"][2]["cols"], 1);
    assert_eq!(report["levels"][2]["rows"], 1);
}

#[test]
fn cut_reports_missing_input_with_actionable_error() {
    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg("does-not-exist.png")
        .arg("--out")
        .arg("out")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Error: Input file was not found")
                .and(predicate::str::contains("Try:"))
                .and(predicate::str::contains("supported image")),
        );
}

#[test]
fn cut_checks_existing_output_before_inspecting_input() {
    let temp = TempDir::new().expect("tempdir");
    let out_dir = temp.path().join("out");
    fs::create_dir_all(&out_dir).expect("create output dir");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg("does-not-exist.png")
        .arg("--out")
        .arg(&out_dir)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Output directory already exists")
                .and(predicate::str::contains("--overwrite"))
                .and(predicate::str::contains("--resume"))
                .and(predicate::str::contains("does-not-exist.png").not()),
        );
}

#[test]
fn resume_checks_output_directory_before_inspecting_input() {
    let temp = TempDir::new().expect("tempdir");
    let out_dir = temp.path().join("missing-out");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg("does-not-exist.png")
        .arg("--out")
        .arg(&out_dir)
        .arg("--resume")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Resume requires an existing output directory")
                .and(predicate::str::contains(
                    "Run the same command without `--resume`",
                ))
                .and(predicate::str::contains("does-not-exist.png").not()),
        );
}

#[test]
fn inspect_rejects_non_image_input_with_actionable_error() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("note.txt");
    fs::write(&input, "hello").expect("write temp file");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("inspect")
        .arg(&input)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Input is not a supported image file")
                .and(predicate::str::contains(".png"))
                .and(predicate::str::contains("Details:")),
        );
}

#[test]
fn clap_rejects_resume_and_overwrite_together() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("dummy.txt");
    fs::write(&input, "hello").expect("write temp file");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg("out")
        .arg("--resume")
        .arg("--overwrite")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "the argument '--resume' cannot be used with '--overwrite'",
        ));
}

#[test]
fn clap_requires_world_units_when_origin_is_set() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("dummy.txt");
    fs::write(&input, "hello").expect("write temp file");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg("out")
        .arg("--world-origin")
        .arg("1,2")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--units-per-pixel"));
}

#[test]
fn clap_requires_skip_empty_for_alpha_threshold() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("dummy.txt");
    fs::write(&input, "hello").expect("write temp file");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg("out")
        .arg("--empty-alpha-threshold")
        .arg("4")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--skip-empty"));
}

#[cfg(not(feature = "vips"))]
#[test]
fn cut_reports_missing_vips_feature_clearly() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("vips.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 32, 32, |_, _| [255, 0, 0, 255]);

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--backend")
        .arg("vips")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("The `vips` backend is not available in this build.")
                .and(predicate::str::contains("--features vips")),
        );
}

#[test]
fn cut_reports_jpeg_transparency_requirement() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("alpha.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 32, 32, |x, y| {
        if x == 0 && y == 0 {
            [255, 0, 0, 0]
        } else {
            [255, 0, 0, 255]
        }
    });

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("16")
        .arg("--format")
        .arg("jpeg")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("JPEG output requires opaque tiles.")
                .and(predicate::str::contains("--flatten-alpha"))
                .and(predicate::str::contains("--format png")),
        );
}

#[test]
fn stitch_rejects_non_png_output() {
    let temp = TempDir::new().expect("tempdir");
    let manifest = temp.path().join("manifest.json");
    fs::write(&manifest, "{}").expect("write manifest placeholder");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("stitch")
        .arg(&manifest)
        .arg("--out")
        .arg(temp.path().join("verify.jpg"))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Stitch output must use a `.png` file extension",
        ));
}

#[test]
fn cut_generates_compact_manifest_and_padded_edges() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("compact.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 513, 513, |x, y| {
        if x == 512 && y == 512 {
            [10, 20, 30, 255]
        } else {
            [0, 0, 0, 0]
        }
    });

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("256")
        .arg("--overview")
        .arg("128")
        .arg("--world-origin")
        .arg("-10,20")
        .arg("--units-per-pixel")
        .arg("0.5")
        .assert()
        .success();

    let manifest = read_manifest(&out_dir);
    assert_eq!(manifest["grid"]["cols"], 3);
    assert_eq!(manifest["grid"]["rows"], 3);
    assert_eq!(manifest["stats"]["tile_count"], 9);
    assert_eq!(manifest["stats"]["skipped_count"], 0);
    assert_eq!(manifest["world"]["origin"][0], -10.0);
    assert!(manifest.get("tiles").is_none());

    let edge_tile = out_dir.join("tiles/x0002_y0002.png");
    assert!(edge_tile.exists());
    assert_eq!(
        image::image_dimensions(&edge_tile).expect("dimensions"),
        (256, 256)
    );
    let edge_pixels = image::open(&edge_tile).expect("edge tile").to_rgba8();
    assert_eq!(edge_pixels.get_pixel(0, 0).0, [10, 20, 30, 255]);
    assert_eq!(edge_pixels.get_pixel(1, 1).0, [0, 0, 0, 0]);
    assert!(out_dir.join("preview/overview.png").exists());

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("validate")
        .arg(out_dir.join("manifest.json"))
        .assert()
        .success();
}

#[test]
fn cut_generates_multi_level_manifest_and_stitch_matches_levels() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("pyramid.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 8, 8, |x, y| [x as u8 * 17, y as u8 * 23, 40, 255]);

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("4")
        .arg("--max-level")
        .arg("1")
        .arg("--tile-index")
        .arg("ndjson")
        .assert()
        .success();

    let manifest = read_manifest(&out_dir);
    assert_eq!(manifest["schema_version"], "2.0.0");
    assert_eq!(manifest["levels"].as_array().expect("levels").len(), 2);
    assert_eq!(manifest["levels"][0]["tile_count"], 4);
    assert_eq!(manifest["levels"][1]["tile_count"], 1);
    assert_eq!(manifest["stats"]["total_slots"], 5);
    assert_eq!(
        manifest["naming"]["path_template"],
        "tiles/l{level}/x{x}_y{y}.png"
    );
    assert!(out_dir.join("tiles/l0000/x0000_y0000.png").exists());
    assert!(out_dir.join("tiles/l0001/x0000_y0000.png").exists());

    let ndjson = fs::read_to_string(out_dir.join("tiles.ndjson")).expect("ndjson exists");
    assert!(ndjson.lines().any(|line| line.contains("\"level\":1")));

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("validate")
        .arg(out_dir.join("manifest.json"))
        .assert()
        .success();

    let stitched_level0 = temp.path().join("stitched-level0.png");
    let stitched_level1 = temp.path().join("stitched-level1.png");
    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("stitch")
        .arg(out_dir.join("manifest.json"))
        .arg("--out")
        .arg(&stitched_level0)
        .assert()
        .success();
    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("stitch")
        .arg(out_dir.join("manifest.json"))
        .arg("--out")
        .arg(&stitched_level1)
        .arg("--level")
        .arg("1")
        .assert()
        .success();

    let original = read_rgba(&input);
    let stitched0 = read_rgba(&stitched_level0);
    assert_eq!(stitched0, original);

    let expected_level1 = resize_rgba_to_dimensions(&original, 4, 4);
    let stitched1 = read_rgba(&stitched_level1);
    assert_eq!(stitched1, expected_level1);
}

#[test]
fn cut_generates_multi_level_sharded_paths() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("sharded-pyramid.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 8, 8, |x, y| [x as u8, y as u8, 120, 255]);

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("4")
        .arg("--max-level")
        .arg("1")
        .arg("--layout")
        .arg("sharded")
        .assert()
        .success();

    assert!(out_dir.join("tiles/l0000/y0000/x0000.png").exists());
    assert!(out_dir.join("tiles/l0001/y0000/x0000.png").exists());
}

#[test]
fn cut_generates_full_manifest_and_ndjson_for_skip_empty() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("sparse.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 513, 513, |x, y| {
        if x < 16 && y < 16 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 0]
        }
    });

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("256")
        .arg("--manifest")
        .arg("full")
        .arg("--tile-index")
        .arg("ndjson")
        .arg("--skip-empty")
        .assert()
        .success();

    let manifest = read_manifest(&out_dir);
    assert_eq!(manifest["stats"]["tile_count"], 1);
    assert_eq!(manifest["stats"]["skipped_count"], 8);
    assert_eq!(manifest["tiles"].as_array().expect("tiles").len(), 9);
    assert_eq!(manifest["index"]["path"], "tiles.ndjson");
    assert!(!out_dir.join("tiles/x0002_y0002.png").exists());

    let ndjson = fs::read_to_string(out_dir.join("tiles.ndjson")).expect("ndjson exists");
    let lines = ndjson.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let tile: serde_json::Value = serde_json::from_str(lines[0]).expect("ndjson line");
    assert_eq!(tile["x"], 0);
    assert_eq!(tile["y"], 0);

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("validate")
        .arg(out_dir.join("manifest.json"))
        .assert()
        .success();
}

#[test]
fn stitch_keeps_skipped_tiles_transparent() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("skip-stitch.png");
    let out_dir = temp.path().join("out");
    let stitched = temp.path().join("stitched.png");
    write_rgba_fixture(&input, 8, 8, |x, y| {
        if x < 4 && y < 4 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 0]
        }
    });

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("4")
        .arg("--skip-empty")
        .assert()
        .success();

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("stitch")
        .arg(out_dir.join("manifest.json"))
        .arg("--out")
        .arg(&stitched)
        .assert()
        .success();

    let stitched = read_rgba(&stitched);
    assert_eq!(stitched.get_pixel(6, 6).0, [0, 0, 0, 0]);
    assert_eq!(stitched.get_pixel(1, 1).0, [255, 255, 255, 255]);
}

#[test]
fn resume_rebuilds_missing_tiles() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("resume.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 512, 512, |x, y| {
        [(x % 255) as u8, (y % 255) as u8, 64, 255]
    });

    let mut base = Command::cargo_bin("tilecut").expect("binary");
    base.arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("256")
        .assert()
        .success();

    fs::remove_file(out_dir.join("tiles/x0001_y0001.png")).expect("remove tile");
    fs::remove_file(out_dir.join("manifest.json")).expect("remove manifest");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("256")
        .arg("--resume")
        .assert()
        .success();

    assert!(out_dir.join("tiles/x0001_y0001.png").exists());
    assert!(out_dir.join("manifest.json").exists());
}

#[test]
fn validate_reports_invalid_level_metadata() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("invalid-levels.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 8, 8, |x, y| [x as u8, y as u8, 200, 255]);

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("4")
        .arg("--max-level")
        .arg("1")
        .assert()
        .success();

    let manifest_path = out_dir.join("manifest.json");
    let mut manifest = read_manifest(&out_dir);
    manifest["levels"][1]["scale"] = serde_json::json!(0.4);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("rewrite manifest");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("validate")
        .arg(&manifest_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("level 1 has invalid scale"));
}

#[test]
fn validate_fails_when_tile_file_is_missing() {
    let temp = TempDir::new().expect("tempdir");
    let input = temp.path().join("missing.png");
    let out_dir = temp.path().join("out");
    write_rgba_fixture(&input, 512, 512, |_, _| [0, 255, 0, 255]);

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("cut")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .arg("--tile-size")
        .arg("256")
        .assert()
        .success();

    fs::remove_file(out_dir.join("tiles/x0000_y0000.png")).expect("remove tile");

    Command::cargo_bin("tilecut")
        .expect("binary")
        .arg("validate")
        .arg(out_dir.join("manifest.json"))
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Manifest validation failed")
                .and(predicate::str::contains("missing tile file"))
                .and(predicate::str::contains(
                    "Re-run `tilecut cut` with `--overwrite`",
                )),
        );
}
