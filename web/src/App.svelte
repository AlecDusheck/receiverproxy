<script lang="ts">
  import Sidebar from "./parts/Sidebar.svelte";
  import StatusBar from "./parts/StatusBar.svelte";
  import Banner from "./parts/Banner.svelte";
  import Cards from "./screens/Cards.svelte";
  import Wall from "./screens/Wall.svelte";
  import Builder from "./screens/Builder.svelte";
  import Library from "./screens/Library.svelte";
  import { app } from "./lib/state.svelte";

  // Hash router: #/screen?query
  let hash = $state(location.hash);
  const route = $derived.by(() => {
    const m = /^#\/([a-z]*)\??(.*)$/.exec(hash);
    return { screen: m?.[1] || "builder", query: new URLSearchParams(m?.[2] ?? "") };
  });
  const screen = $derived(route.screen === "cards" && app.daemon !== "present" ? "builder" : route.screen);

  $effect(() => {
    document.body.classList.toggle("busy", app.status.kind === "busy");
  });
</script>

<svelte:window onhashchange={() => (hash = location.hash)} />

<div class="shell">
  {#if app.banner}
    <Banner />
  {/if}
  <div class="body">
    <Sidebar current={screen} />
    <main>
      {#if screen === "cards"}
        <Cards query={route.query} />
      {:else if screen === "wall"}
        <Wall />
      {:else if screen === "library"}
        <Library />
      {:else}
        <Builder query={route.query} />
      {/if}
    </main>
  </div>
  <StatusBar />
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  main {
    flex: 1;
    overflow: auto;
    padding: var(--s5);
    min-width: 0;
  }
</style>
