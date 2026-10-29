export function formatTime(ms) {
  if (!ms) return "";
  try {
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(ms));
  } catch {
    return new Date(ms).toLocaleString();
  }
}

export function itemMatches(item, needle, text) {
  if (!needle) return true;
  const parts = [item.label, item.displayName, item.captureMode, formatItemLabel(item, text || {})];
  const hay = parts.filter(Boolean).join(" ").toLowerCase();
  return hay.includes(needle);
}

/** Infer capture mode for older history items without `captureMode`. */
export function inferCaptureMode(item) {
  const mode = String(item?.captureMode || "").trim().toLowerCase();
  if (mode === "display" || mode === "window" || mode === "region" || mode === "delay") {
    return mode;
  }
  const name = String(item?.displayName || "").trim();
  const label = String(item?.label || "").trim();
  if (/^Region(\b|$)/i.test(name) || /^Region\s*·/i.test(label)) return "region";
  if (/^Display\s+\d+$/i.test(name)) return "display";
  if (/^Window\s+\d+$/i.test(name)) return "window";
  return "";
}

function mediaFileName(item) {
  const rel = String(item?.mediaFile || "").trim();
  if (!rel) return "";
  const parts = rel.split(/[/\\]/);
  return parts[parts.length - 1] || "";
}

/** Localized list title by capture type (屏幕/窗口/区域/延时) · filename. */
export function formatItemLabel(item, text) {
  const mode = inferCaptureMode(item);
  let title = "";
  if (mode === "region") title = text.captureRegion || "Region capture";
  else if (mode === "delay") title = text.captureDelay || "Delayed capture";
  else if (mode === "window") title = text.captureWindow || "Window capture";
  else if (mode === "display") title = text.captureScreen || "Screen capture";
  else {
    const fallback = String(item?.displayName || item?.label || "")
      .replace(/\s*·\s*\d+\s*[×x]\s*\d+\s*$/i, "")
      .trim();
    title = /^Region$/i.test(fallback)
      ? text.captureRegion || "Region capture"
      : fallback || text.title || "Screenshot";
  }
  const file = mediaFileName(item);
  return file ? `${title} · ${file}` : title;
}

/** Map sidecar / host errors to a short user-facing string. */
export function friendlyError(error, text) {
  const raw = error?.message || String(error || "");
  if (
    /Image file missing|Image blob missing|Failed to read image blob:.*No such file|os error 2|ENOENT|Path does not exist/i.test(
      raw,
    )
  ) {
    return text?.mediaMissing || "Image file is missing";
  }
  if (/The operation is insecure|SecurityError|tainted/i.test(raw)) {
    return text?.annotateExportInsecure || "Could not export the annotated image. Reload and try again.";
  }
  return raw;
}
