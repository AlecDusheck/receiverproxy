<script lang="ts">
  // Provision the selected card: the spec handed over by Panels or the
  // Builder, an optional firmware, the position, the chain index. `?provision=<index>`
  // (the Wall's link) selects the receiver whose x,y is the position and
  // whose index the EEPROM writes address. The dry run is a job too;
  // "commit" appears under its plan.
  import { page } from "$app/state";
  import { untrack } from "svelte";
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import JobLines from "$parts/JobLines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { dryRun, job } from "../job";
  import type { Job } from "$api/types";

  // index: "" broadcasts (one card on the link), a number addresses that chain position.
  let prov = $state({ spec_toml: "", firmware_path: "", x: 0, y: 0, index: "" });
  try {
    prov.spec_toml = localStorage.getItem("rxp.builder.toml") ?? "";
  } catch {
    /* no storage */
  }
  const from = untrack(() => page.url.searchParams.get("provision"));
  if (from !== null && Number.isInteger(Number(from))) {
    app.card = Number(from);
    prov.index = from;
  }
  const rec = app.wall.receivers.find((q) => q.index === untrack(() => app.card));
  if (rec) {
    prov.x = rec.x ?? 0;
    prov.y = rec.y ?? 0;
  }
  const index = $derived(prov.index.trim() === "" ? undefined : Number(prov.index));
  const indexError = $derived(index !== undefined && !(Number.isInteger(index) && index >= 0 && index <= 0xfffe) ? "a whole number 0 to 65534" : "");
  const provision = new Action<Job>("provision");
  const run = (commit: boolean) =>
    provision.run(() => job(ops.card!.provision({ spec_toml: prov.spec_toml, firmware_path: prov.firmware_path || undefined, position: [prov.x, prov.y], index, commit })));
</script>

<ControlHead title="Provision">
  <div class="form">
    <Field label="spec TOML" wide><textarea rows="10" bind:value={prov.spec_toml} spellcheck="false"></textarea></Field>
    <Field label="firmware" caption="optional: a firmware.toml name or a .hex path on the daemon's machine" wide><input bind:value={prov.firmware_path} class="mono" /></Field>
    <Field label="position x" caption="the card's window on the screen"><input type="number" bind:value={prov.x} min="0" /></Field>
    <Field label="position y"><input type="number" bind:value={prov.y} min="0" /></Field>
    <Field label="chain index" caption="the card's place in the Ethernet chain; empty broadcasts, one card on the link" error={indexError}><input bind:value={prov.index} inputmode="numeric" class="mono" /></Field>
  </div>
  <div class="actions"><button class="primary" onclick={() => run(false)} disabled={provision.busy || !prov.spec_toml || !!indexError}>dry run</button></div>
  <JobLines act={provision} />
  {#if dryRun(provision.result) && !provision.busy}
    <p class="confirm">This writes firmware, flash block 7 and the EEPROM of {index === undefined ? "the card on the link" : `chain index ${index}`}; power-cycle it afterwards.</p>
    <button onclick={() => run(true)} disabled={provision.busy || !!indexError}>commit</button>
  {/if}
</ControlHead>
