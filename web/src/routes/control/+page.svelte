<script lang="ts">
  // The discovered cards, the brightness, and the two things worth reaching
  // for without leaving the page: blank, and a test pattern. A row selects
  // the card the sibling pages act on.
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import Lines from "$parts/Lines.svelte";
  import KeyValue from "$parts/KeyValue.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { Card, Outcome, Pattern, Started } from "$api/types";

  const cards = $derived(app.live?.cards ?? app.health?.cards ?? []);
  const show = $derived(app.live?.show ?? null);
  const position = (c: Card) => {
    const r = app.wall.receivers.find((q) => q.index === c.controller);
    return r ? `${r.x},${r.y}` : "";
  };

  const discover = new Action<Card[]>("discover");
  const runDiscover = () => discover.run(() => ops.card!.discover());

  let brightness = $state(app.live?.brightness ?? app.settings?.brightness ?? 255);
  const bright = new Action<number>("brightness");
  const setBrightness = () => bright.run(() => ops.card!.brightness(brightness));

  let pattern = $state<Pattern>("rgb");
  let hold = $state(false);
  const test = new Action<Outcome | Started>("test pattern");
  const runPattern = () => test.run(() => ops.card!.showPattern({ name: pattern, hold }));
  const blank = new Action<Outcome>("blank");
  const runBlank = () => blank.run(() => ops.card!.showBlank());
</script>

<ControlHead title="Control">
  {#snippet action()}
    {#if ops.card}<button class="primary" onclick={runDiscover} disabled={discover.busy}>discover</button>{/if}
  {/snippet}

  <section>
    <div class="scroll">
      <table>
        <thead><tr><th class="num">index</th><th>model</th><th>card id</th><th>firmware</th><th>size</th><th>position</th><th>showing</th></tr></thead>
        <tbody>
          {#each cards as c (c.controller)}
            <tr class={["selectable", { selected: c.controller === app.card }]} tabindex="0" onclick={() => (app.card = c.controller)} onkeydown={(k) => k.key === "Enter" && (app.card = c.controller)}>
              <td class="num">{c.controller}</td>
              <td>{c.model ?? "unknown"}</td>
              <td class="mono">0x{c.card_id.toString(16).padStart(2, "0")}</td>
              <td class="mono">{c.ver_major}.{c.ver_minor}</td>
              <td class="mono">{c.cols}x{c.rows}</td>
              <td class="mono">{position(c)}</td>
              <td>{show ? `${show.kind} ${show.source}` : "nothing"}</td>
            </tr>
          {:else}
            <tr><td colspan="7" class="muted">no card answered</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if discover.error}<p class="error">{discover.error}</p>{/if}
  </section>

  {#if show}
    <section>
      <KeyValue
        title="On the panel"
        rows={[["kind", show.kind], ["source", show.source], ["fps", show.fps === null ? "still" : String(show.fps)], ["layout", show.layout], ["started", show.started], ["job", show.job ?? "none"]]}
      />
    </section>
  {/if}

  <section>
    <h2>Brightness</h2>
    <div class="row">
      <input type="range" min="0" max="255" bind:value={brightness} aria-label="brightness" />
      <input type="number" min="0" max="255" bind:value={brightness} aria-label="brightness value" />
      <button onclick={setBrightness} disabled={bright.busy}>set</button>
    </div>
    {#if bright.error}<p class="error">{bright.error}</p>{/if}
    {#if bright.result !== null}<p class="ok">brightness {bright.result}</p>{/if}
  </section>

  <section>
    <h2>Test pattern</h2>
    <div class="form">
      <Field label="pattern"><select bind:value={pattern}>{#each ["rgb", "border", "rows", "gradient", "white"] as n (n)}<option value={n}>{n}</option>{/each}</select></Field>
      <Field label="hold" caption="refresh until cancelled"><input type="checkbox" bind:checked={hold} /></Field>
    </div>
    <div class="actions">
      <button class="primary" onclick={runPattern} disabled={test.busy}>show pattern</button>
      <button onclick={runBlank} disabled={blank.busy}>blank</button>
    </div>
    {#if test.error}<p class="error">{test.error}</p>{/if}
    {#if test.result && "lines" in test.result}<Lines lines={test.result.lines} files={test.result.files} />{/if}
    {#if test.result && "id" in test.result}<p class="ok">job {test.result.id} holds the pattern; stop it in the status bar</p>{/if}
    {#if blank.error}<p class="error">{blank.error}</p>{/if}
    {#if blank.result}<Lines lines={blank.result.lines} files={blank.result.files} />{/if}
  </section>
</ControlHead>

<style>
  input[type="range"] {
    width: 256px;
    max-width: 100%;
  }
</style>
