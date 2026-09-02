<script lang="ts">
  // Needs the WASM module: client-rendered (+page.ts). `?panel=<path>` opens a
  // library spec, `?chip=<path>` picks a chip library; both are cleared once read.
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { title } from "$lib/site";
  import TitleRow from "$parts/TitleRow.svelte";
  import Drop from "$parts/Drop.svelte";
  import Lines from "$parts/Lines.svelte";
  import BuilderForm from "./BuilderForm.svelte";
  import BuilderTools from "./BuilderTools.svelte";
  import { app, handSpec } from "$lib/state.svelte";
  import { ops, type Format, type Generated, type Imported } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { errText } from "$lib/error";
  import { save, toB64 } from "$lib/download";
  import { defaultSpec, fromToml, toToml, type PanelSpec } from "$lib/spec";
  import type { GatedOutcome, Libraries, Outcome } from "$api/types";

  const query = page.url.searchParams;

  let libs = $state.raw<Libraries | null>(null);
  let formats = $state.raw<Format[]>([]);
  let format = $state("rcvbp");
  let spec = $state<PanelSpec>(defaultSpec());
  let toml = $state(toToml(defaultSpec()));
  let tomlError = $state("");

  // Form and TOML stay in sync both ways with a 300 ms debounce; a TOML that
  // does not parse leaves the form at the last valid parse.
  let timer = 0;
  const debounce = (f: () => void) => {
    clearTimeout(timer);
    timer = window.setTimeout(f, 300);
  };
  function parse(text: string) {
    try {
      spec = fromToml(text);
      tomlError = "";
    } catch (e) {
      tomlError = errText(e);
    }
    handSpec(text);
  }
  function setToml(text: string) {
    toml = text;
    parse(text);
  }
  const onToml = (text: string) => {
    toml = text;
    debounce(() => parse(text));
  };
  const onForm = () =>
    debounce(() => {
      toml = toToml(spec);
      tomlError = "";
      handSpec(toml);
    });

  // The last TOML edited or handed over (Gallery, import) seeds the pane.
  try {
    const s = localStorage.getItem("rxp.builder.toml");
    if (s) setToml(s);
  } catch {
    /* no storage */
  }
  void Promise.all([ops.pure.libraries(), ops.pure.formats()]).then(([l, f]) => {
    libs = l;
    formats = f.filter((x) => x.generate);
    const p = query.get("panel");
    const c = query.get("chip");
    const lib = p && l.panels.find((x) => x.path === p);
    if (lib) setToml(lib.toml);
    if (c && l.chips.some((x) => x.path === c)) {
      spec.chip.library = c;
      toml = toToml(spec);
      handSpec(toml);
    }
    if (p || c) void goto("/builder", { replaceState: true });
  });

  const imp = new Action<Imported & { file: string }>("import");
  function importFile(files: File[]) {
    const f = files[0]!;
    void imp.run(async () => {
      const r = await ops.pure.importSpec(new Uint8Array(await f.arrayBuffer()));
      setToml(r.spec_toml);
      return { ...r, file: f.name };
    });
  }

  const gen = new Action<Generated>("generate");
  const generate = () => gen.run(() => ops.pure.generate(toml, format));

  const send = new Action<Outcome>("config send");
  const write = new Action<GatedOutcome>("config write");
  async function writeCard(commit: boolean) {
    if (!ops.card) return;
    const card = ops.card;
    await write.run(async () => {
      const g = await ops.pure.generate(toml, "rcvbp");
      const file = g.files.find((f) => f.name.endsWith(".rcvbp"));
      if (!file) throw new Error(`generate: no .rcvbp among ${g.files.map((f) => f.name).join(", ")}`);
      return card.configWrite({ rcvbp: toB64(file.bytes), commit });
    });
  }

  const wasmOff = $derived(app.wasm !== "ready");
</script>

<svelte:head><title>{title("Builder")}</title></svelte:head>

