<script lang="ts">
  import Field from "../parts/Field.svelte";
  import Lines from "../parts/Lines.svelte";
  import { app } from "../lib/state.svelte";
  import { ops, type CardOps } from "../api/ops";
  import { errText } from "../lib/error";
  import type { Fit, GatedOutcome, Job, Outcome, Pattern } from "../api/types";

  let { query }: { query: URLSearchParams } = $props();

  const busy = $derived(app.status.kind === "busy");
  const cards = $derived(app.health?.cards ?? []);
  const hex2 = (n: number) => "0x" + n.toString(16).padStart(2, "0");

  // One error slot and one output slot per section, keyed by name. A
  // started job is followed to its end; an outcome is shown as it is.
  let errors = $state<Record<string, string>>({});
  let outs = $state<Record<string, Outcome | GatedOutcome | Job | null>>({});
  async function run<T>(key: string, f: (card: CardOps) => Promise<T>): Promise<T | null> {
    const card = ops.card;
    if (!card) return null;
    errors[key] = "";
    try {
      const r = await f(card);
      if (r && typeof r === "object") {
        if ("lines" in r) outs[key] = r as unknown as Outcome;
        else if ("id" in r && typeof r.id === "string") outs[key] = await card.follow(r.id);
      }
      return r;
    } catch (e) {
      errors[key] = errText(e);
      return null;
    }
  }
  const gated = (key: string) => outs[key] as GatedOutcome | null;
  const jobOut = (key: string) => outs[key] as Job | null;
  const canCommit = (key: string) => {
    const o = outs[key];
    if (!o) return false;
    if ("state" in o) return o.state === "done" && !!o.result && "committed" in o.result && !o.result.committed;
    return "committed" in o && !o.committed;
  };

  // discover
  const discover = () => run("discover", async (c) => { const cards = await c.discover(); if (app.health) app.health.cards = cards; return cards; });

  // brightness
  let brightness = $state(app.settings?.brightness ?? 255);
  const setBrightness = () => run("brightness", async (c) => { const v = await c.brightness(brightness); if (app.settings) app.settings.brightness = v; return v; });

  // show
  let pattern = $state<Pattern>("rgb");
  let hold = $state(false);
  let fit = $state<Fit>("stretch");
  let fill = $state("#ff8000");
  let imagePath = $state("");
  let imageFile = $state<File | null>(null);
  let video = $state({ path: "", loop: true, fps: 30, fit: "contain" as Fit });

  // provision
  let prov = $state({ spec_toml: "", firmware_path: "", x: 0, y: 0 });
  try {
    prov.spec_toml = localStorage.getItem("e120.builder.toml") ?? "";
  } catch {
    /* no storage */
  }
  // The Wall's "provision this card" sets the position from the receiver.
  $effect(() => {
    const provIndex = query.get("provision");
    if (provIndex === null) return;
    const r = app.wall.receivers.find((q) => q.index === Number(provIndex));
    if (r) {
      prov.x = r.x ?? 0;
      prov.y = r.y ?? 0;
    }
  });
  const provision = (commit: boolean) =>
    run("provision", (c) => c.provision({ spec_toml: prov.spec_toml, firmware_path: prov.firmware_path || undefined, position: [prov.x, prov.y], commit }));

  // firmware, flash, card state
  let fw = $state("");
  let snapDir = $state("");
  let restoreDir = $state("");
  let size = $state({ width: 128, height: 64 });
  let test = $state(0);
  let layout = $state({ w: 128, h: 64 });
</script>

<h1>Cards</h1>

