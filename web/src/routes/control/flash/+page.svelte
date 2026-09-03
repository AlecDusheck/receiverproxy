<script lang="ts">
  // The card's flash: write a configuration (generated here from a library
  // spec, or an uploaded .rcvbp), snapshot every block, or restore a
  // snapshot. Each is gated: the dry run first, "commit" under its plan.
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import FileDrop, { type Picked } from "$parts/FileDrop.svelte";
  import JobRunner from "$parts/JobRunner.svelte";
  import LibraryPicker from "$parts/LibraryPicker.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { save } from "$lib/download";
  import type { Generated } from "$api/types";

  // The configuration to write: generated from a spec, or a file's bytes.
  let from = $state<"library" | "file">("library");
  let specPath = $state("");
  let file = $state<Picked | null>(null);
  const gen = new Action<Generated>("generate");
  const generate = (toml: string) => gen.run(() => ops.pure.generate(toml, "rcvbp"));

  const built = $derived(gen.result?.files.find((f) => f.name.endsWith(".rcvbp")) ?? null);
  const bytes = $derived(from === "file" ? (file?.bytes ?? null) : (built?.bytes ?? null));
  const label = $derived(from === "file" ? (file?.file.name ?? "") : (built?.name ?? ""));

  let dir = $state("");
</script>

<ControlHead title="Flash">
  <section>
    <h2>Write a configuration</h2>
    <div class="form">
      <Field label="from">
        <select bind:value={from}><option value="library">a spec from the library</option><option value="file">a .rcvbp file</option></select>
      </Field>
    </div>
    {#if from === "library"}
      <LibraryPicker bind:value={specPath} onpick={(s) => void generate(s.toml)} />
      {#if gen.busy}<p class="muted">generating</p>{/if}
      {#if gen.error}<p class="error">{gen.error}</p>{/if}
      {#if built}
        <p class="ok mono">{built.name}, {built.bytes.length} bytes <button onclick={() => save(built.name, built.bytes)}>download</button></p>
      {/if}
    {:else}
      <FileDrop label=".rcvbp" accept=".rcvbp" bind:picked={file} />
    {/if}
    <JobRunner
      label="write"
      disabled={!bytes}
      reason={bytes ? "" : "no configuration chosen"}
      confirm="This writes {label} to the parameter block of card {app.card}; the old block 7 is backed up first."
      run={(commit) => ops.card!.configWriteBytes(bytes!, { commit, index: app.card })}
    />
  </section>

  <section>
    <h2>Snapshot and restore</h2>
    <div class="form">
      <Field label="directory" caption="on the daemon's machine; empty writes under its data directory" wide mono>
        <input bind:value={dir} class="mono" />
      </Field>
    </div>
    <JobRunner label="snapshot" run={() => ops.card!.flashSnapshot({ dir: dir || undefined, index: app.card })} />
    <JobRunner
      label="restore"
      disabled={!dir}
      reason={dir ? "" : "restore needs a snapshot directory"}
      confirm="This writes every block of {dir} to card {app.card}."
      run={(commit) => ops.card!.flashRestore({ dir, commit, index: app.card })}
    />
  </section>
</ControlHead>
