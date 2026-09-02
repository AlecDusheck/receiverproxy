<script lang="ts">
  // The screen's title row: name on the left, the primary action on the
  // right. Under it, on the first visit without a daemon, one dismissible
  // install line; when the daemon answers without a token, the token field.
  import type { Snippet } from "svelte";
  import { app } from "../lib/state.svelte";
  import { ops } from "../api/ops";

  let { title, action }: { title: string; action?: Snippet } = $props();
  let token = $state("");

  function dismiss() {
    app.install = false;
    try {
      localStorage.setItem("rxp.install", "off");
    } catch {
      /* no storage */
    }
  }
  function connect(e: SubmitEvent) {
    e.preventDefault();
    void ops.connect(token.trim());
  }
</script>

<div class="title">
  <h1>{title}</h1>
  {#if action}<div class="row">{@render action()}</div>{/if}
</div>
{#if app.daemon === "locked"}
  <form class="notice" onsubmit={connect}>
    <span>The daemon answers but needs its token, the part after <code>#token=</code> in the URL <code>rxp ui</code> printed.</span>
    <input class={["mono", { invalid: !!app.tokenError }]} bind:value={token} placeholder="token" autocomplete="off" aria-label="token" />
    <button type="submit" disabled={!token.trim()}>connect</button>
    {#if app.tokenError}<span class="error">{app.tokenError}</span>{/if}
  </form>
{:else if app.install && app.daemon === "absent"}
  <div class="notice">
    <span>Card actions need the daemon: <code>cargo install --path crates/cli</code>, then <code>rxp ui</code>.</span>
    <button onclick={() => ops.probe()}>retry</button>
    <button onclick={dismiss}>dismiss</button>
  </div>
{/if}

<style>
  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s3);
    min-height: 32px;
    margin-bottom: var(--s4);
  }
  .notice {
    display: flex;
    align-items: center;
    gap: var(--s3);
    flex-wrap: wrap;
    padding: var(--s2) var(--s3);
    margin: calc(-1 * var(--s2)) 0 var(--s4);
    border: 1px solid var(--line);
    background: var(--bg-2);
  }
  .notice > span:first-child {
    flex: 1;
  }
  .notice input {
    width: 320px;
  }
</style>
