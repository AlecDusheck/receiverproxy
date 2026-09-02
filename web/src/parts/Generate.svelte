<script lang="ts">
  // Generate a config from a spec and offer its files: the format picker, the
  // per-file table, all-as-zip, and the sources list. Used by the Builder and
  // the panel pages.
  import { ops, type Generated } from "$api/ops";
  import { app } from "$lib/state.svelte";
  import { Action } from "$lib/action.svelte";
  import { save } from "$lib/download";
  import { zip } from "$lib/zip";
  import type { Format } from "$api/types";

  let {
    toml,
    formats,
    disabled = false,
    disabledReason = "",
  }: { toml: string; formats: Format[]; disabled?: boolean; disabledReason?: string } = $props();

  let format = $state("rcvbp");
  $effect(() => {
    if (formats.length && !formats.some((f) => f.name === format)) format = formats[0]!.name;
  });

  const gen = new Action<Generated>("generate");
  const off = $derived(disabled || app.wasm === "failed");
  export const generate = () => gen.run(() => ops.pure.generate(toml, format));

  /** The format's own file: what most people want. */
  const main = (g: Generated) => g.files.find((f) => f.name.endsWith(`.${ext()}`)) ?? g.files[0];
  const ext = () => formats.find((f) => f.name === format)?.extension ?? "rcvbp";

  function saveZip(g: Generated) {
    save(
      `${g.name}.zip`,
      zip([...g.files, { name: `${g.name}-sources.txt`, bytes: new TextEncoder().encode(g.sources.join("\n") + "\n") }]),
    );
  }
</script>

<div class="row">
  {#if formats.length > 1}
    <label>format <select bind:value={format} aria-label="output format">
        {#each formats as f (f.name)}<option value={f.name}>{f.vendor} {f.name}</option>{/each}
      </select></label>
  {/if}
  <button class="primary" onclick={generate} disabled={off || gen.busy}>Generate</button>
</div>
{#if app.wasm === "failed"}<p class="error">{app.wasmError}</p>{/if}
{#if disabled && disabledReason}<p class="caption">{disabledReason}</p>{/if}
{#if gen.error}<p class="error">{gen.error}</p>{/if}
{#if gen.result}
  {@const g = gen.result}
  {@const first = main(g)}
  <div class="row mt-2">
    {#if first}<button class="primary mono" onclick={() => save(first.name, first.bytes)}>{first.name}</button>{/if}
    <button class="mono" onclick={() => saveZip(g)}>{g.name}.zip</button>
  </div>
  <div class="scroll mt-2">
    <table class="files">
      <thead><tr><th>file</th><th class="num">bytes</th><th></th></tr></thead>
      <tbody>
        {#each g.files as f (f.name)}
          <tr><td class="mono">{f.name}</td><td class="num">{f.bytes.length}</td><td><button onclick={() => save(f.name, f.bytes)}>download</button></td></tr>
        {/each}
        <tr><td class="mono">{g.name}-sources.txt</td><td class="num">{g.sources.join("\n").length + 1}</td><td><button onclick={() => save(`${g.name}-sources.txt`, new TextEncoder().encode(g.sources.join("\n") + "\n"))}>download</button></td></tr>
      </tbody>
    </table>
  </div>
  {#if g.notes.length}<pre>{g.notes.join("\n")}</pre>{/if}
  <details class="mt-2">
    <summary>sources</summary>
    <pre>{g.sources.join("\n")}</pre>
  </details>
{/if}

<style>
  .files td:last-child { text-align: right; }
  summary { cursor: pointer; color: var(--text-2); }
</style>
