<script lang="ts">
  // The layout JSON as editable tables: the same document as the drawing,
  // for the walls the grid cannot express. Import and export the JSON.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import Drop from "$parts/Drop.svelte";
  import WallTables, { type Sel } from "../WallTables.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { save } from "$lib/download";
  import { addPanel, addReceiver, normalize } from "$lib/layout";
  import { errText } from "$lib/error";
  import { WALL_NAV } from "../nav";
  import type { Canvas } from "$api/types";

  let sel = $state<Sel>(null);
  const wall = $derived(app.wall);
  $effect(() => {
    try {
      localStorage.setItem("rxp.wall", JSON.stringify(app.wall));
    } catch {
      /* no storage */
    }
  });
  const verdict = $derived.by(() => {
    try {
      return ops.pure.validateLayout(app.wall);
    } catch (e) {
      return errText(e);
    }
  });

  function remove() {
    if (!sel) return;
    if (sel.kind === "panel") wall.panels.splice(sel.i, 1);
    else {
      const idx = wall.receivers[sel.i]!.index;
      wall.receivers.splice(sel.i, 1);
      app.wall.panels = wall.panels.filter((p) => p.receiver !== idx);
    }
    sel = null;
  }
  const imp = new Action<string>("import layout");
  const importFile = (files: File[]) =>
    imp.run(async () => {
      app.wall = normalize(JSON.parse(await files[0]!.text()) as Canvas);
      sel = null;
      return files[0]!.name;
    });
</script>

<svelte:window onkeydown={(k) => k.key === "Escape" && (sel = null)} />

<Head title="Layout" noindex />

<TitleRow title="Layout">
  {#snippet action()}
    <button class="primary" onclick={() => save("wall.json", JSON.stringify(normalize(app.wall), null, 2) + "\n")}>wall.json</button>
  {/snippet}
</TitleRow>
<SubNav links={WALL_NAV} />

<div class="row mb-4">
  <label>screen <input type="number" bind:value={app.wall.width} min="1" aria-label="screen width" /> x <input type="number" bind:value={app.wall.height} min="1" aria-label="screen height" /></label>
  <button onclick={() => { addReceiver(wall); sel = { kind: "receiver", i: wall.receivers.length - 1 }; }}>add card</button>
  <button onclick={() => { const r = sel?.kind === "receiver" ? wall.receivers[sel.i]!.index : (wall.receivers[0]?.index ?? 0); addPanel(wall, r); sel = { kind: "panel", i: wall.panels.length - 1 }; }} disabled={!wall.receivers.length}>add panel</button>
  <button onclick={remove} disabled={!sel}>remove selected</button>
</div>

<WallTables bind:sel />
<p class={verdict === "ok" ? "ok" : "error"}>{verdict}</p>

<section>
  <h2>Import</h2>
  <Drop label="wall.json" accept=".json,application/json" onfiles={importFile} />
  {#if imp.error}<p class="error">{imp.error}</p>{/if}
  {#if imp.result}<p class="ok">imported {imp.result}</p>{/if}
</section>

<style>
  label {
    display: flex;
    gap: var(--s1);
    align-items: center;
  }
</style>
