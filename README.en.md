# TileCut

[中文](README.md) | [English](README.en.md)

TileCut is a Rust CLI tool for game minimap pipelines and offline asset builds. It cuts a large image into fixed-size tiles and writes a stable, versionable `manifest.json` that other tools or game projects can load, position, and validate.

Current features include:

- `inspect`: inspect source image size, grid counts, estimated memory usage, and backend recommendation
- `cut`: generate tiles, a manifest, an optional overview image, and optional indexes
- `stitch`: rebuild a PNG preview for a specific manifest level
- `validate`: verify that the manifest, index, and generated output are consistent
- `compact` / `full` manifest modes
- streaming `tiles.ndjson` index
- `overwrite` / `resume`
- `skip-empty`
- multi-level pyramid output (`--max-level`)
- world coordinate mapping
- static validation demo (always reads `demo/data/manifest.json`)
- default `image` backend
- optional `vips` backend

## Design Goals

- sensible defaults for game minimap asset generation
- stable output structure that works well with version control and regression checks
- fixed-size tiles with `pad` as the default edge behavior
- compact manifests by default to avoid huge JSON files on very large maps
- room to scale to huge images via the `vips` backend

## Current Scope

This project does not currently include:

- engine-specific asset formats

## Requirements

- Rust toolchain
- Linux-first environment
- For the `vips` backend:
  - build with `--features vips`
  - install `vips` / `libvips` in the runtime environment

## Build

Default build:

```bash
cargo build
```

Build with `vips` enabled:

```bash
cargo build --features vips
```

## Quick Start

Inspect an input image:

```bash
cargo run -- inspect world_map.png --tile-size 256
```

Print the inspection report as JSON:

```bash
cargo run -- inspect world_map.png --tile-size 256 --json
```

Preview a multi-level pyramid plan:

```bash
cargo run -- inspect world_map.png --tile-size 256 --max-level 2 --json
```

Cut tiles:

```bash
cargo run -- cut world_map.png \
  --out build/minimap \
  --tile-size 256 \
  --overview 2048 \
  --world-origin=-4096,4096 \
  --units-per-pixel 0.5
```

Write a full manifest and enable the NDJSON index:

```bash
cargo run -- cut world_map.png \
  --out build/minimap \
  --tile-size 256 \
  --max-level 2 \
  --manifest full \
  --tile-index ndjson
```

Skip fully transparent tiles:

```bash
cargo run -- cut world_map.png \
  --out build/minimap \
  --tile-size 256 \
  --skip-empty \
  --empty-alpha-threshold 0
```

Validate generated output:

```bash
cargo run -- validate build/minimap/manifest.json
```

Stitch one level back into a verification image:

```bash
cargo run -- stitch build/minimap/manifest.json --out verify.png --level 1
```

## Static Demo

The repository ships with a fully static validation demo that always reads `demo/data/manifest.json`.

Recommended command for producing demo data with a pyramid, overview, and index:

```bash
cargo run -- cut world_map.png \
  --out demo/data \
  --tile-size 256 \
  --max-level 2 \
  --overview 1024 \
  --tile-index ndjson
```

If you want to validate `full` manifests, `skip-empty`, `sharded` layout, or world coordinates, just add those flags to the same command. The demo reads the actual `manifest.json` that was generated.

Start the demo with a simple static server:

```bash
cd demo
python -m http.server 8000
```

Then open:

```text
http://127.0.0.1:8000
```

Notes:

- Do not open `index.html` via `file://`; the browser will block local `fetch` requests
- The demo supports `compact` / `full` manifests, optional `tiles.ndjson`, `flat` / `sharded` layout, multi-level pyramids, world coordinates, and overview images
- See [`demo/README.md`](demo/README.md) for the full manual validation checklist

## Command Reference

### `tilecut inspect <input>`

Inspect the source image and preview the cut plan without writing output files.

Common options:

- `--tile-size <N>`: set a square tile size, default `256`
- `--tile-width <N>` / `--tile-height <N>`: set rectangular tile dimensions
- `--edge <pad|crop|skip>`: edge behavior, default `pad`
- `--max-level <N>`: build a 1/2 pyramid from `level 0` through `level N`
- `--max-in-memory-mib <N>`: memory budget used by `backend=auto`, default `2048`
- `--json`: print JSON output

### `tilecut cut <input> --out <dir>`

Cut tiles and write generated output.

Common options:

