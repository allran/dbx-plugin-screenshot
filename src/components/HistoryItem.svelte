<script>
  import { onDestroy } from "svelte";
  import { formatItemLabel, formatTime } from "../lib/format.js";

  let {
    item,
    text,
    favoritesMode = false,
    oncopy,
    onfavorite,
    ondelete,
    onpreview,
    onreveal,
    onannotate,
  } = $props();

  let copied = $state(false);
  let viewTimer = 0;
  let copyTimer = 0;

  function scheduleView(action) {
    return (event) => {
      event.stopPropagation();
      clearTimeout(viewTimer);
      viewTimer = setTimeout(action, 220);
    };
  }

  async function handleCopy() {
    clearTimeout(viewTimer);
    await oncopy?.(item);
    copied = true;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      copied = false;
    }, 500);
  }

  onDestroy(() => {
    clearTimeout(viewTimer);
    clearTimeout(copyTimer);
  });
</script>

<div class="row" class:copied role="listitem" title={text.hint} ondblclick={handleCopy}>
  <button
    class="thumb-btn"
    type="button"
    title={text.viewImage}
    onclick={scheduleView(() => onpreview?.(item))}
  >
    {#if item.previewDataUrl}
      <img class="thumb" src={item.previewDataUrl} alt="" />
    {:else}
      <div class="thumb placeholder"></div>
    {/if}
  </button>

  <div class="meta">
    <div class="label">{formatItemLabel(item, text)}</div>
    <div class="sub">
      {formatTime(item.createdAt)}
      {#if item.width && item.height}
        · {text.size(item.width, item.height)}
      {/if}
    </div>
  </div>

  <div class="actions">
    <button
      class="icon"
      type="button"
      title={text.annotate}
      aria-label={text.annotate}
      onclick={(e) => {
        e.stopPropagation();
        onannotate?.(item);
      }}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
        <path
          fill="currentColor"
          d="M3 17.3V21h3.8l10.9-11-3.8-3.7L3 17.3Zm17.7-10.9a1 1 0 0 0 0-1.4l-2.6-2.6a1 1 0 0 0-1.4 0l-1.8 1.8 3.8 3.7 2-1.5Z"
        />
      </svg>
    </button>
    <button
      class="icon"
      class:active={item.favorite}
      type="button"
      title={item.favorite ? text.unfavorite : text.favorite}
      aria-label={item.favorite ? text.unfavorite : text.favorite}
      onclick={(e) => {
        e.stopPropagation();
        onfavorite?.(item);
      }}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
        <path
          fill="currentColor"
          d="M12 17.3 6.2 20.5l1.6-6.6L3 9.5l6.7-.5L12 3l2.3 6 6.7.5-4.8 4.4 1.6 6.6z"
        />
      </svg>
    </button>
    <button
      class="icon"
      type="button"
      title={text.reveal}
      aria-label={text.reveal}
      onclick={(e) => {
        e.stopPropagation();
        onreveal?.(item);
      }}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
        <path
          fill="currentColor"
          d="M10 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-8l-2-2Zm0 4h10v10H4V6h5.2L10 8Z"
        />
      </svg>
    </button>
    <button
      class="icon danger"
      type="button"
      title={favoritesMode ? text.unfavorite : text.delete}
      aria-label={favoritesMode ? text.unfavorite : text.delete}
      onclick={(e) => {
        e.stopPropagation();
        ondelete?.(item);
      }}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
        <path
          fill="currentColor"
          d="M9 3h6a1 1 0 0 1 1 1v1h4v2h-1v12a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V7H4V5h4V4a1 1 0 0 1 1-1Zm1 2v0h4V5h-4Zm-2 4v10h8V9H8Z"
        />
      </svg>
    </button>
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    cursor: default;
  }

  .row:hover {
    background: var(--hover);
  }

  .row.copied {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .thumb-btn {
    flex-shrink: 0;
    width: 88px;
    height: 56px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    background: color-mix(in srgb, var(--fg) 4%, transparent);
    cursor: pointer;
  }

  .thumb {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    object-position: top center;
    background: #0f172a;
  }

  .thumb.placeholder {
    background: repeating-linear-gradient(
      -45deg,
      transparent,
      transparent 6px,
      color-mix(in srgb, var(--fg) 8%, transparent) 6px,
      color-mix(in srgb, var(--fg) 8%, transparent) 12px
    );
  }

  .meta {
    flex: 1;
    min-width: 0;
  }

  .label {
    font-weight: 560;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sub {
    margin-top: 2px;
    color: var(--muted);
    font-size: 12px;
  }

  .actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .icon {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }

  .icon:hover {
    background: var(--active);
    color: var(--fg);
  }

  .icon.active {
    color: #eab308;
  }

  .icon.danger:hover {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 10%, transparent);
  }
</style>
