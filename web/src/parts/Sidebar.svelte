<script lang="ts">
  // 180 px, text only; the current item by weight and the accent bar; the
  // daemon state at the bottom.
  import { app } from "../lib/state.svelte";

  let { current }: { current: string } = $props();
  const items = $derived([["gallery", "Gallery"], ["builder", "Builder"], ["wall", "Wall"], ...(app.daemon === "present" ? [["cards", "Cards"]] : [])]);
  const daemon = $derived.by(() => {
    switch (app.daemon) {
      case "present":
        return `daemon: ${location.host || "127.0.0.1:7120"}`;
      case "locked":
        return "daemon: token required";
      case "probing":
        return "daemon: probing";
      default:
        return "daemon not running: install";
    }
  });
</script>

<nav>
  <div class="brand">e120</div>
  {#each items as [id, label] (id)}
    <a href="#/{id}" class={{ active: current === id }} aria-current={current === id ? "page" : undefined}>{label}</a>
  {/each}
  <div class="foot">
    {#if app.daemon === "absent"}
      <a href="#/cards">{daemon}</a>
    {:else}
      <div>{daemon}</div>
    {/if}
    <div>wasm: {app.wasm}</div>
  </div>
</nav>

<style>
  nav {
    width: 180px;
    flex-shrink: 0;
    background: var(--bg-2);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    padding: var(--s3) 0;
  }
  .brand {
    font-weight: 600;
    padding: 0 var(--s4) var(--s3);
  }
  nav > a {
    display: block;
    padding: var(--s1) var(--s4);
    border-left: 3px solid transparent;
    color: var(--text);
    text-decoration: none;
  }
  nav > a.active {
    font-weight: 600;
    border-left-color: var(--accent);
  }
  .foot {
    margin-top: auto;
    padding: 0 var(--s4);
    font-size: 11px;
    color: var(--text-2);
    word-break: break-all;
  }
</style>
