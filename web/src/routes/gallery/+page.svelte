<script lang="ts">
  // The gallery table, prerendered from config/panels at build time; the
  // filters and the sort run in the browser. A vendor file dropped here is
  // read back into a spec through the WASM module.
  import { goto } from "$app/navigation";
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import Drop from "$parts/Drop.svelte";
  import { ops, type Entry, type Imported } from "$api/ops";
  import { app, handSpec } from "$lib/state.svelte";
  import { Action } from "$lib/action.svelte";

  let { data } = $props();
  const entries = $derived(data.entries);

  // Filters
  let q = $state("");
  let chip = $state("");
  let scan = $state("");
  let status = $state("");
  const chips = $derived([...new Set(entries.map((e) => e.chip.name))].sort());
  const scans = $derived([...new Set(entries.map((e) => e.module.scan))].sort((a, b) => a - b));

  // Sort by clicking a header; the same header again reverses.
  type Col = "pitch" | "module" | "scan" | "chip" | "formats" | "sources" | "status";
  let sortBy = $state<Col>("pitch");
  let asc = $state(true);
  const key = (e: Entry, c: Col): number | string => {
    switch (c) {
      case "pitch":
        return e.meta.pitch_mm ?? Number.POSITIVE_INFINITY;
      case "module":
        return e.module.width * 100000 + e.module.height;
      case "scan":
        return e.module.scan;
      case "chip":
        return e.chip.name;
      case "formats":
        return e.formats.join(",");
      case "sources":
        return e.meta.sources;
      case "status":
        return e.meta.status;
    }
  };
  const rows = $derived.by(() => {
    const t = q.trim().toLowerCase();
    const list = entries.filter(
      (e) =>
        (!t || e.name.toLowerCase().includes(t) || e.path.toLowerCase().includes(t) || e.chip.name.toLowerCase().includes(t) || e.meta.vendors.join(" ").toLowerCase().includes(t)) &&
        (!chip || e.chip.name === chip) &&
        (!scan || String(e.module.scan) === scan) &&
        (!status || e.meta.status === status),
    );
    return list.sort((a, b) => {
      const x = key(a, sortBy), y = key(b, sortBy);
      const c = x < y ? -1 : x > y ? 1 : a.name.localeCompare(b.name);
      return asc ? c : -c;
    });
  });
  function sort(c: Col) {
    if (sortBy === c) asc = !asc;
    else {
      sortBy = c;
      asc = true;
    }
  }
  const href = (e: Entry) => `/gallery/${encodeURIComponent(e.name)}`;

  // Import: a vendor file becomes a spec; the format is detected from the bytes.
  const imp = new Action<Imported & { file: string }>("import");
  function importFile(files: File[]) {
    const f = files[0]!;
    void imp.run(async () => ({ ...(await ops.pure.importSpec(new Uint8Array(await f.arrayBuffer()))), file: f.name }));
  }
  function toBuilder(text: string) {
    handSpec(text);
    void goto("/builder");
  }
  const cols: [Col, string, boolean][] = [["pitch", "pitch", true], ["module", "module", false], ["scan", "scan", true], ["chip", "chip", false], ["formats", "formats", false], ["sources", "sources", true], ["status", "status", false]];
</script>

<Head title="Gallery" description="Every panel spec under config/panels: pitch, module, scan, driver chip, the formats it generates, how many vendor files it came from, and whether it was driven on a bench." path="/gallery" />

<TitleRow title="Gallery" />

{#if app.wasm === "failed"}<p class="error">{app.wasmError}</p>{/if}

<Drop label="Import a vendor file" disabled={app.wasm === "failed"} onfiles={importFile} />
{#if imp.error}<p class="error">{imp.error}</p>{/if}
{#if imp.result}
  <section>
    <h2>{imp.result.file}: {imp.result.format}</h2>
    {#if imp.result.unresolved.length}
      <p class="warn">unresolved: {imp.result.unresolved.join(", ")}</p>
    {:else}
      <p class="ok">every field resolved</p>
    {/if}
    <pre>{imp.result.spec_toml}</pre>
    <div class="actions">
      <button class="primary" onclick={() => toBuilder(imp.result!.spec_toml)}>open in Builder</button>
    </div>
  </section>
{/if}

<div class="row mb-3">
  <input type="search" placeholder="filter name, path, chip, vendor" bind:value={q} aria-label="filter" class="w-70" />
  <select bind:value={chip} aria-label="chip">
    <option value="">any chip</option>
    {#each chips as c (c)}<option value={c}>{c}</option>{/each}
  </select>
  <select bind:value={scan} aria-label="scan">
    <option value="">any scan</option>
    {#each scans as s (s)}<option value={String(s)}>1/{s}</option>{/each}
  </select>
  <select bind:value={status} aria-label="status">
    <option value="">any status</option>
    <option value="tested">tested</option>
    <option value="generates">generates</option>
  </select>
  <span class="caption">{rows.length} of {entries.length}</span>
</div>

<div class="scroll">
  <table>
    <thead>
      <tr>
        <th>name</th>
        {#each cols as [c, label, num] (c)}
          <th class={["sort", { num }]} tabindex="0" onclick={() => sort(c)} onkeydown={(k) => k.key === "Enter" && sort(c)} aria-sort={sortBy === c ? (asc ? "ascending" : "descending") : undefined}>{label}{sortBy === c ? (asc ? " +" : " -") : ""}</th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as e (e.path)}
        <tr>
          <td class="mono"><a href={href(e)}>{e.name}</a></td>
          <td class="num">{e.meta.pitch_mm ?? ""}</td>
          <td class="mono">{e.module.width}x{e.module.height}</td>
          <td class="num">1/{e.module.scan}</td>
          <td>{e.chip.name}</td>
          <td>{e.formats.join(", ")}</td>
          <td class="num">{e.meta.sources}</td>
          <td>{e.meta.status}</td>
        </tr>
      {:else}
        <tr><td colspan="8" class="muted">{entries.length ? "no entry matches the filter" : "no entries"}</td></tr>
      {/each}
    </tbody>
  </table>
</div>
