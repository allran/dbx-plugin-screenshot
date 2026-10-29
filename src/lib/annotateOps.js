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

export function resizeCrop(rect, handle, point, bounds) {
  let { x, y, w, h } = rect;
  const right = x + w;
  const bottom = y + h;
  let nx1 = x;
  let ny1 = y;
  let nx2 = right;
  let ny2 = bottom;
  if (handle.includes("n")) ny1 = point.y;
  if (handle.includes("s")) ny2 = point.y;
  if (handle.includes("w")) nx1 = point.x;
  if (handle.includes("e")) nx2 = point.x;
  let out = {
    x: Math.min(nx1, nx2),
    y: Math.min(ny1, ny2),
    w: Math.max(8, Math.abs(nx2 - nx1)),
    h: Math.max(8, Math.abs(ny2 - ny1)),
  };
  out.x = Math.max(0, Math.min(out.x, bounds.w - 8));
  out.y = Math.max(0, Math.min(out.y, bounds.h - 8));
  out.w = Math.min(out.w, bounds.w - out.x);
  out.h = Math.min(out.h, bounds.h - out.y);
  return out;
}

export function moveCrop(rect, dx, dy, bounds) {
  const x = Math.max(0, Math.min(rect.x + dx, bounds.w - rect.w));
  const y = Math.max(0, Math.min(rect.y + dy, bounds.h - rect.h));
  return { ...rect, x, y };
}
