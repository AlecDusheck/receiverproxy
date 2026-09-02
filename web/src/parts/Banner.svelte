<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { ops } from "../api/ops";

  let token = $state("");

  function dismiss() {
    app.banner = false;
    try {
      sessionStorage.setItem("e120.banner", "off");
    } catch {
      /* no storage */
    }
  }

  function connect(e: SubmitEvent) {
    e.preventDefault();
    void ops.connect(token.trim());
  }
</script>

<div class="banner">
  {#if app.daemon === "locked"}
    <form onsubmit={connect}>
      <span>The e120 daemon answers but needs its token, the part after <code>#token=</code> in the URL <code>e120 ui</code> printed.</span>
      <input class="mono" bind:value={token} placeholder="token" autocomplete="off" class:invalid={!!app.tokenError} />
      <button type="submit" disabled={!token.trim()}>connect</button>
      {#if app.tokenError}<span class="error">{app.tokenError}</span>{/if}
    </form>
  {:else}
    <span>
      The e120 daemon is not running. Install with <code>cargo install --path crates/cli</code>, then run <code>e120 ui</code>.
    </span>
    <button onclick={() => ops.probe()} disabled={app.daemon === "probing"}>retry</button>
  {/if}
  <button onclick={dismiss}>dismiss</button>
</div>

<style>
  .banner,
  form {
    display: flex;
    align-items: center;
    gap: var(--s3);
  }
  .banner {
    padding: var(--s2) var(--s4);
    background: var(--muted);
    border-bottom: 1px solid var(--line);
  }
  span,
  form {
    flex: 1;
  }
  form > span:first-child {
    flex: 1;
  }
  input {
    width: 320px;
  }
  .error {
    flex: 0;
    color: var(--error);
    white-space: nowrap;
  }
</style>
