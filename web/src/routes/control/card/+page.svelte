<script lang="ts">
  // The selected card's state: screen size (read, or write with a dry run
  // first), test mode, RAM layout, reload.
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import Lines from "$parts/Lines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { GatedOutcome, Outcome, SizeOutcome } from "$api/types";

  type Op = "read screen size" | "write screen size" | "test mode" | "set layout" | "reload" | "full reload";
  let op = $state<Op>("read screen size");
  let size = $state({ width: 128, height: 64 });
  let test = $state(0);
  let layout = $state({ w: 128, h: 64 });
  const card = new Action<Outcome | SizeOutcome | { width: number; height: number }>("card");
  const run = (commit: boolean) =>
    card.run(async () => {
      const c = ops.card!;
      const index = app.card;
      switch (op) {
        case "read screen size": {
          const r = await c.screenSize({ index });
          size = r;
          return r;
        }
        case "write screen size":
          return c.setScreenSize({ ...size, commit, index });
        case "test mode":
          return c.testMode({ n: test, index });
        case "set layout":
          return c.setLayout({ panel_width: layout.w, panel_height: layout.h, index });
        case "reload":
          return c.reload({ index });
        default:
          return c.reload({ index, full: true });
      }
    });
  const gated = $derived(card.result && "committed" in card.result ? (card.result as GatedOutcome) : null);
</script>

<ControlHead title="Card state">
  <div class="form">
    <Field label="operation">
      <select bind:value={op}>
        {#each ["read screen size", "write screen size", "test mode", "set layout", "reload", "full reload"] as o (o)}<option value={o}>{o}</option>{/each}
      </select>
    </Field>
    {#if op === "write screen size" || op === "read screen size"}
      <Field label="width" caption="pixels"><input type="number" bind:value={size.width} min="1" disabled={op === "read screen size"} /></Field>
      <Field label="height" caption="pixels"><input type="number" bind:value={size.height} min="1" disabled={op === "read screen size"} /></Field>
    {:else if op === "test mode"}
      <Field label="mode" caption="0-255, 0 is off"><input type="number" bind:value={test} min="0" max="255" /></Field>
    {:else if op === "set layout"}
      <Field label="panel width" caption="RAM only"><input type="number" bind:value={layout.w} min="1" /></Field>
      <Field label="panel height"><input type="number" bind:value={layout.h} min="1" /></Field>
    {/if}
  </div>
  <div class="actions"><button class="primary" onclick={() => run(false)} disabled={card.busy}>{op === "write screen size" ? "dry run" : "run"}</button></div>
  {#if card.error}<p class="error">{card.error}</p>{/if}
  {#if card.result}
    {#if "width" in card.result}<p class="ok">screen size {card.result.width}x{card.result.height}</p>{/if}
    {#if "lines" in card.result}<Lines lines={card.result.lines} files={card.result.files} />{/if}
    {#if gated && !gated.committed}
      <p class="confirm">This writes the screen size to the EEPROM of card {app.card}.</p>
      <button onclick={() => run(true)} disabled={card.busy}>commit</button>
    {/if}
  {/if}
</ControlHead>
