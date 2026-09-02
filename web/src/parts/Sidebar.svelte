<script lang="ts">
  import { app } from "../lib/state.svelte";

  let { current }: { current: string } = $props();
  const items = $derived([
    ...(app.daemon === "present" ? [["cards", "Cards"]] : []),
    ["wall", "Wall"],
    ["builder", "Builder"],
    ["library", "Library"],
  ]);
</script>

<nav>
  <div class="brand">e120</div>
  {#each items as [id, label] (id)}
    <a href="#/{id}" class={{ active: current === id }}>{label}</a>
  {/each}
  <div class="foot">
    <div>daemon: {app.daemon}</div>
    <div>wasm: {app.wasm}</div>
  </div>
</nav>

<style>
  nav {
    width: 160px;
    flex-shrink: 0;
    background: var(--muted);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    padding: var(--s3) 0;
  }
  .brand {
    font-weight: 600;
    padding: 0 var(--s4) var(--s3);
  }
  a {
    display: block;
    padding: var(--s1) var(--s4);
    color: CanvasText;
    text-decoration: none;
  }
  a.active {
    background: AccentColor;
    color: AccentColorText;
  }
  .foot {
    margin-top: auto;
    padding: 0 var(--s4);
    color: GrayText;
  }
</style>
