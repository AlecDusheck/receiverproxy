<script lang="ts">
  // 28 px: the interface and cards on the left, the job or status text on the right, monospace.
  import { app } from "../lib/state.svelte";
  import { ops } from "../api/ops";

  const left = $derived.by(() => {
    if (app.daemon !== "present" || !app.health) return "standalone";
    const h = app.health;
    const cards = h.cards.map((c) => `${c.model ?? "card " + c.card_id.toString(16)} ${c.ver_major}.${c.ver_minor} ${c.cols}x${c.rows}`).join(", ");
    return `iface ${h.iface} · ${h.cards.length} card${h.cards.length === 1 ? "" : "s"}${cards ? ": " + cards : ""}`;
  });
  const running = $derived(app.job?.state === "running");
  const last = $derived(app.job?.lines.at(-1)?.text ?? "");

  async function cancel() {
    if (app.job) await ops.card?.cancel(app.job.id).catch(() => {});
  }
</script>

<footer class="mono">
  <span>{left}</span>
  <span class="right {app.status.kind}">
    {#if running && app.job}
      <span>{app.job.kind} {app.job.id}{last ? ": " + last : ""}</span>
      <button onclick={cancel}>cancel</button>
    {:else}
      <span>{app.status.text}</span>
    {/if}
  </span>
</footer>

<style>
  footer {
    height: 28px;
    flex-shrink: 0;
    background: var(--bg-2);
    border-top: 1px solid var(--line);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--s3);
    gap: var(--s3);
    white-space: nowrap;
    overflow: hidden;
  }
  .right {
    display: flex;
    align-items: center;
    gap: var(--s2);
    overflow: hidden;
  }
  .right span {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .busy {
    color: var(--accent);
  }
  .error {
    color: var(--err);
  }
  button {
    height: 20px;
    padding: 0 var(--s2);
    font-size: 11px;
  }
</style>
