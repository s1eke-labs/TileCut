# TileCut Demo

这个目录提供一个纯静态的 TileCut 验收页，用来在浏览器里检查生成结果是否正确。

## 固定目录约定

demo 只认一个数据根目录：`demo/data/`。

推荐直接把 TileCut 输出写到这里：

```bash
cargo run -- cut world_map.png \
  --out demo/data \
  --tile-size 256 \
  --max-level 2 \
  --overview 1024 \
  --tile-index ndjson
```

如果你想验证 `skip-empty`、`full` manifest 或 `sharded` 布局，也可以把相应参数加到这条命令上；demo 会按 `manifest.json` 自动读取。

## 启动方式

进入 `demo/` 目录后，用 Python 的静态服务器启动：

```bash
cd demo
python -m http.server 8000
```

然后打开：

```text
http://127.0.0.1:8000
```

不要直接双击 `index.html` 用 `file://` 打开，因为浏览器会拦截本地 `fetch`。

## 可以验证的内容

- 拖拽平移、滚轮缩放、Fit、100% 像素视图和重置视图
- 自动或手动切换 pyramid level
- grid、tile label、skipped tile 和 debug 信息开关
- compact/full manifest、可选 `tiles.ndjson`
- flat/sharded 布局
- `pad` / `crop` / `skip` 边缘模式
- world 坐标映射
- `preview/overview.png` overview 快速定位
- 缺失 tile 或损坏 tile 的错误占位

## 建议手工验收清单

至少覆盖下面这些场景：

1. `full` manifest + 单层 + `flat`
2. `compact` + `tiles.ndjson` + `--skip-empty`
3. 多层 pyramid + `sharded`
4. `pad` 边缘
5. `crop` 边缘
6. 带 `world-origin` / `units-per-pixel` 和 overview
7. 人为删除一个 tile 文件，确认页面出现红色错误占位
