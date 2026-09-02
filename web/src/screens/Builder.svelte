<script lang="ts">
  import Field from "../parts/Field.svelte";
  import Hex from "../parts/Hex.svelte";
  import Lines from "../parts/Lines.svelte";
  import { app } from "../lib/state.svelte";
  import { ready, errText } from "../lib/wasm";
  import { api } from "../lib/api";
  import { save, toB64 } from "../lib/download";
  import { defaultSpec, fromToml, toToml, type PanelSpec } from "../lib/spec";
  import type { Diff, GatedOutcome, Generated, Inspection, Libraries, Outcome } from "../lib/types";

  let { query }: { query: URLSearchParams } = $props();

  let libs = $state<Libraries | null>(null);
  let spec = $state<PanelSpec>(defaultSpec());
  let toml = $state(toToml(defaultSpec()));
  let tomlError = $state("");
  let tab = $state<"build" | "inspect" | "diff">("build");

  // The last TOML edited here survives a reload and seeds the provision form.
  try {
    const s = localStorage.getItem("e120.builder.toml");
    if (s) setToml(s);
  } catch {
    /* no storage */
  }

  void ready.then((m) => {
    libs = m.libraries();
    const p = query.get("panel");
    const c = query.get("chip");
    const lib = p && libs.panels.find((x) => x.path === p);
    if (lib) setToml(lib.toml);
    if (c && libs.chips.some((x) => x.path === c)) {
      spec.chip.library = c;
      fromForm();
    }
    if (p || c) location.hash = "#/builder";
  });

  function setToml(text: string) {
    toml = text;
    try {
      spec = fromToml(text);
      tomlError = "";
    } catch (e) {
      tomlError = errText(e);
    }
    try {
      localStorage.setItem("e120.builder.toml", text);
    } catch {
      /* no storage */
    }
  }
  function fromForm() {
    toml = toToml(spec);
    tomlError = "";
    try {
      localStorage.setItem("e120.builder.toml", toml);
    } catch {
      /* no storage */
    }
  }

  // Generate
  let gen = $state<Generated | null>(null);
  let genError = $state("");
  async function generate() {
    genError = "";
    gen = null;
    try {
      const m = await ready;
      gen = m.generate(toml);
      save(`${gen.name}.rcvbp`, gen.rcvbp);
      save(`${gen.name}-basic-pack.bin`, gen.basic_pack);
      if (gen.block7) save(`${gen.name}-block7.bin`, gen.block7);
      save(`${gen.name}-sources.txt`, gen.sources.join("\n") + "\n");
    } catch (e) {
      genError = errText(e);
    }
  }

  // Card actions (daemon present)
  let sendOut = $state<Outcome | null>(null);
  let sendError = $state("");
  async function send() {
    sendError = "";
    sendOut = null;
    try {
      sendOut = await api.configSend(toml);
    } catch (e) {
      sendError = errText(e);
    }
  }
  let writeOut = $state<GatedOutcome | null>(null);
  let writeError = $state("");
  async function write(commit: boolean) {
    writeError = "";
    if (!commit) writeOut = null;
    try {
      const m = await ready;
      const g = m.generate(toml);
      writeOut = await api.configWrite(toB64(g.rcvbp), commit);
    } catch (e) {
      writeError = errText(e);
    }
  }

  // Inspect
  let insp = $state<Inspection | null>(null);
  let inspError = $state("");
  let inspName = $state("");
  const readFile = (f: File) => f.arrayBuffer().then((b) => new Uint8Array(b));
  async function inspect(files: FileList | null) {
    const f = files?.[0];
    if (!f) return;
    inspName = f.name;
    insp = null;
    inspError = "";
    try {
      const m = await ready;
      insp = m.inspect(await readFile(f));
    } catch (e) {
      inspError = errText(e);
    }
  }
  function drop(e: DragEvent) {
    e.preventDefault();
    void inspect(e.dataTransfer?.files ?? null);
  }

  // Diff
  let fa = $state<File | null>(null);
  let fb = $state<File | null>(null);
  let dif = $state<Diff | null>(null);
  let difError = $state("");
  async function diff() {
    if (!fa || !fb) return;
    dif = null;
    difError = "";
    try {
      const m = await ready;
      dif = m.diff(await readFile(fa), await readFile(fb));
    } catch (e) {
      difError = errText(e);
    }
  }

  const hex = (n: number, w = 2) => "0x" + n.toString(16).padStart(w, "0");
  const wasmOff = $derived(app.wasm !== "ready");
  const busy = $derived(app.status.kind === "busy");
  let overrideOffset = $state("");
  let overrideValue = $state("");