<section>
  <h2>Discovered</h2>
  <table>
    <thead><tr><th>controller</th><th>card id</th><th>firmware</th><th>detected size</th></tr></thead>
    <tbody>
      {#each cards as c (c.controller)}
        <tr><td>{c.controller}</td><td class="mono">{hex2(c.card_id)}</td><td>{c.ver_major}.{c.ver_minor}</td><td>{c.cols}x{c.rows}</td></tr>
      {:else}
        <tr><td colspan="4" class="muted">no card answered</td></tr>
      {/each}
    </tbody>
  </table>
  <div class="row" style="margin-top: var(--s2)">
    <button onclick={discover} disabled={busy}>discover</button>
    {#if app.health}<span class="muted">daemon {app.health.version}, iface {app.health.iface}</span>{/if}
  </div>
  {#if errors.discover}<div class="error">{errors.discover}</div>{/if}
</section>

<section>
  <h2>Brightness</h2>
  <div class="row">
    <input type="range" min="0" max="255" bind:value={brightness} onchange={setBrightness} disabled={busy} style="width: 256px" />
    <input type="number" min="0" max="255" bind:value={brightness} onchange={setBrightness} disabled={busy} />
  </div>
  {#if errors.brightness}<div class="error">{errors.brightness}</div>{/if}
</section>

<section>
  <h2>Show</h2>
  <div class="form">
    <Field label="options">
      <label><input type="checkbox" bind:checked={hold} /> hold (refresh until cancelled)</label>
      <label>fit <select bind:value={fit}><option>stretch</option><option>contain</option><option>cover</option></select></label>
    </Field>
    <Field label="pattern">
      <select bind:value={pattern}>{#each ["rgb", "border", "rows", "gradient", "white"] as n (n)}<option value={n}>{n}</option>{/each}</select>
      <button onclick={() => run("show", (c) => c.showPattern({ name: pattern, hold }))} disabled={busy}>show</button>
    </Field>
    <Field label="fill">
      <input type="color" bind:value={fill} />
      <span class="mono">{fill}</span>
      <button onclick={() => run("show", (c) => c.showFill({ rgb: fill.slice(1), hold }))} disabled={busy}>show</button>
    </Field>
    <Field label="image">
      <input type="file" accept="image/*" onchange={(e) => (imageFile = e.currentTarget.files?.[0] ?? null)} />
      <button onclick={() => imageFile && run("show", (c) => c.showImageFile(imageFile!, fit, hold))} disabled={busy || !imageFile}>show file</button>
    </Field>
    <Field label="image path">
      <input bind:value={imagePath} placeholder="path as the daemon sees it" style="width: 320px" />
      <button onclick={() => run("show", (c) => c.showImage({ path: imagePath, fit, hold }))} disabled={busy || !imagePath}>show</button>
    </Field>
    <Field label="video path">
      <input bind:value={video.path} placeholder="clip.mp4" style="width: 320px" />
      <label><input type="checkbox" bind:checked={video.loop} /> loop</label>
      <label>fps <input type="number" bind:value={video.fps} min="1" max="120" /></label>
      <label>fit <select bind:value={video.fit}><option>stretch</option><option>contain</option><option>cover</option></select></label>
      <button onclick={() => run("show", (c) => c.showVideo({ path: video.path, loop: video.loop, fps: video.fps, fit: video.fit }))} disabled={busy || !video.path}>play</button>
    </Field>
    <Field label="blank"><button onclick={() => run("show", (c) => c.showBlank())} disabled={busy}>blank</button></Field>
  </div>
  {#if errors.show}<div class="error">{errors.show}</div>{/if}
  {#if outs.show}<Lines lines={outs.show.lines} files={"files" in outs.show ? outs.show.files : []} />{/if}
</section>

<section>
  <h2>Provision</h2>
  <p class="muted">Snapshot, firmware, EEPROM read, config, EEPROM write. The dry run discovers the card and prints the plan; nothing is written until "Write to card". Power-cycle the card afterwards.</p>
  <div class="form">
    <Field label="spec TOML"><textarea rows="8" bind:value={prov.spec_toml} spellcheck="false" style="width: 100%"></textarea></Field>
    <Field label="firmware path"><input bind:value={prov.firmware_path} placeholder="optional .hex, as the daemon sees it" style="width: 100%" /></Field>
    <Field label="position"><input type="number" bind:value={prov.x} min="0" /> , <input type="number" bind:value={prov.y} min="0" /></Field>
    <Field label="">
      <button class="primary" onclick={() => provision(false)} disabled={busy || !prov.spec_toml}>dry run</button>
      {#if canCommit("provision")}<button class="primary" onclick={() => provision(true)} disabled={busy}>Write to card</button>{/if}
    </Field>
  </div>
  {#if errors.provision}<div class="error">{errors.provision}</div>{/if}
  {#if jobOut("provision")}
    {@const j = jobOut("provision")!}
    <p class={j.state === "done" ? "ok" : j.state === "failed" ? "error" : "muted"}>{j.kind} {j.id}: {j.state}{j.error ? `: ${j.error}` : ""}{j.result && "committed" in j.result ? (j.result.committed ? ", written" : ", dry run") : ""}</p>
    <Lines lines={j.lines} files={j.result?.files ?? []} />
  {/if}
</section>

<section>
  <h2>Firmware</h2>
  <div class="form">
    <Field label="image path"><input bind:value={fw} placeholder=".hex as the daemon sees it" style="width: 100%" /></Field>
    <Field label="">
      <button onclick={() => run("firmware", (c) => c.firmwareInstall({ path: fw, commit: false }))} disabled={busy || !fw}>dry run</button>
      {#if canCommit("firmware")}<button class="primary" onclick={() => run("firmware", (c) => c.firmwareInstall({ path: fw, commit: true }))} disabled={busy}>Write to card</button>{/if}
    </Field>
  </div>
  {#if errors.firmware}<div class="error">{errors.firmware}</div>{/if}
  {#if jobOut("firmware")}
    {@const j = jobOut("firmware")!}
    <p class={j.state === "done" ? "ok" : j.state === "failed" ? "error" : "muted"}>{j.kind} {j.id}: {j.state}{j.error ? `: ${j.error}` : ""}</p>
    <Lines lines={j.lines} files={j.result?.files ?? []} />
  {/if}
</section>

<section>
  <h2>Flash</h2>
  <div class="form">
    <Field label="snapshot to">
      <input bind:value={snapDir} placeholder="directory; default under the daemon's data dir" style="width: 320px" />
      <button onclick={() => run("snapshot", (c) => c.flashSnapshot({ dir: snapDir || undefined }))} disabled={busy}>snapshot</button>
    </Field>
    <Field label="restore from">
      <input bind:value={restoreDir} placeholder="snapshot directory" style="width: 320px" />
      <button onclick={() => run("restore", (c) => c.flashRestore({ dir: restoreDir, commit: false }))} disabled={busy || !restoreDir}>dry run</button>
      {#if canCommit("restore")}<button class="primary" onclick={() => run("restore", (c) => c.flashRestore({ dir: restoreDir, commit: true }))} disabled={busy}>Write to card</button>{/if}
    </Field>
  </div>
  {#each ["snapshot", "restore"] as k (k)}
    {#if errors[k]}<div class="error">{errors[k]}</div>{/if}
    {#if jobOut(k)}
      {@const j = jobOut(k)!}
      <p class={j.state === "done" ? "ok" : j.state === "failed" ? "error" : "muted"}>{j.kind} {j.id}: {j.state}{j.error ? `: ${j.error}` : ""}</p>
      <Lines lines={j.lines} files={j.result?.files ?? []} />
    {/if}
  {/each}
</section>

<section>
  <h2>Card state</h2>
  <div class="form">
    <Field label="screen size">
      <input type="number" bind:value={size.width} min="1" /> x <input type="number" bind:value={size.height} min="1" />
      <button onclick={() => run("size", async (c) => { const r = await c.screenSize(); size = r; return r; })} disabled={busy}>read</button>
      <button onclick={() => run("size", (c) => c.setScreenSize({ ...size, commit: false }))} disabled={busy}>dry run</button>
      {#if canCommit("size")}<button class="primary" onclick={() => run("size", (c) => c.setScreenSize({ ...size, commit: true }))} disabled={busy}>Write to card</button>{/if}
    </Field>
    <Field label="test mode">
      <input type="number" bind:value={test} min="0" max="255" />
      <button onclick={() => run("test", (c) => c.testMode({ n: test }))} disabled={busy}>set</button>
      <span class="muted">0 is off</span>
    </Field>
    <Field label="set layout (RAM)">
      <input type="number" bind:value={layout.w} min="1" /> x <input type="number" bind:value={layout.h} min="1" />
      <button onclick={() => run("layout", (c) => c.setLayout({ panel_width: layout.w, panel_height: layout.h }))} disabled={busy}>send</button>
    </Field>
    <Field label="reload">
      <button onclick={() => run("reload", (c) => c.reload())} disabled={busy}>reload</button>
      <button onclick={() => run("reload", (c) => c.reload({ full: true }))} disabled={busy}>full reload</button>
    </Field>
  </div>
  {#each ["size", "test", "layout", "reload"] as k (k)}
    {#if errors[k]}<div class="error">{errors[k]}</div>{/if}
    {#if outs[k] && "lines" in outs[k]!}
      {@const o = outs[k]!}
      {#if k === "size" && gated(k) && "committed" in o}<p class={o.committed ? "ok" : "muted"}>{o.committed ? "written" : "dry run, nothing written"}</p>{/if}
      <Lines lines={o.lines} files={"files" in o ? o.files : []} />
    {/if}
  {/each}
</section>
