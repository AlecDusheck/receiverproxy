<script lang="ts">
  // Mirror a screen, a window or a tab onto the wall: getDisplayMedia into a
  // canvas the size of the wall, then one POST /show/frame per frame, the
  // same 12-byte header and rgb24 payload `rxp show serve` reads. The
  // daemon's `show/stream` job holds the link and its lines show below.
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import Lines from "$parts/Lines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { errText } from "$lib/error";
  import { header, rgb24, place } from "./frame";
  import type { Fit } from "$api/types";

  let fps = $state(15);
  let fit = $state<Fit>("contain");
  let error = $state("");
  let sent = $state(0);
  let dropped = $state(0);
  let jobId = $state("");
  let running = $state(false);

  let video: HTMLVideoElement | null = null;
  let canvas: HTMLCanvasElement | null = null;
  let media: MediaStream | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;
  let busy = false;

  const size = $derived({ w: app.wall.width, h: app.wall.height });
  const job = $derived(app.job?.id === jobId ? app.job : null);

  async function start() {
    error = "";
    sent = 0;
    dropped = 0;
    try {
      media = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: false });
    } catch (e) {
      error = errText(e);
      return;
    }
    media.getVideoTracks()[0]?.addEventListener("ended", stop);
    video = document.createElement("video");
    video.srcObject = media;
    video.muted = true;
    await video.play();
    canvas = document.createElement("canvas");
    canvas.width = size.w;
    canvas.height = size.h;
    running = true;
    timer = setInterval(() => void frame(), Math.max(1, Math.round(1000 / fps)));
  }

  function stop() {
    running = false;
    if (timer) clearInterval(timer);
    timer = null;
    media?.getTracks().forEach((t) => t.stop());
    media = null;
    video = null;
    if (jobId) void ops.card?.cancel(jobId).catch(() => {});
  }

  async function frame() {
    if (busy || !video || !canvas) return;
    // One frame at a time: a mirror that runs ahead of the panel is latency.
    busy = true;
    try {
      const ctx = canvas.getContext("2d", { willReadFrequently: true });
      if (!ctx) throw new Error("this browser gave no 2d canvas");
      const [sx, sy, sw, sh, dx, dy, dw, dh] = place(fit, video.videoWidth, video.videoHeight, size.w, size.h);
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, size.w, size.h);
      if (sw > 0 && sh > 0) ctx.drawImage(video, sx, sy, sw, sh, dx, dy, dw, dh);
      const rgba = ctx.getImageData(0, 0, size.w, size.h).data;
      const body = new Uint8Array(12 + size.w * size.h * 3);
      body.set(header(size.w, size.h, fps));
      body.set(rgb24(rgba), 12);
      const started = await ops.card!.showFrame(body, { source: "screen", fit: "stretch" });
      sent += 1;
      if (started.id !== jobId) {
        jobId = started.id;
        void ops.card!.follow(started.id).then(() => {
          if (jobId === started.id) stop();
        });
      }
    } catch (e) {
      dropped += 1;
      error = errText(e);
      if (dropped > 4) stop();
    } finally {
      busy = false;
    }
  }
</script>

<ControlHead title="Mirror">
  <div class="form">
    <Field label="fps" caption="1-30; the browser reads the screen at this rate"><input type="number" bind:value={fps} min="1" max="30" disabled={running} /></Field>
    <Field label="fit" caption="the wall is {size.w}x{size.h}"><select bind:value={fit}><option>stretch</option><option>contain</option><option>cover</option></select></Field>
  </div>

  <div class="actions">
    {#if running}
      <button class="primary" onclick={stop}>stop</button>
    {:else}
      <button class="primary" onclick={() => void start()} disabled={!ops.card}>choose a screen</button>
    {/if}
    <span class="caption">{sent} frames sent{dropped ? `, ${dropped} refused` : ""}{jobId ? `, job ${jobId}` : ""}</span>
  </div>

  {#if error}<p class="error">{error}</p>{/if}
  {#if job}
    <p class={job.state === "running" ? "muted" : job.state === "done" ? "ok" : "error"}>{job.kind} {job.id}: {job.state}{job.error ? `: ${job.error}` : ""}</p>
    <Lines lines={job.lines} />
  {/if}
  <p class="caption">
    The screen is scaled in the browser to the wall's size and sent as
    rgb24; the daemon draws each frame as it arrives and ends the stream
    five seconds after the last one.
  </p>
</ControlHead>
