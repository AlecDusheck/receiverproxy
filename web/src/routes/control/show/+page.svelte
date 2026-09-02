<script lang="ts">
  // Show: one source, one button. A held show is a job whose lines follow here.
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import Lines from "$parts/Lines.svelte";
  import JobLines from "$parts/JobLines.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { Fit, Outcome, Pattern, Job } from "$api/types";

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
  // The job view while a show runs or once it became a job; the outcome's lines otherwise.
  const job = $derived(show.busy || (show.result && "state" in show.result) ? { busy: show.busy, error: show.error, result: show.result && "state" in show.result ? show.result : null } : null);
</script>

<ControlHead title="Show">
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
      <Field label="path" caption="on the daemon's machine" wide><input bind:value={imagePath} class="mono" /></Field>
    {:else if source === "video path"}
      <Field label="path" caption="on the daemon's machine" wide><input bind:value={video.path} class="mono" /></Field>
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
  <div class="actions"><button class="primary" onclick={runShow} disabled={show.busy || !canShow}>show</button></div>
  {#if job}
    <JobLines act={job} />
  {:else}
    {#if show.error}<p class="error">{show.error}</p>{/if}
    {#if show.result && "files" in show.result}<Lines lines={show.result.lines} files={show.result.files} />{/if}
  {/if}
</ControlHead>
