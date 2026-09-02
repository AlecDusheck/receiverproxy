<script lang="ts">
  // One line under the top bar, only when it applies: the daemon is absent
  // (the install command, dismissed for the session) or answers without a
  // token (the token field). Nothing when the daemon is present.
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { REPO } from "$lib/site";

  let token = $state("");

  function dismiss() {
    app.install = false;
    try {
      sessionStorage.setItem("rxp.install", "off");
    } catch {
      /* no storage */
    }
  }
  function connect(e: SubmitEvent) {
    e.preventDefault();
    void ops.connect(token.trim());
  }
</script>

{#if app.daemon === "locked"}
  <form class="banner" onsubmit={connect}>
    <label for="token">daemon token</label>
    <input id="token" class={["mono", { invalid: !!app.tokenError }]} bind:value={token} autocomplete="off" />
    <button type="submit" disabled={!token.trim()}>connect</button>
    {#if app.tokenError}<span class="error">{app.tokenError}</span>{/if}
  </form>
{:else if app.install && app.daemon === "absent"}
  <div class="banner">
    <span class="wide-only">daemon not running: <code>cargo install --path crates/cli &amp;&amp; rxp ui</code></span>
    <span class="narrow-only">Control needs the desktop daemon: <a href={REPO}>{REPO.replace("https://", "")}</a></span>
    <button class="wide-only" onclick={() => ops.probe()}>retry</button>
    <button onclick={dismiss}>dismiss</button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--s2) var(--s3);
    padding: var(--s1) var(--s4);
    min-height: 32px;
    border-bottom: 1px solid var(--line);
  }
  input {
    flex: 1 1 200px;
    max-width: 360px;
  }
  button {
    height: 24px;
  }
</style>
