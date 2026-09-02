<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { probe } from "../lib/api";

  function dismiss() {
    app.banner = false;
    try {
      sessionStorage.setItem("e120.banner", "off");
    } catch {
      /* no storage */
    }
  }
</script>

<div class="banner">
  <span>
    The e120 daemon is not running. Install with <code>cargo install --path crates/e120-cli</code>, then run <code>e120 ui</code>.
  </span>
  <button onclick={probe} disabled={app.daemon === "probing"}>retry</button>
  <button onclick={dismiss}>dismiss</button>
</div>

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: var(--s3);
    padding: var(--s2) var(--s4);
    background: var(--muted);
    border-bottom: 1px solid var(--line);
  }
  span {
    flex: 1;
  }
</style>
