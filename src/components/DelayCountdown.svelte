<script>
  let {
    open = false,
    seconds = 0,
    message = "",
    cancelLabel = "Cancel",
    oncancel,
  } = $props();

  function onKeydown(event) {
    if (event.key === "Escape" && open) oncancel?.();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <div class="modal">
    <button
      class="modal-backdrop"
      type="button"
      aria-label={cancelLabel}
      onclick={() => oncancel?.()}
    ></button>
    <div class="modal-panel" role="dialog" aria-modal="true" aria-live="polite">
      <div class="count">{seconds}</div>
      <p class="message">{message}</p>
      <button class="btn" type="button" onclick={() => oncancel?.()}>
        {cancelLabel}
      </button>
    </div>
  </div>
{/if}

<style>
  .modal {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 20px;
  }

  .modal-backdrop {
    position: absolute;
    inset: 0;
    border: 0;
    padding: 0;
    background: color-mix(in srgb, #000 45%, transparent);
    cursor: pointer;
  }

  .modal-panel {
    position: relative;
    width: max-content;
    min-width: 160px;
    max-width: calc(100vw - 40px);
    padding: 24px 20px 18px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--bg);
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.28);
    display: inline-grid;
    justify-items: stretch;
    gap: 10px;
    text-align: center;
  }

  .count {
    font-size: 48px;
    font-weight: 700;
    line-height: 1;
    letter-spacing: -0.03em;
    font-variant-numeric: tabular-nums;
  }

  .message {
    margin: 0;
    color: color-mix(in srgb, currentColor 68%, transparent);
    font-size: 13px;
    line-height: 1.4;
  }

  .btn {
    margin-top: 4px;
    height: 34px;
    padding: 0 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn:hover {
    background: var(--hover);
  }
</style>
