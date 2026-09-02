<script lang="ts">
  // One job's output where its action was: the lines as they arrive with a
  // cancel button while it runs, the final state and lines once it ends, the
  // error verbatim. `act` is the Action that started the job.
  import Lines from "./Lines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import type { Job } from "$api/types";

  let { act }: { act: { busy: boolean; error: string; result: Job | null } } = $props();
  const running = $derived(act.busy && app.job?.state === "running" ? app.job : null);
  const line = (j: Job) => `${j.kind} ${j.id}: ${j.state}${j.result && "committed" in j.result ? (j.result.committed ? ", written" : ", dry run") : ""}`;

  async function cancel() {
    if (app.job) await ops.card?.cancel(app.job.id).catch(() => {});
  }
</script>

{#if running}
  <p class="muted">{running.kind} {running.id}: running <button onclick={cancel}>cancel</button></p>
  <Lines lines={running.lines} />
{/if}
{#if act.error}<p class="error">{act.error}</p>{/if}
{#if act.result && !running}
  <p class={act.result.state === "done" ? "ok" : act.result.state === "failed" ? "error" : "muted"}>{line(act.result)}{act.result.error ? `: ${act.result.error}` : ""}</p>
  <Lines lines={act.result.lines} files={act.result.result?.files ?? []} />
{/if}

<style>
  button {
    height: 24px;
    margin-left: var(--s2);
  }
</style>
