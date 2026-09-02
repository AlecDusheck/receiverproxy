<script lang="ts">
  // One spec: downloads first, then the module, wiring and timing tables as
  // key-value blocks, then the TOML. The page is static; the files are
  // generated in the browser when a download button is pressed.
  import { goto } from "$app/navigation";
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import KeyValue from "$parts/KeyValue.svelte";
  import Module from "$parts/Module.svelte";
  import { ops, type Generated } from "$api/ops";
  import { app, handSpec } from "$lib/state.svelte";
  import { Action } from "$lib/action.svelte";
  import { panelTitle } from "$lib/panel";
  import { parseToml, type Tables } from "$lib/spec";
  import { save } from "$lib/download";

  let { data } = $props();
  const entry = $derived(data.entry);
  const toml = $derived(data.entry.toml);
  const title = $derived(panelTitle(entry));

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
  const blocks = (names: string[], rest = false): [string, [string, string][]][] =>
    Object.entries(tables)
      .filter(([name, t]) => name !== "meta" && Object.keys(t).length && (rest ? !named.has(name) : names.includes(name)))
      .map(([name, t]) => [name || "spec", Object.entries(t).map(([k, v]) => [k, show(v, k)])]);
  const meta = $derived.by((): [string, string][] => {
    const m = entry.meta;
    const rows: [string, string][] = [
      ["file", entry.path],
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
  // At most 155 characters: title, what the page offers, and how far the spec is tested.
  const description = $derived.by(() => {
    const cards = data.tested.map((t) => t.card);
    const tail = entry.meta.status === "tested" ? `Tested on the bench${cards.length ? ` with the ${[...new Set(cards)].join(", ")}` : ""}.` : `Generated from ${entry.meta.sources} vendor file${entry.meta.sources === 1 ? "" : "s"}, not driven.`;
    return `${title} panel config for Colorlight receiving cards: rcvbp download, wiring, timing. ${tail}`;
  });

  const gen = new Action<Generated & { format: string }>("generate");
  function downloadAs(name: string) {
    void gen.run(async () => {
      const g = await ops.pure.generate(toml, name);
      for (const file of g.files) save(file.name, file.bytes);
      return { ...g, format: name };
    });
  }
  function go(path: string) {
    handSpec(toml);
    void goto(path);
  }
</script>

<Head title="{title} panel" {description} path="/panels/{encodeURIComponent(entry.name)}" />

<TitleRow {title}>
  {#snippet action()}
    <a href="/panels">Panels</a>
  {/snippet}
</TitleRow>
<SubNav links={[["#downloads", "Downloads"], ["#module", "Module"], ["#wiring", "Wiring"], ["#timing", "Timing"], ["#toml", "TOML"]]} />

<section id="downloads">
  <h2>Downloads</h2>
  <div class="row">
    {#each data.formats.filter((f) => f.generate) as f (f.name)}
      <button onclick={() => downloadAs(f.name)} disabled={gen.busy || app.wasm === "failed"}>{entry.name}.{f.extension}</button>
    {/each}
    <button onclick={() => save(`${entry.name}.toml`, toml)}>{entry.name}.toml</button>
    <button class="primary" onclick={() => go("/builder")}>open in Builder</button>
    {#if ops.card}
      <button onclick={() => go("/control/provision")}>provision</button>
    {/if}
  </div>
  {#if app.wasm === "failed"}<p class="error">{app.wasmError}</p>{/if}
  {#if gen.error}<p class="error">{gen.error}</p>{/if}
  {#if gen.result}
    <p class="ok">{gen.result.files.map((f) => `${f.name} (${f.bytes.length} bytes)`).join(", ")}</p>
    {#if gen.result.notes.length}<pre>{gen.result.notes.join("\n")}</pre>{/if}
  {/if}
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
    {#each blocks(GROUPS[0]![1]) as [name, rows] (name)}<KeyValue title={name} rows={rows} />{/each}
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
  <h2>{entry.path}</h2>
  <pre class="toml">{toml}</pre>
</section>

<style>
  .toml {
    max-height: none;
  }
</style>
