<script lang="ts">
  // The layout JSON as editable tables: the same document as the drawing.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import WallTables from "../WallTables.svelte";
  import type { Sel } from "../WallCanvas.svelte";
  import { app } from "$lib/state.svelte";
  import { save } from "$lib/download";
  import { normalize } from "$lib/layout";
  import { WALL_NAV } from "../nav";

  let sel = $state<Sel>(null);
  $effect(() => {
    try {
      localStorage.setItem("rxp.wall", JSON.stringify(app.wall));
    } catch {
      /* no storage */
    }
  });
</script>

<Head title="Layout" noindex />

<TitleRow title="Layout">
  {#snippet action()}
    <button class="primary" onclick={() => save("wall.json", JSON.stringify(normalize(app.wall), null, 2) + "\n")}>wall.json</button>
  {/snippet}
</TitleRow>
<SubNav links={WALL_NAV} />

<div class="row mb-4">
  <label>screen <input type="number" bind:value={app.wall.width} min="1" aria-label="screen width" /> x <input type="number" bind:value={app.wall.height} min="1" aria-label="screen height" /></label>
</div>

<WallTables bind:sel />

<style>
  label {
    display: flex;
    gap: var(--s1);
    align-items: center;
  }
</style>
