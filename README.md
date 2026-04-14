# TileCut

[中文](README.md) | [English](README.en.md)

TileCut 是一个面向游戏小地图和离线资源构建流程的 Rust CLI 工具，用来把一张大图切成固定尺寸的瓦片，并输出一份稳定、可版本化的 `manifest.json`，供别的工具或游戏项目按需加载、定位和校验。

当前版本已经支持：

- `inspect`：检查输入图片尺寸、网格数量、估算内存占用并推荐后端
- `cut`：执行切图，输出 tile、manifest、可选 overview 和索引
- `stitch`：按 manifest 重建指定 level 的 PNG 预览
- `validate`：校验 manifest、索引和输出文件是否一致
- `compact` / `full` manifest
- `tiles.ndjson` 流式索引
- `overwrite` / `resume`
- `skip-empty`
- 多级缩放 / pyramid（`--max-level`）
- world 坐标映射
- 静态地图验收 demo（固定读取 `demo/data/manifest.json`）
- `image` 默认后端
- `vips` 可选后端

## 设计目标

- 默认行为适合游戏小地图资源构建
- 输出结构稳定，适合版本管理和回归测试
- 支持固定尺寸 tile，边缘默认 `pad`
- manifest 默认紧凑，避免超大地图时 JSON 过大
- 为超大图处理预留 `vips` 后端

## 当前范围

本项目当前尚未包含：

- 游戏引擎专用资源格式输出

## 环境要求

- Rust 工具链
- Linux 优先
- 如需 `vips` 后端：
  - 构建时启用 `--features vips`
  - 运行环境中安装 `vips` / `libvips`

## 构建

默认构建：

```bash
cargo build
```

启用 `vips` 后端：

```bash
cargo build --features vips
```

## 快速开始

检查输入图片：

```bash
cargo run -- inspect world_map.png --tile-size 256
```

输出 JSON 检查报告：

```bash
cargo run -- inspect world_map.png --tile-size 256 --json
```

查看多级缩放规划：

```bash
cargo run -- inspect world_map.png --tile-size 256 --max-level 2 --json
```

执行切图：

```bash
cargo run -- cut world_map.png \
  --out build/minimap \
  --tile-size 256 \
  --overview 2048 \
  --world-origin=-4096,4096 \
  --units-per-pixel 0.5
```

输出完整 manifest 并启用 NDJSON 索引：

```bash
cargo run -- cut world_map.png \
  --out build/minimap \
  --tile-size 256 \
  --max-level 2 \
  --manifest full \
  --tile-index ndjson
```

跳过透明空白 tile：

```bash
cargo run -- cut world_map.png \
  --out build/minimap \
  --tile-size 256 \
  --skip-empty \
  --empty-alpha-threshold 0
```

校验输出：

```bash
cargo run -- validate build/minimap/manifest.json
```

拼回某一层做检查：

```bash
cargo run -- stitch build/minimap/manifest.json --out verify.png --level 1
```

## 静态 Demo

仓库内置了一个纯静态的地图验收页，固定读取 `demo/data/manifest.json`。

推荐先生成一份带 pyramid、overview 和索引的输出：

```bash
cargo run -- cut world_map.png \
  --out demo/data \
  --tile-size 256 \
  --max-level 2 \
  --overview 1024 \
  --tile-index ndjson
```

如果你想验证 `full` manifest、`skip-empty`、`sharded` 布局或 world 坐标，也可以直接在这条命令上增加对应参数；demo 会按实际 `manifest.json` 自动读取。

启动方式：

```bash
cd demo
python -m http.server 8000
```

然后打开：

```text
http://127.0.0.1:8000
```

说明：

- 不支持 `file://` 直接打开 `index.html`，因为浏览器会拦截本地 `fetch`
- demo 默认支持 `compact` / `full` manifest、可选 `tiles.ndjson`、`flat` / `sharded` 布局、多级 pyramid、world 坐标和 overview
- 详细的手工验收清单见 [`demo/README.md`](demo/README.md)

## 命令说明

### `tilecut inspect <input>`

用于查看输入图的基础信息和切图规划结果。

常用参数：

- `--tile-size <N>`：设置正方形 tile 尺寸，默认 `256`
- `--tile-width <N>` / `--tile-height <N>`：设置矩形 tile 尺寸
- `--edge <pad|crop|skip>`：边缘策略，默认 `pad`
- `--max-level <N>`：输出从 `level 0` 到 `level N` 的 1/2 pyramid
- `--max-in-memory-mib <N>`：`backend=auto` 的内存阈值参考，默认 `2048`
- `--json`：输出 JSON

### `tilecut cut <input> --out <dir>`

