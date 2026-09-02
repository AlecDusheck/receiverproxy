<script lang="ts">
  // Inspect one .rcvbp; diff two. Results appear where the action was.
  import Drop from "$parts/Drop.svelte";
  import Hex from "$parts/Hex.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { Diff, Inspection } from "$api/types";

  const bytes = (f: File) => f.arrayBuffer().then((b) => new Uint8Array(b));
  const hex = (n: number, w = 2) => "0x" + n.toString(16).padStart(w, "0");
  const wasmOff = $derived(app.wasm !== "ready");

  const insp = new Action<Inspection & { file: string }>("inspect");
  const inspect = (files: File[]) => insp.run(async () => ({ ...(await ops.pure.inspect(await bytes(files[0]!))), file: files[0]!.name }));

  let fa = $state<File | null>(null);
  let fb = $state<File | null>(null);
  const dif = new Action<Diff>("diff");
  const diff = () => fa && fb && dif.run(async () => ops.pure.diff(await bytes(fa!), await bytes(fb!)));
</script>

<section>
  <h2>Inspect</h2>
  <Drop label=".rcvbp file" accept=".rcvbp" disabled={wasmOff} onfiles={inspect} />
  {#if insp.error}<p class="error">{insp.error}</p>{/if}
  {#if insp.result}
    {@const r = insp.result}
    <p>{r.file}: version {r.version}{r.cabinet ? `, cabinet ${r.cabinet[0]}x${r.cabinet[1]}` : ""}, {r.records.length} records</p>
    <div class="scroll">
      <table>
        <thead><tr><th>offset</th><th>type</th><th class="num">length</th><th class="num">non-zero</th><th>description</th></tr></thead>
        <tbody>
          {#each r.records as rec (rec.offset)}
            <tr class={{ muted: rec.empty }}>
              <td class="mono">{hex(rec.offset, 4)}</td>
              <td class="mono">{rec.type}</td>
              <td class="num">{rec.length}</td>
              <td class="num">{rec.nonzero}</td>
              <td>{rec.description}{rec.empty ? " (empty)" : ""}</td>
            </tr>
            {#if rec.fields}
              <tr>
                <td></td>
                <td colspan="4" class="fields">
                  <div class="row">
                    {#each Object.entries(rec.fields) as [k, v] (k)}
                      {#if !(v instanceof Uint8Array)}
                        <span><span class="muted">{k}</span> <span class="mono">{Array.isArray(v) ? v.join("x") : v}</span></span>
                      {/if}
                    {/each}
                  </div>
                  <div class="caption">swap_ramp (+0x19A)</div>
                  <Hex bytes={rec.fields.swap_ramp} offset={0x19a} />
                  <div class="caption">chip_custom (+0x06A)</div>
                  <Hex bytes={rec.fields.chip_custom} offset={0x6a} />
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<section>
  <h2>Diff</h2>
  <div class="row">
    <label>a <input type="file" accept=".rcvbp" onchange={(e) => (fa = e.currentTarget.files?.[0] ?? null)} /></label>
    <label>b <input type="file" accept=".rcvbp" onchange={(e) => (fb = e.currentTarget.files?.[0] ?? null)} /></label>
    <button onclick={diff} disabled={!fa || !fb || wasmOff || dif.busy}>Diff</button>
  </div>
  {#if dif.error}<p class="error">{dif.error}</p>{/if}
  {#if dif.result}
    {@const d = dif.result}
    <p>{d.a_records} records in a, {d.b_records} in b. {d.only_a.length ? `Only in a: ${d.only_a.join(", ")}. ` : ""}{d.only_b.length ? `Only in b: ${d.only_b.join(", ")}.` : ""}</p>
    {#if d.records.length === 0}
      <p class="ok">no differing records</p>
    {:else}
      <div class="scroll">
      <table>
        <thead><tr><th>record</th><th class="num">len a</th><th class="num">len b</th><th>differing offsets</th></tr></thead>
        <tbody>
          {#each d.records as r (r.type)}
            <tr>
              <td class="mono">{r.type}</td>
              <td class="num">{r.len_a}</td>
              <td class="num">{r.len_b}</td>
              <td class="mono offsets">{r.offsets.length}: {r.offsets.map((o) => hex(o, 3)).join(" ")}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      </div>
    {/if}
  {/if}
</section>

<style>
  .fields {
    height: auto;
    padding: var(--s2);
    white-space: normal;
  }
  .fields .row {
    gap: var(--s1) var(--s4);
    margin-bottom: var(--s2);
  }
  .offsets {
    white-space: normal;
  }
  label {
    display: flex;
    gap: var(--s1);
    align-items: center;
  }
</style>
