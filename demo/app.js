const DATA_ROOT = "./data";
const MANIFEST_URL = `${DATA_ROOT}/manifest.json`;
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 16;
const FIT_PADDING = 64;

class TilecutDemo {
  constructor() {
    this.canvas = document.getElementById("mapCanvas");
    this.canvasWrap = document.getElementById("canvasWrap");
    this.ctx = this.canvas.getContext("2d");
    this.datasetSummary = document.getElementById("datasetSummary");
    this.viewSummary = document.getElementById("viewSummary");
    this.healthSummary = document.getElementById("healthSummary");
    this.hoverSummary = document.getElementById("hoverSummary");
    this.levelSelect = document.getElementById("levelSelect");
    this.gridToggle = document.getElementById("gridToggle");
    this.labelsToggle = document.getElementById("labelsToggle");
    this.skippedToggle = document.getElementById("skippedToggle");
    this.debugToggle = document.getElementById("debugToggle");
    this.statusTitle = document.getElementById("statusTitle");
    this.statusMessage = document.getElementById("statusMessage");
    this.statusDetails = document.getElementById("statusDetails");
    this.canvasOverlay = document.getElementById("canvasOverlay");
    this.overviewCard = document.getElementById("overviewCard");
    this.overviewImage = document.getElementById("overviewImage");
    this.overviewViewport = document.getElementById("overviewViewport");
    this.overviewButton = document.getElementById("overviewButton");
    this.fullscreenButton = document.getElementById("fullscreenButton");

    this.state = {
      manifest: null,
      manifestUrl: new URL(MANIFEST_URL, window.location.href),
      imageBaseUrl: new URL(`${DATA_ROOT}/`, window.location.href),
      overviewUrl: new URL(`${DATA_ROOT}/preview/overview.png`, window.location.href),
      entriesByLevel: new Map(),
      entryMapsByLevel: new Map(),
      currentLevelKey: "auto",
      centerX: 0,
      centerY: 0,
      zoom: 1,
      dragPointerId: null,
      dragStart: null,
      hovering: null,
      brokenTiles: new Set(),
      loadingTiles: new Set(),
      hasOverview: false,
      ready: false,
      userAdjustedView: false,
      dpr: window.devicePixelRatio || 1,
    };

    this.imageCache = new Map();
    this.needsRender = false;

    this.bindEvents();
    this.showStatus(
      "Preparing viewer",
      "Loading ./data/manifest.json and validating the TileCut output."
    );
    this.resizeCanvas();
    this.load().catch((error) => {
      this.fail(error);
    });
  }

