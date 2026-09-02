<script lang="ts">
  // One spec: the photo and the maker's links, the downloads, then the
  // module, wiring and timing tables as key-value blocks, then the TOML. The
  // page is static; the files are generated in the browser on a download.
  import { goto } from "$app/navigation";
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import KeyValue, { type Row } from "$parts/KeyValue.svelte";
  import Module from "$parts/Module.svelte";
  import Generate from "$parts/Generate.svelte";
  import { ops } from "$api/ops";
  import { app, handSpec } from "$lib/state.svelte";
  import { repoFile } from "$lib/site";
  import { chipLabel, formatLabel, panelTitle } from "$lib/panel";
  import { parseToml, type Tables } from "$lib/spec";
  import { save } from "$lib/download";

  let { data } = $props();
  const entry = $derived(data.entry);
  const toml = $derived(data.entry.toml);
  const title = $derived(panelTitle(entry));
  const formats = $derived(data.formats.filter((f) => f.generate));
  const formatNames = $derived(formats.map((f) => f.name).join(", "));
  const formatLabels = $derived(formats.map(formatLabel).join(", "));

  const HEX = new Set(["gclock", "family_id", "sub_id"]);
  const show = (v: unknown, k = ""): string =>
    Array.isArray(v) ? `[${v.map((x) => show(x)).join(", ")}]` : HEX.has(k) && typeof v === "number" ? "0x" + v.toString(16).padStart(2, "0") : String(v);
  const tables = $derived.by((): Tables => {
    try {
      return parseToml(toml);
    } catch (e) {
      return { spec: { "parse error": e instanceof Error ? e.message : String(e) } };
    }
  });
  // The spec's tables as blocks, grouped into the page's sections; a table not named below joins the last group.
  const GROUPS: [string, string[]][] = [
    ["module", ["", "module", "screen", "chip"]],
    ["wiring", ["color", "mapping", "boot", "record01_overrides"]],
    ["timing", ["timing", "current"]],
  ];
  const named = new Set(GROUPS.flatMap(([, t]) => t));
  const blocks = (names: string[], rest = false): [string, Row[]][] =>
    Object.entries(tables)
      .filter(([name, t]) => name !== "meta" && Object.keys(t).length && (rest ? !named.has(name) : names.includes(name)))
      .map(([name, t]) => [name || "spec", Object.entries(t).map(([k, v]) => [k, show(v, k)])]);
  // The maker's block: who makes it, the product page, the specification sheet.
  const maker = $derived.by((): Row[] => {
    const m = entry.meta;
    const rows: Row[] = [];
    if (m.maker) rows.push(["maker", m.maker]);
    if (m.product) rows.push(["product", m.url ? { href: m.url, text: m.product } : m.product]);
    else if (m.url) rows.push(["product", { href: m.url, text: "product page" }]);
    if (m.datasheet) rows.push(["specification", { href: m.datasheet, text: "specification" }]);
    return rows;
  });
  const chipRows = $derived.by((): Row[] => {
    const rows: Row[] = [["name", entry.chip.name], ["library", { text: entry.chip.library, href: repoFile(entry.chip.library) }]];
    if (entry.chip.vendor) rows.push(["vendor", entry.chip.vendor]);
    if (entry.chip.datasheet) rows.push(["datasheet", { href: entry.chip.datasheet, text: "datasheet" }]);
    return rows;
  });
  const meta = $derived.by((): Row[] => {
    const m = entry.meta;
    const rows: Row[] = [
      ["file", { text: entry.path, href: repoFile(entry.path) }],
      ["status", m.status],
      ["origin", m.origin],
      ["vendor files", String(m.sources)],
    ];
    if (m.pitch_mm !== undefined) rows.splice(1, 0, ["pitch", `${m.pitch_mm} mm`]);
    if (m.agreement !== undefined) rows.push(["agreement", String(m.agreement)]);
    if (m.vendors.length) rows.push(["vendors", m.vendors.join(", ")]);
    if (m.examples.length) rows.push(["examples", m.examples.join("\n")]);
    if (m.notes) rows.push(["notes", m.notes]);
    return rows;
  });
  // One line under the title: module, scan, chip, pitch, formats.
  const summary = $derived([`${entry.module.width}x${entry.module.height}`, `1/${entry.module.scan} scan`, chipLabel(entry.chip.name), entry.meta.pitch_mm !== undefined ? `${entry.meta.pitch_mm} mm pitch` : "", formatNames].filter(Boolean).join(", "));
  // At most 155 characters: the module, the format, the download, and how far the spec is tested.
  const description = $derived.by(() => {
    const module = `${entry.meta.pitch_mm !== undefined ? `P${entry.meta.pitch_mm} ` : ""}${entry.module.width}x${entry.module.height} 1/${entry.module.scan} scan ${chipLabel(entry.chip.name)} module`;
    const tail = entry.meta.status === "tested" ? "Tested on the bench." : `Generated from ${entry.meta.sources} vendor file${entry.meta.sources === 1 ? "" : "s"}.`;
    return `Download the ${formatLabels} receiving card config file for a ${module}, or customize it. ${tail}`;
  });

  function go(path: string) {
    handSpec(toml);
    void goto(path);
  }
