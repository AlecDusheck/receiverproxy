<script lang="ts" module>
  /** One spec of the embedded library: what the picker hands back. */
  export type PickedSpec = { path: string; name: string; toml: string };
</script>

<script lang="ts">
  // Pick a panel spec from the library the WASM module embeds (the same
  // files config/panels holds and the Panels pages list). A filter and a
  // select; the chosen path is shown under it. The module loads on first
  // use, so a screen that never opens the picker never fetches it.
  import Field from "./Field.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { Libraries } from "$api/types";

  let { value = $bindable(""), onpick }: { value?: string; onpick: (s: PickedSpec) => void } = $props();

  let filter = $state("");
  const load = new Action<Libraries>("panel library");
  void load.run(() => ops.pure.libraries());

  const all = $derived(load.result?.panels ?? []);
  const rows = $derived.by(() => {
    const t = filter.trim().toLowerCase();
    return t ? all.filter((p) => p.path.toLowerCase().includes(t) || p.name.toLowerCase().includes(t)) : all;
  });
  const chosen = $derived(all.find((p) => p.path === value) ?? null);

  function pick(path: string) {
    value = path;
    const p = all.find((q) => q.path === path);
    if (p) onpick({ path: p.path, name: p.name, toml: p.toml });
  }
</script>

<div class="form">
  <Field label="filter" caption="{rows.length} of {all.length} specs">
    <input type="search" bind:value={filter} aria-label="filter specs" />
  </Field>
  <Field label="panel spec" caption={chosen ? chosen.path : "none chosen"} mono wide>
    <select value={value} onchange={(e) => pick(e.currentTarget.value)} aria-label="panel spec" disabled={!all.length}>
      <option value="">choose a spec</option>
      {#each rows as p (p.path)}<option value={p.path}>{p.name}</option>{/each}
    </select>
  </Field>
</div>
{#if load.busy}<p class="muted">loading the panel library</p>{/if}
{#if load.error}<p class="error">{load.error}</p>{/if}
{#if !load.busy && !load.error && !all.length}<p class="muted">the WASM module embeds no panel spec</p>{/if}
