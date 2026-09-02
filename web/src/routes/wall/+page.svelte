<script lang="ts">
  // The Wall: the grid form, the drawing, the selected card. The form writes
  // the layout JSON (lib/wall.ts); an imported layout the grid cannot
  // express keeps the drawing and leaves the form blank. The same JSON as
  // tables is /wall/layout.
  import { untrack } from "svelte";
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import Drop from "$parts/Drop.svelte";
  import WallForm from "./WallForm.svelte";
  import WallDrawing from "./WallDrawing.svelte";
  import WallCard from "./WallCard.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { save } from "$lib/download";
  import { normalize } from "$lib/layout";
  import { gridOf, gridValid, layoutFromGrid, type Grid } from "$lib/wall";
  import { panelTitle } from "$lib/panel";
  import { errText } from "$lib/error";
  import { WALL_NAV } from "./nav";
  import type { Canvas, Entry } from "$api/types";

  let entries = $state<Entry[]>([]);
  let form = $state<Grid | null>(gridOf(app.wall));
  let panel = $state("");
  let sel = $state<number | null>(null);

  void ops.pure.gallery().then((g) => {
    entries = [...g].sort((a, b) => panelTitle(a).localeCompare(panelTitle(b)));
    panel = matchPanel(form, panel);
  });

  // The listed panel with the grid's module: the current one when it fits, else the first.
  function matchPanel(g: Grid | null, current: string): string {
    if (!g) return "";
    const fits = (e: Entry) => e.module.width === g.module.width && e.module.height === g.module.height;
    const cur = entries.find((e) => e.path === current);
    return cur && fits(cur) ? current : (entries.find(fits)?.path ?? "");
  }

  // A layout that arrives from outside (import, the daemon) re-reads the form;
  // one the form wrote keeps the form as it is.
  let seen = app.wall;
  $effect(() => {
    const w = app.wall;
    untrack(() => {
      if (w === seen) return;
      seen = w;
      if (form && gridValid(form) && JSON.stringify(layoutFromGrid(form)) === JSON.stringify(normalize(w))) return;
      form = gridOf(w);
      panel = matchPanel(form, panel);
      sel = null;
    });
  });
  $effect(() => {
    try {
      localStorage.setItem("rxp.wall", JSON.stringify(app.wall));
    } catch {
      /* no storage */
    }
  });

  function regen() {
    if (!form) return;
    const e = entries.find((q) => q.path === panel);
    if (e) form.module = { width: e.module.width, height: e.module.height };
    if (!gridValid(form)) return;
    app.wall = layoutFromGrid(form);
    seen = app.wall;
    if (sel !== null && !app.wall.receivers.some((r) => r.index === sel)) sel = null;
  }
  function newGrid() {
    const e = entries.find((q) => q.path === panel) ?? entries[0]!;
    panel = e.path;
    form = { module: { width: e.module.width, height: e.module.height }, perCard: { columns: 1, rows: 1 }, cards: { columns: 1, rows: 1 }, start: "top-left", direction: "rows", serpentine: false };
    regen();
  }

  const verdict = $derived.by(() => {
    try {
      return ops.pure.validateLayout(app.wall);
    } catch (e) {
      return errText(e);
    }
  });

  const imp = new Action<string>("import layout");
  const importFile = (files: File[]) =>
    imp.run(async () => {
      app.wall = normalize(JSON.parse(await files[0]!.text()) as Canvas);
      return files[0]!.name;
    });
  const saved = new Action<Canvas>("save wall");
  const saveDaemon = () =>
    saved.run(async () => {
      app.wall = await ops.card!.saveWall(normalize(app.wall));
      return app.wall;
    });
</script>

<svelte:window onkeydown={(k) => k.key === "Escape" && (sel = null)} />

<Head title="Wall" noindex />

<TitleRow title="Wall">
  {#snippet action()}
    <button class="primary" onclick={() => save("wall.json", JSON.stringify(normalize(app.wall), null, 2) + "\n")}>wall.json</button>
  {/snippet}
</TitleRow>
<SubNav links={WALL_NAV} />

<WallForm bind:grid={form} bind:panel {entries} onchange={regen} onnew={newGrid} />
{#if app.wasm === "failed"}<p class="error">{app.wasmError}</p>{/if}

<WallDrawing bind:sel />
<p class={verdict === "ok" ? "ok" : "error"}>{verdict}</p>

{#if sel !== null}
  <WallCard index={sel} spec={panel} />
{/if}

<section class="narrow">
  <h2>Import</h2>
  <Drop label="wall.json" accept=".json,application/json" onfiles={importFile} />
  {#if imp.error}<p class="error">{imp.error}</p>{/if}
  {#if imp.result}<p class="ok">imported {imp.result}</p>{/if}
</section>

{#if ops.card}
  <section class="narrow">
    <div class="row">
      <button onclick={saveDaemon} disabled={saved.busy || verdict !== "ok"}>save as the daemon's wall</button>
    </div>
    {#if saved.error}<p class="error">{saved.error}</p>{/if}
    {#if saved.result}<p class="ok">saved</p>{/if}
  </section>
{/if}

<style>
  .narrow {
    max-width: 960px;
  }
</style>
