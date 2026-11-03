/** Shared annotation geometry / style helpers for Annotator. */

export const MOSAIC_BLOCK = { soft: 8, standard: 14, strong: 22 };
export const MOSAIC_BLUR = { soft: 6, standard: 12, strong: 20 };

export function cloneOps(list) {
  return JSON.parse(JSON.stringify(list));
}

export function distToSegment(px, py, x1, y1, x2, y2) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len2 = dx * dx + dy * dy;
  if (len2 <= 1e-6) return Math.hypot(px - x1, py - y1);
  let t = ((px - x1) * dx + (py - y1) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
}

export function constrainShift(tool, start, point) {
  if (tool === "line" || tool === "arrow" || tool === "label") {
    const dx = Math.abs(point.x - start.x);
    const dy = Math.abs(point.y - start.y);
    if (dx >= dy) return { x: point.x, y: start.y };
    return { x: start.x, y: point.y };
  }
  if (tool === "rect" || tool === "ellipse" || tool === "highlight" || tool === "mosaic") {
    const dx = point.x - start.x;
    const dy = point.y - start.y;
    const side = Math.max(Math.abs(dx), Math.abs(dy));
    return {
      x: start.x + Math.sign(dx || 1) * side,
      y: start.y + Math.sign(dy || 1) * side,
    };
  }
  return point;
}

export function nextCalloutNumber(ops) {
  let max = 0;
  for (const op of ops) {
    if (op.type === "callout" && Number.isFinite(op.number)) {
      max = Math.max(max, op.number);
    }
  }
  return max + 1;
}

export function applyStyleToOp(op, { color, width, background, size }) {
  const next = cloneOps([op])[0];
  if (color != null && next.color != null) next.color = color;
  if (width != null && next.width != null) next.width = width;
  if (next.type === "text" || next.type === "label" || next.type === "callout") {
    if (background != null) next.background = background;
    if (size != null) next.size = size;
  }
  if (next.type === "watermark") {
    if (color != null) next.color = color;
    if (size != null) next.fontSize = size;
  }
  if (next.type === "mosaic") {
    if (width != null) next.brush = width;
  }
  return next;
}

export function translateOp(op, dx, dy) {
  const next = cloneOps([op])[0];
  if ((next.type === "pen" || next.type === "mosaic") && Array.isArray(next.points)) {
    next.points = next.points.map((p) => ({ x: p.x + dx, y: p.y + dy }));
  } else if (next.type === "text" || next.type === "watermark") {
    next.x += dx;
    next.y += dy;
  } else if (next.type === "callout") {
    next.cx += dx;
    next.cy += dy;
  } else {
    if (next.x1 != null) next.x1 += dx;
    if (next.y1 != null) next.y1 += dy;
    if (next.x2 != null) next.x2 += dx;
    if (next.y2 != null) next.y2 += dy;
  }
  return next;
}

export function opBounds(op, measureCtx) {
  if (op.type === "pen" || (op.type === "mosaic" && op.points)) {
    const pts = op.points || [];
    if (!pts.length) return null;
    let minX = pts[0].x;
    let minY = pts[0].y;
    let maxX = pts[0].x;
    let maxY = pts[0].y;
    for (const p of pts) {
      minX = Math.min(minX, p.x);
      minY = Math.min(minY, p.y);
      maxX = Math.max(maxX, p.x);
      maxY = Math.max(maxY, p.y);
    }
    const pad = (op.width || op.brush || 4) / 2 + (op.type === "mosaic" ? (op.brush || 20) / 2 : 0);
    return { x: minX - pad, y: minY - pad, w: maxX - minX + pad * 2, h: maxY - minY + pad * 2 };
  }
  if (op.type === "text" || op.type === "watermark") {
    const size = op.size || op.fontSize || 20;
    let tw = (op.text || "").length * size * 0.6;
    if (measureCtx) {
      measureCtx.save();
      measureCtx.font = `600 ${size}px ui-sans-serif, system-ui, -apple-system, sans-serif`;
      tw = measureCtx.measureText(op.text || " ").width;
      measureCtx.restore();
    }
    return { x: op.x, y: op.y, w: Math.max(12, tw), h: size * 1.25 };
  }
  if (op.type === "callout") {
    const r = op.size || 18;
    return { x: op.cx - r, y: op.cy - r, w: r * 2, h: r * 2 };
  }
  if (op.type === "label") {
    const x = Math.min(op.x1, op.x2);
    const y = Math.min(op.y1, op.y2);
    return {
      x,
      y,
      w: Math.max(24, Math.abs(op.x2 - op.x1)),
      h: Math.max(24, Math.abs(op.y2 - op.y1)),
    };
  }
  if (op.x1 == null) return null;
  const x = Math.min(op.x1, op.x2);
  const y = Math.min(op.y1, op.y2);
  return { x, y, w: Math.abs(op.x2 - op.x1), h: Math.abs(op.y2 - op.y1) };
}

