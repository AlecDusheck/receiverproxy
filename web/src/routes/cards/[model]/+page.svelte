<script lang="ts">
  // One card model: the photo and identity, limits, memory map, the panels
  // driven with it, and the firmware manifest.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import { repoFile } from "$lib/site";
  import KeyValue, { type Row } from "$parts/KeyValue.svelte";
  import { panelTitle } from "$lib/panel";

  let { data } = $props();
  const c = $derived(data.card);
  const href = $derived(`/cards/${encodeURIComponent(c.name.toLowerCase())}`);
  const hex = (n: number, w = 2) => "0x" + n.toString(16).padStart(w, "0");
  const blocks = (bytes: number) => Math.ceil(bytes / c.memory.block_bytes);

  const identity = $derived.by((): Row[] => {
    const rows: Row[] = [
      ["name", c.name],
      ["vendor", c.vendor],
      ["family", c.family],
      // No id byte means discovery cannot pick the model; only --card can.
      ["id", c.id === undefined ? "none stated" : hex(c.id)],
      ["status", c.status],
      ["file", { text: c.path, href: repoFile(c.path) }],
      ["image pattern", c.firmware.image_pattern],
      ["sdram staging", String(c.firmware.sdram_staging)],
    ];
    if (c.datasheet) rows.push(["specification", { href: c.datasheet, text: "specification" }]);
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
  const description = $derived(`${c.vendor} ${c.name} receiving card: limits, memory map, firmware images, tested panels.`);
</script>

<Head title="{c.vendor} {c.name}" {description} path={href} />

<TitleRow title="{c.vendor} {c.name}">
  {#snippet action()}
    <a href="/cards">Cards</a>
  {/snippet}
</TitleRow>
<SubNav links={[["#identity", "Photo and identity"], ["#limits", "Limits"], ["#memory", "Memory map"], ["#tested", "Tested panels"], ["#firmware", "Firmware"]]} />

<section id="identity">
  {#if c.image}
    <figure>
      <img src="/{c.image}" alt="{c.vendor} {c.name}" />
      {#if c.image_source}<figcaption class="caption">{c.image_source}</figcaption>{/if}
    </figure>
  {/if}
  <KeyValue title="identity" rows={identity} />
</section>

<section id="limits">
  <KeyValue title="limits" rows={limits} />
</section>

<section id="memory">
  <div class="blocks">
    <KeyValue title="memory map" rows={memory} />
    <KeyValue title="boot image offsets" rows={bootImage} />
  </div>
</section>

<section id="tested">
  <h2>Tested panels</h2>
  {#if data.tested.length}
    <div class="scroll">
      <table>
        <thead><tr><th>panel</th><th>spec</th><th>firmware</th><th>version</th></tr></thead>
        <tbody>
          {#each data.tested as t (t.panel)}
            <tr>
              <td>{#if t.entry}<a href="/panels/{encodeURIComponent(t.entry.name)}">{panelTitle(t.entry)}</a>{:else}{t.panel}{/if}</td>
              <td class="mono"><a href={repoFile(t.panel)}>{t.panel}</a></td>
              <td class="mono">{t.firmware}</td>
              <td class="mono">{t.version ?? ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <p class="muted">none</p>
  {/if}
</section>

<section id="firmware">
  <h2>Firmware</h2>
  <p>
    <a href="{href}/firmware">{data.images} images</a> with their versions, driver chips and checksums.
  </p>
  {#if data.tested.length}
    <ul>
      {#each data.tested as t (t.panel)}
        <li class="mono">{t.firmware}{t.version ? ` (${t.version})` : ""}<span class="caption"> driven with {t.entry ? panelTitle(t.entry) : t.panel}</span></li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  figure {
    margin: 0 0 var(--s4);
  }
  img {
    display: block;
    width: 100%;
    max-width: 480px;
    height: auto;
    border: 1px solid var(--line);
  }
</style>
