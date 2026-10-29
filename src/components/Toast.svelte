<script>
  let { message = "", visible = false, icon = "" } = $props();
</script>

<div class="toast" class:show={visible} class:has-icon={!!icon} role="status" aria-live="polite">
  {#if icon === "loading"}
    <span class="icon spinner" aria-hidden="true"></span>
  {:else if icon === "success"}
    <svg class="icon check" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
      <path
        fill="none"
        stroke="currentColor"
        stroke-width="2.4"
        stroke-linecap="round"
        stroke-linejoin="round"
        d="M5 13l4 4L19 7"
      />
    </svg>
  {/if}
  <span class="label">{message}</span>
</div>

<style>
  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    z-index: 90;
    transform: translateX(-50%) translateY(8px);
    max-width: min(420px, calc(100% - 32px));
    padding: 10px 16px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: color-mix(in srgb, var(--bg) 88%, var(--fg));
    color: var(--fg);
    box-shadow: 0 10px 28px color-mix(in srgb, #000 18%, transparent);
    opacity: 0;
    pointer-events: none;
    transition:
      opacity 160ms ease,
      transform 160ms ease;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .toast.has-icon {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    text-align: left;
  }

  .toast.show {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }

  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .icon {
    flex-shrink: 0;
  }

  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid color-mix(in srgb, currentColor 25%, transparent);
    border-top-color: currentColor;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  .check {
    color: #16a34a;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
