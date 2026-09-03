<script lang="ts">
  // The shell on every route: the top bar, the daemon banner when it applies,
  // the content (960 px, the Wall unbounded). The daemon probe starts once,
  // on the client, after the token is read from the address bar.
  import "../app.css";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { replaceState } from "$app/navigation";
  import TopBar from "$parts/TopBar.svelte";
  import Banner from "$parts/Banner.svelte";
  import StatusBar from "$parts/StatusBar.svelte";
  import { ops } from "$api/ops";

  let { children } = $props();
  const wide = $derived(page.url.pathname === "/wall");

  onMount(() => {
    void ops.start((url) => replaceState(url, {}));
  });
</script>

<TopBar />
<Banner />
<main>
  <div class={["content", { wide }]}>
    {@render children()}
  </div>
</main>
<StatusBar />

<style>
  main {
    /* the last 28 px belong to the status bar when there is one */
    padding: var(--s4) var(--s5) 52px;
    min-width: 0;
  }
  @media (max-width: 640px) {
    main {
      padding: var(--s3) var(--s3) 52px;
    }
  }
  .content {
    max-width: 960px;
    min-width: 0;
  }
  .content.wide {
    max-width: none;
  }
</style>
