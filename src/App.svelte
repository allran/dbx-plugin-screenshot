<script>
  import { onDestroy, onMount } from "svelte";
  import Annotator from "./components/Annotator.svelte";
  import ClearModal from "./components/ClearModal.svelte";
  import ConfirmModal from "./components/ConfirmModal.svelte";
  import DelayCountdown from "./components/DelayCountdown.svelte";
  import HistoryItem from "./components/HistoryItem.svelte";
  import ImageLightbox from "./components/ImageLightbox.svelte";
  import SettingsDrawer from "./components/SettingsDrawer.svelte";
  import Toast from "./components/Toast.svelte";
  import WindowPicker from "./components/WindowPicker.svelte";
  import { friendlyError, itemMatches } from "./lib/format.js";
  import { resolveMessages } from "./lib/i18n.js";

  const DEFAULT_PREFS = {
    delaySecs: 5,
    openAnnotatorAfterCapture: true,
    copyAfterSave: true,
    defaultDisplayIndex: null,
    fileFormat: "jpeg",
    quality: "high",
    thumbSize: "medium",
    historyCount: 0,
    historyMax: 200,
  };

  let text = $state(resolveMessages("en"));
  let items = $state([]);
  let displays = $state([]);
  let windows = $state([]);
  let displayIndex = $state(null);
  let delayCountdownOpen = $state(false);
  let delayCountdownSecs = $state(DEFAULT_PREFS.delaySecs);
  let delayCountdownTimer = 0;
  let windowPickerOpen = $state(false);
  let windowPickerLoading = $state(false);
  let windowPreviewGeneration = 0;
  let bootError = $state("");
  let capturing = $state(false);
  let captureMode = $state("");
  let refreshing = $state(false);
  let clearOpen = $state(false);
  let clearing = $state(false);
  let settingsOpen = $state(false);
  let prefsSaving = $state(false);
  /** @type {typeof DEFAULT_PREFS} */
  let prefs = $state({ ...DEFAULT_PREFS });
  let toastMessage = $state("");
  let toastVisible = $state(false);
  /** @type {"" | "loading" | "success"} */
  let toastIcon = $state("");
  /** @type {null | { src: string, loading?: boolean }} */
  let preview = $state(null);
  /** @type {null | Record<string, any>} */
  let annotateItem = $state(null);
  /** When true, cancel deletes the item (fresh capture not yet confirmed). */
  let annotateDiscardOnCancel = $state(false);
  let tab = $state("all");
  let query = $state("");

  /** @type {{ granted: boolean, canRequest: boolean, platform: string, message: string } | null} */
  let permission = $state(null);
  let requestingPermission = $state(false);
  let recheckingPermission = $state(false);
  /** @type {null | "request" | "settings"} */
  let permissionConfirmKind = $state(null);

  let toastTimer = 0;
  /** @type {Map<string, string>} */
  const mediaCache = new Map();
  /** @type {string | null} */
  let mediaToken = null;
  /** @type {null | (() => void)} */
  let unsubscribeEvents = null;

  const permissionConfirmOpen = $derived(permissionConfirmKind != null);
  const permissionConfirmTitle = $derived(
    permissionConfirmKind === "settings"
      ? text.permissionSettingsConfirmTitle
      : text.permissionRequestConfirmTitle,
  );
  const permissionConfirmBody = $derived(
    permissionConfirmKind === "settings"
      ? text.permissionSettingsConfirmBody
      : text.permissionRequestConfirmBody,
  );
  const permissionConfirmAction = $derived(
    permissionConfirmKind === "settings"
      ? text.permissionSettingsConfirmAction
      : text.permissionRequestConfirmAction,
  );

  const permissionGranted = $derived(permission?.granted === true);
  const favoriteCount = $derived(items.filter((item) => item.favorite).length);
  const tabs = $derived([
    { id: "all", label: text.tabAll },
    { id: "favorite", label: text.tabFavorite(favoriteCount) },
  ]);
  const needle = $derived(query.trim().toLowerCase());
  const favoritesMode = $derived(tab === "favorite");
  const filtered = $derived(
    items.filter((item) => {
      if (tab === "favorite" && !item.favorite) return false;
      return itemMatches(item, needle, text);
    }),
  );
  const hasFilter = $derived(Boolean(needle) || tab !== "all");

  function applyLocale() {
    text = resolveMessages(window.dbxPlugin?.locale || navigator.language || "en");
  }

  function normalizePrefs(raw) {
    const next = { ...DEFAULT_PREFS, ...(raw || {}) };
    next.delaySecs = [3, 5, 10].includes(Number(next.delaySecs)) ? Number(next.delaySecs) : 5;
    next.openAnnotatorAfterCapture = next.openAnnotatorAfterCapture !== false;
    next.copyAfterSave = next.copyAfterSave !== false;
    next.defaultDisplayIndex =
      next.defaultDisplayIndex === null || next.defaultDisplayIndex === undefined || next.defaultDisplayIndex === ""
        ? null
        : Number(next.defaultDisplayIndex);
    if (!Number.isFinite(next.defaultDisplayIndex)) next.defaultDisplayIndex = null;
    next.fileFormat = next.fileFormat === "png" ? "png" : "jpeg";
    next.quality = ["high", "medium", "low"].includes(next.quality) ? next.quality : "high";
    next.thumbSize = ["small", "medium", "large"].includes(next.thumbSize) ? next.thumbSize : "medium";
    next.historyCount = Number(next.historyCount) || 0;
    next.historyMax = [50, 100, 200, 500].includes(Number(next.historyMax))
      ? Number(next.historyMax)
      : 200;
    return next;
  }

  async function loadPrefs() {
    try {
      const result = await window.dbxPlugin.invoke("screenshot/preferences/get", {});
      prefs = normalizePrefs(result);
      applyDefaultDisplayFromPrefs();
    } catch {
      prefs = { ...DEFAULT_PREFS };
    }
  }

  function applyDefaultDisplayFromPrefs() {
    if (prefs.defaultDisplayIndex == null || !displays.length) return;
    const found = displays.find((d) => Number(d.index) === Number(prefs.defaultDisplayIndex));
    if (found) displayIndex = found.index;
  }

  async function patchPrefs(patch, options = {}) {
    prefsSaving = true;
    try {
      const result = await window.dbxPlugin.invoke("screenshot/preferences/set", patch);
      // Keep storage meta from previous get; set only returns prefs fields.
      prefs = normalizePrefs({
        ...prefs,
        ...result,
        historyCount: items.length,
      });
      if (Object.prototype.hasOwnProperty.call(patch, "defaultDisplayIndex")) {
        applyDefaultDisplayFromPrefs();
        if (prefs.defaultDisplayIndex != null) displayIndex = prefs.defaultDisplayIndex;
      }
      if (Object.prototype.hasOwnProperty.call(patch, "historyMax")) {
        await loadList();
        prefs = { ...prefs, historyCount: items.length };
      }
      if (!options.silent) showToast(text.settingsSaved, { icon: "success" });
    } catch (error) {
      toastError(error);
    } finally {
      prefsSaving = false;
    }
  }

  async function onDisplaySelectChange(event) {
    const value = event.currentTarget.value;
    displayIndex = value === "" ? null : Number(value);
    if (!Number.isFinite(displayIndex)) return;
    await patchPrefs({ defaultDisplayIndex: displayIndex }, { silent: true });
  }

  async function openDataFolder() {
    try {
      await window.dbxPlugin.invoke("screenshot/open-data-folder", {});
      showToast(text.settingsFolderOpened, { icon: "success" });
    } catch (error) {
      toastError(error);
    }
  }

  async function maybeCopyAfterSave(item) {
    if (!prefs.copyAfterSave || !item?.id) return false;
    try {
      await window.dbxPlugin.invoke("screenshot/copy", { id: item.id });
      return true;
    } catch {
      return false;
    }
  }

  function clearHistoryFromSettings() {
    settingsOpen = false;
    clearOpen = true;
  }

  /**
   * @param {string} message
   * @param {number | { durationMs?: number, icon?: "" | "loading" | "success" }} [options]
   */
  function showToast(message, options = 1800) {
    const durationMs = typeof options === "number" ? options : (options?.durationMs ?? 1800);
    const icon = typeof options === "number" ? "" : (options?.icon ?? "");
    toastMessage = message;
    toastIcon = icon;
    toastVisible = true;
    clearTimeout(toastTimer);
    toastTimer = 0;
    if (durationMs > 0) {
      toastTimer = setTimeout(() => {
        toastVisible = false;
        toastIcon = "";
      }, durationMs);
    }
  }

  function toastError(error) {
    const msg = friendlyError(error, text);
    showToast(msg === text.mediaMissing ? msg : `${text.error}: ${msg}`);
  }

  async function releaseMediaToken() {
    const token = mediaToken;
    mediaToken = null;
    if (!token || typeof window.dbxPlugin?.media?.close !== "function") return;
    try {
      await window.dbxPlugin.media.close(token);
    } catch {
      // Token may already be expired/closed.
    }
  }

  function closePreview() {
    preview = null;
    void releaseMediaToken();
  }

  async function loadList() {
    const result = await window.dbxPlugin.invoke("screenshot/list", {});
    items = Array.isArray(result?.items) ? result.items : [];
  }

  async function loadPermission() {
    const result = await window.dbxPlugin.invoke("screenshot/permission", {});
    permission = {
      granted: Boolean(result?.granted),
      canRequest: Boolean(result?.canRequest),
      platform: result?.platform || "",
      message: result?.message || "",
    };
    return permission;
  }

  async function loadDisplays() {
    if (!permissionGranted) {
      displays = [];
      return;
    }
    try {
      const result = await window.dbxPlugin.invoke("screenshot/displays", {});
      displays = Array.isArray(result?.displays) ? result.displays : [];
      if (displays.length) {
        const preferred =
          prefs.defaultDisplayIndex != null
            ? displays.find((d) => Number(d.index) === Number(prefs.defaultDisplayIndex))
            : null;
        if (preferred) {
          displayIndex = preferred.index;
        } else if (displayIndex == null) {
          const primary = displays.find((d) => d.isPrimary);
          displayIndex = primary ? primary.index : displays[0].index;
        } else {
          const stillThere = displays.find((d) => Number(d.index) === Number(displayIndex));
          if (!stillThere) {
            const primary = displays.find((d) => d.isPrimary);
            displayIndex = primary ? primary.index : displays[0].index;
          }
        }
      }
      bootError = "";
    } catch (error) {
      displays = [];
      bootError = error?.message || String(error);
    }
  }

  async function loadWindowList() {
    if (!permissionGranted) {
      windows = [];
      return;
    }
    windowPickerLoading = true;
    try {
      // Fast path: titles only. Previews fill in progressively afterwards.
      const result = await window.dbxPlugin.invoke("screenshot/windows", {});
      windows = Array.isArray(result?.windows) ? result.windows : [];
    } catch (error) {
      windows = [];
      bootError = error?.message || String(error);
      showToast(`${text.error}: ${bootError}`);
    } finally {
      windowPickerLoading = false;
    }
  }

  async function fillWindowPreviews(ids) {
    const generation = ++windowPreviewGeneration;
    const queue = ids.slice();
    const workers = Array.from({ length: Math.min(4, queue.length || 1) }, async () => {
      while (queue.length) {
        if (generation !== windowPreviewGeneration || !windowPickerOpen) return;
        const id = queue.shift();
        if (id == null) return;
        try {
          const result = await window.dbxPlugin.invoke("screenshot/window/preview", { id });
          if (generation !== windowPreviewGeneration || !windowPickerOpen) return;
          const previewDataUrl = result?.window?.previewDataUrl;
          windows = windows.map((item) =>
            item.id === id
              ? {
                  ...item,
                  previewDataUrl: previewDataUrl || item.previewDataUrl,
                  previewFailed: !previewDataUrl,
                }
              : item,
          );
        } catch {
          if (generation !== windowPreviewGeneration || !windowPickerOpen) return;
          windows = windows.map((item) =>
            item.id === id ? { ...item, previewFailed: true } : item,
          );
        }
      }
    });
    await Promise.all(workers);
  }

  async function loadTargets() {
    await loadDisplays();
  }

  async function openWindowPicker() {
    if (!(await ensurePermissionForCapture())) return;
    windowPickerOpen = true;
    windows = [];
    await loadWindowList();
    const ids = windows.map((item) => item.id);
    // Don't block the picker UI on thumbnail generation.
    void fillWindowPreviews(ids);
  }

  function closeWindowPicker() {
    if (capturing) return;
    windowPreviewGeneration += 1;
    windowPickerOpen = false;
  }

  async function ensurePermissionForCapture() {
    const current = permission || (await loadPermission());
    if (current?.granted) return true;
    showToast(text.permissionBlockedCapture);
    // Prompt inside the plugin first; never jump straight to the OS dialog.
    promptPermissionRequest();
    return false;
  }

  function promptPermissionRequest() {
    settingsOpen = false;
    permissionConfirmKind = "request";
  }

  function promptPermissionSettings() {
    settingsOpen = false;
    // Already granted: open System Settings directly. Otherwise confirm first.
    if (permission?.granted) {
      void openPermissionSettings();
      return;
    }
    permissionConfirmKind = "settings";
  }

  function cancelPermissionConfirm() {
    if (requestingPermission) return;
    permissionConfirmKind = null;
  }

  async function runCapture(mode, options = {}) {
    if (!(await ensurePermissionForCapture())) return;

    capturing = true;
    captureMode = mode;
    bootError = "";
    if (mode === "region" || mode === "delay") {
      // Select first; backend event switches toast to "capturing…" after the drag ends.
      showToast(text.selectingRegion, { durationMs: 0 });
    } else {
      showToast(text.capturing, { durationMs: 0, icon: "loading" });
    }
    try {
      const selectedDisplay =
        displayIndex === null || displayIndex === "" || displayIndex === undefined
          ? undefined
          : Number(displayIndex);
      const selectedWindow =
        options.windowId === null || options.windowId === undefined
          ? undefined
          : Number(options.windowId);
      const result = await window.dbxPlugin.invoke("screenshot/capture", {
        mode,
        displayIndex: Number.isFinite(selectedDisplay) ? selectedDisplay : undefined,
        windowId: Number.isFinite(selectedWindow) ? selectedWindow : undefined,
        delaySecs: Number(options.delaySecs) || 0,
      });
      if (result?.cancelled) {
        showToast(text.cancelled);
        return;
      }
      if (result?.item) {
        if (mode === "window") {
          windowPickerOpen = false;
        }
        if (prefs.openAnnotatorAfterCapture) {
          openAnnotator(result.item, { discardOnCancel: true });
        } else {
          const copied = await maybeCopyAfterSave(result.item);
          showToast(copied ? text.copiedAfterSave : text.captured, { icon: "success" });
          await loadList();
          prefs = { ...prefs, historyCount: items.length };
        }
      } else {
        await loadList();
      }
    } catch (error) {
      const message = error?.message || String(error);
      bootError = message;
      if (/permission|Screen Recording|屏幕录制/i.test(message)) {
        await loadPermission();
      }
      showToast(`${text.error}: ${message}`);
    } finally {
      capturing = false;
      captureMode = "";
    }
  }

  async function ensureMediaExists(item) {
    await window.dbxPlugin.invoke("filesystem/media/read", { id: item.id, head: true });
  }

  async function openAnnotator(item, options = {}) {
    if (!item?.id) return;
    // Fresh captures just wrote the file — skip probe.
    if (!options.discardOnCancel) {
      try {
        await ensureMediaExists(item);
      } catch (error) {
        toastError(error);
        return;
      }
    }
    preview = null;
    void releaseMediaToken();
    toastVisible = false;
    toastIcon = "";
    clearTimeout(toastTimer);
    annotateDiscardOnCancel = Boolean(options.discardOnCancel);
    annotateItem = item;
  }

  async function onAnnotatorCancel() {
    const item = annotateItem;
    const discard = annotateDiscardOnCancel;
    annotateItem = null;
    annotateDiscardOnCancel = false;
    if (!discard || !item?.id) return;
    try {
      await window.dbxPlugin.invoke("screenshot/delete", { id: item.id });
      mediaCache.delete(item.id);
    } catch {
      // Best-effort discard; list refresh still runs.
    }
    await loadList();
  }

  async function onAnnotatorSaved(updated) {
    const fromCapture = annotateDiscardOnCancel;
    const itemId = updated?.id || annotateItem?.id;
    mediaCache.delete(itemId);
    annotateItem = null;
    annotateDiscardOnCancel = false;
    const copied = await maybeCopyAfterSave({ id: itemId });
    const base = fromCapture ? text.captured : text.annotateSaved;
    showToast(copied ? text.copiedAfterSave || base : base, { icon: "success" });
    await loadList();
    prefs = { ...prefs, historyCount: items.length };
  }

  async function selectWindow(win) {
    if (win?.id == null) return;
    await runCapture("window", { windowId: win.id });
  }

  function clearDelayCountdownTimer() {
    if (delayCountdownTimer) {
      clearInterval(delayCountdownTimer);
      delayCountdownTimer = 0;
    }
  }

  async function startDelayedCapture() {
    if (capturing || delayCountdownOpen) return;
    if (!(await ensurePermissionForCapture())) return;

    clearDelayCountdownTimer();
    delayCountdownSecs = prefs.delaySecs || 5;
    delayCountdownOpen = true;

    delayCountdownTimer = setInterval(() => {
      if (delayCountdownSecs <= 1) {
        clearDelayCountdownTimer();
        delayCountdownOpen = false;
        void runCapture("delay");
        return;
      }
      delayCountdownSecs -= 1;
    }, 1000);
  }

  function cancelDelayedCapture() {
    clearDelayCountdownTimer();
    delayCountdownOpen = false;
    delayCountdownSecs = prefs.delaySecs || DEFAULT_PREFS.delaySecs;
  }

  async function requestPermission() {
    requestingPermission = true;
    try {
      const result = await window.dbxPlugin.invoke("screenshot/permission/request", {
        confirmed: true,
      });
      permission = {
        granted: Boolean(result?.granted),
        canRequest: Boolean(result?.canRequest),
        platform: result?.platform || "",
        message: result?.message || "",
      };
      permissionConfirmKind = null;
      if (permission.granted) {
        showToast(text.permissionGranted);
        await loadTargets();
      } else {
        // Backend opens System Settings when the OS dialog is suppressed.
        showToast(text.permissionRequestNoDialog || text.permissionBlockedCapture);
      }
    } catch (error) {
      showToast(`${text.error}: ${error.message}`);
    } finally {
      requestingPermission = false;
    }
  }

  async function openPermissionSettings() {
    requestingPermission = true;
    try {
      await window.dbxPlugin.invoke("screenshot/permission/open-settings", {
        confirmed: true,
      });
      permissionConfirmKind = null;
    } catch (error) {
      showToast(`${text.error}: ${error.message}`);
    } finally {
      requestingPermission = false;
    }
  }

  async function confirmPermissionAction() {
    if (permissionConfirmKind === "settings") {
      await openPermissionSettings();
      return;
    }
    if (permissionConfirmKind === "request") {
      await requestPermission();
    }
  }

  async function recheckPermission() {
    recheckingPermission = true;
    try {
      await loadPermission();
      if (permissionGranted) {
        showToast(text.permissionGranted);
        await loadTargets();
      } else {
        showToast(permission?.message || text.permissionBlockedCapture);
      }
    } catch (error) {
      showToast(`${text.error}: ${error.message}`);
    } finally {
      recheckingPermission = false;
    }
  }

  async function copyItem(item) {
    try {
      await window.dbxPlugin.invoke("screenshot/copy", { id: item.id });
      showToast(text.copied);
    } catch (error) {
      toastError(error);
    }
  }

  async function deleteItem(item) {
    try {
      await window.dbxPlugin.invoke("screenshot/delete", { id: item.id });
      mediaCache.delete(item.id);
      showToast(text.deleted);
      await loadList();
    } catch (error) {
      showToast(`${text.error}: ${error.message}`);
    }
  }

  async function toggleFavorite(item) {
    try {
      const result = await window.dbxPlugin.invoke("screenshot/favorite", { id: item.id });
      showToast(result?.favorite ? text.favorited : text.unfavorited);
      await loadList();
    } catch (error) {
      showToast(`${text.error}: ${error.message}`);
    }
  }

  async function removeFavorite(item) {
    if (!item.favorite) return;
    try {
      await window.dbxPlugin.invoke("screenshot/favorite", { id: item.id });
      showToast(text.unfavorited);
      await loadList();
    } catch (error) {
      showToast(`${text.error}: ${error.message}`);
    }
  }

  async function revealItem(item) {
    try {
      await window.dbxPlugin.invoke("screenshot/reveal", { id: item.id });
      showToast(text.revealed);
    } catch (error) {
      toastError(error);
    }
  }

  async function previewImage(item) {
    const cached = mediaCache.get(item.id);
    if (cached) {
      await releaseMediaToken();
      preview = { src: cached, loading: false };
      return;
    }
    try {
      await ensureMediaExists(item);
    } catch (error) {
      toastError(error);
      return;
    }
    // Show the list thumbnail immediately, then open the stored file.
    await releaseMediaToken();
    preview = {
      src: item.previewDataUrl || "",
      loading: true,
    };
    try {
      // Prefer host media URL: browser loads the stored JPEG directly (no UI-bridge base64).
      if (
        window.dbxPlugin.capabilities?.mediaUrl &&
        typeof window.dbxPlugin.media?.open === "function"
      ) {
        const opened = await window.dbxPlugin.media.open("filesystem/media/read", {
          id: item.id,
        });
        if (opened?.url) {
          mediaToken = opened.token ?? null;
          preview = { src: opened.url, loading: false };
          return;
        }
      }

      const result = await window.dbxPlugin.invoke("screenshot/media", { id: item.id });
      if (result?.dataUrl) {
        mediaCache.set(item.id, result.dataUrl);
        preview = { src: result.dataUrl, loading: false };
      } else {
        preview = preview ? { ...preview, loading: false } : null;
      }
    } catch (error) {
      preview = null;
      void releaseMediaToken();
      toastError(error);
    }
  }

  async function refresh() {
    refreshing = true;
    try {
      await loadPermission();
      await Promise.all([loadTargets(), loadList()]);
      showToast(text.refreshed, { icon: "success" });
    } catch (error) {
      showToast(`${text.error}: ${error.message}`);
    } finally {
      refreshing = false;
    }
  }

  async function performClear() {
    clearing = true;
    try {
      await window.dbxPlugin.invoke("screenshot/clear", {});
      mediaCache.clear();
      clearOpen = false;
      await loadList();
      prefs = { ...prefs, historyCount: items.length };
      showToast(text.cleared, { icon: "success" });
    } catch (error) {
      showToast(`${text.error}: ${error.message}`);
    } finally {
      clearing = false;
    }
  }

  function onPluginEvent(eventOrMethod, maybeParams) {
    const method =
      typeof eventOrMethod === "string"
        ? eventOrMethod
        : (eventOrMethod?.method ?? eventOrMethod?.type ?? "");
    const params =
      typeof eventOrMethod === "string"
        ? (maybeParams ?? {})
        : (eventOrMethod?.params ?? eventOrMethod?.detail ?? {});
    if (method !== "screenshot/phase") return;
    if (params?.phase === "saving" && capturing) {
      showToast(text.capturing, { durationMs: 0, icon: "loading" });
    }
  }

  onMount(async () => {
    applyLocale();
    window.addEventListener("dbx-plugin-env", applyLocale);
    try {
      await window.dbxPlugin.ready;
      applyLocale();
      if (typeof window.dbxPlugin.onEvent === "function") {
        unsubscribeEvents = window.dbxPlugin.onEvent(onPluginEvent);
      }
      await loadPermission();
      await loadPrefs();
      await Promise.all([loadTargets(), loadList()]);
      prefs = { ...prefs, historyCount: items.length };
    } catch (error) {
      bootError = error?.message || String(error);
    }
  });

  onDestroy(() => {
    clearTimeout(toastTimer);
    clearDelayCountdownTimer();
    void releaseMediaToken();
    unsubscribeEvents?.();
    unsubscribeEvents = null;
    window.removeEventListener("dbx-plugin-env", applyLocale);
  });
