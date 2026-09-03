<script lang="ts">
  // 28 px at the bottom of every page while the daemon answers: one entry
  // per discovered card (index, model, size, position, what is on it), the
  // controls that matter mid-show, and the running job's line. Absent with
  // no daemon. Under 640 px only the selected card and the brightness stay.
  //
  // Everything here reads `app.live`, which +layout subscribes to once;
  // nothing polls.
  import { app } from "$lib/state.svelte";
  import { brightnessCap } from "$lib/layout";
  import { ops } from "$api/ops";
  import { errText } from "$lib/error";
  import type { Card } from "$api/types";

  const live = $derived(app.live);
  const cards = $derived(live?.cards ?? []);
  const show = $derived(live?.show ?? null);
  const job = $derived(live?.job ?? null);
  const running = $derived(job?.state === "running" ? job : null);

  // The value the slider holds while dragging; the daemon's between drags.
  let dragged = $state<number | null>(null);
  const cap = $derived(brightnessCap(app.wall));
  const brightness = $derived(Math.min(dragged ?? live?.brightness ?? 255, cap));
  let error = $state("");

  const position = (c: Card) => {
    const r = app.wall.receivers.find((q) => q.index === c.controller);
    return r ? `@${r.x},${r.y}` : "";
  };
  const model = (c: Card) => c.model ?? `card 0x${c.card_id.toString(16).padStart(2, "0")}`;
  const onIt = $derived(
    show ? `${show.kind}${show.source && show.source !== show.kind ? ` ${show.source}` : ""}${show.fps ? ` ${show.fps} fps` : ""}` : "nothing since the daemon started",
  );

  async function run(f: () => Promise<unknown>) {
    error = "";
    try {
      await f();
    } catch (e) {
      error = errText(e);
    }
  }

  const setBrightness = (v: number) =>
    run(async () => {
      await ops.card!.brightness(v);
      dragged = null;
    });
</script>

{#if ops.card && live}
  <footer class="statusbar">
    <div class="cards">
      {#each cards as c (c.controller)}
        <button
          class={["card", { selected: c.controller === app.card }]}
          onclick={() => (app.card = c.controller)}
          aria-pressed={c.controller === app.card}
        >
          <span class="mono">{c.controller}</span>
          {model(c)}
          <span class="mono">{c.cols}x{c.rows} {position(c)}</span>
          <span class="muted">{onIt}</span>
        </button>
      {:else}
        <span class="muted">no card answered</span>
      {/each}
    </div>

    {#if running}
      <span class="job muted">{running.kind} {running.id}: running</span>
      <button onclick={() => run(() => ops.card!.cancel(running.id))}>stop</button>
    {/if}
    {#if error}<span class="error">{error}</span>{/if}

    <label class="bright">
      brightness
      <input
        type="range"
        min="0"
        max={cap}
        value={brightness}
        oninput={(e) => (dragged = e.currentTarget.valueAsNumber)}
        onchange={(e) => void setBrightness(e.currentTarget.valueAsNumber)}
        aria-label="brightness"
      />
      <span class="mono value">{brightness}{#if cap < 255}<span class="muted">/{cap}</span>{/if}</span>
    </label>
    <button class="blank" onclick={() => run(() => ops.card!.showBlank())}>blank</button>
  </footer>
{/if}

<style>
  .statusbar {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    height: 28px;
    display: flex;
    align-items: center;
    gap: var(--s3);
    padding: 0 var(--s4);
    background: var(--bg-2);
    border-top: 1px solid var(--line);
    font-size: 12px;
    overflow-x: auto;
    white-space: nowrap;
  }
  .cards {
    display: flex;
    gap: var(--s3);
    align-items: center;
    min-width: 0;
  }
  .card {
    height: 20px;
    padding: 0 var(--s2);
    background: none;
    border: 1px solid transparent;
    font-size: 12px;
  }
  .card.selected {
    border-color: var(--accent);
  }
  .card span + span {
    margin-left: var(--s2);
  }
  .bright {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin-left: auto;
  }
  .bright input {
    width: 120px;
  }
  .value {
    width: 3ch;
    text-align: right;
  }
  .statusbar button:not(.card) {
    height: 20px;
    padding: 0 var(--s2);
    font-size: 12px;
  }
  @media (max-width: 640px) {
    .statusbar {
      gap: var(--s2);
      padding: 0 var(--s3);
    }
    /* the selected card and the brightness; the rest is on /control */
    .card:not(.selected),
    .job,
    .blank,
    .card .muted {
      display: none;
    }
    .bright {
      margin-left: auto;
    }
    .bright input {
      width: 88px;
    }
  }
</style>
