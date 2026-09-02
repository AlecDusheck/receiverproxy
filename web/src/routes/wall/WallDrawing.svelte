<script lang="ts">
  // The screen to scale in the width it is given: panels as module dot
  // grids, cards as labelled boxes, the chain as a line through the card
  // centres in index order with a dot on the first. A card is a button:
  // click or Enter selects it.
  import Module from "$parts/Module.svelte";
  import { app } from "$lib/state.svelte";
  import { rotated } from "$lib/layout";

  let { sel = $bindable() }: { sel: number | null } = $props();
  let avail = $state(800);
  const wall = $derived(app.wall);
  // Fits the width; a tall screen is capped at 600 px of height.
  const scale = $derived(Math.max(0.02, Math.min(Math.max(64, avail - 2) / Math.max(1, wall.width), 600 / Math.max(1, wall.height))));
  const px = (v: number) => `${v * scale}px`;
  const cards = $derived([...wall.receivers].sort((a, b) => a.index - b.index));
  const centres = $derived(cards.map((r) => [((r.x ?? 0) + r.width / 2) * scale, ((r.y ?? 0) + r.height / 2) * scale] as [number, number]));
  const first = $derived(centres[0]);
</script>

<div class="frame" bind:clientWidth={avail}>
  <div class="screen" style:width={px(wall.width)} style:height={px(wall.height)}>
    {#each wall.panels as p, i (i)}
      {@const [w, h] = rotated(p)}
      <div class="panel" style:left={px(p.x)} style:top={px(p.y)} style:width={px(w)} style:height={px(h)}>
        <Module width={w} height={h} scan={1} size={Math.max(1, Math.round(Math.max(w, h) * scale))} caption={false} />
      </div>
    {/each}
    {#each cards as r (r.index)}
      <button type="button" class={["card", { on: sel === r.index }]} style:left={px(r.x ?? 0)} style:top={px(r.y ?? 0)} style:width={px(r.width)} style:height={px(r.height)} onclick={() => (sel = sel === r.index ? null : r.index)} aria-pressed={sel === r.index}>
        <span>card {r.index}</span>
        <span>{r.x ?? 0},{r.y ?? 0}</span>
      </button>
    {/each}
    <svg class="chain" width={wall.width * scale} height={wall.height * scale} aria-label="chain">
      {#if centres.length > 1}<polyline points={centres.map(([x, y]) => `${x},${y}`).join(" ")} />{/if}
      {#if first}<circle cx={first[0]} cy={first[1]} r="4" />{/if}
    </svg>
  </div>
</div>

<style>
  .frame {
    width: 100%;
    min-width: 0;
    margin-bottom: var(--s2);
  }
  .screen {
    position: relative;
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .panel,
  .card,
  .chain {
    position: absolute;
    box-sizing: border-box;
  }
  .panel {
    line-height: 0;
    overflow: hidden;
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0;
    padding: 2px 3px;
    height: auto;
    background: none;
    border: 1px solid var(--text);
    border-radius: 0;
    color: var(--text);
    font-size: 11px;
    line-height: 1.2;
    overflow: hidden;
    text-align: left;
    white-space: nowrap;
  }
  .card.on {
    border: 2px solid var(--accent);
    color: var(--accent);
  }
  .card:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .chain {
    left: 0;
    top: 0;
    pointer-events: none;
    fill: var(--accent);
    stroke: var(--accent);
    stroke-width: 1.5;
  }
  .chain polyline {
    fill: none;
  }
</style>