</script>

<svelte:head>
  <title>{text.title}</title>
</svelte:head>

<div class="app">
  <div class="toolbar">
    <div class="toolbar-row capture-row">
      <div class="actions">
        <button
          class="btn"
          type="button"
          disabled={capturing || delayCountdownOpen}
          onclick={() => runCapture("display")}
        >
          <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
            <path
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M4 5.5h16v10H4zM8 19h8M12 15.5V19"
            />
          </svg>
          {text.captureScreen}
        </button>
        <button
          class="btn"
          type="button"
          disabled={capturing || delayCountdownOpen}
          onclick={openWindowPicker}
        >
          <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
            <path
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M5 6.5h14v11H5zM5 10h14"
            />
          </svg>
          {capturing && captureMode === "window" ? text.capturing : text.captureWindow}
        </button>
        <button
          class="btn"
          type="button"
          disabled={capturing || delayCountdownOpen}
          onclick={() => runCapture("region")}
        >
          <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
            <path
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M7 4H4v3M17 4h3v3M7 20H4v-3M17 20h3v-3M8 8h8v8H8z"
            />
          </svg>
          {text.captureRegion}
        </button>
        <button
          class="btn"
          type="button"
          disabled={capturing || delayCountdownOpen}
          onclick={startDelayedCapture}
        >
          <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
            <path
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M12 7v5l3 2M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16Z"
            />
          </svg>
          {text.captureDelay}
        </button>
      </div>

      {#if displays.length > 1}
        <select
          class="select target"
          value={displayIndex}
          disabled={capturing || delayCountdownOpen}
          title={text.captureScreen}
          onchange={onDisplaySelectChange}
        >
          {#each displays as display (display.index)}
            <option value={display.index}>
              {display.name}
              {display.isPrimary ? ` · ${text.capturePrimary}` : ""}
              ({display.width}×{display.height})
            </option>
          {/each}
        </select>
      {/if}

      <button
        class="icon-btn settings-btn"
        type="button"
        title={text.settings}
        aria-label={text.settings}
        onclick={async () => {
          await loadPrefs();
          settingsOpen = true;
        }}
      >
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
          <path
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z"
          />
          <path
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9c.1.7.7 1.2 1.5 1.3H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z"
          />
        </svg>
      </button>
    </div>

    <div class="toolbar-row filter-row">
      <input
        class="search"
        type="search"
        autocomplete="off"
        spellcheck="false"
        placeholder={text.search}
        bind:value={query}
      />

      <button
        class="icon-btn"
        type="button"
        title={text.refresh}
        aria-label={text.refresh}
        disabled={refreshing}
        onclick={refresh}
      >
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
          <path
            fill="currentColor"
            d="M17.65 6.35A7.95 7.95 0 0 0 12 4a8 8 0 1 0 7.75 10h-2.1A6 6 0 1 1 12 6c1.57 0 2.98.61 4.04 1.61L13 10h7V3z"
          />
        </svg>
      </button>
      <button
        class="icon-btn danger"
        type="button"
        title={text.clear}
        aria-label={text.clear}
        onclick={() => (clearOpen = true)}
      >
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
          <path
            fill="currentColor"
            d="M9 3h6a1 1 0 0 1 1 1v1h4v2h-1v12a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V7H4V5h4V4a1 1 0 0 1 1-1Zm1 2v0h4V5h-4Zm-2 4v10h8V9H8Z"
          />
        </svg>
      </button>
    </div>
  </div>

  <div class="tabs" role="tablist">
    {#each tabs as entry (entry.id)}
      <button
        class="tab"
        class:active={tab === entry.id}
        type="button"
        role="tab"
        aria-selected={tab === entry.id}
        onclick={() => (tab = entry.id)}
      >
        {entry.label}
      </button>
    {/each}
  </div>

  <div class="meta">
    <span title={text.hint}>{text.count(filtered.length)} · {text.hint}</span>
  </div>

  {#if bootError}
    <div class="banner">{bootError}</div>
  {/if}

  <div class="list" role="list">
    {#if filtered.length === 0}
      <div class="empty">{hasFilter ? text.emptyFilter : text.empty}</div>
    {:else}
      {#each filtered as item (item.id)}
        <HistoryItem
          {item}
          {text}
          {favoritesMode}
          oncopy={copyItem}
          onfavorite={toggleFavorite}
          ondelete={favoritesMode ? removeFavorite : deleteItem}
          onpreview={previewImage}
          onreveal={revealItem}
          onannotate={openAnnotator}
        />
      {/each}
    {/if}
  </div>
</div>

<Toast message={toastMessage} visible={toastVisible} icon={toastIcon} />

<DelayCountdown
  open={delayCountdownOpen}
  seconds={delayCountdownSecs}
  message={text.delayCountdown}
  cancelLabel={text.cancel}
  oncancel={cancelDelayedCapture}
/>

<WindowPicker
  open={windowPickerOpen}
  title={text.windowPickerTitle}
  hint={text.windowPickerHint}
  loadingLabel={text.windowPickerLoading}
  previewLoadingLabel={text.windowPreviewLoading}
  emptyLabel={text.noWindows}
  closeLabel={text.close}
  loading={windowPickerLoading}
  {windows}
  onclose={closeWindowPicker}
  onselect={(win) => {
    if (capturing || windowPickerLoading) return;
    selectWindow(win);
  }}
/>

<ConfirmModal
  open={permissionConfirmOpen}
  title={permissionConfirmTitle}
  body={permissionConfirmBody}
  cancelLabel={text.cancel}
  confirmLabel={permissionConfirmAction}
  confirming={requestingPermission}
  oncancel={cancelPermissionConfirm}
  onconfirm={confirmPermissionAction}
/>

<ClearModal
  open={clearOpen}
  title={text.clearTitle}
  body={text.clearBody}
  cancelLabel={text.cancel}
  confirmLabel={text.clearConfirm}
  confirming={clearing}
  oncancel={() => (clearOpen = false)}
  onconfirm={performClear}
/>

<SettingsDrawer
  open={settingsOpen}
  {text}
  {prefs}
  saving={prefsSaving}
  {displays}
  itemsCount={items.length}
  {permission}
  {requestingPermission}
  {recheckingPermission}
  onclose={() => (settingsOpen = false)}
  onchange={patchPrefs}
  onopenfolder={openDataFolder}
  onclear={clearHistoryFromSettings}
  onrequestpermission={promptPermissionRequest}
  onpermissionsettings={promptPermissionSettings}
  onrecheckpermission={recheckPermission}
/>

<ImageLightbox
  open={!!preview && !annotateItem}
  src={preview?.src || ""}
  loading={Boolean(preview?.loading)}
  alt={text.title}
  closeLabel={text.close}
  onclose={closePreview}
/>

<Annotator
  open={!!annotateItem}
  item={annotateItem}
  {text}
  fileFormat={prefs.fileFormat === "png" ? "png" : "jpeg"}
  quality={prefs.quality === "low" || prefs.quality === "medium" ? prefs.quality : "high"}
  oncancel={onAnnotatorCancel}
  onsaved={onAnnotatorSaved}
/>

<style>
  :global(:root) {
    color-scheme: light dark;
    --bg: var(--color-background, Canvas);
    --fg: var(--color-foreground, CanvasText);
    --muted: color-mix(in srgb, var(--fg) 55%, transparent);
    --border: color-mix(in srgb, var(--fg) 14%, transparent);
    --hover: color-mix(in srgb, var(--fg) 6%, transparent);
    --active: color-mix(in srgb, var(--fg) 10%, transparent);
    --accent: var(--color-primary, #2563eb);
    --danger: #dc2626;
    --radius: var(--radius-md, 10px);
    --font: var(--font-sans, ui-sans-serif, system-ui, -apple-system, sans-serif);
  }

  :global(*) {
    box-sizing: border-box;
  }

  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: var(--bg);
    color: var(--fg);
    font: 13px/1.45 var(--font);
  }

  :global(#app) {
    height: 100%;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .toolbar {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px 0;
    flex-shrink: 0;
  }

  .toolbar-row {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 0;
  }

  .capture-row {
    flex-wrap: wrap;
  }

  .settings-btn {
    margin-left: auto;
  }

  .filter-row {
    width: 100%;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .select,
  .search,
  .btn,
  .icon-btn {
    height: 34px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: transparent;
    color: inherit;
    font: inherit;
  }

  .select {
    max-width: 260px;
    padding: 0 10px;
  }

  .actions {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .select.target {
    min-width: 160px;
    max-width: 260px;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0 12px;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn svg {
    flex-shrink: 0;
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: wait;
  }

  .search {
    flex: 1;
    min-width: 140px;
    padding: 0 12px;
    outline: none;
  }

  .search:focus {
    border-color: color-mix(in srgb, var(--accent) 70%, var(--border));
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .icon-btn {
    width: 34px;
    display: grid;
    place-items: center;
    cursor: pointer;
    flex-shrink: 0;
  }

  .icon-btn:hover {
    background: var(--hover);
  }

  .icon-btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .icon-btn.danger {
    color: var(--danger);
  }

  .tabs {
    display: flex;
    gap: 4px;
    padding: 8px 14px 0;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .tab {
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    background: transparent;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }

  .tab.active {
    color: var(--fg);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }

  .meta {
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: 12px;
    flex-shrink: 0;
  }

  .banner {
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--danger);
    font-size: 12px;
    white-space: pre-wrap;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .empty {
    display: grid;
    place-items: center;
    height: 100%;
    padding: 32px;
    text-align: center;
    color: var(--muted);
    white-space: pre-line;
  }
</style>
