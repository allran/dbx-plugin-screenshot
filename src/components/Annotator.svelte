<script>
  import { onDestroy } from "svelte";
  import { uploadAnnotatedBlob } from "../lib/annotateUpload.js";
  import {
    cloneOps,
    constrainShift,
    nextCalloutNumber,
    applyStyleToOp,
    translateOp,
    opBounds,
    hitTestOps,
    CROP_HANDLES,
    cropHandleAt,
    resizeCrop,
    moveCrop,
    MOSAIC_BLOCK,
    MOSAIC_BLUR,
  } from "../lib/annotateOps.js";
  import { friendlyError } from "../lib/format.js";

  let {
    open = false,
    item = null,
    text = {},
    /** @type {"jpeg" | "png"} */
    fileFormat = "jpeg",
    /** @type {"high" | "medium" | "low"} */
    quality = "high",
    oncancel,
    onsaved,
  } = $props();

  const exportMime = $derived(fileFormat === "png" ? "image/png" : "image/jpeg");
  const exportJpegQuality = $derived(
    quality === "low" ? 0.68 : quality === "medium" ? 0.82 : 0.92,
  );

  /** @typedef {"select"|"eyedropper"|"pen"|"rect"|"ellipse"|"line"|"arrow"|"text"|"mosaic"|"highlight"|"watermark"|"crop"} Tool */
  /** @typedef {"text"|"callout"|"label"} TextKind */

  let imgEl = $state(/** @type {HTMLImageElement | null} */ (null));
  let canvasEl = $state(/** @type {HTMLCanvasElement | null} */ (null));
  let frameEl = $state(/** @type {HTMLDivElement | null} */ (null));
  let tool = $state(/** @type {Tool} */ ("rect"));
  /** Tool to restore after a one-shot eyedropper pick */
  let lastTool = /** @type {Tool} */ ("rect");
  let color = $state("#ef4444");
  /** @type {null | { left: number, top: number, hex: string, rgb: string, px: number, py: number, ix: number, iy: number }} */
  let eyedropLoupe = $state(null);
  let loupeCanvasEl = $state(/** @type {HTMLCanvasElement | null} */ (null));
  let colorCopyFlash = $state("");
  /** @type {ReturnType<typeof setTimeout> | null} */
  let colorCopyTimer = null;
  let lineWidth = $state(4);
  /** View-space font size for text / callout / label / watermark */
  let fontPx = $state(18);
  let selectedIndex = $state(-1);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state("");
  let imageSrc = $state("");
  let placeholderSrc = $state("");
  /** Same-origin object URL for the loaded bitmap (revoked on replace/close). */
  let localObjectUrl = /** @type {string | null} */ (null);
  let textDraft = $state("");
  let textOpen = $state(false);
  /** @type {"text"|"label"|"callout"|"watermark"|"edit"} */
  let textMode = $state("text");
  let textPos = $state({ x: 0, y: 0 });
  let textAnchorCss = $state({ left: 0, top: 0, maxWidth: 140 });
  let textInputEl = $state(/** @type {HTMLInputElement | null} */ (null));
  let customColorEl = $state(/** @type {HTMLInputElement | null} */ (null));
  /** Pending label/callout op index or draft anchor while editing */
  let textEditIndex = $state(-1);

  let textKind = $state(/** @type {TextKind} */ ("text"));
  let textBg = $state(/** @type {"transparent"|"dark"} */ ("transparent"));
  let mosaicShape = $state(/** @type {"rect"|"brush"} */ ("rect"));
  let mosaicIntensity = $state(/** @type {"soft"|"standard"|"strong"} */ ("standard"));
  let mosaicStyle = $state(/** @type {"pixel"|"blur"} */ ("pixel"));
  let watermarkMode = $state(/** @type {"single"|"tiled"} */ ("single"));

  /** @type {null | { x: number, y: number, w: number, h: number }} */
  let cropSession = $state(null);
  /** @type {null | { kind: "resize"|"move", handle: string, start: {x:number,y:number}, origin: {x:number,y:number,w:number,h:number} }} */
  let cropDrag = null;

  /** @type {any[]} */
  let ops = $state([]);
  /** @type {{ ops: any[], imageSrc: string, naturalWidth: number, naturalHeight: number }[]} */
  let undoStack = $state([]);
  /** @type {{ ops: any[], imageSrc: string, naturalWidth: number, naturalHeight: number }[]} */
  let redoStack = $state([]);
  /** @type {any | null} */
  let draft = null;
  /** @type {null | { index: number, start: { x: number, y: number }, origin: any, preOps: any[] }} */
  let selectDrag = null;
  let drawStart = null;
  let shiftHeld = false;
  let drawing = false;
  /** @type {null | { label: string, x: number, y: number }} */
  let tip = $state(null);
  /** @type {ReturnType<typeof setTimeout> | null} */
  let tipTimer = null;
  let loadToken = 0;
  /** @type {string | number | null} */
  let mediaToken = null;
  /** @type {ResizeObserver | null} */
  let resizeObserver = null;
  let naturalWidth = $state(0);
  let naturalHeight = $state(0);

  const colors = ["#ef4444", "#f59e0b", "#22c55e", "#3b82f6", "#a855f7", "#111827", "#ffffff"];

  function toColorInputValue(c) {
    if (!c || typeof c !== "string") return "#ef4444";
    let h = c.trim();
    if (/^#[0-9a-fA-F]{3}$/.test(h)) {
      h = `#${h[1]}${h[1]}${h[2]}${h[2]}${h[3]}${h[3]}`;
    }
    if (/^#[0-9a-fA-F]{6}$/i.test(h)) return h.toLowerCase();
    return "#ef4444";
  }

  function isPresetColor(c) {
    return colors.includes(toColorInputValue(c));
  }

  const toolDefs = $derived([
    ["select", text.toolSelect],
    ["rect", text.toolRect],
    ["ellipse", text.toolEllipse],
    ["pen", text.toolPen],
    ["line", text.toolLine],
    ["arrow", text.toolArrow],
    ["text", text.toolText],
    ["mosaic", text.toolMosaic],
    ["highlight", text.toolHighlight],
    ["watermark", text.toolWatermark],
    ["crop", text.toolCrop],
  ]);

  $effect(() => {
    if (open && item?.id) {
      void loadItem(item);
    }
    if (!open) {
      resetState();
    }
  });

  $effect(() => {
    if (!open || !imgEl) return;
    resizeObserver?.disconnect();
    resizeObserver = new ResizeObserver(() => syncOverlay());
    resizeObserver.observe(imgEl);
    syncOverlay();
    return () => {
      resizeObserver?.disconnect();
      resizeObserver = null;
    };
  });

  $effect(() => {
    if (eyedropLoupe && loupeCanvasEl) {
      paintLoupe(eyedropLoupe.ix, eyedropLoupe.iy);
    }
  });

  function resetState() {
    loadToken += 1;
    void closeMediaToken();
    ops = [];
    undoStack = [];
    redoStack = [];
    draft = null;
    selectDrag = null;
    cropDrag = null;
    cropSession = null;
    drawing = false;
    drawStart = null;
    loading = false;
    saving = false;
    error = "";
    imageSrc = "";
    placeholderSrc = "";
    textOpen = false;
    textDraft = "";
    textEditIndex = -1;
    tool = "rect";
    selectedIndex = -1;
    naturalWidth = 0;
    naturalHeight = 0;
    textKind = "text";
    fontPx = 18;
    textBg = "transparent";
    mosaicShape = "rect";
    mosaicIntensity = "standard";
    mosaicStyle = "pixel";
    watermarkMode = "single";
    eyedropLoupe = null;
    colorCopyFlash = "";
    if (colorCopyTimer) {
      clearTimeout(colorCopyTimer);
      colorCopyTimer = null;
    }
    hideTip();
  }

  function clearTipTimer() {
    if (tipTimer != null) {
      clearTimeout(tipTimer);
      tipTimer = null;
    }
  }

  /** @param {MouseEvent | FocusEvent} event @param {string} label */
  function showTip(event, label) {
    if (!label) return;
    const el = /** @type {HTMLElement} */ (event.currentTarget);
    const place = () => {
      tipTimer = null;
      const r = el.getBoundingClientRect();
      tip = { label, x: r.left + r.width / 2, y: r.top };
    };
    clearTipTimer();
    if (tip) place();
    else tipTimer = setTimeout(place, 160);
  }

  function hideTip() {
    clearTipTimer();
    tip = null;
  }

  function revokeLocalObjectUrl() {
    if (!localObjectUrl) return;
    try {
      URL.revokeObjectURL(localObjectUrl);
    } catch {
      // ignore
    }
    localObjectUrl = null;
  }

  async function closeMediaToken() {
    const token = mediaToken;
    mediaToken = null;
    revokeLocalObjectUrl();
    if (token == null || typeof window.dbxPlugin?.media?.close !== "function") return;
    try {
      await window.dbxPlugin.media.close(token);
    } catch {
      // ignore
    }
  }

  function base64ToBytes(b64) {
    const binary = atob(String(b64 || "").trim());
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }

  /**
   * Load a same-origin blob URL so canvas export (toBlob / toDataURL) is not tainted.
   * Host media.open URLs are cross-origin and make save throw "The operation is insecure."
   */
  async function resolveFullImageSrc(next) {
    try {
      const blobResult = await window.dbxPlugin.invoke("filesystem/media/read", {
        id: next.id,
      });
      if (blobResult?.dataBase64) {
        const bytes = base64ToBytes(blobResult.dataBase64);
        const blob = new Blob([bytes], {
          type: blobResult.contentType || "image/jpeg",
        });
        const src = URL.createObjectURL(blob);
        return { src, token: null, objectUrl: src };
      }
    } catch {
      // fall through to JPEG data-URL bridge
    }

    const result = await window.dbxPlugin.invoke("screenshot/media", {
      id: next.id,
      full: true,
    });
    if (!result?.dataUrl) throw new Error("Failed to load image");
    return { src: result.dataUrl, token: null, objectUrl: null };
  }

  async function loadItem(next) {
    const token = ++loadToken;
    loading = true;
    error = "";
    ops = [];
    undoStack = [];
    redoStack = [];
    draft = null;
    cropSession = null;
    naturalWidth = 0;
    naturalHeight = 0;
    imageSrc = "";
    placeholderSrc = next.previewDataUrl || "";
    try {
      await closeMediaToken();
      const resolved = await resolveFullImageSrc(next);
      if (token !== loadToken) {
        if (resolved.objectUrl) {
          try {
            URL.revokeObjectURL(resolved.objectUrl);
          } catch {
            // ignore
          }
        }
        if (resolved.token != null) {
          try {
            await window.dbxPlugin.media.close(resolved.token);
          } catch {
            // ignore
          }
        }
        return;
      }
      mediaToken = resolved.token;
      localObjectUrl = resolved.objectUrl;
      imageSrc = resolved.src;
    } catch (err) {
      if (token !== loadToken) return;
      error = friendlyError(err, text);
      loading = false;
    }
  }

  function onImageLoad() {
    const img = imgEl;
    if (!img) return;
    naturalWidth = img.naturalWidth || img.width;
    naturalHeight = img.naturalHeight || img.height;
    loading = false;
    placeholderSrc = "";
    error = "";
    syncOverlay();
    redraw();
  }

  function onImageError() {
    if (!imageSrc) return;
    error = text.mediaMissing || "Image file is missing";
    loading = false;
  }

  function syncOverlay() {
    const img = imgEl;
    const canvas = canvasEl;
    if (!img || !canvas) return;
    const cssW = Math.max(1, Math.round(img.clientWidth));
    const cssH = Math.max(1, Math.round(img.clientHeight));
    const dpr = Math.max(1, window.devicePixelRatio || 1);
    const bufW = Math.max(1, Math.round(cssW * dpr));
    const bufH = Math.max(1, Math.round(cssH * dpr));
    if (canvas.width !== bufW || canvas.height !== bufH) {
      canvas.width = bufW;
      canvas.height = bufH;
    }
    canvas.style.width = `${cssW}px`;
    canvas.style.height = `${cssH}px`;
    redraw();
  }

  function imagePoint(event) {
    const img = imgEl;
    if (!img) return { x: 0, y: 0 };
    const rect = img.getBoundingClientRect();
    const w = Math.max(1, rect.width);
    const h = Math.max(1, rect.height);
    const nw = naturalWidth || img.naturalWidth || w;
    const nh = naturalHeight || img.naturalHeight || h;
    return {
      x: ((event.clientX - rect.left) / w) * nw,
      y: ((event.clientY - rect.top) / h) * nh,
    };
  }

  function viewToImageScale() {
    const img = imgEl;
    if (!img) return 1;
    const cssW = Math.max(1, img.clientWidth);
    const nw = naturalWidth || img.naturalWidth || cssW;
    return nw / cssW;
  }

  function currentSnapshot() {
    return {
      ops: cloneOps(ops),
      imageSrc,
      naturalWidth,
      naturalHeight,
    };
  }

  function snapshotOps() {
    undoStack = [...undoStack, currentSnapshot()].slice(-60);
    redoStack = [];
  }

  async function restoreSnapshot(snap) {
    ops = cloneOps(snap.ops || []);
    if (snap.imageSrc && snap.imageSrc !== imageSrc) {
      imageSrc = snap.imageSrc;
      naturalWidth = snap.naturalWidth || 0;
      naturalHeight = snap.naturalHeight || 0;
      return;
    }
    naturalWidth = snap.naturalWidth || naturalWidth;
    naturalHeight = snap.naturalHeight || naturalHeight;
    redraw();
  }

  function undo() {
    if (!undoStack.length) return;
    const next = undoStack.slice();
    const snap = next.pop();
    redoStack = [...redoStack, currentSnapshot()];
    undoStack = next;
    selectedIndex = -1;
    selectDrag = null;
    if (snap) void restoreSnapshot(snap);
  }

  function redo() {
    if (!redoStack.length) return;
    const next = redoStack.slice();
    const snap = next.pop();
    undoStack = [...undoStack, currentSnapshot()];
    redoStack = next;
    selectedIndex = -1;
    selectDrag = null;
    if (snap) void restoreSnapshot(snap);
  }

  function clearAnnotations() {
    if (!ops.length || saving) return;
    snapshotOps();
    ops = [];
    selectedIndex = -1;
    selectDrag = null;
    draft = null;
    drawing = false;
    redraw();
  }

  function deleteSelection() {
    if (selectedIndex < 0 || selectedIndex >= ops.length) return;
    snapshotOps();
    ops = ops.filter((_, i) => i !== selectedIndex);
    selectedIndex = -1;
    redraw();
  }

  function startCropSession() {
    const nw = naturalWidth;
    const nh = naturalHeight;
    if (!nw || !nh) {
      cropSession = null;
      return;
    }
    const w = Math.round(nw * 0.8);
    const h = Math.round(nh * 0.8);
    cropSession = {
      x: Math.round((nw - w) / 2),
      y: Math.round((nh - h) / 2),
      w,
      h,
    };
    redraw();
  }

  function exitCropWithoutApply() {
    cropSession = null;
    cropDrag = null;
    tool = "select";
    redraw();
  }

  function setTool(next) {
    if (tool === "crop" && next !== "crop") {
      cropSession = null;
      cropDrag = null;
    }
    if (tool !== "eyedropper" && next === "eyedropper") {
      lastTool = tool === "crop" ? "rect" : tool;
    }
    if (next !== "eyedropper") eyedropLoupe = null;
    tool = next;
    selectedIndex = -1;
    selectDrag = null;
    draft = null;
    drawing = false;
    drawStart = null;
    if (next !== "text" && next !== "watermark" && textOpen) {
      cancelText();
    }
    if (next === "crop") {
      startCropSession();
    }
    redraw();
  }

  function parseRgb(hex) {
    const h = toColorInputValue(hex).slice(1);
    return {
      r: Number.parseInt(h.slice(0, 2), 16),
      g: Number.parseInt(h.slice(2, 4), 16),
      b: Number.parseInt(h.slice(4, 6), 16),
    };
  }

  function toHexUpper(hex) {
    return toColorInputValue(hex).toUpperCase();
  }

  function toRgbStr(hex) {
    const { r, g, b } = parseRgb(hex);
    return `${r}, ${g}, ${b}`;
  }

  /** Sample a CSS hex color from the base screenshot at image coords. */
  function sampleColorAtImagePoint(point) {
    const img = imgEl;
    if (!img || !naturalWidth || !naturalHeight) return null;
    const x = Math.max(0, Math.min(naturalWidth - 1, Math.floor(point.x)));
    const y = Math.max(0, Math.min(naturalHeight - 1, Math.floor(point.y)));
    const sample = document.createElement("canvas");
    sample.width = 1;
    sample.height = 1;
    const ctx = sample.getContext("2d", { willReadFrequently: true });
    if (!ctx) return null;
    try {
      ctx.drawImage(img, x, y, 1, 1, 0, 0, 1, 1);
      const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
      if (a < 8) return null;
      return toHexUpper(
        `#${r.toString(16).padStart(2, "0")}${g.toString(16).padStart(2, "0")}${b.toString(16).padStart(2, "0")}`,
      );
    } catch {
      return null;
    }
  }

  function paintLoupe(ix, iy) {
    const canvas = loupeCanvasEl;
    const img = imgEl;
    if (!canvas || !img || !naturalWidth || !naturalHeight) return;
    // LaterScreen: 13×13 → 156pt nearest-neighbor loupe + blue crosshair
    const out = 156;
    const src = 13;
    const half = (src - 1) / 2;
    if (canvas.width !== out || canvas.height !== out) {
      canvas.width = out;
      canvas.height = out;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const cx = Math.max(0, Math.min(naturalWidth - 1, Math.floor(ix)));
    const cy = Math.max(0, Math.min(naturalHeight - 1, Math.floor(iy)));
    ctx.imageSmoothingEnabled = false;
    ctx.fillStyle = "#0b1220";
    ctx.fillRect(0, 0, out, out);
    try {
      ctx.drawImage(img, cx - half, cy - half, src, src, 0, 0, out, out);
    } catch {
      return;
    }
    const cell = out / src;
    const mid = out / 2;
    ctx.strokeStyle = "rgba(33, 150, 243, 0.7)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, mid);
    ctx.lineTo(out, mid);
    ctx.moveTo(mid, 0);
    ctx.lineTo(mid, out);
    ctx.stroke();
    ctx.strokeStyle = "#fff";
    ctx.lineWidth = 1;
    ctx.strokeRect(mid - cell / 2, mid - cell / 2, cell, cell);
  }

  function updateEyedropLoupe(event) {
    if (tool !== "eyedropper" || !imgEl || loading) {
      eyedropLoupe = null;
      return;
    }
    const point = imagePoint(event);
    const hex = sampleColorAtImagePoint(point) || "#000000";
    const board = imgEl.parentElement?.getBoundingClientRect?.();
    if (!board) {
      eyedropLoupe = null;
      return;
    }
    const zoom = 156;
    const infoH = 62;
    // Info row needs room for "#RRGGBB (xxxx, yyyy)" beyond the 156 loupe.
    const panelW = Math.max(zoom, 200);
    const panelH = zoom + infoH;
    let left = event.clientX - board.left + 24;
    let top = event.clientY - board.top + 24;
    if (left + panelW > board.width - 8) left = event.clientX - board.left - 24 - panelW;
    if (top + panelH > board.height - 8) top = event.clientY - board.top - 24 - panelH;
    left = Math.max(4, Math.min(left, board.width - panelW - 4));
    top = Math.max(4, Math.min(top, board.height - panelH - 4));
    eyedropLoupe = {
      left,
      top,
      hex,
      rgb: toRgbStr(hex),
      px: Math.floor(point.x),
      py: Math.floor(point.y),
      ix: point.x,
      iy: point.y,
    };
    requestAnimationFrame(() => paintLoupe(point.x, point.y));
  }

  function flashCopied(label) {
    if (colorCopyTimer) clearTimeout(colorCopyTimer);
    colorCopyFlash = label;
    colorCopyTimer = setTimeout(() => {
      colorCopyFlash = "";
      colorCopyTimer = null;
    }, 2400);
  }

  async function copyColorToClipboard(hex) {
    const value = toHexUpper(hex);
    const rgb = toRgbStr(value);
    try {
      if (typeof window.dbxPlugin?.copy === "function") {
        await window.dbxPlugin.copy(value);
      } else if (typeof window.dbxPlugin?.clipboard?.writeText === "function") {
        await window.dbxPlugin.clipboard.writeText(value);
      } else {
        return false;
      }
      flashCopied(`${text.colorCopied || "Copied"}  ${value}  ·  RGB ${rgb}`);
      return true;
    } catch {
      return false;
    }
  }

  function finishEyedrop(hex) {
    const next = toHexUpper(hex);
    pickColor(next);
    void copyColorToClipboard(next);
    eyedropLoupe = null;
    const restore = lastTool && lastTool !== "eyedropper" ? lastTool : "rect";
    setTool(restore);
  }

  function hitTest(point) {
    const canvas = canvasEl;
    const ctx = canvas?.getContext("2d");
    const tol = 8 * viewToImageScale();
    return hitTestOps(ops, point, tol, ctx);
  }

  function strokeWidthImage(viewPx) {
    return Math.max(1, (viewPx || lineWidth) * viewToImageScale());
  }

  function fontSizeImage() {
    return Math.max(12, fontPx) * viewToImageScale();
  }

  function calloutRadiusImage() {
    return Math.max(12, fontPx * 0.85) * viewToImageScale();
  }

  function brushSizeImage() {
    return Math.max(12, lineWidth * 6) * viewToImageScale();
  }

  function showTextOptions() {
    if (tool === "text") return true;
    if (selectedIndex >= 0) {
      const t = ops[selectedIndex]?.type;
      return t === "text" || t === "label" || t === "callout";
    }
    return false;
  }

  function applyStyleToSelection() {
    if (selectedIndex < 0 || selectedIndex >= ops.length) return;
    const op = ops[selectedIndex];
    snapshotOps();
    const size = fontSizeImage();
    const styled = applyStyleToOp(op, {
      color,
      width: strokeWidthImage(lineWidth),
      background: textBg,
      size: op.type === "callout" ? calloutRadiusImage() : size,
    });
    ops = ops.map((o, i) => (i === selectedIndex ? styled : o));
    redraw();
  }

  function pickColor(next) {
    color = toColorInputValue(next);
    if (selectedIndex >= 0) applyStyleToSelection();
  }

  /** @param {Event & { currentTarget: HTMLInputElement }} event */
  function onCustomColorInput(event) {
    pickColor(event.currentTarget.value);
  }

  /** @param {MouseEvent} event */
  function openCustomColorPicker(event) {
    event.preventDefault();
    hideTip();
    const input = customColorEl;
    if (!input) return;
    try {
      if (typeof input.showPicker === "function") {
        input.showPicker();
        return;
      }
    } catch {
      // fall through
    }
    input.click();
  }

  function onLineWidthInput() {
    if (selectedIndex >= 0) applyStyleToSelection();
  }

  function onFontPxInput() {
    if (selectedIndex >= 0) applyStyleToSelection();
  }

  function setTextKind(next) {
    textKind = next;
    if (textOpen) cancelText();
  }

  function setTextBg(next) {
    textBg = next;
    if (selectedIndex >= 0) applyStyleToSelection();
  }

  function syncFontPxFromOp(op) {
    if (!op) return;
    const scale = viewToImageScale();
    let next = fontPx;
    if (op.type === "callout" && Number.isFinite(op.size)) {
      next = op.size / 0.85 / scale;
    } else if (Number.isFinite(op.size)) {
      next = op.size / scale;
    } else if (Number.isFinite(op.fontSize)) {
      next = op.fontSize / scale;
    } else {
      return;
    }
    fontPx = Math.max(12, Math.min(48, Math.round(next)));
  }

  function textPlaceholder() {
    if (textMode === "label") return text.toolLabelPrompt || "Label text";
    if (textMode === "watermark") return text.toolWatermarkPrompt || "Watermark text";
    if (textMode === "callout") return text.toolTextPrompt || "Text";
    return text.toolTextPrompt || "Text";
  }

  function openTextEditor(event, point, mode = "text", editIndex = -1) {
    textMode = mode;
    textEditIndex = editIndex;
    const img = imgEl;
    const wrapRect = img?.getBoundingClientRect?.();
    if (wrapRect) {
      const pad = 4;
      const rawLeft = Math.max(0, event.clientX - wrapRect.left);
      const rawTop = Math.max(0, event.clientY - wrapRect.top);
      const preferred = Math.min(180, Math.max(72, wrapRect.width - pad * 2));
      let left = rawLeft;
      let maxWidth = preferred;
      const avail = Math.max(48, wrapRect.width - left - pad);
      if (avail < preferred) {
        if (avail >= 72) maxWidth = avail;
        else {
          left = Math.max(pad, wrapRect.width - preferred - pad);
          maxWidth = Math.max(48, wrapRect.width - left - pad);
        }
      }
      const top = Math.min(rawTop, Math.max(0, wrapRect.height - fontPx - 12));
      textAnchorCss = { left, top, maxWidth };
    } else {
      textAnchorCss = { left: 0, top: 0, maxWidth: 140 };
    }
    textPos = point;
    if (editIndex >= 0 && ops[editIndex]) {
      textDraft = ops[editIndex].text || "";
    } else {
      textDraft = "";
    }
    textOpen = true;
    queueMicrotask(() => {
      textInputEl?.focus?.();
      textInputEl?.select?.();
    });
  }

  function onPointerDown(event) {
    if (loading || saving || !imgEl) return;
    shiftHeld = event.shiftKey;
    if (textOpen) commitText();

    event.preventDefault();
    const point = imagePoint(event);

    if (tool === "eyedropper") {
      selectedIndex = -1;
      selectDrag = null;
      const hex = sampleColorAtImagePoint(point);
      if (hex) finishEyedrop(hex);
      else {
        eyedropLoupe = null;
        setTool(lastTool && lastTool !== "eyedropper" ? lastTool : "rect");
      }
      return;
    }

    if (tool === "crop" && cropSession) {
      const tol = 10 * viewToImageScale();
      const hit = cropHandleAt(cropSession, point, tol);
      if (hit) {
        drawing = true;
        cropDrag = {
          kind: hit === "move" ? "move" : "resize",
          handle: hit,
          start: { ...point },
          origin: { ...cropSession },
        };
        canvasEl?.setPointerCapture?.(event.pointerId);
      }
      return;
    }

    if (tool === "select") {
      const hit = hitTest(point);
      selectedIndex = hit;
      if (hit >= 0) {
        const op = ops[hit];
        if (op.color) color = op.color;
        if (op.background === "dark" || op.background === "transparent") textBg = op.background;
        if (op.type === "text" || op.type === "callout" || op.type === "label") {
          textKind = /** @type {TextKind} */ (op.type);
          syncFontPxFromOp(op);
        }
        drawing = true;
        selectDrag = {
          index: hit,
          start: { ...point },
          origin: cloneOps([ops[hit]])[0],
          preOps: cloneOps(ops),
        };
        canvasEl?.setPointerCapture?.(event.pointerId);
      }
      redraw();
      return;
    }

    if (tool === "text" && textKind === "text") {
      selectedIndex = -1;
      openTextEditor(event, point, "text");
      return;
    }

    if (tool === "text" && textKind === "callout") {
      selectedIndex = -1;
      const num = nextCalloutNumber(ops);
      snapshotOps();
      const newOp = {
        type: "callout",
        cx: point.x,
        cy: point.y,
        number: num,
        text: "",
        color,
        size: calloutRadiusImage(),
        background: textBg,
      };
      ops = [...ops, newOp];
      textEditIndex = ops.length - 1;
      openTextEditor(event, point, "callout", textEditIndex);
      redraw();
      return;
    }

    if (tool === "watermark") {
      selectedIndex = -1;
      openTextEditor(event, point, "watermark");
      return;
    }

    selectedIndex = -1;
    drawing = true;
    drawStart = { ...point };
    canvasEl?.setPointerCapture?.(event.pointerId);
    const width = strokeWidthImage(lineWidth);

    if (tool === "pen") {
      draft = { type: "pen", color, width, points: [{ ...point }] };
    } else if (tool === "mosaic" && mosaicShape === "brush") {
      draft = {
        type: "mosaic",
        points: [{ ...point }],
        brush: brushSizeImage(),
        intensity: mosaicIntensity,
        style: mosaicStyle,
      };
    } else if (tool === "rect" || tool === "ellipse" || tool === "highlight") {
      draft = { type: tool, color, width, x1: point.x, y1: point.y, x2: point.x, y2: point.y };
    } else if (tool === "mosaic") {
      draft = {
        type: "mosaic",
        x1: point.x,
        y1: point.y,
        x2: point.x,
        y2: point.y,
        intensity: mosaicIntensity,
        style: mosaicStyle,
      };
    } else if (tool === "arrow" || tool === "line") {
      draft = {
        type: tool,
        color,
        width,
        x1: point.x,
        y1: point.y,
        x2: point.x,
        y2: point.y,
      };
    } else if (tool === "text" && textKind === "label") {
      draft = {
        type: "label",
        color,
        width,
        x1: point.x,
        y1: point.y,
        x2: point.x,
        y2: point.y,
        text: "",
        size: fontSizeImage(),
        background: textBg,
      };
    }
    redraw();
  }

  function draftPoint(event) {
    let point = imagePoint(event);
    if (shiftHeld && drawStart && draft && draft.type !== "pen") {
      const ct =
        draft.type === "label"
          ? "label"
          : draft.type === "mosaic" && !draft.points
            ? "mosaic"
            : draft.type;
      if (["line", "arrow", "label", "rect", "ellipse", "highlight", "mosaic"].includes(ct)) {
        point = constrainShift(ct, drawStart, point);
      }
    }
    return point;
  }

  function onPointerMove(event) {
    shiftHeld = event.shiftKey;
    if (tool === "eyedropper") {
      updateEyedropLoupe(event);
      return;
    }
    if (eyedropLoupe) eyedropLoupe = null;
    if (!drawing) return;
    const point = draftPoint(event);

    if (cropDrag && cropSession) {
      const bounds = { w: naturalWidth, h: naturalHeight };
      if (cropDrag.kind === "move") {
        const dx = point.x - cropDrag.start.x;
        const dy = point.y - cropDrag.start.y;
        cropSession = moveCrop(cropDrag.origin, dx, dy, bounds);
      } else {
        cropSession = resizeCrop(cropDrag.origin, cropDrag.handle, point, bounds);
      }
      redraw();
      return;
    }

    if (selectDrag) {
      const dx = point.x - selectDrag.start.x;
      const dy = point.y - selectDrag.start.y;
      const moved = translateOp(selectDrag.origin, dx, dy);
      ops = ops.map((op, i) => (i === selectDrag.index ? moved : op));
      redraw();
      return;
    }

    if (!draft) return;
    if (draft.type === "pen" || (draft.type === "mosaic" && draft.points)) {
      draft.points.push(point);
    } else {
      draft.x2 = point.x;
      draft.y2 = point.y;
    }
    redraw();
  }

  function onPointerUp(event) {
    if (!drawing) return;
    drawing = false;
    try {
      canvasEl?.releasePointerCapture?.(event.pointerId);
    } catch {
      // ignore
    }

    if (cropDrag) {
      cropDrag = null;
      redraw();
      return;
    }

    if (selectDrag) {
      const point = imagePoint(event);
      const moved =
        Math.hypot(point.x - selectDrag.start.x, point.y - selectDrag.start.y) > 2 * viewToImageScale();
      if (moved) {
        undoStack = [
          ...undoStack,
          {
            ops: selectDrag.preOps,
            imageSrc,
            naturalWidth,
            naturalHeight,
          },
        ].slice(-60);
        redoStack = [];
      } else {
        ops = selectDrag.preOps;
      }
      selectDrag = null;
      drawStart = null;
      redraw();
      return;
    }

    if (!draft) return;
    const finished = draft;
    draft = null;
    drawStart = null;

    if (finished.type === "pen" && (finished.points?.length || 0) < 2) {
      redraw();
      return;
    }
    if (finished.type === "mosaic" && finished.points && (finished.points.length || 0) < 2) {
      redraw();
      return;
    }
    if (finished.x1 != null && finished.type !== "pen") {
      const w = Math.abs(finished.x2 - finished.x1);
      const h = Math.abs(finished.y2 - finished.y1);
      const min = 4 * viewToImageScale();
      if (w < min && h < min) {
        redraw();
        return;
      }
    }

    if (finished.type === "label") {
      snapshotOps();
      ops = [...ops, cloneOps([finished])[0]];
      textEditIndex = ops.length - 1;
      openTextEditor(event, { x: finished.x2, y: finished.y2 }, "label", textEditIndex);
      redraw();
      return;
    }

    snapshotOps();
    ops = [...ops, cloneOps([finished])[0]];
    redraw();
  }

  function onDoubleClick(event) {
    if (loading || saving || !imgEl) return;
    if (tool === "crop" && cropSession) {
      event.preventDefault();
      void commitCropSession();
      return;
    }
    const point = imagePoint(event);
    const hit = hitTest(point);
    if (hit < 0) return;
    const op = ops[hit];
    if (op.type === "text" || op.type === "callout" || op.type === "label") {
      selectedIndex = hit;
      textBg = op.background || "transparent";
      textKind = /** @type {TextKind} */ (op.type);
      syncFontPxFromOp(op);
      openTextEditor(
        event,
        op.type === "callout"
          ? { x: op.cx, y: op.cy }
          : op.type === "label"
            ? { x: op.x2, y: op.y2 }
            : { x: op.x, y: op.y },
        "edit",
        hit,
      );
    }
  }

  async function commitCropSession() {
    const rect = cropSession;
    if (!rect || !imgEl || !naturalWidth || !naturalHeight) return;
    const x = rect.x;
    const y = rect.y;
    const w = rect.w;
    const h = rect.h;
    if (w < 8 || h < 8) return;

    const baked = await renderCompositeCanvas();
    if (!baked) return;
    const tmp = document.createElement("canvas");
    tmp.width = Math.round(w);
    tmp.height = Math.round(h);
    const tctx = tmp.getContext("2d");
    if (!tctx) return;
    tctx.drawImage(baked, x, y, w, h, 0, 0, tmp.width, tmp.height);

    const dataUrl =
      exportMime === "image/png"
        ? tmp.toDataURL("image/png")
        : tmp.toDataURL("image/jpeg", exportJpegQuality);
    snapshotOps();
    ops = [];
    selectedIndex = -1;
    selectDrag = null;
    cropSession = null;
    cropDrag = null;
    naturalWidth = tmp.width;
    naturalHeight = tmp.height;
    revokeLocalObjectUrl();
    imageSrc = dataUrl;
    tool = "select";
  }

  function commitText() {
    if (!textOpen) return;
    const value = textDraft.trim();
    const mode = textMode;
    const idx = textEditIndex;
    textOpen = false;
    textDraft = "";
    textEditIndex = -1;

    if (mode === "edit" && idx >= 0) {
      if (!value) {
        snapshotOps();
        ops = ops.filter((_, i) => i !== idx);
        selectedIndex = -1;
      } else {
        snapshotOps();
        ops = ops.map((op, i) => {
          if (i !== idx) return op;
          const next = cloneOps([op])[0];
          next.text = value;
          if (next.background == null) next.background = textBg;
          return next;
        });
        selectedIndex = idx;
      }
      redraw();
      return;
    }

    if (mode === "callout" && idx >= 0) {
      // Placement already snapshotted — keep one undo step for the whole callout.
      ops = ops.map((op, i) => {
        if (i !== idx) return op;
        const next = cloneOps([op])[0];
        next.text = value;
        return next;
      });
      redraw();
      return;
    }

    if (mode === "label" && idx >= 0) {
      // Placement already snapshotted — empty text cancels the draft label.
      if (!value) {
        ops = ops.filter((_, i) => i !== idx);
      } else {
        ops = ops.map((op, i) => {
          if (i !== idx) return op;
          const next = cloneOps([op])[0];
          next.text = value;
          return next;
        });
      }
      redraw();
      return;
    }

    if (!value) return;

    if (mode === "watermark") {
      snapshotOps();
      const fontSize = fontSizeImage();
      if (watermarkMode === "tiled") {
        ops = [
          ...ops,
          {
            type: "watermark",
            text: value,
            color,
            fontSize,
            opacity: 0.22,
            rotation: -30,
            mode: "tiled",
            x: 0,
            y: 0,
          },
        ];
      } else {
        ops = [
          ...ops,
          {
            type: "watermark",
            text: value,
            color,
            fontSize,
            opacity: 0.45,
            rotation: 0,
            mode: "single",
            x: textPos.x,
            y: textPos.y,
          },
        ];
      }
      redraw();
      return;
    }

    snapshotOps();
    ops = [
      ...ops,
      {
        type: "text",
        x: textPos.x,
        y: textPos.y,
        text: value,
        color,
        size: fontSizeImage(),
        background: textBg,
      },
    ];
    redraw();
  }

  function overlayTransform(ctx) {
    const canvas = canvasEl;
    const nw = naturalWidth || 1;
    const nh = naturalHeight || 1;
    if (!canvas) return;
    ctx.setTransform(canvas.width / nw, 0, 0, canvas.height / nh, 0, 0);
  }

  function redraw(includeDraft = true) {
    const canvas = canvasEl;
    const img = imgEl;
    if (!canvas || !img || !naturalWidth || !naturalHeight) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    overlayTransform(ctx);
    for (const op of ops) drawOp(ctx, op, img);
    if (includeDraft && draft) drawOp(ctx, draft, img, true);
    if (selectedIndex >= 0 && selectedIndex < ops.length) {
      drawSelectionChrome(ctx, ops[selectedIndex]);
    }
    if (tool === "crop" && cropSession) {
      drawCropChrome(ctx, cropSession);
    }
  }

  function drawSelectionChrome(ctx, op) {
    const canvas = canvasEl;
    const measureCtx = canvas?.getContext("2d");
    const b = opBounds(op, measureCtx);
    if (!b) return;
    const pad = 4 * viewToImageScale();
    ctx.save();
    ctx.strokeStyle = "#38bdf8";
    ctx.lineWidth = 1.5 * viewToImageScale();
    ctx.setLineDash([5 * viewToImageScale(), 4 * viewToImageScale()]);
    ctx.strokeRect(b.x - pad, b.y - pad, b.w + pad * 2, b.h + pad * 2);
    ctx.restore();
  }

  function drawCropChrome(ctx, rect) {
    const nw = naturalWidth;
    const nh = naturalHeight;
    const { x, y, w, h } = rect;
    const hs = 7 * viewToImageScale();
    ctx.save();
    ctx.fillStyle = "rgba(0,0,0,0.45)";
    ctx.fillRect(0, 0, nw, nh);
    ctx.clearRect(x, y, w, h);
    for (const op of ops) drawOp(ctx, op, imgEl);
    ctx.strokeStyle = "#fff";
    ctx.lineWidth = 2 * viewToImageScale();
    ctx.setLineDash([6 * viewToImageScale(), 4 * viewToImageScale()]);
    ctx.strokeRect(x, y, w, h);
    ctx.setLineDash([]);
    ctx.fillStyle = "#fff";
    const handles = {
      nw: { x, y },
      n: { x: x + w / 2, y },
      ne: { x: x + w, y },
      e: { x: x + w, y: y + h / 2 },
      se: { x: x + w, y: y + h },
      s: { x: x + w / 2, y: y + h },
      sw: { x, y: y + h },
      w: { x, y: y + h / 2 },
    };
    for (const id of CROP_HANDLES) {
      const p = handles[id];
      ctx.fillRect(p.x - hs / 2, p.y - hs / 2, hs, hs);
    }
    ctx.restore();
  }

  /** Ink box for top-baseline text; falls back to em square. */
  function textInkBox(ctx, text, x, y, size) {
    const m = ctx.measureText(text || " ");
    const ascent = Number.isFinite(m.actualBoundingBoxAscent) ? m.actualBoundingBoxAscent : 0;
    const descent = Number.isFinite(m.actualBoundingBoxDescent) ? m.actualBoundingBoxDescent : 0;
    const inkH = ascent + descent > 1 ? ascent + descent : size;
    return {
      x,
      y: y - ascent,
      w: m.width,
      h: inkH,
    };
  }

  function drawTextWithBg(ctx, op) {
    const txt = op.text || "";
    const size = op.size || 20;
    ctx.font = `600 ${size}px ui-sans-serif, system-ui, -apple-system, sans-serif`;
    ctx.textBaseline = "top";
    ctx.textAlign = "left";
    const ink = textInkBox(ctx, txt, op.x, op.y, size);
    if (op.background === "dark") {
      const padX = size * 0.28;
      const padY = size * 0.22;
      ctx.fillStyle = "rgba(15, 23, 42, 0.82)";
      roundRect(ctx, ink.x - padX, ink.y - padY, ink.w + padX * 2, ink.h + padY * 2, size * 0.22);
      ctx.fill();
    }
    ctx.fillStyle = op.color;
    ctx.fillText(txt, op.x, op.y);
  }

  function roundRect(ctx, x, y, w, h, r) {
    const rad = Math.min(r, w / 2, h / 2);
    ctx.beginPath();
    ctx.moveTo(x + rad, y);
    ctx.arcTo(x + w, y, x + w, y + h, rad);
    ctx.arcTo(x + w, y + h, x, y + h, rad);
    ctx.arcTo(x, y + h, x, y, rad);
    ctx.arcTo(x, y, x + w, y, rad);
    ctx.closePath();
  }

  function drawOp(ctx, op, img, isDraft = false) {
    ctx.save();
    if (op.type === "pen") {
      const pts = op.points || [];
      if (pts.length < 2) {
        ctx.restore();
        return;
      }
      ctx.strokeStyle = op.color;
      ctx.lineWidth = op.width;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.beginPath();
      ctx.moveTo(pts[0].x, pts[0].y);
      for (let i = 1; i < pts.length; i += 1) ctx.lineTo(pts[i].x, pts[i].y);
      ctx.stroke();
    } else if (op.type === "rect") {
      ctx.strokeStyle = op.color;
      ctx.lineWidth = op.width;
      const x = Math.min(op.x1, op.x2);
      const y = Math.min(op.y1, op.y2);
      ctx.strokeRect(x, y, Math.abs(op.x2 - op.x1), Math.abs(op.y2 - op.y1));
    } else if (op.type === "ellipse") {
      ctx.strokeStyle = op.color;
      ctx.lineWidth = op.width;
      const x = Math.min(op.x1, op.x2);
      const y = Math.min(op.y1, op.y2);
      const w = Math.abs(op.x2 - op.x1);
      const h = Math.abs(op.y2 - op.y1);
      ctx.beginPath();
      ctx.ellipse(x + w / 2, y + h / 2, Math.max(0.5, w / 2), Math.max(0.5, h / 2), 0, 0, Math.PI * 2);
      ctx.stroke();
    } else if (op.type === "highlight") {
      ctx.fillStyle = hexToRgba(op.color || "#f59e0b", 0.35);
      const x = Math.min(op.x1, op.x2);
      const y = Math.min(op.y1, op.y2);
      ctx.fillRect(x, y, Math.abs(op.x2 - op.x1), Math.abs(op.y2 - op.y1));
    } else if (op.type === "line") {
      ctx.strokeStyle = op.color;
      ctx.lineWidth = op.width;
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(op.x1, op.y1);
      ctx.lineTo(op.x2, op.y2);
      ctx.stroke();
    } else if (op.type === "arrow") {
      drawArrow(ctx, op.x1, op.y1, op.x2, op.y2, op.color, op.width);
    } else if (op.type === "text") {
      drawTextWithBg(ctx, op);
    } else if (op.type === "callout") {
      drawCallout(ctx, op);
    } else if (op.type === "label") {
      drawLabel(ctx, op);
    } else if (op.type === "watermark") {
      drawWatermark(ctx, op);
    } else if (op.type === "mosaic") {
      if (op.points) drawMosaicBrush(ctx, img, op);
      else drawMosaicRect(ctx, img, op);
    }
    ctx.restore();
  }

  function drawCallout(ctx, op) {
    const r = op.size || 18;
    ctx.fillStyle = op.color || "#ef4444";
    ctx.beginPath();
    ctx.arc(op.cx, op.cy, r, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#fff";
    ctx.font = `700 ${Math.round(r * 0.95)}px ui-sans-serif, system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(String(op.number ?? ""), op.cx, op.cy + r * 0.04);
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    if (op.text) {
      const size = op.size ? op.size * 0.85 : 16;
      const tx = op.cx + r + 6;
      const ty = op.cy - size / 2;
      ctx.font = `600 ${size}px ui-sans-serif, system-ui, sans-serif`;
      ctx.textBaseline = "top";
      ctx.textAlign = "left";
      if (op.background === "dark") {
        const ink = textInkBox(ctx, op.text, tx, ty, size);
        const padX = 4;
        const padY = 3;
        ctx.fillStyle = "rgba(15, 23, 42, 0.82)";
        roundRect(ctx, ink.x - padX, ink.y - padY, ink.w + padX * 2, ink.h + padY * 2, 4);
        ctx.fill();
      }
      ctx.fillStyle = op.color || "#ef4444";
      ctx.fillText(op.text, tx, ty);
    }
  }

  function drawLabel(ctx, op) {
    ctx.strokeStyle = op.color;
    ctx.lineWidth = op.width || 2;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(op.x1, op.y1);
    ctx.lineTo(op.x2, op.y2);
    ctx.stroke();
    const txt = op.text || "";
    const size = op.size || 18;
    const padX = 6;
    const padY = 4;
    const bx = op.x2;
    const by = op.y2;
    ctx.font = `600 ${size}px ui-sans-serif, system-ui, sans-serif`;
    ctx.textBaseline = "top";
    ctx.textAlign = "left";
    const probe = textInkBox(ctx, txt || " ", 0, 0, size);
    const ascent = -probe.y;
    const textX = bx + padX;
    const textY = by + padY + ascent;
    const boxW = Math.max(24, probe.w) + padX * 2;
    const boxH = probe.h + padY * 2;
    if (op.background === "dark") {
      ctx.fillStyle = "rgba(15, 23, 42, 0.88)";
      roundRect(ctx, bx, by, boxW, boxH, 8);
      ctx.fill();
    } else {
      ctx.fillStyle = "rgba(255,255,255,0.92)";
      roundRect(ctx, bx, by, boxW, boxH, 8);
      ctx.fill();
      ctx.strokeStyle = op.color;
      ctx.lineWidth = 1.5;
      roundRect(ctx, bx, by, boxW, boxH, 8);
      ctx.stroke();
    }
    ctx.fillStyle = op.color;
    ctx.fillText(txt, textX, textY);
  }

  function drawWatermark(ctx, op) {
    const txt = op.text || "";
    if (!txt) return;
    const fontSize = op.fontSize || 24;
    const opacity = op.opacity ?? 0.35;
    const rotation = ((op.rotation ?? 0) * Math.PI) / 180;
    ctx.globalAlpha = opacity;
    ctx.fillStyle = op.color || "#64748b";
    ctx.font = `600 ${fontSize}px ui-sans-serif, system-ui, sans-serif`;
    if (op.mode === "tiled") {
      const nw = naturalWidth;
      const nh = naturalHeight;
      const stepX = fontSize * Math.max(4, txt.length * 0.55);
      const stepY = fontSize * 3.2;
      ctx.save();
      ctx.translate(nw / 2, nh / 2);
      ctx.rotate(rotation);
      ctx.translate(-nw / 2, -nh / 2);
      for (let y = -stepY; y < nh + stepY; y += stepY) {
        for (let x = -stepX; x < nw + stepX; x += stepX) {
          ctx.fillText(txt, x, y);
        }
      }
      ctx.restore();
    } else {
      ctx.save();
      ctx.translate(op.x, op.y);
      ctx.rotate(rotation);
      ctx.fillText(txt, 0, 0);
      ctx.restore();
    }
    ctx.globalAlpha = 1;
  }

  function drawArrow(ctx, x1, y1, x2, y2, strokeColor, width) {
    const dx = x2 - x1;
    const dy = y2 - y1;
    const len = Math.hypot(dx, dy) || 1;
    const ux = dx / len;
    const uy = dy / len;
    const head = Math.max(12 * viewToImageScale(), width * 3.5);
    ctx.strokeStyle = strokeColor;
    ctx.fillStyle = strokeColor;
    ctx.lineWidth = width;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(x1, y1);
    ctx.lineTo(x2, y2);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(x2, y2);
    ctx.lineTo(x2 - ux * head - uy * head * 0.45, y2 - uy * head + ux * head * 0.45);
    ctx.lineTo(x2 - ux * head + uy * head * 0.45, y2 - uy * head - ux * head * 0.45);
    ctx.closePath();
    ctx.fill();
  }

  function mosaicBlockSize(intensity) {
    const base = MOSAIC_BLOCK[intensity] || MOSAIC_BLOCK.standard;
    return Math.max(4, Math.round(base * viewToImageScale()));
  }

  function mosaicBlurPx(intensity) {
    const base = MOSAIC_BLUR[intensity] || MOSAIC_BLUR.standard;
    return Math.max(2, Math.round(base * viewToImageScale()));
  }

  function drawMosaicRect(ctx, img, op) {
    const x = Math.max(0, Math.round(Math.min(op.x1, op.x2)));
    const y = Math.max(0, Math.round(Math.min(op.y1, op.y2)));
    const w = Math.max(1, Math.round(Math.abs(op.x2 - op.x1)));
    const h = Math.max(1, Math.round(Math.abs(op.y2 - op.y1)));
    applyMosaicRegion(ctx, img, x, y, w, h, op.intensity || "standard", op.style || "pixel");
  }

  function drawMosaicBrush(ctx, img, op) {
    const pts = op.points || [];
    if (pts.length < 1) return;
    const brush = op.brush || 20;
    const half = brush / 2;
    const intensity = op.intensity || "standard";
    const style = op.style || "pixel";
    const step = Math.max(4, brush * 0.35);
    for (let i = 0; i < pts.length; i += 1) {
      const p = pts[i];
      applyMosaicRegion(
        ctx,
        img,
        Math.round(p.x - half),
        Math.round(p.y - half),
        Math.round(brush),
        Math.round(brush),
        intensity,
        style,
      );
      if (i > 0) {
        const a = pts[i - 1];
        const dist = Math.hypot(p.x - a.x, p.y - a.y);
        for (let d = step; d < dist; d += step) {
          const t = d / dist;
          const mx = a.x + (p.x - a.x) * t;
          const my = a.y + (p.y - a.y) * t;
          applyMosaicRegion(
            ctx,
            img,
            Math.round(mx - half),
            Math.round(my - half),
            Math.round(brush),
            Math.round(brush),
            intensity,
            style,
          );
        }
      }
    }
  }

  function applyMosaicRegion(ctx, img, x, y, w, h, intensity, style) {
    const nx = Math.max(0, x);
    const ny = Math.max(0, y);
    const nw = naturalWidth;
    const nh = naturalHeight;
    const cw = Math.min(w, nw - nx);
    const ch = Math.min(h, nh - ny);
    if (cw < 1 || ch < 1) return;

    if (style === "blur") {
      const blur = mosaicBlurPx(intensity);
      const tmp = document.createElement("canvas");
      tmp.width = cw;
      tmp.height = ch;
      const tctx = tmp.getContext("2d");
      if (!tctx) return;
      tctx.filter = `blur(${blur}px)`;
      tctx.drawImage(img, nx, ny, cw, ch, 0, 0, cw, ch);
      tctx.filter = "none";
      ctx.drawImage(tmp, 0, 0, cw, ch, nx, ny, cw, ch);
      return;
    }

    const block = mosaicBlockSize(intensity);
    const sample = document.createElement("canvas");
    sample.width = Math.max(1, Math.ceil(cw / block));
    sample.height = Math.max(1, Math.ceil(ch / block));
    const sctx = sample.getContext("2d");
    if (!sctx) return;
    sctx.imageSmoothingEnabled = false;
    sctx.drawImage(img, nx, ny, cw, ch, 0, 0, sample.width, sample.height);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(sample, 0, 0, sample.width, sample.height, nx, ny, cw, ch);
    ctx.imageSmoothingEnabled = true;
  }

  function hexToRgba(hex, alpha) {
    const raw = hex.replace("#", "");
    const full = raw.length === 3 ? raw.split("").map((c) => c + c).join("") : raw;
    const n = Number.parseInt(full, 16);
    const r = (n >> 16) & 255;
    const g = (n >> 8) & 255;
    const b = n & 255;
    return `rgba(${r},${g},${b},${alpha})`;
  }

  function renderCompositeCanvas() {
    const img = imgEl;
    if (!img || !naturalWidth || !naturalHeight) return null;
    const out = document.createElement("canvas");
    out.width = naturalWidth;
    out.height = naturalHeight;
    const ctx = out.getContext("2d");
    if (!ctx) return null;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, out.width, out.height);
    ctx.drawImage(img, 0, 0, naturalWidth, naturalHeight);
    for (const op of ops) drawOp(ctx, op, img);
    return out;
  }

  async function save() {
    if (saving || loading || !imgEl || !item?.id) return;
    saving = true;
    error = "";
    try {
      const out = renderCompositeCanvas();
      if (!out) throw new Error("Export failed");
      const blob = await new Promise((resolve, reject) => {
        const fail = new Error(exportMime === "image/png" ? "PNG export failed" : "JPEG export failed");
        if (exportMime === "image/png") {
          out.toBlob((result) => (result ? resolve(result) : reject(fail)), "image/png");
        } else {
          out.toBlob(
            (result) => (result ? resolve(result) : reject(fail)),
            "image/jpeg",
            exportJpegQuality,
          );
        }
      });
      const updated = await uploadAnnotatedBlob(item.id, blob);
      await closeMediaToken();
      onsaved?.(updated);
    } catch (err) {
      error = friendlyError(err, text);
    } finally {
      saving = false;
    }
  }

  function cancel() {
    if (saving) return;
    void closeMediaToken();
    oncancel?.();
  }

  function cancelText() {
    if (textMode === "callout" && textEditIndex >= 0) {
      // keep callout without text
    } else if (textMode === "label" && textEditIndex >= 0 && !textDraft.trim()) {
      ops = ops.filter((_, i) => i !== textEditIndex);
      redraw();
    }
    textOpen = false;
    textDraft = "";
    textEditIndex = -1;
  }

  function onTextKeydown(event) {
    if (event.key === "Enter") {
      event.preventDefault();
      commitText();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelText();
    }
  }

  function onKeydown(event) {
    if (!open) return;
    if (event.key === "Escape") {
      if (textOpen) {
        cancelText();
        return;
      }
      if (tool === "eyedropper") {
        eyedropLoupe = null;
        setTool(lastTool && lastTool !== "eyedropper" ? lastTool : "rect");
        return;
      }
      if (tool === "crop") {
        exitCropWithoutApply();
        return;
      }
      if (selectedIndex >= 0) {
        selectedIndex = -1;
        redraw();
        return;
      }
      cancel();
    }
    if (textOpen) return;
    if ((event.key === "Delete" || event.key === "Backspace") && selectedIndex >= 0) {
      event.preventDefault();
      deleteSelection();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z") {
      event.preventDefault();
      if (event.shiftKey) redo();
      else undo();
    }
  }

  onDestroy(() => {
    loadToken += 1;
    resizeObserver?.disconnect();
    void closeMediaToken();
  });
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <div class="annotator" role="dialog" aria-modal="true" aria-label={text.annotateTitle || "Annotate"}>
    <div class="stage">
      {#if error && !imageSrc}
        <div class="state error">{error}</div>
      {:else}
        <div class="frame" bind:this={frameEl}>
          <div
            class="board-wrap"
            class:placeholder={loading && !!placeholderSrc}
            style:background-image={loading && placeholderSrc ? `url(${placeholderSrc})` : "none"}
          >
            {#if imageSrc}
              <img
                bind:this={imgEl}
                class="base"
                class:pending={loading}
                src={imageSrc}
                alt=""
                draggable="false"
                onload={onImageLoad}
                onerror={onImageError}
              />
            {:else if loading}
              <div class="base-slot" aria-hidden="true"></div>
            {/if}
            <canvas
              bind:this={canvasEl}
              class="overlay"
              class:select-cursor={tool === "select"}
              class:crop-cursor={tool === "crop"}
              class:eyedropper-cursor={tool === "eyedropper"}
              onpointerdown={onPointerDown}
              onpointermove={onPointerMove}
              onpointerup={onPointerUp}
              onpointercancel={onPointerUp}
              onpointerleave={(e) => {
                if (tool !== "eyedropper") return;
                const wrap = /** @type {HTMLElement} */ (e.currentTarget).parentElement;
                if (wrap?.contains(/** @type {Node | null} */ (e.relatedTarget))) return;
                eyedropLoupe = null;
              }}
              ondblclick={onDoubleClick}
            ></canvas>
            {#if eyedropLoupe}
              <div class="eyedrop-loupe" style:left={`${eyedropLoupe.left}px`} style:top={`${eyedropLoupe.top}px`}>
                <canvas bind:this={loupeCanvasEl} class="eyedrop-glass" width="156" height="156"></canvas>
                <div class="eyedrop-info">
                  <div class="eyedrop-info-row">
                    <span class="eyedrop-swatch" style:background={eyedropLoupe.hex}></span>
                    <div class="eyedrop-mono">
                      {eyedropLoupe.hex}
                      <span class="eyedrop-coords">({eyedropLoupe.px}, {eyedropLoupe.py})</span>
                    </div>
                  </div>
                  <div class="eyedrop-mono muted">RGB {eyedropLoupe.rgb}</div>
                  <div class="eyedrop-hint">{text.eyedropHint || "Click to pick & copy"}</div>
                </div>
              </div>
            {/if}
            {#if colorCopyFlash}
              <div class="color-copy-flash">{colorCopyFlash}</div>
            {/if}
            {#if textOpen}
              <input
                bind:this={textInputEl}
                class="text-inline"
                type="text"
                maxlength="120"
                size="1"
                placeholder={textPlaceholder()}
                style:left={`${textAnchorCss.left}px`}
                style:top={`${textAnchorCss.top}px`}
                style:width={`${textAnchorCss.maxWidth}px`}
                style:max-width={`${textAnchorCss.maxWidth}px`}
                style:color={color}
                style:font-size={`${Math.max(12, fontPx)}px`}
                bind:value={textDraft}
                onkeydown={onTextKeydown}
                onblur={() => commitText()}
              />
            {/if}
            {#if loading}
              <div class="loading">{text.annotateLoading || "Loading…"}</div>
            {/if}
            {#if tool === "crop" && cropSession}
              <div class="crop-hint">{text.cropHint || "Drag handles · Double-click to apply"}</div>
            {/if}
          </div>

          <div class="chrome">
            {#if showTextOptions() || tool === "mosaic" || tool === "watermark"}
              <div class="options-strip">
                {#if tool === "text"}
                  <span class="opt-label">{text.textType || "Type"}</span>
                  <div class="opt-seg" role="group" aria-label={text.textType || "Type"}>
                    <button type="button" class="opt-seg-btn" class:active={textKind === "text"} onclick={() => setTextKind("text")}>
                      {text.textTypePlain || text.toolText}
                    </button>
                    <button type="button" class="opt-seg-btn" class:active={textKind === "callout"} onclick={() => setTextKind("callout")}>
                      {text.textTypeCallout || text.toolCallout}
                    </button>
                    <button type="button" class="opt-seg-btn" class:active={textKind === "label"} onclick={() => setTextKind("label")}>
                      {text.textTypeLabel || text.toolLabel}
                    </button>
                  </div>
                {/if}
                {#if showTextOptions()}
                  <label class="font-size" title={text.textFontSize || "Size"}>
                    <span class="opt-label">{text.textFontSize || "Size"}</span>
                    <input
                      type="range"
                      min="12"
                      max="48"
                      step="1"
                      bind:value={fontPx}
                      oninput={onFontPxInput}
                    />
                    <span class="font-size-val">{fontPx}</span>
                  </label>
                  <span class="opt-label">{text.textBg || "Text bg"}</span>
                  <div class="opt-seg" role="group" aria-label={text.textBg || "Text bg"}>
                    <button type="button" class="opt-seg-btn" class:active={textBg === "transparent"} onclick={() => setTextBg("transparent")}>
                      {text.textBgTransparent || "None"}
                    </button>
                    <button type="button" class="opt-seg-btn" class:active={textBg === "dark"} onclick={() => setTextBg("dark")}>
                      {text.textBgDark || "Dark"}
                    </button>
                  </div>
                {/if}
                {#if tool === "mosaic"}
                  <span class="opt-label">{text.mosaicShape || "Shape"}</span>
                  <div class="opt-seg" role="group" aria-label={text.mosaicShape || "Shape"}>
                    <button type="button" class="opt-seg-btn" class:active={mosaicShape === "rect"} onclick={() => (mosaicShape = "rect")}>
                      {text.mosaicRect || "Rect"}
                    </button>
                    <button type="button" class="opt-seg-btn" class:active={mosaicShape === "brush"} onclick={() => (mosaicShape = "brush")}>
                      {text.mosaicBrush || "Brush"}
                    </button>
                  </div>
                  <span class="opt-label">{text.mosaicIntensity || "Intensity"}</span>
                  <div class="opt-seg" role="group" aria-label={text.mosaicIntensity || "Intensity"}>
                    <button type="button" class="opt-seg-btn" class:active={mosaicIntensity === "soft"} onclick={() => (mosaicIntensity = "soft")}>
                      {text.mosaicSoft}
                    </button>
                    <button type="button" class="opt-seg-btn" class:active={mosaicIntensity === "standard"} onclick={() => (mosaicIntensity = "standard")}>
                      {text.mosaicStandard}
                    </button>
                    <button type="button" class="opt-seg-btn" class:active={mosaicIntensity === "strong"} onclick={() => (mosaicIntensity = "strong")}>
                      {text.mosaicStrong}
                    </button>
                  </div>
                  <span class="opt-label">{text.mosaicStyle || "Style"}</span>
                  <div class="opt-seg" role="group" aria-label={text.mosaicStyle || "Style"}>
                    <button type="button" class="opt-seg-btn" class:active={mosaicStyle === "pixel"} onclick={() => (mosaicStyle = "pixel")}>
                      {text.mosaicPixel || "Pixel"}
                    </button>
                    <button type="button" class="opt-seg-btn" class:active={mosaicStyle === "blur"} onclick={() => (mosaicStyle = "blur")}>
                      {text.mosaicBlur || "Blur"}
                    </button>
                  </div>
                {/if}
                {#if tool === "watermark"}
                  <span class="opt-label">{text.watermarkMode || "Watermark"}</span>
                  <div class="opt-seg" role="group" aria-label={text.watermarkMode || "Watermark"}>
                    <button type="button" class="opt-seg-btn" class:active={watermarkMode === "single"} onclick={() => (watermarkMode = "single")}>
                      {text.watermarkSingle || "Single"}
                    </button>
                    <button type="button" class="opt-seg-btn" class:active={watermarkMode === "tiled"} onclick={() => (watermarkMode = "tiled")}>
                      {text.watermarkTiled || "Tiled"}
                    </button>
                  </div>
                  <label class="font-size" title={text.textFontSize || "Size"}>
                    <span class="opt-label">{text.textFontSize || "Size"}</span>
                    <input
                      type="range"
                      min="12"
                      max="48"
                      step="1"
                      bind:value={fontPx}
                      oninput={onFontPxInput}
                    />
                    <span class="font-size-val">{fontPx}</span>
                  </label>
                {/if}
              </div>
            {/if}

            <div class="bar">
              <div class="dims" title={text.size?.(naturalWidth, naturalHeight) || `${naturalWidth}×${naturalHeight}`}>
                {#if naturalWidth && naturalHeight}
                  {text.size?.(naturalWidth, naturalHeight) || `${naturalWidth}×${naturalHeight}`}
                {:else}
                  —
                {/if}
              </div>

              <div class="bar-main">
                <div class="tools">
                  {#each toolDefs as [id, label]}
                    <button
                      type="button"
                      class="tool"
                      class:active={tool === id}
                      aria-label={label}
                      onmouseenter={(e) => showTip(e, label)}
                      onmouseleave={hideTip}
                      onfocus={(e) => showTip(e, label)}
                      onblur={hideTip}
                      onclick={() => {
                        hideTip();
                        setTool(/** @type {Tool} */ (id));
                      }}
                    >
                      {#if id === "select"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M4 4l7.5 16 2-6.5L20 11.5Z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"/></svg>
                      {:else if id === "pen"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                      {:else if id === "rect"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><rect x="4.5" y="6.5" width="15" height="11" rx="1" fill="none" stroke="currentColor" stroke-width="2"/></svg>
                      {:else if id === "ellipse"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><ellipse cx="12" cy="12" rx="8" ry="5.5" fill="none" stroke="currentColor" stroke-width="2"/></svg>
                      {:else if id === "line"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M5 19 19 5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
                      {:else if id === "arrow"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M7 17L17 7M17 7H9M17 7v8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                      {:else if id === "text"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M4 7V5h16v2M12 5v14M9 19h6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                      {:else if id === "mosaic"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><g fill="currentColor"><rect x="3" y="3" width="5" height="5" rx="0.5" opacity=".95"/><rect x="9.5" y="3" width="5" height="5" rx="0.5" opacity=".55"/><rect x="16" y="3" width="5" height="5" rx="0.5" opacity=".8"/><rect x="3" y="9.5" width="5" height="5" rx="0.5" opacity=".45"/><rect x="9.5" y="9.5" width="5" height="5" rx="0.5" opacity=".9"/><rect x="16" y="9.5" width="5" height="5" rx="0.5" opacity=".35"/><rect x="3" y="16" width="5" height="5" rx="0.5" opacity=".7"/><rect x="9.5" y="16" width="5" height="5" rx="0.5" opacity=".4"/><rect x="16" y="16" width="5" height="5" rx="0.5" opacity=".85"/></g></svg>
                      {:else if id === "highlight"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><rect x="3.5" y="7" width="17" height="10" rx="1.5" fill="currentColor" opacity="0.28"/><rect x="3.5" y="7" width="17" height="10" rx="1.5" fill="none" stroke="currentColor" stroke-width="2"/><path d="M7 12h10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" opacity="0.7"/></svg>
                      {:else if id === "watermark"}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M8 11.5V8.2a4 4 0 0 1 8 0v3.3" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><ellipse cx="12" cy="15" rx="7" ry="3.2" fill="none" stroke="currentColor" stroke-width="2"/><path d="M5 20.5h14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
                      {:else}
                        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 3v3H3M18 3v3h3M6 21v-3H3M18 21v-3h3" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><rect x="7" y="7" width="10" height="10" rx="1" fill="none" stroke="currentColor" stroke-width="2"/></svg>
                      {/if}
                    </button>
                  {/each}
                </div>

                <div class="colors">
                  {#each colors as swatch}
                    <button
                      type="button"
                      class="swatch"
                      class:active={toColorInputValue(color) === swatch}
                      style:background={swatch}
                      aria-label={swatch}
                      onclick={() => pickColor(swatch)}
                    ></button>
                  {/each}
                  <button
                    type="button"
                    class="swatch custom"
                    class:active={!isPresetColor(color)}
                    class:filled={!isPresetColor(color)}
                    style:--custom-color={toColorInputValue(color)}
                    aria-label={text.customColor || "Custom color"}
                    onmouseenter={(e) => showTip(e, text.customColor || "Custom color")}
                    onmouseleave={hideTip}
                    onclick={openCustomColorPicker}
                  >
                    <input
                      bind:this={customColorEl}
                      class="color-anchor"
                      type="color"
                      tabindex="-1"
                      aria-hidden="true"
                      value={toColorInputValue(color)}
                      oninput={onCustomColorInput}
                      onchange={onCustomColorInput}
                    />
                  </button>
                  <button
                    type="button"
                    class="tool eyedrop-tool"
                    class:active={tool === "eyedropper"}
                    aria-label={text.toolEyedropper}
                    onmouseenter={(e) => showTip(e, text.toolEyedropper)}
                    onmouseleave={hideTip}
                    onfocus={(e) => showTip(e, text.toolEyedropper)}
                    onblur={hideTip}
                    onclick={() => {
                      hideTip();
                      setTool("eyedropper");
                    }}
                  >
                    <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="m15.2 4.8 4 4M13.5 6.5 18 11M9.5 10.5 5 15l-1 5 5-1 4.5-4.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="m14 7 3 3" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
                  </button>
                  <label
                    class="width"
                    aria-label={text.toolWidth || "Size"}
                    onmouseenter={(e) => showTip(e, text.toolWidth || "Size")}
                    onmouseleave={hideTip}
                  >
                    <input type="range" min="2" max="14" bind:value={lineWidth} oninput={onLineWidthInput} />
                  </label>
                  <button
                    type="button"
                    class="tool glyph"
                    aria-label={text.undo}
                    onmouseenter={(e) => showTip(e, text.undo)}
                    onmouseleave={hideTip}
                    onfocus={(e) => showTip(e, text.undo)}
                    onblur={hideTip}
                    onclick={() => {
                      hideTip();
                      undo();
                    }}
                    disabled={!undoStack.length}
                  >
                    ↶
                  </button>
                  <button
                    type="button"
                    class="tool glyph"
                    aria-label={text.redo}
                    onmouseenter={(e) => showTip(e, text.redo)}
                    onmouseleave={hideTip}
                    onfocus={(e) => showTip(e, text.redo)}
                    onblur={hideTip}
                    onclick={() => {
                      hideTip();
                      redo();
                    }}
                    disabled={!redoStack.length}
                  >
                    ↷
                  </button>
                  <button
                    type="button"
                    class="tool"
                    aria-label={text.clearAnnotations}
                    onmouseenter={(e) => showTip(e, text.clearAnnotations)}
                    onmouseleave={hideTip}
                    onfocus={(e) => showTip(e, text.clearAnnotations)}
                    onblur={hideTip}
                    onclick={() => {
                      hideTip();
                      clearAnnotations();
                    }}
                    disabled={!ops.length || saving}
                  >
                    <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                  </button>
                </div>
              </div>

              <div class="actions">
                <button
                  type="button"
                  class="tool danger"
                  aria-label={text.cancel}
                  onmouseenter={(e) => showTip(e, text.cancel)}
                  onmouseleave={hideTip}
                  onfocus={(e) => showTip(e, text.cancel)}
                  onblur={hideTip}
                  onclick={() => {
                    hideTip();
                    cancel();
                  }}
                  disabled={saving}
                >
                  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
                </button>
                <button
                  type="button"
                  class="tool ok"
                  aria-label={text.annotateSave}
                  onmouseenter={(e) => showTip(e, text.annotateSave)}
                  onmouseleave={hideTip}
                  onfocus={(e) => showTip(e, text.annotateSave)}
                  onblur={hideTip}
                  onclick={() => {
                    hideTip();
                    void save();
                  }}
                  disabled={saving || loading || !naturalWidth}
                >
                  {#if saving}
                    <span class="spinner" aria-hidden="true"></span>
                  {:else}
                    <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="m5 12 5 5L20 7" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                  {/if}
                </button>
              </div>
            </div>
          </div>
        </div>
      {/if}
      {#if error && imageSrc}
        <div class="inline-error">{error}</div>
      {/if}
    </div>
  </div>
  {#if tip}
    <div class="hover-tip" style:left="{tip.x}px" style:top="{tip.y}px">{tip.label}</div>
  {/if}
{/if}

<style>
  .annotator {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    flex-direction: column;
    background: #0b1220;
    color: #f8fafc;
  }

  .stage {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: grid;
    place-items: center;
    padding: 20px;
  }

  .frame {
    display: inline-flex;
    flex-direction: column;
    align-items: stretch;
    max-width: 100%;
  }

  .board-wrap {
    position: relative;
    display: inline-block;
    max-width: 100%;
    line-height: 0;
    overflow: hidden;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.45);
    background-color: #111;
    background-position: center;
    background-repeat: no-repeat;
    background-size: contain;
  }

  .board-wrap.placeholder {
    min-width: min(720px, calc(100vw - 40px));
    min-height: min(420px, calc(100vh - 160px));
  }

  .base,
  .base-slot {
    display: block;
    max-width: min(100%, calc(100vw - 40px));
    max-height: calc(100vh - 96px);
    width: auto;
    height: auto;
    user-select: none;
    -webkit-user-drag: none;
    background: transparent;
  }

  .base.pending {
    opacity: 0;
    position: absolute;
  }

  .base-slot {
    width: min(720px, calc(100vw - 40px));
    height: min(420px, calc(100vh - 160px));
  }

  .overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    cursor: crosshair;
    touch-action: none;
  }

  .overlay.select-cursor {
    cursor: default;
  }

  .overlay.crop-cursor {
    cursor: crosshair;
  }

  .overlay.eyedropper-cursor {
    cursor: crosshair;
  }

  .eyedrop-loupe {
    position: absolute;
    z-index: 5;
    width: max-content;
    min-width: 156px;
    padding: 1px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.9);
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
    pointer-events: none;
  }

  .eyedrop-glass {
    display: block;
    width: 156px;
    height: 156px;
    background: #0b1220;
  }

  .eyedrop-info {
    padding: 6px 8px 8px;
    color: #fff;
  }

  .eyedrop-info-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .eyedrop-swatch {
    width: 14px;
    height: 14px;
    border-radius: 2px;
    border: 1px solid rgba(255, 255, 255, 0.35);
    flex: 0 0 14px;
  }

  .eyedrop-mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.35;
    letter-spacing: 0.01em;
    white-space: nowrap;
  }

  .eyedrop-coords {
    margin-left: 6px;
    color: rgba(255, 255, 255, 0.72);
  }

  .eyedrop-mono.muted {
    margin-top: 2px;
    color: rgba(255, 255, 255, 0.86);
  }

  .eyedrop-hint {
    margin-top: 4px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
    line-height: 1.3;
  }

  .color-copy-flash {
    position: absolute;
    left: 50%;
    bottom: 14px;
    z-index: 6;
    transform: translateX(-50%);
    padding: 10px 16px;
    border-radius: 10px;
    background: rgba(15, 23, 42, 0.94);
    border: 2px solid rgba(255, 255, 255, 0.28);
    color: #f8fafc;
    font-size: 13px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }

  .loading {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: rgba(11, 18, 32, 0.35);
    color: rgba(248, 250, 252, 0.9);
    font-size: 13px;
    line-height: 1.4;
    pointer-events: none;
  }

  .crop-hint {
    position: absolute;
    left: 50%;
    bottom: 12px;
    transform: translateX(-50%);
    padding: 4px 10px;
    border-radius: 6px;
    background: rgba(15, 23, 42, 0.88);
    border: 1px solid rgba(255, 255, 255, 0.15);
    color: rgba(248, 250, 252, 0.9);
    font-size: 12px;
    pointer-events: none;
    z-index: 2;
  }

  .chrome {
    position: relative;
    margin-top: 10px;
    z-index: 4;
  }

  .bar {
    display: flex;
    flex-wrap: nowrap;
    gap: 8px;
    align-items: flex-start;
    padding: 7px 10px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.12);
    backdrop-filter: blur(8px);
    overflow-x: hidden;
  }

  .dims {
    flex: 0 0 auto;
    min-width: 7ch;
    padding: 0 6px 0 2px;
    color: rgba(248, 250, 252, 0.72);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    letter-spacing: 0.01em;
    white-space: nowrap;
    line-height: 28px;
  }

  .bar-main {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  .tools,
  .colors,
  .actions {
    display: flex;
    flex-wrap: nowrap;
    gap: 4px;
    align-items: center;
    flex: 0 0 auto;
  }

  .actions {
    margin-left: auto;
  }

  .options-strip {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + 6px);
    display: flex;
    flex-wrap: nowrap;
    gap: 4px;
    align-items: center;
    padding: 6px 10px;
    border-radius: 10px;
    background: rgba(15, 23, 42, 0.94);
    border: 1px solid rgba(255, 255, 255, 0.12);
    backdrop-filter: blur(8px);
    overflow-x: auto;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    pointer-events: auto;
  }

  .opt-label {
    font-size: 11px;
    color: rgba(248, 250, 252, 0.55);
    margin-right: 2px;
    margin-left: 6px;
    white-space: nowrap;
  }

  .opt-seg {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 7px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.03);
  }

  .opt-seg-btn {
    padding: 3px 9px;
    border: 0;
    border-right: 1px solid rgba(255, 255, 255, 0.1);
    background: transparent;
    color: rgba(248, 250, 252, 0.7);
    font-size: 11px;
    line-height: 1.2;
    cursor: pointer;
  }

  .opt-seg-btn:last-child {
    border-right: 0;
  }

  .opt-seg-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(248, 250, 252, 0.92);
  }

  .opt-seg-btn.active {
    background: rgba(255, 255, 255, 0.16);
    color: #fff;
  }

  .tool {
    width: 32px;
    height: 32px;
    flex: 0 0 32px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: #e5e5e5;
    cursor: pointer;
  }

  .tool:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }

  .tool.active {
    color: #fff;
    background: rgba(255, 255, 255, 0.14);
    border-color: rgba(255, 255, 255, 0.28);
  }

  .tool.ok {
    width: 40px;
    flex-basis: 40px;
    color: #161616;
    background: #f2f2f2;
    border-color: #f2f2f2;
  }

  .tool.ok:hover:not(:disabled) {
    background: #fff;
  }

  .tool .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid color-mix(in srgb, currentColor 25%, transparent);
    border-top-color: currentColor;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .tool.danger {
    color: #fca5a5;
  }


  .tool:disabled {
    opacity: 0.32;
    cursor: default;
  }

  .tool svg {
    display: block;
    pointer-events: none;
  }

  .hover-tip {
    position: fixed;
    z-index: 10000;
    transform: translate(-50%, calc(-100% - 8px));
    padding: 4px 8px;
    border-radius: 6px;
    background: rgba(15, 23, 42, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.16);
    color: #f8fafc;
    font-size: 12px;
    line-height: 1.2;
    white-space: nowrap;
    pointer-events: none;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
  }

  .tool.glyph {
    font-size: 16px;
    line-height: 1;
  }

  .swatch {
    position: relative;
    width: 18px;
    height: 18px;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.5);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.28);
    cursor: pointer;
    flex: 0 0 auto;
    padding: 0;
    overflow: hidden;
  }

  .swatch.active {
    border-color: #fff;
    box-shadow:
      0 0 0 1px #fff,
      inset 0 0 0 1px rgba(0, 0, 0, 0.28);
  }

  .swatch.custom {
    overflow: visible;
    background: conic-gradient(
      from 0deg,
      #ef4444,
      #f59e0b,
      #22c55e,
      #3b82f6,
      #a855f7,
      #ef4444
    );
  }

  .swatch.custom.filled {
    background: var(--custom-color, #ef4444);
  }

  /* Anchor the system picker above the bar, same side as options / tips. */
  .swatch.custom .color-anchor {
    position: absolute;
    left: 50%;
    bottom: calc(100% + 14px);
    width: 32px;
    height: 32px;
    margin: 0;
    padding: 0;
    border: 0;
    transform: translateX(-50%);
    opacity: 0;
    pointer-events: none;
  }

  .width {
    display: flex;
    align-items: center;
  }

  .width input {
    width: 72px;
  }

  .font-size {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 2px;
  }

  .font-size input {
    width: 72px;
  }

  .font-size-val {
    min-width: 2ch;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: rgba(248, 250, 252, 0.72);
  }

  .state,
  .inline-error {
    color: #fca5a5;
    font-size: 13px;
  }

  .inline-error {
    margin-top: 10px;
    text-align: center;
  }

  .text-inline {
    position: absolute;
    z-index: 3;
    box-sizing: border-box;
    margin: 0;
    padding: 2px 4px;
    border: 1px dashed rgba(255, 255, 255, 0.55);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.35);
    color: #ef4444;
    font: 600 16px ui-sans-serif, system-ui, -apple-system, sans-serif;
    line-height: 1.2;
    outline: none;
    transform: translateY(-2px);
    caret-color: currentColor;
    overflow: hidden;
    text-overflow: clip;
    white-space: nowrap;
  }
</style>
