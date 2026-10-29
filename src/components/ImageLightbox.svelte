<script>
  import PreviewShell from "./PreviewShell.svelte";

  let {
    open = false,
    src = "",
    alt = "",
    loading = false,
    closeLabel = "Close",
    onclose,
  } = $props();
</script>

<PreviewShell open={open && (!!src || loading)} {closeLabel} variant="media" {onclose}>
  <div class="frame" class:loading>
    {#if src}
      <img {src} {alt} />
    {/if}
    {#if loading}
      <div class="spinner" aria-hidden="true"></div>
    {/if}
  </div>
</PreviewShell>

<style>
  .frame {
    position: relative;
    display: grid;
    place-items: center;
    max-width: 100%;
    max-height: min(92vh, 100%);
  }

  img {
    display: block;
    width: auto;
    height: auto;
    max-width: 100%;
    max-height: min(92vh, 100%);
    border-radius: 12px;
    background: #111;
    object-fit: contain;
  }

  .frame.loading img {
    filter: brightness(0.72);
  }

  .spinner {
    position: absolute;
    width: 28px;
    height: 28px;
    border: 2px solid rgba(255, 255, 255, 0.25);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
