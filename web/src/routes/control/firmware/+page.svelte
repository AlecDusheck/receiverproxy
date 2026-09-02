<script lang="ts">
  // Install a firmware image on the selected card: dry run, then commit.
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import JobLines from "$parts/JobLines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { dryRun, job } from "../job";
  import type { Job } from "$api/types";

  let fw = $state("");
  const firmware = new Action<Job>("firmware install");
  const run = (commit: boolean) => firmware.run(() => job(ops.card!.firmwareInstall({ path: fw, commit })));
</script>

<ControlHead title="Firmware">
  <div class="form">
    <Field label="image" caption="a firmware.toml name or a .hex path on the daemon's machine" wide><input bind:value={fw} class="mono" /></Field>
  </div>
  <div class="actions"><button class="primary" onclick={() => run(false)} disabled={firmware.busy || !fw}>dry run</button></div>
  <JobLines act={firmware} />
  {#if dryRun(firmware.result) && !firmware.busy}
    <p class="confirm">This programs the firmware bank of card {app.card}.</p>
    <button onclick={() => run(true)} disabled={firmware.busy}>commit</button>
  {/if}
</ControlHead>
