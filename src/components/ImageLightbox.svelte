<script>
  import PreviewShell from "./PreviewShell.svelte";

  let {
    open = false,
    src = "",
    poster = "",
    alt = "",
    loading = false,
    kind = "image",
    note = "",
    revealLabel = "",
    closeLabel = "Close",
    onclose,
    onreveal,
  } = $props();

  const isVideo = $derived(kind === "video");
  const showShell = $derived(
    open && (!!src || loading || (isVideo && (!!poster || !!note))),
  );
</script>

<PreviewShell open={showShell} {closeLabel} variant="media" {onclose}>
  <div class="frame" class:loading>
    {#if isVideo}
      {#if src}
        {#key src}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video
            {src}
            poster={poster || undefined}
            controls
            playsinline
            autoplay
            preload="auto"
          ></video>
        {/key}
      {:else if poster}
        <img class="poster" src={poster} alt="" />
      {/if}
    {:else if src}
      <img {src} {alt} />
    {/if}
    {#if loading}
      <div class="spinner" aria-hidden="true"></div>
    {/if}
  </div>
  {#if note && !loading}
    <div class="note">
      <p>{note}</p>
      {#if revealLabel && onreveal}
        <button type="button" class="reveal" onclick={() => onreveal?.()}>
          {revealLabel}
        </button>
      {/if}
    </div>
  {/if}
</PreviewShell>

<style>
  .frame {
    position: relative;
    display: grid;
    place-items: center;
    max-width: 100%;
    max-height: min(92vh, 100%);
  }

  img,
  video {
    display: block;
    width: auto;
    height: auto;
    max-width: 100%;
    max-height: min(92vh, 100%);
    border-radius: 12px;
    background: #111;
    object-fit: contain;
  }

  .poster {
    opacity: 0.85;
  }

  .frame.loading img,
  .frame.loading video {
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

  .note {
    margin-top: 12px;
    max-width: min(420px, 90vw);
    text-align: center;
    color: rgba(255, 255, 255, 0.88);
    font-size: 13px;
    line-height: 1.45;
  }

  .note p {
    margin: 0;
  }

  .reveal {
    margin-top: 10px;
    appearance: none;
    border: 1px solid rgba(255, 255, 255, 0.28);
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
    border-radius: 8px;
    padding: 6px 12px;
    font: inherit;
    cursor: pointer;
  }

  .reveal:hover {
    background: rgba(255, 255, 255, 0.18);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
