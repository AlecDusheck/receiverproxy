<script lang="ts">
  // One source on the wall: a still (an image file or a path, or a colour),
  // a built-in pattern, or a video. A held still and a video are jobs, so
  // they run through JobRunner and stop from the status bar.
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import FileDrop, { type Picked } from "$parts/FileDrop.svelte";
  import JobRunner from "$parts/JobRunner.svelte";
  import Lines from "$parts/Lines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { Fit, Outcome, Pattern, Started } from "$api/types";

  type Source = "image file" | "image path" | "colour" | "pattern" | "video" | "blank";
  let source = $state<Source>("pattern");
  let pattern = $state<Pattern>("rgb");
  let fill = $state("#ff8000");
  let hold = $state(false);
  let fit = $state<Fit>("stretch");
  let image = $state<Picked | null>(null);
  let imagePath = $state("");
  let video = $state({ path: "", loop: true, fps: 30 });

  const wall = $derived(app.live?.show?.layout ?? `${app.wall.width}x${app.wall.height}`);
  const ready = $derived(source === "image file" ? !!image : source === "image path" ? !!imagePath : source === "video" ? !!video.path : true);
  const missing = $derived(ready ? "" : source === "video" ? "no video path" : source === "image file" ? "no file chosen" : "no image path");

  // The still sources answer at once unless they are held; video is always a job.
  const still = new Action<Outcome | Started>("show");
  const runStill = () =>
    still.run(async () => {
      const c = ops.card!;
      switch (source) {
        case "pattern":
          return c.showPattern({ name: pattern, hold });
        case "colour":
          return c.showFill({ rgb: fill.slice(1), hold });
        case "image file":
          return c.showImageFile(image!.file, fit, hold);
        case "image path":
          return c.showImage({ path: imagePath, fit, hold });
        default:
          return c.showBlank();
      }
    });
</script>

<ControlHead title="Show">
  <div class="form">
    <Field label="source">
      <select bind:value={source}>
        {#each ["image file", "image path", "colour", "pattern", "video", "blank"] as s (s)}<option value={s}>{s}</option>{/each}
      </select>
    </Field>
    {#if source === "pattern"}
      <Field label="pattern"><select bind:value={pattern}>{#each ["rgb", "border", "rows", "gradient", "white"] as n (n)}<option value={n}>{n}</option>{/each}</select></Field>
    {:else if source === "colour"}
      <Field label="colour" caption={fill} mono><input type="color" bind:value={fill} /></Field>
    {:else if source === "image path"}
      <Field label="path" caption="on the daemon's machine" wide><input bind:value={imagePath} class="mono" /></Field>
    {:else if source === "video"}
      <Field label="path" caption="on the daemon's machine, read by ffmpeg" wide><input bind:value={video.path} class="mono" /></Field>
      <Field label="loop"><input type="checkbox" bind:checked={video.loop} /></Field>
      <Field label="fps" caption="1-120"><input type="number" bind:value={video.fps} min="1" max="120" /></Field>
    {/if}
    {#if source !== "pattern" && source !== "colour" && source !== "blank"}
      <Field label="fit" caption="the wall is {wall}"><select bind:value={fit}><option>stretch</option><option>contain</option><option>cover</option></select></Field>
    {/if}
    {#if source !== "video" && source !== "blank"}
      <Field label="hold" caption="refresh until cancelled, as a job"><input type="checkbox" bind:checked={hold} /></Field>
    {/if}
  </div>

  {#if source === "image file"}
    <FileDrop label="image" accept="image/*" bind:picked={image} />
  {/if}

  {#if source === "video"}
    <JobRunner
      label="play"
      disabled={!ready}
      reason={missing}
      run={() => ops.card!.showVideo({ path: video.path, loop: video.loop, fps: video.fps, fit })}
    />
  {:else}
    <div class="actions">
      <button class="primary" onclick={runStill} disabled={still.busy || !ready}>show</button>
      {#if !ready}<span class="caption">{missing}</span>{/if}
    </div>
    {#if still.error}<p class="error">{still.error}</p>{/if}
    {#if still.result && "lines" in still.result}<Lines lines={still.result.lines} files={still.result.files} />{/if}
    {#if still.result && "id" in still.result}<p class="ok">job {still.result.id} holds it; stop it in the status bar</p>{/if}
  {/if}
</ControlHead>