</script>

<h1>Builder</h1>
<div class="row" style="margin-bottom: var(--s4)">
  {#each [["build", "Build"], ["inspect", "Inspect"], ["diff", "Diff"]] as [id, label] (id)}
    <button class={{ primary: tab === id }} onclick={() => (tab = id as typeof tab)}>{label}</button>
  {/each}
</div>
{#if app.wasm === "failed"}
  <p class="error">{app.wasmError}</p>
{/if}

{#if tab === "build"}
  <div class="split">
    <div class="form" oninput={fromForm} onchange={fromForm}>
      <Field label="name"><input bind:value={spec.name} /></Field>

      <h2 class="wide">Module</h2>
      <Field label="size"><input type="number" bind:value={spec.module.width} min="1" /> x <input type="number" bind:value={spec.module.height} min="1" /></Field>
      <Field label="scan"><input type="number" bind:value={spec.module.scan} min="1" max="255" /></Field>
      <Field label="line_dir"><input type="number" bind:value={spec.module.line_dir} min="0" max="255" /></Field>
      <Field label="data_groups"><input type="number" bind:value={spec.module.data_groups} min="1" max="255" /></Field>
      <Field label="serial_clock"><input type="number" bind:value={spec.module.serial_clock} placeholder="chip default" /></Field>
      <Field label="gray_bits"><input type="number" bind:value={spec.module.gray_bits} placeholder="default" /></Field>

      <h2 class="wide">Screen</h2>
      <Field label="size"><input type="number" bind:value={spec.screen.width} min="1" /> x <input type="number" bind:value={spec.screen.height} min="1" /></Field>

      <h2 class="wide">Chip</h2>
      <Field label="library">
        <select bind:value={spec.chip.library} disabled={!libs}>
          <option value="">choose</option>
          {#each libs?.chips ?? [] as c (c.path)}<option value={c.path}>{c.name} ({c.path})</option>{/each}
        </select>
      </Field>

      <h2 class="wide">Colour</h2>
      <Field label="swap"><input type="number" bind:value={spec.color.swap} min="0" max="255" /></Field>
      <Field label="source">
        {#each [0, 1, 2] as i (i)}<input type="number" bind:value={spec.color.source[i]} min="0" max="2" />{/each}
      </Field>

      <h2 class="wide">Current</h2>
      <Field label="gains R G B vR">
        {#each [0, 1, 2, 3] as i (i)}<input type="number" bind:value={spec.current.gains[i]} min="0" max="63" />{/each}
      </Field>
      <Field label="percent R G B">
        {#each [0, 1, 2] as i (i)}<input type="number" bind:value={spec.current.percent[i]} step="0.01" min="0" max="1" />{/each}
      </Field>

      <h2 class="wide">Timing</h2>
      <Field label="gamma"><input type="number" bind:value={spec.timing.gamma} step="0.1" /></Field>
      <Field label="refresh_hz"><input type="number" bind:value={spec.timing.refresh_hz} step="1" /></Field>
      <Field label="gclock"><input type="number" bind:value={spec.timing.gclock} min="0" max="255" /> <span class="mono muted">{hex(spec.timing.gclock)}</span></Field>
      <Field label="min_oe"><input type="number" bind:value={spec.timing.min_oe} step="0.00001" /></Field>
      <Field label="luminance_level"><input type="number" bind:value={spec.timing.luminance_level} min="0" max="65535" /></Field>
      <Field label="oe_8ns"><input type="checkbox" bind:checked={spec.timing.oe_8ns} /></Field>

      <h2 class="wide">Mapping</h2>
      <Field label="reversed_groups"><input type="checkbox" bind:checked={spec.mapping.reversed_groups} /></Field>
      <Field label="reversed_lines"><input type="checkbox" bind:checked={spec.mapping.reversed_lines} /></Field>
      <Field label="block"><input type="number" bind:value={spec.mapping.block} placeholder="module width" /></Field>
      <Field label="gate_phantom_positions"><input type="checkbox" bind:checked={spec.mapping.gate_phantom_positions} /></Field>

      <h2 class="wide">Boot</h2>
      <Field label="arm_at_boot"><input type="checkbox" bind:checked={spec.boot.arm_at_boot} /></Field>

      <h2 class="wide">Record 0x01 overrides</h2>
      {#each spec.overrides as ov, i (i)}
        <Field label={ov.offset}>
          <input type="number" bind:value={ov.value} min="0" max="255" /> <span class="mono muted">{hex(ov.value)}</span>
          <button onclick={() => { spec.overrides.splice(i, 1); fromForm(); }}>remove</button>
        </Field>
      {/each}
      <Field label="add">
        <input placeholder="0x02F" bind:value={overrideOffset} style="width: 80px" class="mono" />
        <input placeholder="0x01" bind:value={overrideValue} style="width: 80px" class="mono" />
        <button
          disabled={!/^0x[0-9a-fA-F]+$/.test(overrideOffset) || Number.isNaN(Number(overrideValue))}
          onclick={() => { spec.overrides.push({ offset: overrideOffset, value: Number(overrideValue) }); overrideOffset = ""; overrideValue = ""; fromForm(); }}>add</button>
      </Field>
    </div>

    <div class="toml">
      <h2>TOML</h2>
      <textarea rows="30" value={toml} oninput={(e) => setToml(e.currentTarget.value)} spellcheck="false"></textarea>
      {#if tomlError}<div class="error">{tomlError}</div>{/if}
      <div class="row" style="margin-top: var(--s3)">
        <button class="primary" onclick={generate} disabled={wasmOff || !!tomlError}>Generate</button>
        <button onclick={() => save(`${spec.name}.toml`, toml)}>Download TOML</button>
        {#if app.daemon === "present"}
          <button onclick={send} disabled={busy}>Send to card (RAM)</button>
          <button onclick={() => write(false)} disabled={busy || wasmOff}>Write to card: dry run</button>
          {#if writeOut && !writeOut.committed}
            <button class="primary" onclick={() => write(true)} disabled={busy}>Write to card</button>
          {/if}
        {/if}
      </div>
      {#if genError}<div class="error">{genError}</div>{/if}
      {#if gen}
        <section style="margin-top: var(--s4)">
          <h2>{gen.name}</h2>
          <p>rcvbp {gen.rcvbp.length} bytes, basic pack {gen.basic_pack.length} bytes, block 7 {gen.block7 ? `${gen.block7.length} bytes` : "not built"}. Files downloaded.</p>
          {#if gen.notes.length}<pre>{gen.notes.join("\n")}</pre>{/if}
          <h2 style="margin-top: var(--s3)">Sources</h2>
          <pre>{gen.sources.join("\n")}</pre>
        </section>
      {/if}
      {#if sendError}<div class="error">{sendError}</div>{/if}
      {#if sendOut}<Lines lines={sendOut.lines} files={sendOut.files} />{/if}
      {#if writeError}<div class="error">{writeError}</div>{/if}
      {#if writeOut}
        <p class={writeOut.committed ? "ok" : "muted"}>{writeOut.committed ? "written" : "dry run, nothing written"}</p>
        <Lines lines={writeOut.lines} files={writeOut.files} />
      {/if}
    </div>
  </div>
{:else if tab === "inspect"}
  <div class="dropzone" ondragover={(e) => e.preventDefault()} ondrop={drop} role="region" aria-label="drop a .rcvbp file">
    <input type="file" accept=".rcvbp" onchange={(e) => inspect(e.currentTarget.files)} disabled={wasmOff} />
    <span class="muted">or drop a .rcvbp here</span>
  </div>
  {#if inspError}<div class="error">{inspError}</div>{/if}
  {#if insp}
    <section>
      <h2>{inspName}</h2>
      <p>version {insp.version}{insp.cabinet ? `, cabinet ${insp.cabinet[0]}x${insp.cabinet[1]}` : ""}, {insp.records.length} records</p>
      <div class="scroll">
        <table>
          <thead><tr><th>offset</th><th>type</th><th>length</th><th>non-zero</th><th>description</th></tr></thead>
          <tbody>
            {#each insp.records as r (r.offset)}
              <tr class={{ muted: r.empty }}>
                <td class="mono">{hex(r.offset, 4)}</td>
                <td class="mono">{r.type}</td>
                <td>{r.length}</td>
                <td>{r.nonzero}</td>
                <td>{r.description}{r.empty ? " (empty)" : ""}</td>
              </tr>
              {#if r.fields}
                <tr>
                  <td></td>
                  <td colspan="4">
                    <div class="fields">
                      {#each Object.entries(r.fields) as [k, v] (k)}
                        {#if !(v instanceof Uint8Array)}
                          <span><span class="muted">{k}</span> <span class="mono">{Array.isArray(v) ? v.join("x") : v}</span></span>
                        {/if}
                      {/each}
                    </div>
                    <div class="muted">swap_ramp (+0x19A)</div>
                    <Hex bytes={r.fields.swap_ramp} offset={0x19a} />
                    <div class="muted">chip_custom (+0x06A)</div>
                    <Hex bytes={r.fields.chip_custom} offset={0x6a} />
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}
{:else}
  <div class="form">
    <Field label="a"><input type="file" accept=".rcvbp" onchange={(e) => (fa = e.currentTarget.files?.[0] ?? null)} /></Field>
    <Field label="b"><input type="file" accept=".rcvbp" onchange={(e) => (fb = e.currentTarget.files?.[0] ?? null)} /></Field>
    <Field label=""><button class="primary" onclick={diff} disabled={!fa || !fb || wasmOff}>Diff</button></Field>
  </div>
  {#if difError}<div class="error">{difError}</div>{/if}
  {#if dif}
    <section style="margin-top: var(--s4)">
      <p>{dif.a_records} records in a, {dif.b_records} in b. {dif.only_a.length ? `Only in a: ${dif.only_a.join(", ")}. ` : ""}{dif.only_b.length ? `Only in b: ${dif.only_b.join(", ")}.` : ""}</p>
      {#if dif.records.length === 0}
        <p class="ok">no differing records</p>
      {:else}
        <table>
          <thead><tr><th>record</th><th>len a</th><th>len b</th><th>differing offsets</th></tr></thead>
          <tbody>
            {#each dif.records as r (r.type)}
              <tr>
                <td class="mono">{r.type}</td>
                <td>{r.len_a}</td>
                <td>{r.len_b}</td>
                <td class="mono">{r.offsets.length}: {r.offsets.slice(0, 64).map((o) => hex(o, 3)).join(" ")}{r.offsets.length > 64 ? " ..." : ""}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
  {/if}
{/if}

<style>
  .split {
    display: grid;
    grid-template-columns: minmax(360px, 560px) minmax(320px, 1fr);
    gap: var(--s6);
    align-items: start;
  }
  @media (max-width: 900px) {
    .split {
      grid-template-columns: 1fr;
    }
  }
  .form h2 {
    margin: var(--s4) 0 var(--s1);
  }
  .form h2:first-of-type {
    margin-top: 0;
  }
  .toml textarea {
    width: 100%;
  }
  .dropzone {
    border: 1px dashed var(--line);
    padding: var(--s4);
    display: flex;
    gap: var(--s3);
    align-items: center;
    margin-bottom: var(--s4);
  }
  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s1) var(--s4);
    margin-bottom: var(--s2);
  }
</style>
