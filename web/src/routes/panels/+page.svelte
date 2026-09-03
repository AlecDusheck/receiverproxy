<script lang="ts">
  // The panel table, prerendered from config/panels at build time; the
  // filters and the sort run in the browser.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import Module from "$parts/Module.svelte";
  import { chipLabel, formatLabel, panelTitle } from "$lib/panel";

  let { data } = $props();
  type Row = (typeof data.entries)[number];
  const entries = $derived(data.entries);

  // Filters
  let q = $state("");
  let vendor = $state("");
  let chip = $state("");
  let scan = $state("");
  let status = $state("");
  const vendors = $derived([...new Set(entries.flatMap((e) => e.meta.vendors))].sort());
  const chips = $derived([...new Set(entries.map((e) => e.chip.name))].sort());
  const scans = $derived([...new Set(entries.map((e) => e.module.scan))].sort((a, b) => a - b));

  // Sort by clicking a header; the same header again reverses.
  type Col = "title" | "pitch" | "module" | "scan" | "chip" | "status" | "formats" | "cards";
  let sortBy = $state<Col>("status");
  let asc = $state(true);
  const key = (e: Row, c: Col): number | string => {
    switch (c) {
      case "title":
        return panelTitle(e);
      case "pitch":
        return e.meta.pitch_mm ?? Number.POSITIVE_INFINITY;
      case "module":
        return e.module.width * 100000 + e.module.height;
      case "scan":
        return e.module.scan;
      case "chip":
        return e.chip.name;
      case "status":
        return e.meta.status;
      case "formats":
        return e.formats.join(",");
      case "cards":
        return e.cards.join(",") || "~";
    }
  };
  const rows = $derived.by(() => {
    const t = q.trim().toLowerCase();
    const list = entries.filter(
      (e) =>
        (!t || [panelTitle(e), e.name, e.path, e.chip.name, e.meta.maker ?? "", e.meta.product ?? "", ...e.meta.vendors, ...e.cards].join(" ").toLowerCase().includes(t)) &&
        (!vendor || e.meta.vendors.includes(vendor)) &&
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
  const href = (e: Row) => `/panels/${encodeURIComponent(e.name)}`;
  const cols: [Col, string, boolean][] = [["title", "panel", false], ["pitch", "pitch", true], ["module", "module", false], ["scan", "scan", true], ["chip", "chip", false], ["status", "status", false], ["formats", "formats", false], ["cards", "tested with", false]];
  // The description: the formats, the chips and the pitches the table covers.
  const formats = $derived([...new Set(data.formats.map(formatLabel))].join(", "));
  const chipNames = $derived([...new Set(entries.map((e) => chipLabel(e.chip.name)))].sort().join(", "));
  const pitches = $derived([...new Set(entries.map((e) => e.meta.pitch_mm).filter((p): p is number => p !== undefined))].sort((a, b) => a - b).map((p) => `P${p}`).join(", "));
  const description = $derived(`${entries.length} receiving card config files (${formats}) by module, scan and chip: ${chipNames}${pitches ? `; ${pitches}` : ""}. Download or customize.`);
</script>

<Head title="Panel configs ({formats})" {description} path="/panels" />

<TitleRow title="Panels" />

<div class="row mb-3">
  <input type="search" placeholder="filter" bind:value={q} aria-label="filter" class="w-60" />
  <select bind:value={vendor} aria-label="vendor">
    <option value="">any vendor</option>
    {#each vendors as v (v)}<option value={v}>{v}</option>{/each}
  </select>
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
    <option value="verified">verified</option>
    <option value="derived">derived</option>
    <option value="stub">stub</option>
  </select>
  <span class="caption">{rows.length} of {entries.length}</span>
</div>

<div class="scroll">
  <table>
    <thead>
      <tr>
        <th></th>
        {#each cols as [c, label, num] (c)}
          <th class={["sort", { num }]} tabindex="0" onclick={() => sort(c)} onkeydown={(k) => k.key === "Enter" && sort(c)} aria-sort={sortBy === c ? (asc ? "ascending" : "descending") : undefined}>{label}{sortBy === c ? (asc ? " +" : " -") : ""}</th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as e (e.path)}
        <tr>
          <td class="pic">
            <a href={href(e)} aria-label={panelTitle(e)}>
              {#if e.meta.image}<img src={e.meta.image} alt={panelTitle(e)} />{:else}<Module width={e.module.width} height={e.module.height} scan={e.module.scan} size={48} caption={false} />{/if}
            </a>
          </td>
          <td>
            <a href={href(e)}>{panelTitle(e)}</a>
            {#if e.meta.maker || e.meta.product}<div class="caption">{[e.meta.maker, e.meta.product].filter(Boolean).join(" ")}</div>{/if}
          </td>
          <td class="num">{e.meta.pitch_mm ?? ""}</td>
          <td class="mono">{e.module.width}x{e.module.height}</td>
          <td class="num">1/{e.module.scan}</td>
          <td>{e.chip.name}</td>
          <td>{e.meta.status}</td>
          <td>{e.formats.join(", ")}</td>
          <td>{#each e.cards as c, i (c)}{i ? ", " : ""}<a href="/cards/{encodeURIComponent(c.toLowerCase())}">{c}</a>{/each}</td>
        </tr>
      {:else}
        <tr><td colspan="9" class="muted">{entries.length ? "no panel matches the filter" : "no panels"}</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .pic {
    padding: var(--s1) var(--s2);
  }
  .pic a {
    display: block;
    line-height: 0;
  }
  img {
    height: 48px;
    width: auto;
    border: 1px solid var(--line);
  }
  td .caption {
    line-height: 1.2;
    padding-bottom: var(--s1);
  }
</style>
