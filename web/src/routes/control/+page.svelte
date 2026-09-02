<script lang="ts">
  // The discovered cards and the brightness. A row selects the card the
  // sibling pages act on.
  import ControlHead from "$parts/ControlHead.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { Card } from "$api/types";

  const cards = $derived(app.health?.cards ?? []);
  const position = (c: Card) => {
    const r = app.wall.receivers.find((q) => q.index === c.controller);
    return r ? `${r.x ?? 0},${r.y ?? 0}` : "";
  };

  const discover = new Action<Card[]>("discover");
  const runDiscover = () =>
    discover.run(async () => {
      const list = await ops.card!.discover();
      if (app.health) app.health.cards = list;
      return list;
    });

  let brightness = $state(app.settings?.brightness ?? 255);
  const bright = new Action<number>("brightness");
  const setBrightness = () =>
    bright.run(async () => {
      const v = await ops.card!.brightness(brightness);
      if (app.settings) app.settings.brightness = v;
      return v;
    });
</script>

<ControlHead title="Control">
  {#snippet action()}
    {#if ops.card}<button class="primary" onclick={runDiscover} disabled={discover.busy}>discover</button>{/if}
  {/snippet}
  <section>
    <div class="scroll">
      <table>
        <thead><tr><th class="num">index</th><th>model</th><th>card id</th><th>firmware</th><th>size</th><th>position</th></tr></thead>
        <tbody>
          {#each cards as c (c.controller)}
            <tr class={["selectable", { selected: c.controller === app.card }]} tabindex="0" onclick={() => (app.card = c.controller)} onkeydown={(k) => k.key === "Enter" && (app.card = c.controller)}>
              <td class="num">{c.controller}</td>
              <td>{c.model ?? "unknown"}</td>
              <td class="mono">0x{c.card_id.toString(16).padStart(2, "0")}</td>
              <td class="mono">{c.ver_major}.{c.ver_minor}</td>
              <td class="mono">{c.cols}x{c.rows}</td>
              <td class="mono">{position(c)}</td>
            </tr>
          {:else}
            <tr><td colspan="6" class="muted">no card answered</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if discover.error}<p class="error">{discover.error}</p>{/if}
  </section>

  <section>
    <h2>Brightness</h2>
    <div class="row">
      <input type="range" min="0" max="255" bind:value={brightness} aria-label="brightness" />
      <input type="number" min="0" max="255" bind:value={brightness} aria-label="brightness value" />
      <button onclick={setBrightness} disabled={bright.busy}>set</button>
    </div>
    {#if bright.error}<p class="error">{bright.error}</p>{/if}
    {#if bright.result !== null}<p class="ok">brightness {bright.result}</p>{/if}
  </section>
</ControlHead>

<style>
  input[type="range"] {
    width: 256px;
    max-width: 100%;
  }
</style>
