<script>
  let {
    open = false,
    title = "",
    hint = "",
    loadingLabel = "Loading windows…",
    previewLoadingLabel = "Loading preview…",
    emptyLabel = "No windows",
    closeLabel = "Close",
    loading = false,
    windows = [],
    onclose,
    onselect,
  } = $props();

  function onKeydown(event) {
    if (event.key === "Escape" && open && !loading) onclose?.();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <div class="overlay" role="dialog" aria-modal="true" aria-label={title}>
    <button class="backdrop" type="button" aria-label={closeLabel} onclick={() => onclose?.()}></button>

    <div class="stage">
      <header class="top">
        <div class="titles">
          <div class="heading">{title}</div>
          <div class="hint">{hint}</div>
        </div>
        <button class="close" type="button" aria-label={closeLabel} title={closeLabel} onclick={() => onclose?.()}>
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
            <path
              fill="currentColor"
              d="M18.3 5.7a1 1 0 0 0-1.4-1.4L12 9.17 7.1 4.3A1 1 0 0 0 5.7 5.7L10.59 10.6 5.7 15.49a1 1 0 1 0 1.4 1.42L12 12l4.9 4.9a1 1 0 0 0 1.4-1.42L13.41 10.6z"
            />
          </svg>
        </button>
      </header>

      {#if loading}
        <div class="state">{loadingLabel}</div>
      {:else if windows.length === 0}
        <div class="state">{emptyLabel}</div>
      {:else}
        <div class="grid">
          {#each windows as win (win.id)}
            <button
              class="card"
              class:focused={win.isFocused}
              type="button"
              title={`${win.appName}${win.title && win.title !== win.appName ? ` — ${win.title}` : ""}`}
              onclick={() => onselect?.(win)}
            >
              <div class="preview">
                {#if win.previewDataUrl}
                  <img src={win.previewDataUrl} alt="" />
                {:else if win.previewFailed}
                  <div class="missing"></div>
                {:else}
                  <div class="preview-loading" aria-busy="true" aria-label={previewLoadingLabel}>
                    <span class="spinner" aria-hidden="true"></span>
                    <span class="preview-loading-text">{previewLoadingLabel}</span>
                  </div>
                {/if}
              </div>
              <div class="meta">
                <div class="app">{win.appName || win.title}</div>
                {#if win.title && win.title !== win.appName}
                  <div class="win-title">{win.title}</div>
                {/if}
              </div>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 70;
    overflow: hidden;
    display: grid;
    place-items: stretch;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    border: 0;
    padding: 0;
    background:
      radial-gradient(ellipse at center, rgba(40, 48, 70, 0.55), rgba(8, 10, 16, 0.88)),
      rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(18px);
    cursor: pointer;
  }

  .stage {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    width: 100%;
    max-width: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    margin: 0;
    padding: 20px 22px 28px;
    box-sizing: border-box;
    color: #f5f7ff;
  }

  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 18px;
    flex-shrink: 0;
    min-width: 0;
  }

  .titles {
    min-width: 0;
    flex: 1;
  }

  .heading {
    font-size: 18px;
    font-weight: 650;
    letter-spacing: 0.01em;
  }

  .hint {
    margin-top: 4px;
    color: rgba(245, 247, 255, 0.65);
    font-size: 12px;
  }

  .close {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    color: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .close:hover {
    background: rgba(255, 255, 255, 0.16);
  }

  .state {
    flex: 1;
    display: grid;
    place-items: center;
    color: rgba(245, 247, 255, 0.7);
    font-size: 14px;
  }

  .grid {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(260px, 100%), 1fr));
    gap: 18px 16px;
    align-content: start;
    padding: 10px 4px 16px;
    box-sizing: border-box;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    max-width: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      transform 160ms ease,
      filter 160ms ease;
  }

  .card:hover,
  .card:focus-visible {
    transform: translateY(-3px);
    outline: none;
  }

  .card.focused .preview {
    box-shadow:
      0 0 0 2px rgba(96, 165, 250, 0.95),
      0 18px 40px rgba(0, 0, 0, 0.45);
  }

  .preview {
    aspect-ratio: 16 / 10;
    width: 100%;
    border-radius: 12px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.06);
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.38);
    border: 1px solid rgba(255, 255, 255, 0.12);
  }

  .preview img,
  .missing {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: top center;
    background: linear-gradient(145deg, #1f2937, #0f172a);
  }

  .preview-loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    width: 100%;
    height: 100%;
    background: linear-gradient(145deg, #1f2937, #0f172a);
    color: rgba(245, 247, 255, 0.72);
    font-size: 12px;
  }

  .spinner {
    width: 22px;
    height: 22px;
    border: 2px solid rgba(255, 255, 255, 0.18);
    border-top-color: rgba(147, 197, 253, 0.95);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  .preview-loading-text {
    max-width: 90%;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .meta {
    padding: 0 4px;
    min-width: 0;
  }

  .app {
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .win-title {
    margin-top: 2px;
    font-size: 11px;
    color: rgba(245, 247, 255, 0.58);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