- `--tile-size <N>`: square tile size
- `--tile-width <N>` / `--tile-height <N>`: rectangular tile dimensions
- `--format <png|jpeg|webp>`: output format, default `png`
- `--quality <1..100>`: `jpeg/webp` quality, default `90`
- `--edge <pad|crop|skip>`: edge behavior, default `pad`
- `--pad-color r,g,b,a`: padding color, default `0,0,0,0`
- `--layout <flat|sharded>`: output directory layout
- `--max-level <N>`: build a 1/2 pyramid from `level 0` through `level N`
- `--manifest <compact|full>`: manifest mode
- `--tile-index <none|ndjson>`: optional tile index
- `--backend <auto|image|vips>`: backend selection
- `--threads <N>`: worker thread count
- `--overview <N>`: write `preview/overview.png` with a maximum edge length of `N`
- `--overwrite`: replace an existing output directory
- `--resume`: resume from an existing output directory
- `--skip-empty`: skip transparent empty tiles
- `--empty-alpha-threshold <0..255>`: alpha threshold for empty tile detection
- `--world-origin x,y`: world-space coordinate of the source image top-left corner
- `--units-per-pixel <F64>`: world units per source pixel
- `--y-axis <down|up>`: world-space Y direction, default `down`
- `--flatten-alpha r,g,b,a`: background color for JPEG flattening
- `--dry-run`: print the plan without writing files
- `--max-in-memory-mib <N>`: memory budget used by `backend=auto`

Notes:

- `--resume` and `--overwrite` cannot be used together
- JPEG does not support transparency; if a tile contains transparent pixels the build fails unless you explicitly pass `--flatten-alpha`
- When `compact manifest + --skip-empty` are both enabled, TileCut automatically writes `tiles.ndjson`

### `tilecut stitch <manifest> --out <png>`

Rebuild a single PNG from one manifest level for verification of tiling and pyramid behavior.

Common options:

- `--out <PNG>`: output PNG path
- `--level <N>`: level to stitch, default `0`

### `tilecut validate <manifest>`

Check:

- whether the manifest structure is valid
- whether tile counts and grid metadata are consistent
- whether `tiles.ndjson` can be parsed
- whether expected tile files actually exist
- whether tile dimensions match manifest rules

Add `--json` for structured output.

## Output Layout

Default `flat` layout:

```text
build/minimap/
  manifest.json
  tiles/
    x0000_y0000.png
    x0001_y0000.png
    ...
  preview/
    overview.png
  .tilecut/
    plan.json
    state.json
```

`sharded` layout:

```text
build/minimap/
  manifest.json
  tiles/
    y0000/
      x0000.png
      x0001.png
```

For multi-level output, TileCut adds a level directory:

```text
build/minimap/
  tiles/
    l0000/
      x0000_y0000.png
    l0001/
      x0000_y0000.png
```

Default naming rules:

- top-left starts at `(0, 0)`
- `x` increases from left to right
- `y` increases from top to bottom
- coordinates are zero-padded to at least 4 digits

## Manifest

Core fields in `manifest.json`:

- `schema_version`
- `generator`
- `source`
- `tile`
- `grid`
- `naming`
- `world`
- `levels`
- `stats`
- `index`
- `tiles`

Notes:

- `compact` mode stores rules and statistics instead of expanding every tile
- `full` mode writes each tile record with `level`, `src_rect`, `content_rect`, path, and `skipped` state
- enabling `ndjson` writes an extra `tiles.ndjson`
- `levels` records per-level dimensions, grid info, and summary stats

## Backends

### `image` backend

- available by default
- decodes the full image into memory and cuts tiles in parallel
- suitable for small and medium images

### `vips` backend

- requires `--features vips`
- requires `vips` to be installed at runtime
- better suited for very large images

When `backend=auto` is used, TileCut estimates RGBA memory usage as `width * height * 4` and compares it to `--max-in-memory-mib`:

- below the threshold it prefers `image`
- above the threshold it tries `vips`
- if `vips` is not enabled or not available, TileCut prints a clear error

## Development and Testing

Formatting:

```bash
cargo fmt --check
```

Linting:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Testing:

```bash
cargo test
cargo test --all-features
```

## Project Structure

```text
src/
  backend/
    image_backend.rs
    vips_backend.rs
  cmd/
    cut.rs
    inspect.rs
    validate.rs
  cli.rs
  coords.rs
  manifest.rs
  naming.rs
  overview.rs
  plan.rs
  validate.rs
  lib.rs
  main.rs
tests/
  cli.rs
```

## Notes

- the current implementation is Linux-first
- the `vips` feature can compile, but you still need to install `vips` before using it at runtime
- `resume` requires the output directory's `.tilecut/plan.json` to exactly match the current build parameters
- `skip-empty` currently uses the alpha threshold to decide whether a tile is empty

## License

MIT. See [LICENSE](LICENSE).