执行切图并写出结果。

常用参数：

- `--tile-size <N>`：正方形 tile 尺寸
- `--tile-width <N>` / `--tile-height <N>`：矩形 tile 尺寸
- `--format <png|jpeg|webp>`：输出格式，默认 `png`
- `--quality <1..100>`：`jpeg/webp` 质量，默认 `90`
- `--edge <pad|crop|skip>`：边缘策略，默认 `pad`
- `--pad-color r,g,b,a`：补边颜色，默认 `0,0,0,0`
- `--layout <flat|sharded>`：输出目录布局
- `--max-level <N>`：输出从 `level 0` 到 `level N` 的 1/2 pyramid
- `--manifest <compact|full>`：manifest 模式
- `--tile-index <none|ndjson>`：附加 tile 索引
- `--backend <auto|image|vips>`：后端选择
- `--threads <N>`：并发线程数
- `--overview <N>`：输出 `preview/overview.png`，长边最大值为 `N`
- `--overwrite`：覆盖已有输出目录
- `--resume`：从已有输出目录恢复构建
- `--skip-empty`：跳过透明空白 tile
- `--empty-alpha-threshold <0..255>`：判定空白 tile 的 alpha 阈值
- `--world-origin x,y`：原图左上角对应的世界坐标
- `--units-per-pixel <F64>`：每像素对应世界单位
- `--y-axis <down|up>`：世界坐标 y 方向，默认 `down`
- `--flatten-alpha r,g,b,a`：导出 `jpeg` 时用于铺底
- `--dry-run`：只输出规划，不写文件
- `--max-in-memory-mib <N>`：`backend=auto` 的内存阈值

说明：

- `--resume` 与 `--overwrite` 不能同时使用
- `jpeg` 不支持透明；如果 tile 含透明像素，默认会报错，需显式传 `--flatten-alpha`
- 当 `compact manifest + --skip-empty` 同时开启时，工具会自动输出 `tiles.ndjson`

### `tilecut stitch <manifest> --out <png>`

用于把 manifest 中的某一层重新拼接成单张 PNG，方便核对切图与层级缩放结果。

常用参数：

- `--out <PNG>`：输出 PNG 文件
- `--level <N>`：要拼接的层级，默认 `0`

### `tilecut validate <manifest>`

用于检查：

- manifest 结构是否合法
- tile 数量和网格信息是否一致
- `tiles.ndjson` 是否可解析
- 应存在的 tile 文件是否真的存在
- tile 尺寸是否与 manifest 中的规则一致

可加 `--json` 输出结构化报告。

## 输出目录结构

默认 `flat` 布局：

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

`sharded` 布局：

```text
build/minimap/
  manifest.json
  tiles/
    y0000/
      x0000.png
      x0001.png
```

多级缩放时会按 level 增加一层目录，例如：

```text
build/minimap/
  tiles/
    l0000/
      x0000_y0000.png
    l0001/
      x0000_y0000.png
```

命名规则默认使用：

- 左上角为 `(0, 0)`
- `x` 从左到右递增
- `y` 从上到下递增
- 坐标默认零填充，最少 4 位

## Manifest 说明

`manifest.json` 的核心字段包括：

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

其中：

- `compact` 模式默认不展开所有 tile，只保存规则和统计信息
- `full` 模式会在 `tiles` 字段中写出每个 tile 的 `level`、`src_rect`、`content_rect`、路径和 `skipped` 状态
- 启用 `ndjson` 时会额外输出 `tiles.ndjson`
- `levels` 现在记录每一层的尺寸、网格和统计信息

## 后端说明

### `image` 后端

- 默认可用
- 整图解码到内存后并发裁切
- 适合中小规模图片

### `vips` 后端

- 需要 `--features vips`
- 运行时要求系统已安装 `vips`
- 更适合超大图

`backend=auto` 时，工具会根据 `width * height * 4` 估算 RGBA 内存占用，并与 `--max-in-memory-mib` 对比：

- 未超过阈值时优先使用 `image`
- 超过阈值时尝试使用 `vips`
- 若 `vips` 未启用或不可用，会给出明确错误提示

## 开发与测试

格式化检查：

```bash
cargo fmt --check
```

静态检查：

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

测试：

```bash
cargo test
cargo test --all-features
```

## 项目结构

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

## 注意事项

- 当前实现是 `Linux First`
- `vips` feature 可编译，但实际使用前需要先在系统里安装 `vips`
- `resume` 依赖输出目录中的 `.tilecut/plan.json` 与当前参数完全匹配
- `skip-empty` 目前按 alpha 阈值判断空白块

## License

MIT，见 [LICENSE](LICENSE)。
