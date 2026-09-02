<script lang="ts">
  import TitleRow from "../parts/TitleRow.svelte";
  import Drop from "../parts/Drop.svelte";
  import Lines from "../parts/Lines.svelte";
  import WallCanvas, { type Sel } from "./WallCanvas.svelte";
  import WallTables from "./WallTables.svelte";
  import { app } from "../lib/state.svelte";
  import { ops } from "../api/ops";
  import { Action } from "../lib/action.svelte";
  import { save } from "../lib/download";
  import { addPanel, addReceiver, normalize, snapSize } from "../lib/layout";
  import { errText } from "../lib/error";
  import type { Canvas, Outcome, Pattern } from "../api/types";

  let sel = $state<Sel>(null);
  let ex = $state({ cols: 2, rows: 1, w: 128, h: 64 });
  let pattern = $state<Pattern>("rgb");

  const wall = $derived(app.wall);
  const grid = $derived(snapSize(wall));
  const verdict = $derived.by(() => {
    try {
      return app.wasm === "loading" ? "ok" : ops.pure.validateLayout(wall);
    } catch (e) {
      return errText(e);
    }
  });

  $effect(() => {
    try {
      localStorage.setItem("e120.wall", JSON.stringify(wall));
    } catch {
      /* no storage */
    }
  });

  function setWall(c: Canvas) {
    app.wall = normalize(c);
    sel = null;
  }
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
      setWall(JSON.parse(await files[0]!.text()) as Canvas);
      return files[0]!.name;
    });
  const saved = new Action<Canvas>("save wall");
  const saveDaemon = () =>
    saved.run(async () => {
      app.wall = await ops.card!.saveWall(normalize(wall));
      return app.wall;
    });
  const shown = new Action<Outcome | { id: string }>("show pattern");
  const showOnWall = () =>
    shown.run(async () => {
      await ops.card!.saveWall(normalize(wall));
      return ops.card!.showPattern({ name: pattern, hold: false });
    });
</script>

<svelte:window onkeydown={(k) => k.key === "Escape" && (sel = null)} />

<TitleRow title="Wall">
  {#snippet action()}
    <button class="primary" onclick={() => save("wall.json", JSON.stringify(normalize(wall), null, 2) + "\n")}>export wall.json</button>
  {/snippet}
</TitleRow>

<p class="muted">The layout <code>e120 show --layout</code> reads: receivers are cards, each keeping its window of the screen; panels hang off a receiver. Drag to move; positions snap to {grid} px.</p>

<div class="row tools">
  <label>screen <input type="number" bind:value={app.wall.width} min="1" aria-label="screen width" /> x <input type="number" bind:value={app.wall.height} min="1" aria-label="screen height" /></label>
  <button onclick={() => { addReceiver(wall); sel = { kind: "receiver", i: wall.receivers.length - 1 }; }}>add card</button>
  <button onclick={() => { const r = sel?.kind === "receiver" ? wall.receivers[sel.i]!.index : (wall.receivers[0]?.index ?? 0); addPanel(wall, r); sel = { kind: "panel", i: wall.panels.length - 1 }; }} disabled={!wall.receivers.length}>add panel</button>
  <button onclick={remove} disabled={!sel}>remove selected</button>
</div>
<div class="row tools">
  <label>example <input type="number" bind:value={ex.cols} min="1" class="short" aria-label="columns" /> x <input type="number" bind:value={ex.rows} min="1" class="short" aria-label="rows" /> cards of <input type="number" bind:value={ex.w} min="1" aria-label="card width" /> x <input type="number" bind:value={ex.h} min="1" aria-label="card height" /></label>
  <button onclick={() => setWall(ops.pure.layoutExample(ex.cols, ex.rows, ex.w, ex.h))}>layout example</button>
</div>

<div class="split">
  <div class="drawing">
    <WallCanvas bind:sel />
    <p class={verdict === "ok" ? "ok" : "error"}>{verdict}</p>
  </div>
  <div class="tables">
    <WallTables bind:sel />
  </div>
</div>

<section>
  <h2>Import</h2>
  <Drop label="A wall.json" accept=".json,application/json" onfiles={importFile} />
  {#if imp.error}<p class="error">{imp.error}</p>{/if}
  {#if imp.result}<p class="ok">imported {imp.result}</p>{/if}
</section>

{#if ops.card}
  <section>
    <h2>Daemon</h2>
    <div class="row">
      <button onclick={saveDaemon} disabled={saved.busy || verdict !== "ok"}>save as the daemon's wall</button>
      <label>pattern <select bind:value={pattern}>{#each ["rgb", "border", "rows", "gradient", "white"] as n (n)}<option value={n}>{n}</option>{/each}</select></label>
      <button onclick={showOnWall} disabled={shown.busy || verdict !== "ok"}>show on the wall</button>
    </div>
    {#if verdict !== "ok"}<p class="caption">disabled: the layout does not validate</p>{/if}
    {#if saved.error}<p class="error">{saved.error}</p>{/if}
    {#if saved.result}<p class="ok">saved as the daemon's wall</p>{/if}
    {#if shown.error}<p class="error">{shown.error}</p>{/if}
    {#if shown.result && "lines" in shown.result}<Lines lines={shown.result.lines} files={shown.result.files} />{/if}
  </section>
{/if}

<style>
  .tools {
    margin-bottom: var(--s2);
  }
  .tools label {
    display: flex;
    gap: var(--s1);
    align-items: center;
  }
  .short {
    width: 56px;
  }
  .split {
    display: flex;
    gap: var(--s5);
    align-items: flex-start;
    margin: var(--s3) 0 var(--s5);
  }
  .drawing {
    flex: 0 0 auto;
    max-width: 100%;
    overflow: auto;
  }
  .tables {
    flex: 1;
    min-width: 320px;
    max-width: 960px;
  }
  @media (max-width: 1100px) {
    .split {
      flex-direction: column;
    }
  }
  section label {
    display: flex;
    gap: var(--s1);
    align-items: center;
  }
</style>