  bindEvents() {
    window.addEventListener("resize", () => {
      this.handleViewportResize();
    });

    this.resizeObserver = new ResizeObserver(() => {
      this.handleViewportResize();
    });
    this.resizeObserver.observe(this.canvasWrap);

    this.canvas.addEventListener("pointerdown", (event) => {
      if (!this.state.ready) {
        return;
      }
      this.canvas.setPointerCapture(event.pointerId);
      this.canvas.classList.add("dragging");
      this.state.dragPointerId = event.pointerId;
      this.state.userAdjustedView = true;
      this.state.dragStart = {
        x: event.clientX,
        y: event.clientY,
        centerX: this.state.centerX,
        centerY: this.state.centerY,
      };
    });

    this.canvas.addEventListener("pointermove", (event) => {
      if (!this.state.ready) {
        return;
      }

      if (this.state.dragPointerId === event.pointerId && this.state.dragStart) {
        const dx = (event.clientX - this.state.dragStart.x) / this.state.zoom;
        const dy = (event.clientY - this.state.dragStart.y) / this.state.zoom;
        this.state.centerX = this.state.dragStart.centerX - dx;
        this.state.centerY = this.state.dragStart.centerY - dy;
        this.constrainCamera();
        this.requestRender();
      }

      this.updateHover(event);
    });

    this.canvas.addEventListener("pointerup", (event) => {
      if (this.state.dragPointerId !== event.pointerId) {
        return;
      }
      this.endDrag();
    });

    this.canvas.addEventListener("pointercancel", () => {
      this.endDrag();
    });

    this.canvas.addEventListener(
      "wheel",
      (event) => {
        if (!this.state.ready) {
          return;
        }
        event.preventDefault();
        this.state.userAdjustedView = true;
        const factor = event.deltaY > 0 ? 0.88 : 1.14;
        this.zoomAroundPoint(factor, event.offsetX, event.offsetY);
      },
      { passive: false }
    );

    this.canvas.addEventListener("mouseleave", () => {
      this.state.hovering = null;
      this.renderInspector();
      this.requestRender();
    });

    document.getElementById("zoomInButton").addEventListener("click", () => {
      if (!this.state.ready) {
        return;
      }
      this.state.userAdjustedView = true;
      this.zoomAroundPoint(1.2, this.canvas.clientWidth / 2, this.canvas.clientHeight / 2);
    });

    document.getElementById("zoomOutButton").addEventListener("click", () => {
      if (!this.state.ready) {
        return;
      }
      this.state.userAdjustedView = true;
      this.zoomAroundPoint(0.84, this.canvas.clientWidth / 2, this.canvas.clientHeight / 2);
    });

    document.getElementById("fitButton").addEventListener("click", () => {
      if (!this.state.ready) {
        return;
      }
      this.state.userAdjustedView = false;
      this.fitToView();
    });

    document.getElementById("overviewFitButton").addEventListener("click", () => {
      if (!this.state.ready) {
        return;
      }
      this.state.userAdjustedView = false;
      this.fitToView();
    });

    document.getElementById("pixelButton").addEventListener("click", () => {
      if (!this.state.ready) {
        return;
      }
      this.state.userAdjustedView = true;
      this.state.zoom = 1;
      this.constrainCamera();
      this.requestRender();
      this.renderViewSummary();
    });

    document.getElementById("resetButton").addEventListener("click", () => {
      if (!this.state.ready) {
        return;
      }
      this.state.userAdjustedView = false;
      this.state.currentLevelKey = "auto";
      this.levelSelect.value = "auto";
      this.fitToView();
    });

    this.fullscreenButton.addEventListener("click", async () => {
      const root = document.documentElement;
      if (document.fullscreenElement) {
        await document.exitFullscreen();
        return;
      }
      await root.requestFullscreen();
    });

    this.levelSelect.addEventListener("change", () => {
      this.state.userAdjustedView = this.levelSelect.value !== "auto";
      this.state.currentLevelKey = this.levelSelect.value;
      this.requestRender();
      this.renderViewSummary();
    });

    [
      this.gridToggle,
      this.labelsToggle,
      this.skippedToggle,
      this.debugToggle,
    ].forEach((element) => {
      element.addEventListener("change", () => {
        this.renderInspector();
        this.renderViewSummary();
        this.requestRender();
      });
    });

    this.overviewButton.addEventListener("click", (event) => {
      if (!this.state.ready || !this.state.hasOverview) {
        return;
      }
      const rect = this.overviewButton.getBoundingClientRect();
      const x = (event.clientX - rect.left) / rect.width;
      const y = (event.clientY - rect.top) / rect.height;
      const { width, height } = this.baseDimensions();
      this.state.userAdjustedView = true;
      this.state.centerX = width * x;
      this.state.centerY = height * y;
      this.constrainCamera();
      this.requestRender();
      this.renderViewSummary();
    });
  }

  async load() {
    const manifest = await this.fetchJson(this.state.manifestUrl, "Failed to load manifest.json");
    this.validateManifestShape(manifest);

    const inventory = await this.collectInventory(manifest);
    const entriesByLevel = groupByLevel(inventory);
    const entryMapsByLevel = new Map();
    for (const [level, entries] of entriesByLevel.entries()) {
      entryMapsByLevel.set(
        level,
        new Map(entries.map((entry) => [`${entry.x}:${entry.y}`, entry]))
      );
    }

    this.state.manifest = manifest;
    this.state.entriesByLevel = entriesByLevel;
    this.state.entryMapsByLevel = entryMapsByLevel;
    this.populateLevelSelect();
    this.renderDatasetSummary();
    this.renderHealthSummary();
    this.loadOverview();
    await this.waitForStableCanvas();
    this.resizeCanvas();
    this.fitToView();
    this.state.ready = true;
    this.hideStatus();
    this.renderInspector();
    this.requestRender();
  }

