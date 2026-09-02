<script lang="ts">
  // The shell on every route: the sidebar, the content pane (960 px, the
  // Wall unbounded) with the footer under it, and the status bar. The daemon
  // probe starts once, on the client, after the token is read from the
  // address bar.
  import "../app.css";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { replaceState } from "$app/navigation";
  import Sidebar from "$parts/Sidebar.svelte";
  import StatusBar from "$parts/StatusBar.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { REPO } from "$lib/site";
  import { version } from "../../package.json";

  let { children } = $props();
  const wide = $derived(page.url.pathname === "/wall");

  onMount(() => {
    void ops.start((url) => replaceState(url, {}));
  });
  $effect(() => {
    document.body.classList.toggle("busy", app.status.kind === "busy");
  });
</script>

<div class="flex h-screen flex-col">
  <div class="flex min-h-0 flex-1">
    <Sidebar />
    <main class="min-w-0 flex-1 overflow-auto px-6 pt-4 pb-6">
      <div class={["content", { wide }]}>
        {@render children()}
      </div>
      <footer class="caption mt-6 flex gap-3 border-t border-line pt-3">
        <a href={REPO}>{REPO.replace("https://", "")}</a>
        <span>version {version}</span>
      </footer>
    </main>
  </div>
  <StatusBar />
</div>

<style>
  .content {
    max-width: 960px;
  }
  .content.wide {
    max-width: none;
  }
  footer {
    max-width: 960px;
  }
</style>