export function hitTestOps(ops, point, tol, measureCtx) {
  for (let i = ops.length - 1; i >= 0; i -= 1) {
    const op = ops[i];
    if (op.type === "line" || op.type === "arrow" || op.type === "label") {
      if (distToSegment(point.x, point.y, op.x1, op.y1, op.x2, op.y2) <= tol + (op.width || 0) / 2) {
        return i;
      }
      if (op.type === "label") {
        const b = opBounds(op, measureCtx);
        if (
          b &&
          point.x >= b.x - tol &&
          point.x <= b.x + b.w + tol &&
          point.y >= b.y - tol &&
          point.y <= b.y + b.h + tol
        ) {
          return i;
        }
      }
      continue;
    }
    if (op.type === "pen" || (op.type === "mosaic" && op.points)) {
      const pts = op.points || [];
      const brush = (op.brush || op.width || 4) / 2;
      for (let j = 1; j < pts.length; j += 1) {
        if (
          distToSegment(point.x, point.y, pts[j - 1].x, pts[j - 1].y, pts[j].x, pts[j].y) <=
          tol + brush
        ) {
          return i;
        }
      }
      continue;
    }
    if (op.type === "callout") {
      if (Math.hypot(point.x - op.cx, point.y - op.cy) <= (op.size || 18) + tol) return i;
      continue;
    }
    if (op.type === "ellipse") {
      const cx = (op.x1 + op.x2) / 2;
      const cy = (op.y1 + op.y2) / 2;
      const rx = Math.abs(op.x2 - op.x1) / 2;
      const ry = Math.abs(op.y2 - op.y1) / 2;
      if (rx < 1 || ry < 1) continue;
      const d = Math.hypot((point.x - cx) / rx, (point.y - cy) / ry);
      const ring = Math.abs(d - 1) * Math.min(rx, ry);
      if (ring <= tol + (op.width || 0) / 2) return i;
      continue;
    }
    const b = opBounds(op, measureCtx);
    if (!b) continue;
    if (
      point.x >= b.x - tol &&
      point.x <= b.x + b.w + tol &&
      point.y >= b.y - tol &&
      point.y <= b.y + b.h + tol
    ) {
      return i;
    }
  }
  return -1;
}

/** Crop handle ids → cursor. */
export const CROP_HANDLES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

export function cropHandleAt(rect, point, tol) {
  const { x, y, w, h } = rect;
  const pts = {
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
    if (Math.hypot(point.x - pts[id].x, point.y - pts[id].y) <= tol) return id;
  }
  if (point.x >= x && point.x <= x + w && point.y >= y && point.y <= y + h) return "move";
  return null;
}

/** Keep rect fully inside image; size first, then shift. */
function clampCropRect(rect, bounds) {
  const min = 8;
  const maxW = Math.max(min, bounds.w);
  const maxH = Math.max(min, bounds.h);
  let w = Math.max(min, Math.min(rect.w, maxW));
  let h = Math.max(min, Math.min(rect.h, maxH));
  let x = Math.max(0, Math.min(rect.x, bounds.w - w));
  let y = Math.max(0, Math.min(rect.y, bounds.h - h));
  return { x, y, w, h };
}

/**
 * Shrink w/h to fit aspect inside the available room from a fixed edge/corner,
 * then place the rect without leaving image bounds.
 */
function placeAspectRect(anchor, handle, w, h, aspect, bounds, min) {
  let aw = Math.max(min, w);
  let ah = aw / aspect;
  if (ah < min) {
    ah = min;
    aw = ah * aspect;
  }

  const maxW = handle.includes("w") ? anchor.x : bounds.w - anchor.x;
  const maxH = handle.includes("n") ? anchor.y : bounds.h - anchor.y;
  // Edge handles also grow the orthogonal axis around center — handled by callers.
  aw = Math.min(aw, Math.max(min, maxW));
  ah = aw / aspect;
  if (ah > maxH) {
    ah = Math.max(min, maxH);
    aw = ah * aspect;
  }
  if (aw > maxW) {
    aw = Math.max(min, maxW);
    ah = aw / aspect;
  }

  let x = handle.includes("w") ? anchor.x - aw : anchor.x;
  let y = handle.includes("n") ? anchor.y - ah : anchor.y;
  return clampCropRect({ x, y, w: aw, h: ah }, bounds);
}