  async fetchJson(url, context) {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(
        `${context}: ${response.status} ${response.statusText}. Make sure you generated TileCut output into demo/data and started the demo via python -m http.server.`
      );
    }
    return response.json();
  }

  async collectInventory(manifest) {
    const fullSlots = deriveInventoryFromCompact(manifest);

    if (Array.isArray(manifest.tiles)) {
      return manifest.tiles
        .map((tile) => normalizeEntry(tile, manifest))
        .sort(byTileCoord);
    }

    if (manifest.index && manifest.index.mode === "ndjson") {
      const indexUrl = new URL(manifest.index.path, this.state.manifestUrl);
      const response = await fetch(indexUrl);
      if (!response.ok) {
        throw new Error(
          `Failed to load ${manifest.index.path}: ${response.status} ${response.statusText}`
        );
      }
      const raw = await response.text();
      const presentEntries = raw
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => {
          try {
            return normalizeEntry(JSON.parse(line), manifest);
          } catch (error) {
            throw new Error(`Failed to parse tiles.ndjson line: ${error.message}`);
          }
        });
      return mergeIndexInventory(fullSlots, presentEntries).sort(byTileCoord);
    }

    return fullSlots.sort(byTileCoord);
  }

  validateManifestShape(manifest) {
    if (!manifest || !Array.isArray(manifest.levels) || manifest.levels.length === 0) {
      throw new Error("manifest.json is missing a valid levels array.");
    }
    if (!manifest.source || typeof manifest.source.width !== "number") {
      throw new Error("manifest.json is missing source dimensions.");
    }
    if (!manifest.tile || typeof manifest.tile.width !== "number") {
      throw new Error("manifest.json is missing tile sizing information.");
    }
  }

  populateLevelSelect() {
    for (const level of this.state.manifest.levels) {
      const option = document.createElement("option");
      option.value = String(level.level);
      option.textContent = `Level ${level.level} (${formatScale(level.scale)})`;
      this.levelSelect.appendChild(option);
    }
  }

  async loadOverview() {
    try {
      const image = await this.loadImage(this.state.overviewUrl.href);
      this.overviewImage.src = image.src;
      this.overviewCard.classList.remove("hidden");
      this.state.hasOverview = true;
      this.updateOverviewViewport();
    } catch (_) {
      this.state.hasOverview = false;
      this.overviewCard.classList.add("hidden");
    }
  }

  async loadImage(src) {
    const cached = this.imageCache.get(src);
    if (cached?.promise) {
      return cached.promise;
    }

    const promise = new Promise((resolve, reject) => {
      const image = new Image();
      image.onload = () => {
        this.imageCache.set(src, { state: "loaded", image, promise });
        resolve(image);
      };
      image.onerror = () => {
        this.imageCache.set(src, { state: "error", promise });
        reject(new Error(`Failed to load image ${src}`));
      };
      image.src = src;
    });

    this.imageCache.set(src, { state: "loading", promise });
    return promise;
  }

  getTileImage(entry) {
    if (!entry.path) {
      return { state: "skipped" };
    }

    const url = new URL(entry.path, this.state.imageBaseUrl).href;
    let record = this.imageCache.get(url);
    if (!record) {
      const image = new Image();
      record = { state: "loading", image };
      this.imageCache.set(url, record);
      this.state.loadingTiles.add(entry.path);
      image.onload = () => {
        record.state = "loaded";
        this.state.loadingTiles.delete(entry.path);
        this.requestRender();
        this.renderHealthSummary();
      };
      image.onerror = () => {
        record.state = "error";
        this.state.loadingTiles.delete(entry.path);
        this.state.brokenTiles.add(entry.path);
        this.renderHealthSummary();
        this.requestRender();
      };
      image.src = url;
    }

    return record;
  }

  baseDimensions() {
    const manifest = this.state.manifest;
    return { width: manifest.source.width, height: manifest.source.height };
  }

  resizeCanvas() {
    const rect = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    this.state.dpr = dpr;
    this.canvas.width = Math.max(1, Math.round(rect.width * dpr));
    this.canvas.height = Math.max(1, Math.round(rect.height * dpr));
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  handleViewportResize() {
    this.resizeCanvas();
    if (!this.state.ready) {
      this.requestRender();
      return;
    }

    if (this.state.userAdjustedView) {
      this.constrainCamera();
    } else {
      this.fitToView();
      return;
    }

    this.requestRender();
  }

  async waitForStableCanvas() {
    for (let attempt = 0; attempt < 4; attempt += 1) {
      await nextFrame();
      await nextFrame();
      const rect = this.canvas.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) {
        return;
      }
    }
  }

  fitToView() {
    const { width, height } = this.baseDimensions();
    const viewWidth = Math.max(1, this.canvas.clientWidth - FIT_PADDING * 2);
    const viewHeight = Math.max(1, this.canvas.clientHeight - FIT_PADDING * 2);
    this.state.zoom = clamp(Math.min(viewWidth / width, viewHeight / height), MIN_ZOOM, MAX_ZOOM);
    this.state.centerX = width / 2;
    this.state.centerY = height / 2;
    this.constrainCamera();
    this.requestRender();
    this.renderViewSummary();
  }

  constrainCamera() {
    const { width, height } = this.baseDimensions();
    const halfVisibleWidth = this.canvas.clientWidth / (2 * this.state.zoom);
    const halfVisibleHeight = this.canvas.clientHeight / (2 * this.state.zoom);

    if (width <= halfVisibleWidth * 2) {
      this.state.centerX = width / 2;
    } else {
      this.state.centerX = clamp(this.state.centerX, halfVisibleWidth, width - halfVisibleWidth);
    }

    if (height <= halfVisibleHeight * 2) {
      this.state.centerY = height / 2;
    } else {
      this.state.centerY = clamp(this.state.centerY, halfVisibleHeight, height - halfVisibleHeight);
    }

    this.updateOverviewViewport();
  }

  zoomAroundPoint(factor, screenX, screenY) {
    const before = this.screenToBase(screenX, screenY);
    this.state.zoom = clamp(this.state.zoom * factor, MIN_ZOOM, MAX_ZOOM);
    const after = this.screenToBase(screenX, screenY);
    this.state.centerX += before.x - after.x;
    this.state.centerY += before.y - after.y;
    this.constrainCamera();
    this.renderViewSummary();
    this.requestRender();
  }

  screenToBase(screenX, screenY) {
    return {
      x: this.state.centerX + (screenX - this.canvas.clientWidth / 2) / this.state.zoom,
      y: this.state.centerY + (screenY - this.canvas.clientHeight / 2) / this.state.zoom,
    };
  }

  baseToScreen(baseX, baseY) {
    return {
      x: (baseX - this.state.centerX) * this.state.zoom + this.canvas.clientWidth / 2,
      y: (baseY - this.state.centerY) * this.state.zoom + this.canvas.clientHeight / 2,
    };
  }

  visibleBaseBounds() {
    const topLeft = this.screenToBase(0, 0);
    const bottomRight = this.screenToBase(this.canvas.clientWidth, this.canvas.clientHeight);
    return {
      left: topLeft.x,
      top: topLeft.y,
      right: bottomRight.x,
      bottom: bottomRight.y,
    };
  }

  activeLevel() {
    const manifest = this.state.manifest;
    if (this.state.currentLevelKey !== "auto") {
      return manifest.levels.find((level) => String(level.level) === this.state.currentLevelKey);
    }

    const ranked = [...manifest.levels].map((level) => ({
      level,
      delta: Math.abs(Math.log2(this.state.zoom / level.scale)),
    }));
    ranked.sort((left, right) => left.delta - right.delta);
    return ranked[0].level;
  }

  visibleEntries(level) {
    const entries = this.state.entriesByLevel.get(level.level) || [];
    const bounds = this.visibleBaseBounds();
    return entries.filter((entry) => {
      const rect = entry.baseRect;
      return (
        rect.x + rect.w >= bounds.left &&
        rect.y + rect.h >= bounds.top &&
        rect.x <= bounds.right &&
        rect.y <= bounds.bottom
      );
    });
  }

  updateHover(event) {
    const level = this.activeLevel();
    const basePoint = this.screenToBase(event.offsetX, event.offsetY);
    const levelX = Math.floor(basePoint.x * level.scale);
    const levelY = Math.floor(basePoint.y * level.scale);
    const tileX = Math.floor(levelX / this.state.manifest.tile.width);
    const tileY = Math.floor(levelY / this.state.manifest.tile.height);
    const entryMap = this.state.entryMapsByLevel.get(level.level);
    const entry = entryMap ? entryMap.get(`${tileX}:${tileY}`) : null;

    if (!entry || !pointInRect(basePoint, entry.baseRect)) {
      this.state.hovering = {
        basePoint,
        entry: null,
        level,
      };
    } else {
      this.state.hovering = {
        basePoint,
        entry,
        level,
      };
    }

    this.renderInspector();
    this.requestRender();
  }

  renderInspector() {
    if (!this.debugToggle.checked) {
      this.hoverSummary.innerHTML =
        '<p class="muted">Debug panel is disabled. Toggle "Debug" to inspect tile metadata.</p>';
      return;
    }

    const hover = this.state.hovering;
    if (!hover) {
      this.hoverSummary.innerHTML =
        '<p class="muted">Move the cursor over the map to inspect tile coordinates.</p>';
      return;
    }

    const basePixel = {
      x: clamp(Math.floor(hover.basePoint.x), 0, this.state.manifest.source.width - 1),
      y: clamp(Math.floor(hover.basePoint.y), 0, this.state.manifest.source.height - 1),
    };
    const world = this.worldForBasePixel(basePixel.x, basePixel.y);
    const tile = hover.entry;

    if (!tile) {
      this.hoverSummary.innerHTML = `
        <dl>
          ${statRow("Base pixel", `${basePixel.x}, ${basePixel.y}`)}
          ${statRow("Active level", hover.level.level)}
          ${statRow("World", world ?? "n/a")}
          ${statRow("Tile", "Outside tile bounds")}
        </dl>
      `;
      return;
    }

    this.hoverSummary.innerHTML = `
      <dl>
        ${statRow("Level", tile.level)}
        ${statRow("Tile", `${tile.x}, ${tile.y}`)}
        ${statRow("Skipped", tile.skipped ? "yes" : "no")}
        ${statRow("Path", tile.path ?? "n/a")}
        ${statRow("Base pixel", `${basePixel.x}, ${basePixel.y}`)}
        ${statRow("World", world ?? "n/a")}
        ${statRow("Src rect", rectToString(tile.srcRect))}
        ${statRow("Content rect", rectToString(tile.contentRect))}
      </dl>
    `;
  }

  worldForBasePixel(x, y) {
    const world = this.state.manifest.world;
    if (!world || !world.enabled) {
      return null;
    }
    const worldX = world.origin[0] + x * world.units_per_pixel;
    const deltaY = y * world.units_per_pixel;
    const worldY = world.y_axis === "up" ? world.origin[1] - deltaY : world.origin[1] + deltaY;
    return `${formatFloat(worldX)}, ${formatFloat(worldY)}`;
  }

  renderDatasetSummary() {
    const manifest = this.state.manifest;
    const levelList = manifest.levels.map((level) => `L${level.level}`).join(" / ");
    this.datasetSummary.innerHTML = `
      ${statCard(
        "Source",
        formatDimensions(manifest.source.width, manifest.source.height),
        `${manifest.source.format} source`,
        "",
        "stat-compact"
      )}
      ${statCard(
        "Tileset",
        formatDimensions(manifest.tile.width, manifest.tile.height),
        `${manifest.naming.layout} / ${manifest.tile.format}`,
        "",
        "stat-compact"
      )}
      ${statCard(
        "Pyramid",
        `${manifest.levels.length} levels`,
        levelList,
        "",
        "stat-tight"
      )}
      ${statCard(
        "Build",
        manifest.generator?.version ?? "unknown",
        `schema ${manifest.schema_version}`,
        `${manifest.stats.tile_count} tiles / ${manifest.index ? manifest.index.mode : "no index"} / ${manifest.world ? "world on" : "world off"}`,
        "stat-tight"
      )}
    `;
  }

  renderViewSummary() {
    if (!this.state.manifest) {
      return;
    }
    const activeLevel = this.activeLevel();
    const mode = this.state.currentLevelKey === "auto" ? "auto" : "manual";
    const visible = this.visibleBaseBounds();
    const visibleSize = {
      w: Math.max(0, Math.round(visible.right - visible.left)),
      h: Math.max(0, Math.round(visible.bottom - visible.top)),
    };

    this.viewSummary.innerHTML = `
      ${statCard("Zoom", `${this.state.zoom.toFixed(2)}x`, mode)}
      ${statCard("Level", `L${activeLevel.level}`, formatScale(activeLevel.scale))}
      ${statCard(
        "Center",
        `x ${formatNumber(Math.round(this.state.centerX))}`,
        `y ${formatNumber(Math.round(this.state.centerY))}`,
        "camera center",
        "stat-compact"
      )}
      ${statCard(
        "Visible",
        formatDimensions(visibleSize.w, visibleSize.h),
        "viewport span",
        "",
        "stat-compact"
      )}
      ${statCard("Overlay", this.gridToggle.checked ? "grid on" : "grid off", this.labelsToggle.checked ? "labels on" : "labels off", "", "stat-tight")}
      ${statCard("Inspect", this.skippedToggle.checked ? "skipped shown" : "skipped hidden", this.debugToggle.checked ? "debug on" : "debug off", "", "stat-tight")}
    `;
  }

  renderHealthSummary() {
    const manifest = this.state.manifest;
    const broken = this.state.brokenTiles.size;
    const loading = this.state.loadingTiles.size;
    const skipped = manifest ? manifest.stats.skipped_count : 0;
    this.healthSummary.innerHTML = `
      ${statCard("Render errors", String(broken), broken ? "check canvas" : "clean")}
      ${statCard("Loading", String(loading), "pending")}
      ${statCard("Skipped", String(skipped), "from manifest")}
      ${statCard("Overview", this.state.hasOverview ? "loaded" : "not present", this.state.hasOverview ? "available" : "not found", "", "stat-tight")}
    `;
  }

  updateOverviewViewport() {
    if (!this.state.hasOverview) {
      return;
    }
    const { width, height } = this.baseDimensions();
    const visible = this.visibleBaseBounds();
    const left = clamp(visible.left / width, 0, 1);
    const top = clamp(visible.top / height, 0, 1);
    const viewportWidth = clamp((visible.right - visible.left) / width, 0.02, 1);
    const viewportHeight = clamp((visible.bottom - visible.top) / height, 0.02, 1);
    this.overviewViewport.style.left = `${left * 100}%`;
    this.overviewViewport.style.top = `${top * 100}%`;
    this.overviewViewport.style.width = `${viewportWidth * 100}%`;
    this.overviewViewport.style.height = `${viewportHeight * 100}%`;
  }

  requestRender() {
    if (this.needsRender) {
      return;
    }
    this.needsRender = true;
    requestAnimationFrame(() => {
      this.needsRender = false;
      this.render();
    });
  }

  render() {
    const ctx = this.ctx;
    const width = this.canvas.clientWidth;
    const height = this.canvas.clientHeight;
    ctx.clearRect(0, 0, width, height);
    this.drawWorkbenchBackground(ctx, width, height);

    if (!this.state.ready) {
      return;
    }

    const activeLevel = this.activeLevel();
    const visibleEntries = this.visibleEntries(activeLevel);
    const bounds = this.visibleBaseBounds();

    this.drawMapBounds(ctx);

    for (const entry of visibleEntries) {
      if (entry.skipped) {
        if (this.skippedToggle.checked) {
          this.drawSkippedTile(ctx, entry);
        }
        continue;
      }

      const record = this.getTileImage(entry);
      if (record.state === "loaded") {
        this.drawTile(ctx, entry, record.image);
      } else if (record.state === "error") {
        this.drawBrokenTile(ctx, entry);
      } else {
        this.drawLoadingTile(ctx, entry);
      }
    }

    if (this.gridToggle.checked) {
      this.drawGrid(ctx, activeLevel);
    }

    if (this.labelsToggle.checked) {
      this.drawLabels(ctx, visibleEntries);
    }

    if (this.state.hovering?.entry) {
      this.drawHoverOutline(ctx, this.state.hovering.entry);
    }

    this.drawViewportBadge(ctx, activeLevel, bounds, visibleEntries.length);
    this.renderViewSummary();
    this.renderHealthSummary();
    this.updateOverviewViewport();
  }

  drawWorkbenchBackground(ctx, width, height) {
    const gradient = ctx.createLinearGradient(0, 0, 0, height);
    gradient.addColorStop(0, "#102035");
    gradient.addColorStop(1, "#080d13");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);

    ctx.strokeStyle = "rgba(255,255,255,0.04)";
    ctx.lineWidth = 1;
    for (let x = 0; x < width; x += 28) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, height);
      ctx.stroke();
    }
    for (let y = 0; y < height; y += 28) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
      ctx.stroke();
    }
  }

  drawMapBounds(ctx) {
    const { width, height } = this.baseDimensions();
    const topLeft = this.baseToScreen(0, 0);
    const bottomRight = this.baseToScreen(width, height);
    ctx.fillStyle = "rgba(18, 29, 39, 0.42)";
    ctx.fillRect(topLeft.x, topLeft.y, bottomRight.x - topLeft.x, bottomRight.y - topLeft.y);
    ctx.strokeStyle = "rgba(230, 183, 111, 0.42)";
    ctx.lineWidth = 2;
    ctx.strokeRect(topLeft.x, topLeft.y, bottomRight.x - topLeft.x, bottomRight.y - topLeft.y);
  }

  drawTile(ctx, entry, image) {
    const destination = this.baseRectToScreenRect(entry.baseRect);
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(
      image,
      entry.contentRect.x,
      entry.contentRect.y,
      entry.contentRect.w,
      entry.contentRect.h,
      destination.x,
      destination.y,
      destination.w,
      destination.h
    );
  }

  drawSkippedTile(ctx, entry) {
    this.drawHatchedTile(ctx, entry, "rgba(87, 207, 197, 0.14)", "rgba(87, 207, 197, 0.42)");
  }

  drawBrokenTile(ctx, entry) {
    this.drawHatchedTile(ctx, entry, "rgba(245, 101, 101, 0.20)", "rgba(245, 101, 101, 0.62)");
  }

  drawLoadingTile(ctx, entry) {
    const rect = this.baseRectToScreenRect(entry.baseRect);
    ctx.fillStyle = "rgba(255,255,255,0.06)";
    ctx.fillRect(rect.x, rect.y, rect.w, rect.h);
    ctx.strokeStyle = "rgba(255,255,255,0.18)";
    ctx.strokeRect(rect.x, rect.y, rect.w, rect.h);
  }

  drawHatchedTile(ctx, entry, fill, stroke) {
    const rect = this.baseRectToScreenRect(entry.baseRect);
    ctx.save();
    ctx.fillStyle = fill;
    ctx.fillRect(rect.x, rect.y, rect.w, rect.h);
    ctx.strokeStyle = stroke;
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    for (let x = rect.x - rect.h; x < rect.x + rect.w; x += 12) {
      ctx.moveTo(x, rect.y);
      ctx.lineTo(x + rect.h, rect.y + rect.h);
    }
    ctx.stroke();
    ctx.strokeRect(rect.x, rect.y, rect.w, rect.h);
    ctx.restore();
  }

  drawGrid(ctx, activeLevel) {
    const tileWidth = this.state.manifest.tile.width / activeLevel.scale;
    const tileHeight = this.state.manifest.tile.height / activeLevel.scale;
    const { width, height } = this.baseDimensions();
    const bounds = this.visibleBaseBounds();
    const startCol = Math.max(0, Math.floor(bounds.left / tileWidth));
    const endCol = Math.min(activeLevel.cols, Math.ceil(bounds.right / tileWidth));
    const startRow = Math.max(0, Math.floor(bounds.top / tileHeight));
    const endRow = Math.min(activeLevel.rows, Math.ceil(bounds.bottom / tileHeight));

    ctx.strokeStyle = "rgba(230, 183, 111, 0.18)";
    ctx.lineWidth = 1;

    for (let col = startCol; col <= endCol; col += 1) {
      const x = Math.min(col * tileWidth, width);
      const start = this.baseToScreen(x, 0);
      const end = this.baseToScreen(x, height);
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();
    }

    for (let row = startRow; row <= endRow; row += 1) {
      const y = Math.min(row * tileHeight, height);
      const start = this.baseToScreen(0, y);
      const end = this.baseToScreen(width, y);
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();
    }
  }

  drawLabels(ctx, entries) {
    ctx.save();
    ctx.font = '12px "SFMono-Regular", "Liberation Mono", Consolas, monospace';
    ctx.textBaseline = "top";

    for (const entry of entries) {
      const rect = this.baseRectToScreenRect(entry.baseRect);
      if (rect.w < 84 || rect.h < 40) {
        continue;
      }
      ctx.fillStyle = "rgba(0, 0, 0, 0.62)";
      ctx.fillRect(rect.x + 6, rect.y + 6, Math.min(rect.w - 12, 136), 28);
      ctx.fillStyle = "rgba(255, 240, 210, 0.95)";
      ctx.fillText(`L${entry.level} X${entry.x} Y${entry.y}`, rect.x + 12, rect.y + 12);
    }

    ctx.restore();
  }

  drawHoverOutline(ctx, entry) {
    const rect = this.baseRectToScreenRect(entry.baseRect);
    ctx.save();
    ctx.strokeStyle = "rgba(113, 214, 209, 0.92)";
    ctx.lineWidth = 2;
    ctx.strokeRect(rect.x, rect.y, rect.w, rect.h);
    ctx.restore();
  }

  drawViewportBadge(ctx, activeLevel, bounds, visibleCount) {
    const label = `L${activeLevel.level}  ${formatScale(activeLevel.scale)}  visible ${visibleCount}  x:${Math.round(bounds.left)}-${Math.round(bounds.right)}  y:${Math.round(bounds.top)}-${Math.round(bounds.bottom)}`;
    ctx.save();
    ctx.font = '12px "SFMono-Regular", "Liberation Mono", Consolas, monospace';
    const width = ctx.measureText(label).width + 24;
    ctx.fillStyle = "rgba(0, 0, 0, 0.62)";
    ctx.fillRect(18, 18, width, 34);
    ctx.fillStyle = "rgba(255, 240, 210, 0.94)";
    ctx.fillText(label, 30, 29);
    ctx.restore();
  }

  baseRectToScreenRect(rect) {
    const topLeft = this.baseToScreen(rect.x, rect.y);
    const bottomRight = this.baseToScreen(rect.x + rect.w, rect.y + rect.h);
    return {
      x: topLeft.x,
      y: topLeft.y,
      w: bottomRight.x - topLeft.x,
      h: bottomRight.y - topLeft.y,
    };
  }

  showStatus(title, message, details = "") {
    this.statusTitle.textContent = title;
    this.statusMessage.textContent = message;
    if (details) {
      this.statusDetails.textContent = details;
      this.statusDetails.classList.remove("hidden");
    } else {
      this.statusDetails.textContent = "";
      this.statusDetails.classList.add("hidden");
    }
    this.canvasOverlay.classList.remove("hidden");
  }

  hideStatus() {
    this.canvasOverlay.classList.add("hidden");
  }

  fail(error) {
    console.error(error);
    this.showStatus(
      "Viewer failed to boot",
      error.message,
      [
        "Expected workflow:",
        "1. Run tilecut cut ... --out demo/data",
        "2. cd demo",
        "3. python -m http.server",
        "4. Open the served URL",
      ].join("\n")
    );
  }

  endDrag() {
    this.state.dragPointerId = null;
    this.state.dragStart = null;
    this.canvas.classList.remove("dragging");
  }
}

