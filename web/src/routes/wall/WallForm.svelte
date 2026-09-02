<script lang="ts">
  // The grid form: panel, panels per card, cards, chain. Every change calls
  // `onchange`; the results (screen, card, counts) stand beside the form. A
  // null grid is a layout the grid cannot express: the fields stay blank.
  import Field from "$parts/Field.svelte";
  import KeyValue from "$parts/KeyValue.svelte";
  import { panelTitle } from "$lib/panel";
  import { cardSize, CORNERS, DIRECTIONS, gridValid, screenSize, type Grid } from "$lib/wall";
  import type { Entry } from "$api/types";

  let {
    grid = $bindable(),
    panel = $bindable(),
    entries,
    onchange,
    onnew,
  }: { grid: Grid | null; panel: string; entries: Entry[]; onchange: () => void; onnew: () => void } = $props();

  const results = $derived.by((): [string, string][] => {
    if (!grid || !gridValid(grid)) return [];
    const s = screenSize(grid), c = cardSize(grid);
    const cards = grid.cards.columns * grid.cards.rows;
    return [
      ["screen", `${s.width} x ${s.height}`],
      ["card", `${c.width} x ${c.height}`],
      ["cards", String(cards)],
      ["panels", String(cards * grid.perCard.columns * grid.perCard.rows)],
    ];
  });
  // The module the grid has when no panel in the list has it.
  const unlisted = $derived(grid && !entries.some((e) => e.module.width === grid.module.width && e.module.height === grid.module.height) ? `${grid.module.width}x${grid.module.height}` : "");
  const off = $derived(grid === null);
</script>

<div class="wrap">
  <div class="form grid">
    <Field label="panel">
      <select bind:value={panel} onchange={onchange} disabled={off} aria-label="panel">
        {#if unlisted}<option value="">{unlisted}</option>{/if}
        {#each entries as e (e.path)}<option value={e.path}>{panelTitle(e)}</option>{/each}
      </select>
    </Field>
    <Field label="panels per card" caption="columns x rows">
      <span class="pair">
        {#if grid}
          <input type="number" min="1" bind:value={grid.perCard.columns} oninput={onchange} aria-label="panels per card, columns" />
          x
          <input type="number" min="1" bind:value={grid.perCard.rows} oninput={onchange} aria-label="panels per card, rows" />
        {:else}
          <input type="number" disabled aria-label="panels per card, columns" /> x <input type="number" disabled aria-label="panels per card, rows" />
        {/if}
      </span>
    </Field>
    <Field label="cards" caption="columns x rows">
      <span class="pair">
        {#if grid}
          <input type="number" min="1" bind:value={grid.cards.columns} oninput={onchange} aria-label="cards, columns" />
          x
          <input type="number" min="1" bind:value={grid.cards.rows} oninput={onchange} aria-label="cards, rows" />
        {:else}
          <input type="number" disabled aria-label="cards, columns" /> x <input type="number" disabled aria-label="cards, rows" />
        {/if}
      </span>
    </Field>
    <Field label="chain start">
      {#if grid}
        <select bind:value={grid.start} onchange={onchange} aria-label="chain start">
          {#each CORNERS as c (c)}<option value={c}>{c}</option>{/each}
        </select>
      {:else}
        <select disabled aria-label="chain start"></select>
      {/if}
    </Field>
    <Field label="chain direction">
      {#if grid}
        <select bind:value={grid.direction} onchange={onchange} aria-label="chain direction">
          {#each DIRECTIONS as d (d)}<option value={d}>{d}</option>{/each}
        </select>
      {:else}
        <select disabled aria-label="chain direction"></select>
      {/if}
    </Field>
    <Field label="serpentine">
      {#if grid}
        <label class="check"><input type="checkbox" bind:checked={grid.serpentine} onchange={onchange} /> {grid.serpentine ? "on" : "off"}</label>
      {:else}
        <label class="check"><input type="checkbox" disabled /> off</label>
      {/if}
    </Field>
  </div>
  <div class="results">
    {#if grid}
      <KeyValue rows={results} />
    {:else}
      <p class="muted">irregular layout: edit in the <a href="/wall/layout">table</a></p>
      <button onclick={onnew} disabled={!entries.length}>new grid</button>
    {/if}
  </div>
</div>

<style>
  .wrap {
    display: flex;
    gap: var(--s4) var(--s5);
    flex-wrap: wrap;
    align-items: start;
    margin-bottom: var(--s4);
  }
  .grid {
    flex: 1 1 480px;
    min-width: 0;
  }
  .results {
    flex: 0 0 auto;
    min-width: 160px;
  }
  .pair {
    display: inline-flex;
    gap: var(--s1);
    align-items: center;
  }
  .pair input {
    width: 64px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    height: 32px;
  }
</style>
