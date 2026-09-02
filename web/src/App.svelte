<script lang="ts">
  import Sidebar from "./parts/Sidebar.svelte";
  import StatusBar from "./parts/StatusBar.svelte";
  import Gallery from "./screens/Gallery.svelte";
  import Builder from "./screens/Builder.svelte";
  import Wall from "./screens/Wall.svelte";
  import Cards from "./screens/Cards.svelte";
  import { app } from "./lib/state.svelte";

  // Hash routes: #/gallery, #/gallery/<name>, #/builder, #/wall, #/cards, each with an optional ?query.
  let hash = $state(location.hash);
  const route = $derived.by(() => {
    const m = /^#\/([a-z]*)(?:\/([^?]*))?\??(.*)$/.exec(hash);
    return { screen: m?.[1] || "gallery", arg: decodeURIComponent(m?.[2] ?? ""), query: new URLSearchParams(m?.[3] ?? "") };
  });
  const screen = $derived(["gallery", "builder", "wall", "cards"].includes(route.screen) ? route.screen : "gallery");

  $effect(() => {
    document.body.classList.toggle("busy", app.status.kind === "busy");
  });
</script>

<svelte:window onhashchange={() => (hash = location.hash)} />

<div class="shell">
  <div class="body">
    <Sidebar current={screen} />
    <main>
      <div class={["content", { wide: screen === "wall" }]}>
        {#if screen === "cards"}
          <Cards query={route.query} />
        {:else if screen === "wall"}
          <Wall />
        {:else if screen === "builder"}
          <Builder query={route.query} />
        {:else}
          <Gallery selected={route.arg} />
        {/if}
      </div>
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
    padding: var(--s4) var(--s5) var(--s5);
    min-width: 0;
  }
  .content {
    max-width: 960px;
  }
  .content.wide {
    max-width: none;
  }
</style>
