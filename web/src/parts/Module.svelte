<script lang="ts">
  // A module as a dot grid in the line token, at the module's aspect; one
  // dot per pixel, or per k pixels when a pixel would be under 3 px wide.
  // The scan is the caption, when known.
  let { width, height, scan = 0, size = 96, caption = true }: { width: number; height: number; scan?: number; size?: number; caption?: boolean } = $props();
  const w = $derived(Math.max(1, width));
  const h = $derived(Math.max(1, height));
  // The longer side spans `size`; the other follows the aspect.
  const boxW = $derived(w >= h ? size : Math.round((size * w) / h));
  const boxH = $derived(w >= h ? Math.round((size * h) / w) : size);
  const k = $derived(Math.max(1, Math.ceil(3 / (boxW / w))));
  const pitch = $derived((boxW / w) * k);
  const id = $props.id();
</script>

<figure>
  <svg width={boxW} height={boxH} viewBox="0 0 {boxW} {boxH}" role="img" aria-label="{w}x{h} module{scan ? `, 1/${scan} scan` : ''}">
    <defs>
      <pattern id="dots-{id}" width={pitch} height={pitch} patternUnits="userSpaceOnUse">
        <circle cx={pitch / 2} cy={pitch / 2} r={pitch * 0.3} />
      </pattern>
    </defs>
    <rect x="0" y="0" width={boxW} height={boxH} class="board" />
    <rect x="0" y="0" width={boxW} height={boxH} fill="url(#dots-{id})" />
  </svg>
  {#if caption}<figcaption class="caption">{w}x{h}{scan ? ` 1/${scan}` : ""}</figcaption>{/if}
</figure>

<style>
  figure {
    margin: 0;
    display: inline-flex;
    flex-direction: column;
    gap: var(--s1);
  }
  svg {
    display: block;
    fill: var(--line);
  }
  .board {
    fill: none;
    stroke: var(--line);
  }
</style>
