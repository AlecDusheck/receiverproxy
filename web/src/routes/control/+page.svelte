<script lang="ts">
  // The daemon's card actions: client-rendered (+page.ts). `?provision=<index>`
  // opens the provision form for a receiver of the wall.
  import { page } from "$app/state";
  import { title } from "$lib/site";
  import TitleRow from "$parts/TitleRow.svelte";
  import Field from "$parts/Field.svelte";
  import Lines from "$parts/Lines.svelte";
  import ControlWrite from "./ControlWrite.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { Card, Fit, Outcome, Pattern, Job } from "$api/types";

  const query = page.url.searchParams;

  const cards = $derived(app.health?.cards ?? []);
  const position = (c: Card) => {
    const r = app.wall.receivers.find((q) => q.index === c.controller);
    return r ? `${r.x ?? 0},${r.y ?? 0}` : "";
  };
  let selected = $state(0);

  const discover = new Action<Card[]>("discover");
  const runDiscover = () =>
    discover.run(async () => {
      const list = await ops.card!.discover();
      if (app.health) app.health.cards = list;
      return list;
    });

  // Brightness
  let brightness = $state(app.settings?.brightness ?? 255);
  const bright = new Action<number>("brightness");
  const setBrightness = () =>
    bright.run(async () => {
      const v = await ops.card!.brightness(brightness);
      if (app.settings) app.settings.brightness = v;
      return v;
    });

  // Show: one source, one button. A held show is a job the status bar follows.
  type Source = "pattern" | "fill" | "image file" | "image path" | "video path" | "blank";
  let source = $state<Source>("pattern");
  let pattern = $state<Pattern>("rgb");
  let fill = $state("#ff8000");
  let hold = $state(false);
  let fit = $state<Fit>("stretch");
  let imageFile = $state<File | null>(null);
  let imagePath = $state("");
  let video = $state({ path: "", loop: true, fps: 30 });
  const show = new Action<Outcome | Job>("show");
  const canShow = $derived(source === "image file" ? !!imageFile : source === "image path" ? !!imagePath : source === "video path" ? !!video.path : true);
  const runShow = () =>
    show.run(async () => {
      const c = ops.card!;
      let r: Outcome | { id: string };
      switch (source) {
        case "pattern":
          r = await c.showPattern({ name: pattern, hold });
          break;
        case "fill":
          r = await c.showFill({ rgb: fill.slice(1), hold });
          break;
        case "image file":
          r = await c.showImageFile(imageFile!, fit, hold);
          break;
        case "image path":
          r = await c.showImage({ path: imagePath, fit, hold });
          break;
        case "video path":
          r = await c.showVideo({ path: video.path, loop: video.loop, fps: video.fps, fit });
          break;
        default:
          r = await c.showBlank();
      }
      return "id" in r ? c.follow(r.id) : r;
    });
</script>

<svelte:head><title>{title("Control")}</title></svelte:head>

<TitleRow title="Control">
  {#snippet action()}
    <button class="primary" onclick={runDiscover} disabled={discover.busy}>discover</button>
  {/snippet}
</TitleRow>

{#if !ops.card}
  <p>Card actions go through the daemon, which is not running.</p>
  <pre>cargo install --path crates/cli
rxp ui</pre>
{:else}
  <section>
    <table>
      <thead><tr><th class="num">index</th><th>model</th><th>card id</th><th>firmware</th><th>size</th><th>position</th></tr></thead>
      <tbody>
        {#each cards as c (c.controller)}
          <tr class={["selectable", { selected: c.controller === selected }]} tabindex="0" onclick={() => (selected = c.controller)} onkeydown={(k) => k.key === "Enter" && (selected = c.controller)}>
            <td class="num">{c.controller}</td>
            <td>{c.model ?? "unknown"}</td>
            <td class="mono">0x{c.card_id.toString(16).padStart(2, "0")}</td>
            <td class="mono">{c.ver_major}.{c.ver_minor}</td>
            <td class="mono">{c.cols}x{c.rows}</td>
            <td class="mono">{position(c)}</td>
          </tr>
        {:else}
          <tr><td colspan="6" class="muted">no card answered</td></tr>
        {/each}
      </tbody>
    </table>
    {#if app.health}<p class="caption">daemon {app.health.version}, iface {app.health.iface}</p>{/if}
    {#if discover.error}<p class="error">{discover.error}</p>{/if}
  </section>

  <section>
    <h2>Show</h2>
    <div class="form">
      <Field label="source">
        <select bind:value={source}>
          {#each ["pattern", "fill", "image file", "image path", "video path", "blank"] as s (s)}<option value={s}>{s}</option>{/each}
        </select>
      </Field>
      {#if source === "pattern"}
        <Field label="pattern"><select bind:value={pattern}>{#each ["rgb", "border", "rows", "gradient", "white"] as n (n)}<option value={n}>{n}</option>{/each}</select></Field>
      {:else if source === "fill"}
        <Field label="colour" caption={fill}><input type="color" bind:value={fill} /></Field>
      {:else if source === "image file"}
        <Field label="file" wide><input type="file" accept="image/*" onchange={(e) => (imageFile = e.currentTarget.files?.[0] ?? null)} /></Field>
      {:else if source === "image path"}
        <Field label="path" caption="as the daemon's process sees it" wide><input bind:value={imagePath} class="mono" /></Field>
      {:else if source === "video path"}
        <Field label="path" caption="as the daemon's process sees it" wide><input bind:value={video.path} class="mono" /></Field>
        <Field label="loop"><input type="checkbox" bind:checked={video.loop} /></Field>
        <Field label="fps" caption="1-120"><input type="number" bind:value={video.fps} min="1" max="120" /></Field>
      {/if}
      {#if source !== "pattern" && source !== "fill" && source !== "blank"}
        <Field label="fit"><select bind:value={fit}><option>stretch</option><option>contain</option><option>cover</option></select></Field>
      {/if}
      {#if source !== "video path" && source !== "blank"}
        <Field label="hold" caption="refresh until cancelled"><input type="checkbox" bind:checked={hold} /></Field>
      {/if}
    </div>
    <div class="actions"><button onclick={runShow} disabled={show.busy || !canShow}>show</button></div>
    {#if show.error}<p class="error">{show.error}</p>{/if}
    {#if show.result}
      {#if "state" in show.result}<p class={show.result.state === "done" ? "ok" : "muted"}>{show.result.kind} {show.result.id}: {show.result.state}</p>{/if}
      <Lines lines={show.result.lines} files={"files" in show.result ? show.result.files : []} />
    {/if}
  </section>

  <section>
    <h2>Brightness</h2>
    <div class="row">
      <input type="range" min="0" max="255" bind:value={brightness} aria-label="brightness" />
      <input type="number" min="0" max="255" bind:value={brightness} aria-label="brightness value" />
      <span class="caption">0-255, sent in every sync frame</span>
      <button onclick={setBrightness} disabled={bright.busy}>set</button>
    </div>
    {#if bright.error}<p class="error">{bright.error}</p>{/if}
    {#if bright.result !== null}<p class="ok">brightness {bright.result}</p>{/if}
  </section>

  <ControlWrite index={selected} {query} />
{/if}

<style>
  input[type="range"] {
    width: 256px;
  }
</style>
