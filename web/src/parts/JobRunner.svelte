<script lang="ts">
  // One operation with its four states in one place: the button, the lines
  // as they arrive with a cancel while it runs, the final state, and — when
  // the operation is gated — the confirm line and "commit" under the plan.
  //
  // `run` returns whatever the route returns: `{ id }` for a job (followed
  // to its end over SSE), an Outcome or a GatedOutcome for a synchronous
  // command. Both render the same way.
  import Lines from "./Lines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { GatedOutcome, Job, Outcome, Started } from "$api/types";

  type Result = Started | Job | Outcome | GatedOutcome;

  let {
    label,
    confirm = "",
    commitLabel = "commit",
    disabled = false,
    reason = "",
    run,
    onresult,
  }: {
    label: string;
    /** The sentence above "commit"; empty when the operation is not gated. */
    confirm?: string;
    commitLabel?: string;
    disabled?: boolean;
    /** Why the button is disabled, said in place. */
    reason?: string;
    run: (commit: boolean) => Promise<Result>;
    onresult?: (r: Result) => void;
  } = $props();

  const act = new Action<Job | Outcome | GatedOutcome>("job");
  const start = (commit: boolean) =>
    act.run(async () => {
      const r = await run(commit);
      const done = "id" in r && !("state" in r) ? await ops.card!.follow(r.id) : (r as Job | Outcome | GatedOutcome);
      onresult?.(done);
      return done;
    });

  const running = $derived(act.busy && app.job?.state === "running" ? app.job : null);
  const result = $derived(act.state === "done" ? act.result : null);
  const job = $derived(result && "state" in result ? result : null);
  const lines = $derived(result?.lines ?? []);
  const files = $derived(job ? (job.result?.files ?? []) : ((result as Outcome | null)?.files ?? []));
  const gate = $derived(
    result && "result" in result && result.result && "committed" in result.result
      ? !result.result.committed
      : result && "committed" in result
        ? !result.committed
        : false,
  );
  const done = (j: Job) => `${j.kind} ${j.id}: ${j.state}${j.error ? `: ${j.error}` : ""}`;
</script>

<div class="actions">
  <button class="primary" onclick={() => start(false)} disabled={disabled || act.busy}>{confirm ? "dry run" : label}</button>
  {#if disabled && reason}<span class="caption">{reason}</span>{/if}
</div>

{#if running}
  <p class="muted">{running.kind} {running.id}: running <button onclick={() => ops.card?.cancel(running.id).catch(() => {})}>cancel</button></p>
  <Lines lines={running.lines} />
{/if}
{#if act.error}<p class="error">{act.error}</p>{/if}
{#if result && !running}
  {#if job}
    <p class={job.state === "done" ? "ok" : job.state === "failed" ? "error" : "muted"}>{done(job)}</p>
  {/if}
  <Lines {lines} {files} />
  {#if confirm && gate}
    <p class="confirm">{confirm}</p>
    <button onclick={() => start(true)} disabled={act.busy}>{commitLabel}</button>
  {/if}
{/if}

<style>
  p.muted button {
    height: 24px;
    margin-left: var(--s2);
  }
</style>
