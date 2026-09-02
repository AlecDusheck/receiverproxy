<script lang="ts">
  // The firmware manifest for one card model, prerendered from
  // config/firmware.toml; the filters run in the browser.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import { REPO } from "$lib/site";

  let { data } = $props();
  type Row = (typeof data.images)[number];
  // The asset host when the manifest names one, else the file in the repository.
  const link = (i: Row) => (i.location.remote ? i.location.href : `${REPO}/blob/main/${i.location.href}`);
  const c = $derived(data.card);
  const images = $derived(data.images);
  const card = $derived(`/cards/${encodeURIComponent(c.name.toLowerCase())}`);

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
  const description = $derived(`${images.length} firmware images for the ${c.vendor} ${c.name} receiving card: version, board revision, build kind, driver chips and sha256.`);
</script>

<Head title="{c.vendor} {c.name} firmware" {description} path="{card}/firmware" />

<TitleRow title="{c.vendor} {c.name} firmware">
  {#snippet action()}
    <a href="/cards">Cards</a>
  {/snippet}
</TitleRow>
<SubNav links={[[card, "Card"], [`${card}/firmware`, "Firmware"]]} />

<p class="caption">
  Every firmware image Colorlight ships for this card. Each one drives a
  particular set of driver chips: pick the image that names the chip on your
  module.
</p>

<div class="row mb-3">
  <input type="search" placeholder="filter" bind:value={q} aria-label="filter" class="w-60" />
  <select bind:value={kind} aria-label="kind">
    <option value="">any kind</option>
    {#each kinds as k (k)}<option value={k}>{k}</option>{/each}
  </select>
  <select bind:value={chip} aria-label="chip">
    <option value="">any chip</option>
    {#each chips as x (x)}<option value={x}>{x}</option>{/each}
  </select>
  <span class="caption">{rows.length} of {images.length}</span>
</div>

<div class="scroll">
  <table>
    <thead>
      <tr><th>name</th><th>version</th><th>pcb</th><th>kind</th><th>chips</th><th class="num">size</th><th>sha256</th><th>download</th></tr>
    </thead>
    <tbody>
      {#each rows as i (i.name)}
        <tr>
          <td class="mono">
            {i.name}
            {#if data.tested.includes(i.name)}<div class="caption">driven on the {c.name}</div>{/if}
          </td>
          <td class="mono">{i.version}</td>
          <td class="mono">{i.pcb ?? ""}</td>
          <td>{i.kind}</td>
          <td>{i.chips.join(", ")}</td>
          <td class="num">{data.size}</td>
          <td class="mono sha">{i.sha256}</td>
          <td><a href={link(i)}>{i.location.remote ? "download" : "repository"}</a></td>
        </tr>
      {:else}
        <tr><td colspan="8" class="muted">no image matches the filter</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .sha {
    font-size: 11px;
  }
  td .caption {
    line-height: 1.2;
  }
</style>
