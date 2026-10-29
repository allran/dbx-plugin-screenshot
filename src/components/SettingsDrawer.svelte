<script>
  let {
    open = false,
    text = {},
    prefs = null,
    saving = false,
    displays = [],
    itemsCount = 0,
    permission = null,
    requestingPermission = false,
    recheckingPermission = false,
    onclose,
    onchange,
    onopenfolder,
    onclear,
    onrequestpermission,
    onpermissionsettings,
    onrecheckpermission,
  } = $props();

  function setField(patch) {
    if (saving) return;
    onchange?.(patch);
  }

  function onKeydown(event) {
    if (event.key === "Escape" && open) onclose?.();
  }

  const isMac = $derived(
    String(permission?.platform || "").toLowerCase().includes("mac") ||
      String(permission?.platform || "").toLowerCase() === "darwin",
  );
  const historyMax = $derived(Number(prefs?.historyMax) || 200);
  /** Required permissions shown in settings (macOS Screen Recording for now). */
  const permissionRows = $derived(
    isMac
      ? [
          {
            id: "screen-recording",
            title: text.permissionScreenRecording || "Screen Recording",
            description:
              text.permissionScreenRecordingWhy ||
              "Required to capture displays, windows, and regions.",
            granted: Boolean(permission?.granted),
            canRequest: permission?.canRequest !== false,
          },
        ]
      : [],
  );
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <div class="drawer" role="dialog" aria-modal="true" aria-label={text.settingsTitle || "Settings"}>
    <button class="backdrop" type="button" aria-label={text.close} onclick={() => onclose?.()}></button>
    <aside class="panel">
      <header class="header">
        <h2 class="title">{text.settingsTitle || "Settings"}</h2>
        <button class="icon-close" type="button" aria-label={text.close} onclick={() => onclose?.()}>
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
            <path
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              d="M6 6l12 12M18 6 6 18"
            />
          </svg>
        </button>
      </header>

      <div class="body">
        <section class="section">
          <h3 class="section-title">{text.settingsCapture || "Capture"}</h3>

          <div class="row">
            <div class="row-label">{text.settingsDelay || "Delay"}</div>
            <div class="seg">
              {#each [3, 5, 10] as secs}
                <button
                  type="button"
                  class="seg-btn"
                  class:active={Number(prefs?.delaySecs) === secs}
                  disabled={saving}
                  onclick={() => setField({ delaySecs: secs })}
                >
                  {secs}s
                </button>
              {/each}
            </div>
          </div>

          {#if displays.length > 1}
            <div class="row stack">
              <div class="row-label">{text.settingsDefaultDisplay || "Default display"}</div>
              <select
                class="select"
                disabled={saving}
                value={prefs?.defaultDisplayIndex ?? displays[0]?.index ?? ""}
                onchange={(e) => {
                  const v = e.currentTarget.value;
                  setField({
                    defaultDisplayIndex: v === "" ? null : Number(v),
                  });
                }}
              >
                {#each displays as display (display.index)}
                  <option value={display.index}>
                    {display.name}
                    {display.isPrimary ? ` · ${text.capturePrimary}` : ""}
                    ({display.width}×{display.height})
                  </option>
                {/each}
              </select>
            </div>
          {/if}

          <label class="toggle-row">
            <span class="row-label">{text.settingsOpenAnnotator || "Open annotator after capture"}</span>
            <input
              type="checkbox"
              checked={Boolean(prefs?.openAnnotatorAfterCapture)}
              disabled={saving}
              onchange={(e) => setField({ openAnnotatorAfterCapture: e.currentTarget.checked })}
            />
          </label>

          <label class="perm-row toggle-row">
            <span class="perm-copy">
              <span class="perm-name">{text.settingsCopyAfterSave || "Copy to clipboard after save"}</span>
              <span class="perm-desc">
                {text.settingsCopyAfterSaveWhy || "Paste into docs, chat, or other apps right away."}
              </span>
            </span>
            <input
              type="checkbox"
              checked={Boolean(prefs?.copyAfterSave)}
              disabled={saving}
              onchange={(e) => setField({ copyAfterSave: e.currentTarget.checked })}
            />
          </label>
        </section>

        <section class="section">
          <h3 class="section-title">{text.settingsMedia || "File"}</h3>

          <div class="row">
            <div class="row-label">{text.settingsFileFormat || "Format"}</div>
            <div class="seg">
              <button
                type="button"
                class="seg-btn"
                class:active={prefs?.fileFormat !== "png"}
                disabled={saving}
                onclick={() => setField({ fileFormat: "jpeg" })}
              >
                JPEG
              </button>
              <button
                type="button"
                class="seg-btn"
                class:active={prefs?.fileFormat === "png"}
                disabled={saving}
                onclick={() => setField({ fileFormat: "png" })}
              >
                PNG
              </button>
            </div>
          </div>

          <div class="row">
            <div class="row-label">{text.settingsQuality || "Quality"}</div>
            <div class="seg">
              {#each [
                ["high", text.settingsQualityHigh || "High"],
                ["medium", text.settingsQualityMedium || "Medium"],
                ["low", text.settingsQualityLow || "Low"],
              ] as [id, label]}
                <button
                  type="button"
                  class="seg-btn"
                  class:active={(prefs?.quality || "high") === id}
                  disabled={saving}
                  onclick={() => setField({ quality: id })}
                >
                  {label}
                </button>
              {/each}
            </div>
          </div>

          <div class="row">
            <div class="row-label">{text.settingsThumbSize || "Thumbnail"}</div>
            <div class="seg">
              {#each [
                ["small", text.settingsThumbSmall || "S"],
                ["medium", text.settingsThumbMedium || "M"],
                ["large", text.settingsThumbLarge || "L"],
              ] as [id, label]}
                <button
                  type="button"
                  class="seg-btn"
                  class:active={(prefs?.thumbSize || "medium") === id}
                  disabled={saving}
                  onclick={() => setField({ thumbSize: id })}
                >
                  {label}
                </button>
              {/each}
            </div>
          </div>
        </section>

        <section class="section">
          <h3 class="section-title">{text.settingsStorage || "History & storage"}</h3>
          <div class="perm-list">
            <div class="perm-row">
              <div class="perm-copy">
                <div class="perm-name">{text.settingsHistoryMax || "Keep screenshots"}</div>
                <div class="perm-desc">
                  {text.settingsHistoryMaxWhy?.(itemsCount) ||
                    `Currently ${itemsCount} saved. Older non-favorites are removed first.`}
                </div>
              </div>
              <select
                class="select compact"
                disabled={saving}
                value={historyMax}
                onchange={(e) => setField({ historyMax: Number(e.currentTarget.value) })}
              >
                {#each [50, 100, 200, 500] as max}
                  <option value={max}>{max}</option>
                {/each}
              </select>
            </div>

            <div class="perm-row">
              <div class="perm-copy">
                <div class="perm-name">{text.settingsOpenFolder || "Screenshots folder"}</div>
                <div class="perm-desc">
                  {text.settingsOpenFolderWhy || "Reveal saved screenshot files."}
                </div>
              </div>
              <button
                type="button"
                class="perm-status request"
                disabled={saving}
                onclick={() => onopenfolder?.()}
              >
                {text.settingsOpenFolderAction || "Open"}
              </button>
            </div>

            <div class="perm-row">
              <div class="perm-copy">
                <div class="perm-name">{text.settingsClearHistory || "Clear history"}</div>
                <div class="perm-desc">
                  {text.settingsClearHistoryWhy || "Delete all saved screenshots."}
                </div>
              </div>
              <button
                type="button"
                class="perm-status danger"
                disabled={saving || itemsCount === 0}
                onclick={() => onclear?.()}
              >
                {text.settingsClearHistoryAction || "Clear"}
              </button>
            </div>
          </div>
        </section>

        {#if permissionRows.length}
          <section class="section">
            <h3 class="section-title">{text.settingsPermission || "Permissions"}</h3>
            <div class="perm-list">
              {#each permissionRows as row (row.id)}
                <div class="perm-row">
                  <div class="perm-copy">
                    <div class="perm-name">{row.title}</div>
                    <div class="perm-desc">{row.description}</div>
                  </div>
                  {#if row.granted}
                    <button
                      type="button"
                      class="perm-status granted"
                      disabled={requestingPermission || recheckingPermission}
                      title={text.permissionSettings}
                      onclick={() => onpermissionsettings?.()}
                    >
                      {text.permissionStatusGranted || "Granted"}
                    </button>
                  {:else}
                    <div class="perm-actions">
                      <button
                        type="button"
                        class="perm-status request"
                        disabled={requestingPermission || recheckingPermission || !row.canRequest}
                        onclick={() => onrequestpermission?.()}
                      >
                        {requestingPermission
                          ? "…"
                          : text.permissionStatusRequest || "Request"}
                      </button>
                      <button
                        type="button"
                        class="perm-status request"
                        disabled={requestingPermission || recheckingPermission}
                        onclick={() => onrecheckpermission?.()}
                      >
                        {recheckingPermission
                          ? "…"
                          : text.permissionRecheck || "I've enabled it — recheck"}
                      </button>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          </section>
        {/if}
      </div>
    </aside>
  </div>
{/if}

<style>
  .drawer {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    justify-content: flex-end;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    border: 0;
    padding: 0;
    background: color-mix(in srgb, #000 42%, transparent);
    cursor: pointer;
  }

  .panel {
    position: relative;
    width: min(360px, 92vw);
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border-left: 1px solid var(--border);
    box-shadow: -12px 0 40px color-mix(in srgb, #000 22%, transparent);
    animation: slide-in 160ms ease-out;
  }

  @keyframes slide-in {
    from {
      transform: translateX(18px);
      opacity: 0.6;
    }
    to {
      transform: translateX(0);
      opacity: 1;
    }
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .title {
    margin: 0;
    font-size: 15px;
    font-weight: 650;
  }

  .icon-close {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }

  .icon-close:hover {
    background: var(--hover);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 12px 16px 24px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .section-title {
    margin: 0;
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .row,
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .row.stack {
    flex-direction: column;
    align-items: stretch;
  }

  .row-label,
  .perm-name {
    display: block;
    font-size: 13px;
    font-weight: 400;
    color: var(--fg);
    min-width: 0;
  }

  .toggle-row {
    cursor: pointer;
  }

  .toggle-row input {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
    flex-shrink: 0;
  }

  .select {
    height: 34px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: transparent;
    color: inherit;
    font: inherit;
    padding: 0 10px;
    width: 100%;
  }

  .select.compact {
    width: auto;
    min-width: 5.5rem;
    flex-shrink: 0;
    align-self: center;
  }

  .perm-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .perm-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .perm-copy {
    min-width: 0;
    flex: 1;
  }

  .perm-desc {
    display: block;
    margin-top: 4px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
  }

  .perm-row.toggle-row {
    align-items: flex-start;
  }

  .perm-row.toggle-row input {
    margin-top: 2px;
  }

  .perm-actions {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    flex-shrink: 0;
  }

  .perm-status {
    flex-shrink: 0;
    margin-top: 1px;
    border: 0;
    padding: 4px 0;
    background: transparent;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }

  .perm-status.granted {
    color: color-mix(in srgb, #16a34a 85%, var(--fg));
  }

  .perm-status.request {
    color: var(--accent);
  }

  .perm-status.danger {
    color: var(--danger);
  }

  .perm-status:hover:not(:disabled) {
    text-decoration: underline;
  }

  .perm-status:disabled {
    opacity: 0.55;
    cursor: wait;
  }

  .seg {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    background: color-mix(in srgb, var(--fg) 3%, transparent);
    flex-shrink: 0;
  }

  .seg-btn {
    padding: 5px 10px;
    border: 0;
    border-right: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 12px;
    line-height: 1.2;
    cursor: pointer;
  }

  .seg-btn:last-child {
    border-right: 0;
  }

  .seg-btn:hover:not(:disabled) {
    background: var(--hover);
    color: var(--fg);
  }

  .seg-btn.active {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--fg);
    font-weight: 600;
  }

  .seg-btn:disabled {
    opacity: 0.55;
    cursor: wait;
  }
</style>