function deriveInventoryFromCompact(manifest) {
  const multiLevel =
    manifest.levels.length > 1 || String(manifest.naming.path_template).includes("{level}");
  const entries = [];
  for (const level of manifest.levels) {
    for (let y = 0; y < level.rows; y += 1) {
      for (let x = 0; x < level.cols; x += 1) {
        const srcX = x * manifest.tile.width;
        const srcY = y * manifest.tile.height;
        const srcW = Math.min(level.width - srcX, manifest.tile.width);
        const srcH = Math.min(level.height - srcY, manifest.tile.height);
        entries.push(
          normalizeEntry(
            {
              level: level.level,
              x,
              y,
              path: renderRelPath(
                manifest.naming.layout,
                manifest.tile.format,
                level.zero_pad_width,
                level.level,
                x,
                y,
                multiLevel
              ),
              src_rect: { x: srcX, y: srcY, w: srcW, h: srcH },
              content_rect: { x: 0, y: 0, w: srcW, h: srcH },
              skipped: false,
            },
            manifest
          )
        );
      }
    }
  }
  return entries;
}

function mergeIndexInventory(fullSlots, presentEntries) {
  const presentByCoord = new Map(
    presentEntries.map((entry) => [`${entry.level}:${entry.x}:${entry.y}`, entry])
  );
  return fullSlots.map((slot) => {
    const present = presentByCoord.get(`${slot.level}:${slot.x}:${slot.y}`);
    return (
      present ?? {
        ...slot,
        path: null,
        skipped: true,
      }
    );
  });
}

