<script lang="ts">
  // The same data as the drawing, editable: receivers and panels. A row is
  // selected by a click or by focusing one of its controls (Tab).
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { ROTATIONS } from "$lib/layout";
  import type { Sel } from "./WallCanvas.svelte";

  let { sel = $bindable() }: { sel: Sel } = $props();
  const wall = $derived(app.wall);
</script>

<section>
  <h2>Receivers</h2>
  <div class="scroll">
    <table>
      <thead><tr><th class="num">index</th><th class="num">x</th><th class="num">y</th><th class="num">width</th><th class="num">height</th><th></th></tr></thead>
      <tbody>
        {#each wall.receivers as r, i (i)}
          <tr class={["selectable", { selected: sel?.kind === "receiver" && sel.i === i }]} onclick={() => (sel = { kind: "receiver", i })} onfocusin={() => (sel = { kind: "receiver", i })}>
            <td class="num"><input type="number" bind:value={r.index} min="0" aria-label="index" /></td>
            <td class="num"><input type="number" bind:value={r.x} min="0" aria-label="x" /></td>
            <td class="num"><input type="number" bind:value={r.y} min="0" aria-label="y" /></td>
            <td class="num"><input type="number" bind:value={r.width} min="1" aria-label="width" /></td>
            <td class="num"><input type="number" bind:value={r.height} min="1" aria-label="height" /></td>
            <td>{#if ops.card}<a href="/control?provision={r.index}">provision this card</a>{/if}</td>
          </tr>
        {:else}
          <tr><td colspan="6" class="muted">no receivers</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<section>
  <h2>Panels</h2>
  <div class="scroll">
    <table>
      <thead>
        <tr><th class="num">#</th><th>receiver</th><th class="num">receiver_x</th><th class="num">receiver_y</th><th class="num">x</th><th class="num">y</th><th class="num">width</th><th class="num">height</th><th>rotation</th><th>flip_x</th><th>flip_y</th></tr>
      </thead>
      <tbody>
        {#each wall.panels as p, i (i)}
          <tr class={["selectable", { selected: sel?.kind === "panel" && sel.i === i }]} onclick={() => (sel = { kind: "panel", i })} onfocusin={() => (sel = { kind: "panel", i })}>
            <td class="num">{i}</td>
            <td>
              <select bind:value={p.receiver} aria-label="receiver">
                {#each wall.receivers as r (r.index)}<option value={r.index}>{r.index}</option>{/each}
              </select>
            </td>
            <td class="num"><input type="number" bind:value={p.receiver_x} min="0" aria-label="receiver_x" /></td>
            <td class="num"><input type="number" bind:value={p.receiver_y} min="0" aria-label="receiver_y" /></td>
            <td class="num"><input type="number" bind:value={p.x} min="0" aria-label="x" /></td>
            <td class="num"><input type="number" bind:value={p.y} min="0" aria-label="y" /></td>
            <td class="num"><input type="number" bind:value={p.width} min="1" aria-label="width" /></td>
            <td class="num"><input type="number" bind:value={p.height} min="1" aria-label="height" /></td>
            <td>
              <select bind:value={p.rotation} aria-label="rotation">
                {#each ROTATIONS as r (r)}<option value={r}>{r}</option>{/each}
              </select>
            </td>
            <td><input type="checkbox" bind:checked={p.flip_x} aria-label="flip_x" /></td>
            <td><input type="checkbox" bind:checked={p.flip_y} aria-label="flip_y" /></td>
          </tr>
        {:else}
          <tr><td colspan="11" class="muted">no panels</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>
