<script lang="ts">
  // One card model: identity, limits, memory map, status, the panels driven
  // with it, and the firmware manifest.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import KeyValue from "$parts/KeyValue.svelte";
  import { REPO } from "$lib/site";

  let { data } = $props();
  const c = $derived(data.card);
  const hex = (n: number, w = 2) => "0x" + n.toString(16).padStart(w, "0");
  const blocks = (bytes: number) => Math.ceil(bytes / c.memory.block_bytes);

  const identity = $derived.by((): [string, string][] => {
    const rows: [string, string][] = [
      ["name", c.name],
      ["vendor", c.vendor],
      ["family", c.family],
      ["id", `${hex(c.id)} (first byte of the discovery reply)`],
      ["file", c.path],
      ["image pattern", c.firmware.image_pattern],
      ["sdram staging", String(c.firmware.sdram_staging)],
    ];
    if (c.notes) rows.push(["notes", c.notes]);
    return rows;
  });
  const limits = $derived.by((): [string, string][] => {
    const rows: [string, string][] = [
      ["max width", `${c.limits.max_width} px`],
      ["max height", `${c.limits.max_height} px`],
      ["hub ports", String(c.limits.hub_ports)],
    ];
    if (c.limits.chain !== undefined) rows.push(["chain", String(c.limits.chain)]);
    return rows;
  });
  const memory = $derived.by((): [string, string][] => {
    const m = c.memory;
    const rows: [string, string][] = [
      ["block bytes", hex(m.block_bytes, 5)],
      ["primary bank", `${hex(m.primary_bank, 6)}, ${hex(m.bank_bytes, 6)} bytes, ${blocks(m.bank_bytes)} blocks`],
      ["golden bank", hex(m.golden_bank, 6)],
      ["parameter block", `${hex(m.parameter_block)} (block ${m.parameter_block})`],
      ["eeprom mirror", hex(m.eeprom_mirror, 6)],
    ];
    for (const g of m.guarded) rows.push([`guarded from ${g.from}${g.to ? ` to ${g.to}` : ""}`, g.blocks.map((b) => hex(b)).join(", ")]);
    return rows;
  });
  const bootImage = $derived(c.memory.boot_image.map(([k, v]): [string, string] => [k, k === "map_entries" ? String(v) : hex(v, 4)]));
  const status = $derived.by((): [string, string][] => [
    ["status", c.status],
    ["panels tested", String(c.tested.length)],
  ]);
  const description = $derived(`${c.vendor} ${c.name} receiving card, id ${hex(c.id)}, ${c.limits.max_width}x${c.limits.max_height} control area, ${c.limits.hub_ports} HUB75 ports; ${c.status}, ${c.tested.length} panel${c.tested.length === 1 ? "" : "s"} tested.`);
</script>

<Head title="{c.vendor} {c.name}" {description} path="/cards/{encodeURIComponent(c.name.toLowerCase())}" />

<TitleRow title="{c.vendor} {c.name}">
  {#snippet action()}
    <a href="/cards">Cards</a>
  {/snippet}
</TitleRow>

<section>
  <div class="blocks">
    <KeyValue title="identity" rows={identity} />
    <KeyValue title="limits" rows={limits} />
    <KeyValue title="status" rows={status} />
    <KeyValue title="memory map" rows={memory} />
    <KeyValue title="boot image offsets" rows={bootImage} />
  </div>
</section>

<section>
  <h2>Panels tested</h2>
  {#if data.tested.length}
    <table>
      <thead><tr><th>panel</th><th>spec</th><th>firmware</th><th>version</th></tr></thead>
      <tbody>
        {#each data.tested as t (t.panel)}
          <tr>
            <td>{#if t.name}<a href="/gallery/{encodeURIComponent(t.name)}">{t.name}</a>{:else}{t.panel}{/if}</td>
            <td class="mono">{t.panel}</td>
            <td class="mono">{t.firmware}</td>
            <td class="mono">{t.version ?? ""}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p>No panel has been driven with this card.</p>
  {/if}
</section>

<section>
  <h2>Firmware images</h2>
  <p class="caption">config/firmware.toml; <code>rxp firmware install NAME</code> checks the sha256 before any write. {data.base_url ? `Downloads come from ${data.base_url}.` : "base_url is empty: each image is expected at its path in the repository or in the firmware cache."}</p>
  <div class="scroll">
    <table>
      <thead><tr><th>name</th><th>version</th><th>kind</th><th>pcb</th><th>chips</th><th class="num">size</th><th>sha256</th><th>download</th></tr></thead>
      <tbody>
        {#each data.images as i (i.name)}
          <tr>
            <td class="mono">{i.name}</td>
            <td class="mono">{i.version}</td>
            <td>{i.kind}</td>
            <td>{i.pcb ?? ""}</td>
            <td>{i.chips.join(", ")}</td>
            <td class="num">{i.size}</td>
            <td class="mono sha">{i.sha256}</td>
            <td class="mono">{#if i.location.remote}<a href={i.location.href}>{i.location.href}</a>{:else}<a href="{REPO}/blob/main/{i.location.href}">{i.location.href}</a>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .sha {
    font-size: 11px;
  }
</style>