function normalizeEntry(entry, manifest) {
  const level = manifest.levels.find((candidate) => candidate.level === entry.level);
  if (!level) {
    throw new Error(`Tile references missing level ${entry.level}`);
  }
  const scale = level.scale || 1;
  const srcRect = normalizeRect(entry.src_rect ?? entry.srcRect);
  const contentRect = normalizeRect(entry.content_rect ?? entry.contentRect);
  return {
    level: entry.level,
    x: entry.x,
    y: entry.y,
    path: entry.path ?? null,
    skipped: Boolean(entry.skipped),
    srcRect,
    contentRect,
    baseRect: {
      x: srcRect.x / scale,
      y: srcRect.y / scale,
      w: srcRect.w / scale,
      h: srcRect.h / scale,
    },
  };
}

function normalizeRect(rect) {
  return {
    x: rect.x,
    y: rect.y,
    w: rect.w,
    h: rect.h,
  };
}

function renderRelPath(layout, format, padWidth, level, x, y, multiLevel) {
  const paddedX = String(x).padStart(padWidth, "0");
  const paddedY = String(y).padStart(padWidth, "0");
  const levelPrefix = multiLevel ? `tiles/l${String(level).padStart(4, "0")}/` : "tiles/";
  if (layout === "sharded") {
    return `${levelPrefix}y${paddedY}/x${paddedX}.${format}`;
  }
  return `${levelPrefix}x${paddedX}_y${paddedY}.${format}`;
}

