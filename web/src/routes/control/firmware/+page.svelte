<script lang="ts">
  // Install a firmware image on the selected card. The image is one of the
  // manifest's (the same table as /cards/<model>/firmware, filtered here) or
  // an uploaded .hex, which the daemon checks against the manifest when its
  // name is in it. "suggest for a spec" ranks the manifest for a panel spec
  // through POST /firmware/pick and marks the recommendation with its reason.
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import FileDrop, { type Picked } from "$parts/FileDrop.svelte";
  import JobRunner from "$parts/JobRunner.svelte";
  import LibraryPicker from "$parts/LibraryPicker.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { manifest, type Image, type Manifest } from "$lib/firmware";
  import type { FirmwarePick, FirmwareUpload } from "$api/types";

  const load = new Action<Manifest>("firmware manifest");
  void load.run(() => manifest());
  const images = $derived(load.result?.images ?? []);
  const size = $derived(load.result?.size ?? 0);

  let q = $state("");
  let kind = $state("");
  let chip = $state("");
  const kinds = $derived([...new Set(images.map((i) => i.kind))].sort());
  const chips = $derived([...new Set(images.flatMap((i) => i.chips))].sort());
  const rows = $derived.by(() => {
    const t = q.trim().toLowerCase();
    return images.filter(
      (i) =>
        (!t || [i.name, i.version, i.kind, i.pcb ?? "", ...i.chips].join(" ").toLowerCase().includes(t)) &&
        (!kind || i.kind === kind) &&
        (!chip || i.chips.includes(chip)),
    );
  });

  // The chosen image: a manifest name, or the path the upload was written to.
  let chosen = $state("");
  let upload = $state<Picked | null>(null);
  const uploaded = new Action<FirmwareUpload>("firmware upload");
  const put = (p: Picked) =>
    uploaded.run(async () => {
      const r = await ops.card!.firmwareUpload(p.file);
      chosen = r.path;
      return r;
    });

  const entry = $derived(images.find((i) => i.name === chosen) ?? null);
  const sha = $derived(entry ? entry.sha256 : (uploaded.result?.sha256 ?? ""));
  const matches = $derived(entry ? "in config/firmware.toml" : uploaded.result ? (uploaded.result.verified ? "matches config/firmware.toml" : "not in config/firmware.toml, used as it is") : "");

  // The ranking for a spec, from the same API `provision --firmware auto` uses.
  let specPath = $state("");
  const suggest = new Action<FirmwarePick>("firmware pick");
  const rank = (toml: string) => suggest.run(() => ops.card!.firmwarePick(toml));
  const why = (name: string) => suggest.result?.candidates.find((c) => c.name === name);
  const link = (i: Image) => i.location.href;
</script>

<ControlHead title="Firmware">
  <section>
    <h2>Suggest for a spec</h2>
    <LibraryPicker bind:value={specPath} onpick={(s) => void rank(s.toml)} />
    {#if suggest.error}<p class="error">{suggest.error}</p>{/if}
    {#if suggest.result}
      {@const r = suggest.result}
      <p class={r.chosen ? "ok" : "warn"}>
        {r.card}, chip {r.chip}: {r.chosen ? `${r.chosen} — ${why(r.chosen)?.reasons.join("; ") ?? ""}` : r.refused}
      </p>
      {#if r.chosen}<button onclick={() => (chosen = r.chosen!)}>use {r.chosen}</button>{/if}
    {/if}
  </section>

  <section>
    <h2>Manifest</h2>
    <div class="row mb-3">
      <input type="search" placeholder="filter" bind:value={q} aria-label="filter" />
      <select bind:value={kind} aria-label="kind"><option value="">any kind</option>{#each kinds as k (k)}<option value={k}>{k}</option>{/each}</select>
      <select bind:value={chip} aria-label="chip"><option value="">any chip</option>{#each chips as x (x)}<option value={x}>{x}</option>{/each}</select>
      <span class="caption">{rows.length} of {images.length}</span>
    </div>
    {#if load.error}<p class="error">{load.error}</p>{/if}
    <div class="scroll">
      <table>
        <thead><tr><th>name</th><th>version</th><th>pcb</th><th>kind</th><th>chips</th><th class="num">size</th><th>score</th><th>sha256</th><th>file</th></tr></thead>
        <tbody>
          {#each rows as i (i.name)}
            <tr class={["selectable", { selected: i.name === chosen }]} tabindex="0" onclick={() => (chosen = i.name)} onkeydown={(k) => k.key === "Enter" && (chosen = i.name)}>
              <td class="mono">{i.name}{#if suggest.result?.chosen === i.name}<div class="caption">recommended: {why(i.name)?.reasons.join("; ")}</div>{/if}</td>
              <td class="mono">{i.version}</td>
              <td class="mono">{i.pcb ?? ""}</td>
              <td>{i.kind}</td>
              <td>{i.chips.join(", ")}</td>
              <td class="num">{size}</td>
              <td class="num mono">{why(i.name)?.score ?? ""}</td>
              <td class="mono sha">{i.sha256}</td>
              <td><a href={link(i)}>{i.location.remote ? "download" : "repository"}</a></td>
            </tr>
          {:else}
            <tr><td colspan="9" class="muted">{load.busy ? "loading the manifest" : "no image matches the filter"}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <section>
    <h2>Or an image from this machine</h2>
    <FileDrop label="firmware .hex" accept=".hex" bind:picked={upload} onpick={(p) => void put(p)} />
    {#if uploaded.error}<p class="error">{uploaded.error}</p>{/if}
    {#if uploaded.result}<p class={uploaded.result.verified ? "ok" : "warn"}>{uploaded.result.path}: {matches}</p>{/if}
  </section>

  <section>
    <h2>Install</h2>
    <div class="form">
      <Field label="image" caption="a firmware.toml name or a path on the daemon's machine" wide><input bind:value={chosen} class="mono" /></Field>
    </div>
    {#if sha}<p class="caption mono">sha256 {sha} — {matches}</p>{/if}
    <JobRunner
      label="install"
      disabled={!chosen}
      reason={chosen ? "" : "no image chosen"}
      confirm="This programs the firmware bank of card {app.card}; power-cycle it afterwards."
      run={(commit) => ops.card!.firmwareInstall({ path: chosen, commit })}
    />
  </section>
</ControlHead>

<style>
  .sha {
    font-size: 11px;
  }
  td .caption {
    line-height: 1.2;
    white-space: normal;
  }
</style>