<TitleRow title="Builder">
  {#snippet action()}
    <button class="primary" onclick={generate} disabled={wasmOff || !!tomlError || gen.busy}>Generate</button>
  {/snippet}
</TitleRow>

{#if app.wasm === "failed"}<p class="error">{app.wasmError}</p>{/if}

<Drop label="Import a vendor file or a spec" disabled={wasmOff} onfiles={importFile} />
{#if imp.error}<p class="error">{imp.error}</p>{/if}
{#if imp.result}
  <p class={imp.result.unresolved.length ? "warn" : "ok"}>
    {imp.result.file}: {imp.result.format}{imp.result.unresolved.length ? `, unresolved: ${imp.result.unresolved.join(", ")}` : ", every field resolved"}
  </p>
{/if}

<div class="split">
  <div>
    <BuilderForm bind:spec {libs} onchange={onForm} />
  </div>
  <div class="toml">
    <h2>TOML</h2>
    <textarea rows="36" class={{ invalid: !!tomlError }} value={toml} oninput={(e) => onToml(e.currentTarget.value)} spellcheck="false" aria-label="spec TOML"></textarea>
    {#if tomlError}<div class="error">{tomlError}</div>{/if}
    <div class="actions">
      <button onclick={() => save(`${spec.name}.toml`, toml)}>download TOML</button>
    </div>
  </div>
</div>

<section class="generate">
  <h2>Generate</h2>
  <div class="row">
    <label>format <select bind:value={format} aria-label="output format">
        {#each formats as f (f.name)}<option value={f.name}>{f.name} (.{f.extension}, {f.vendor})</option>{/each}
        {#if !formats.length}<option value="rcvbp">rcvbp</option>{/if}
      </select></label>
    <button class="primary" onclick={generate} disabled={wasmOff || !!tomlError || gen.busy}>Generate</button>
  </div>
  {#if gen.error}<p class="error">{gen.error}</p>{/if}
  {#if gen.result}
    {@const g = gen.result}
    <table class="files">
      <thead><tr><th>file</th><th class="num">bytes</th><th></th></tr></thead>
      <tbody>
        {#each g.files as f (f.name)}
          <tr><td class="mono">{f.name}</td><td class="num">{f.bytes.length}</td><td><button onclick={() => save(f.name, f.bytes)}>download</button></td></tr>
        {/each}
      </tbody>
    </table>
    <div class="actions">
      <button onclick={() => g.files.forEach((f) => save(f.name, f.bytes))}>download all</button>
      <button onclick={() => save(`${g.name}-sources.txt`, g.sources.join("\n") + "\n")}>download sources</button>
    </div>
    {#if g.notes.length}<pre>{g.notes.join("\n")}</pre>{/if}
    <h2 class="sources">Sources</h2>
    <pre>{g.sources.join("\n")}</pre>
  {/if}
</section>

{#if ops.card}
  <section>
    <h2>Card</h2>
    <div class="row">
      <button onclick={() => send.run(() => ops.card!.configSend({ spec_toml: toml }))} disabled={send.busy || !!tomlError}>send to card (RAM only)</button>
      <button onclick={() => writeCard(false)} disabled={write.busy || wasmOff || !!tomlError}>write to card: dry run</button>
    </div>
    {#if send.error}<p class="error">{send.error}</p>{/if}
    {#if send.result}<Lines lines={send.result.lines} files={send.result.files} />{/if}
    {#if write.error}<p class="error">{write.error}</p>{/if}
    {#if write.result}
      <p class={write.result.committed ? "ok" : "muted"}>{write.result.committed ? "written" : "dry run, nothing written"}</p>
      <Lines lines={write.result.lines} files={write.result.files} />
      {#if !write.result.committed}
        <p class="confirm">This writes flash block 7 and the EEPROM mirror.</p>
        <button onclick={() => writeCard(true)} disabled={write.busy}>commit</button>
      {/if}
    {/if}
  </section>
{/if}

<BuilderTools />

<style>
  .split {
    display: grid;
    grid-template-columns: minmax(320px, 1fr) minmax(320px, 1fr);
    gap: var(--s5);
    align-items: start;
    margin-bottom: var(--s5);
  }
  @media (max-width: 860px) {
    .split {
      grid-template-columns: 1fr;
    }
  }
  .toml textarea {
    width: 100%;
  }
  .files {
    margin-top: var(--s3);
    width: auto;
    min-width: 420px;
  }
  .sources {
    margin-top: var(--s3);
  }
  label {
    display: flex;
    gap: var(--s2);
    align-items: center;
  }
</style>