function byTileCoord(left, right) {
  return left.level - right.level || left.y - right.y || left.x - right.x;
}

function groupByLevel(entries) {
  const grouped = new Map();
  for (const entry of entries) {
    if (!grouped.has(entry.level)) {
      grouped.set(entry.level, []);
    }
    grouped.get(entry.level).push(entry);
  }
  return grouped;
}

function pointInRect(point, rect) {
  return (
    point.x >= rect.x &&
    point.y >= rect.y &&
    point.x <= rect.x + rect.w &&
    point.y <= rect.y + rect.h
  );
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function formatScale(scale) {
  return `${scale.toFixed(3)}x`;
}

function formatNumber(value) {
  return new Intl.NumberFormat("en-US").format(value);
}

function formatDimensions(width, height) {
  return `${formatNumber(width)} x ${formatNumber(height)}`;
}

function rectToString(rect) {
  return `${rect.x}, ${rect.y}, ${rect.w}, ${rect.h}`;
}

function formatFloat(value) {
  return Number(value).toFixed(2);
}

function statCard(label, primary, secondary = "", tertiary = "", classes = "") {
  const secondaryLine = secondary
    ? `<span class="stat-secondary">${escapeHtml(String(secondary))}</span>`
    : "";
  const tertiaryLine = tertiary
    ? `<span class="stat-tertiary">${escapeHtml(String(tertiary))}</span>`
    : "";
  const className = classes ? `stat ${classes}` : "stat";
  return `
    <div class="${className}">
      <span class="stat-label">${escapeHtml(String(label))}</span>
      <div class="stat-body">
        <span class="stat-primary">${escapeHtml(String(primary))}</span>
        ${secondaryLine}
        ${tertiaryLine}
      </div>
    </div>
  `;
}

function statRow(label, value) {
  return `<div><dt>${label}</dt><dd>${escapeHtml(String(value))}</dd></div>`;
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function nextFrame() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      resolve();
    });
  });
}

window.addEventListener("DOMContentLoaded", () => {
  new TilecutDemo();
});
