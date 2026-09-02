<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { ops } from "../api/ops";

  const left = $derived.by(() => {
    if (app.daemon !== "present" || !app.health) return "standalone";
    const h = app.health;
    const cards = h.cards.map((c) => `E120 ${c.ver_major}.${c.ver_minor} ${c.cols}x${c.rows}`).join(", ");
    return `iface ${h.iface} · ${h.cards.length} card${h.cards.length === 1 ? "" : "s"}${cards ? ": " + cards : ""}`;
  });
  const running = $derived(app.job?.state === "running");
  const last = $derived(app.job?.lines.at(-1)?.text ?? "");

  async function cancel() {
    if (app.job) await ops.card?.cancel(app.job.id).catch(() => {});
  }
</script>

<footer>
  <span class="mono">{left}</span>
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
    background: var(--muted);
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
    text-overflow: ellipsis;
  }
  .right span {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .busy {
    color: var(--busy);
  }
  .error {
    color: var(--error);
  }
  button {
    height: 20px;
    padding: 0 var(--s2);
  }
</style>