</script>

<Head title="{title} {formatLabels} config" {description} path="/panels/{encodeURIComponent(entry.name)}" />

<TitleRow {title}>
  {#snippet action()}
    <a href="/panels">Panels</a>
  {/snippet}
</TitleRow>
<p class="summary">{summary}</p>
<SubNav links={[["#download", "Download"], ["#module", "Module"], ["#wiring", "Wiring"], ["#timing", "Timing"], ["#toml", "TOML"]]} />

{#if entry.meta.image || maker.length}
  <section class="maker">
    {#if entry.meta.image}
      <figure>
        <img src={entry.meta.image} alt={title} />
        {#if entry.meta.image_source}<figcaption class="caption">{entry.meta.image_source}</figcaption>{/if}
      </figure>
    {/if}
    {#if maker.length}<KeyValue rows={maker} />{/if}
  </section>
{/if}

<section id="download">
  <h2>Download</h2>
  <Generate {toml} {formats} />
  <div class="row mt-2">
    <button onclick={() => go("/builder")}>customize</button>
    <button class="mono" onclick={() => save(`${entry.name}.toml`, toml)}>{entry.name}.toml</button>
    {#if ops.card}
      <button onclick={() => go("/control/provision")}>provision</button>
    {/if}
  </div>
  {#if data.tested.length}
    <div class="scroll mt-3">
      <table>
        <thead><tr><th>tested with</th><th>firmware</th></tr></thead>
        <tbody>
          {#each data.tested as t (t.card + t.firmware)}
            <tr><td><a href="/cards/{encodeURIComponent(t.card.toLowerCase())}">{t.card}</a></td><td class="mono">{t.firmware}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<section id="module">
  <h2>Module</h2>
  <Module width={entry.module.width} height={entry.module.height} scan={entry.module.scan} size={192} />
  <div class="blocks mt-4">
    <KeyValue title="entry" rows={meta} />
    <KeyValue title="chip" rows={chipRows} />
    {#each blocks(GROUPS[0]![1]).filter(([name]) => name !== "chip") as [name, rows] (name)}<KeyValue title={name} rows={rows} />{/each}
  </div>
</section>

<section id="wiring">
  <h2>Wiring</h2>
  <div class="blocks">
    {#each [...blocks(GROUPS[1]![1]), ...blocks([], true)] as [name, rows] (name)}<KeyValue title={name} rows={rows} />{/each}
  </div>
</section>

<section id="timing">
  <h2>Timing</h2>
  <div class="blocks">
    {#each blocks(GROUPS[2]![1]) as [name, rows] (name)}<KeyValue title={name} rows={rows} />{/each}
  </div>
</section>

<section id="toml">
  <h2><a href={repoFile(entry.path)}>{entry.path}</a></h2>
  <pre class="toml">{toml}</pre>
</section>

<style>
  .summary {
    margin: calc(-1 * var(--s2)) 0 var(--s4);
    color: var(--text-2);
  }
  .maker {
    display: flex;
    gap: var(--s4) var(--s5);
    flex-wrap: wrap;
    align-items: flex-start;
  }
  figure {
    margin: 0;
  }
  img {
    display: block;
    width: 100%;
    max-width: 320px;
    height: auto;
    border: 1px solid var(--line);
  }
  .toml {
    max-height: none;
  }
</style>