/** Keep center, fit aspect into current box then clamp to image bounds. */
export function fitCropAspect(rect, aspect, bounds) {
  if (!(aspect > 0) || !rect) return rect;
  const cx = rect.x + rect.w / 2;
  const cy = rect.y + rect.h / 2;
  let w = rect.w;
  let h = w / aspect;
  if (h > rect.h) {
    h = rect.h;
    w = h * aspect;
  }
  w = Math.min(w, bounds.w);
  h = w / aspect;
  if (h > bounds.h) {
    h = bounds.h;
    w = h * aspect;
  }
  return clampCropRect({ x: cx - w / 2, y: cy - h / 2, w, h }, bounds);
}

export function resizeCrop(rect, handle, point, bounds, aspect = null) {
  const min = 8;
  const bw = Math.max(min, bounds.w);
  const bh = Math.max(min, bounds.h);
  // Keep the active pointer inside the image so the box never leaves.
  const px = Math.max(0, Math.min(bw, point.x));
  const py = Math.max(0, Math.min(bh, point.y));
  const right = rect.x + rect.w;
  const bottom = rect.y + rect.h;

  if (!(aspect > 0)) {
    let nx1 = rect.x;
    let ny1 = rect.y;
    let nx2 = right;
    let ny2 = bottom;
    // Clamp each moving edge against the fixed opposite edge + image bounds.
    if (handle.includes("n")) ny1 = Math.max(0, Math.min(py, bottom - min));
    if (handle.includes("s")) ny2 = Math.min(bh, Math.max(py, rect.y + min));
    if (handle.includes("w")) nx1 = Math.max(0, Math.min(px, right - min));
    if (handle.includes("e")) nx2 = Math.min(bw, Math.max(px, rect.x + min));
    return {
      x: Math.min(nx1, nx2),
      y: Math.min(ny1, ny2),
      w: Math.max(min, Math.abs(nx2 - nx1)),
      h: Math.max(min, Math.abs(ny2 - ny1)),
    };
  }

  if (handle === "e" || handle === "w") {
    const fixedX = handle === "e" ? rect.x : right;
    let w = handle === "e" ? px - fixedX : fixedX - px;
    w = Math.max(min, w);
    w = Math.min(w, handle === "e" ? bw - fixedX : fixedX);
    let h = w / aspect;
    const cy = rect.y + rect.h / 2;
    // Limit height so the centered box stays inside top/bottom.
    const maxH = 2 * Math.min(cy, bh - cy);
    if (h > maxH) {
      h = Math.max(min, maxH);
      w = h * aspect;
      w = Math.min(w, handle === "e" ? bw - fixedX : fixedX);
      h = w / aspect;
    }
    const x = handle === "e" ? fixedX : fixedX - w;
    const y = cy - h / 2;
    return clampCropRect({ x, y, w, h }, bounds);
  }

  if (handle === "n" || handle === "s") {
    const fixedY = handle === "s" ? rect.y : bottom;
    let h = handle === "s" ? py - fixedY : fixedY - py;
    h = Math.max(min, h);
    h = Math.min(h, handle === "s" ? bh - fixedY : fixedY);
    let w = h * aspect;
    const cx = rect.x + rect.w / 2;
    const maxW = 2 * Math.min(cx, bw - cx);
    if (w > maxW) {
      w = Math.max(min, maxW);
      h = w / aspect;
      h = Math.min(h, handle === "s" ? bh - fixedY : fixedY);
      w = h * aspect;
    }
    const x = cx - w / 2;
    const y = handle === "s" ? fixedY : fixedY - h;
    return clampCropRect({ x, y, w, h }, bounds);
  }

  const anchor = {
    nw: { x: right, y: bottom },
    ne: { x: rect.x, y: bottom },
    se: { x: rect.x, y: rect.y },
    sw: { x: right, y: rect.y },
  }[handle] || { x: rect.x, y: rect.y };

  const dx = Math.abs(px - anchor.x);
  const dy = Math.abs(py - anchor.y);
  let w = Math.max(min, dx);
  let h = w / aspect;
  if (h < dy) {
    h = Math.max(min, dy);
    w = h * aspect;
  }
  return placeAspectRect(anchor, handle, w, h, aspect, bounds, min);
}

export function moveCrop(rect, dx, dy, bounds) {
  const w = Math.min(rect.w, bounds.w);
  const h = Math.min(rect.h, bounds.h);
  const x = Math.max(0, Math.min(rect.x + dx, bounds.w - w));
  const y = Math.max(0, Math.min(rect.y + dy, bounds.h - h));
  return { x, y, w, h };
}
