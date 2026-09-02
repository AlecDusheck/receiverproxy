<script lang="ts">
  // Snapshot the selected card's flash, or restore a snapshot (dry run, then commit).
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import JobLines from "$parts/JobLines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { dryRun, job } from "../job";
  import type { Job } from "$api/types";

  let op = $state<"snapshot" | "restore">("snapshot");
  let dir = $state("");
  const flash = new Action<Job>("flash");
  const run = (commit: boolean) =>
    flash.run(() => job(op === "snapshot" ? ops.card!.flashSnapshot({ dir: dir || undefined, index: app.card }) : ops.card!.flashRestore({ dir, commit, index: app.card })));
</script>

<ControlHead title="Flash">
  <div class="form">
    <Field label="operation"><select bind:value={op}><option value="snapshot">snapshot</option><option value="restore">restore</option></select></Field>
    <Field label="directory" caption={op === "snapshot" ? "empty: under the daemon's data dir" : "a snapshot directory on the daemon's machine"} wide><input bind:value={dir} class="mono" /></Field>
  </div>
  <div class="actions"><button class="primary" onclick={() => run(false)} disabled={flash.busy || (op === "restore" && !dir)}>{op === "snapshot" ? "snapshot" : "dry run"}</button></div>
  <JobLines act={flash} />
  {#if op === "restore" && dryRun(flash.result) && !flash.busy}
    <p class="confirm">This writes every block of the snapshot to card {app.card}.</p>
    <button onclick={() => run(true)} disabled={flash.busy}>commit</button>
  {/if}
</ControlHead>
