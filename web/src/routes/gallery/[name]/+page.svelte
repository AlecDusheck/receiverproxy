<script lang="ts">
  // One spec: the entry facts and every table of the TOML as key-value
  // blocks, the TOML itself, and the downloads. The page is static; the
  // files are generated in the browser when a download button is pressed.
  import { goto } from "$app/navigation";
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import KeyValue from "$parts/KeyValue.svelte";
  import { ops, type Generated } from "$api/ops";
  import { app, handSpec } from "$lib/state.svelte";
  import { Action } from "$lib/action.svelte";
  import { parseToml } from "$lib/spec";
  import { save } from "$lib/download";

  let { data } = $props();
  const entry = $derived(data.entry);
  const toml = $derived(data.entry.toml);

  const HEX = new Set(["gclock", "family_id", "sub_id"]);
  const show = (v: unknown, k = ""): string =>
    Array.isArray(v) ? `[${v.map((x) => show(x)).join(", ")}]` : HEX.has(k) && typeof v === "number" ? "0x" + v.toString(16).padStart(2, "0") : String(v);
  const meta = $derived.by((): [string, string][] => {
    const m = entry.meta;
    const rows: [string, string][] = [
      ["path", entry.path],
      ["status", m.status],
      ["origin", m.origin],
      ["sources", String(m.sources)],
    ];
    if (m.pitch_mm !== undefined) rows.splice(1, 0, ["pitch", `${m.pitch_mm} mm`]);
    if (m.agreement !== undefined) rows.push(["agreement", String(m.agreement)]);
    rows.push(["chip", `${entry.chip.name} (${entry.chip.library}, family 0x${entry.chip.family_id.toString(16).padStart(4, "0")})`]);
    rows.push(["formats", entry.formats.join(", ")]);
    if (m.vendors.length) rows.push(["vendors", m.vendors.join(", ")]);
    if (m.examples.length) rows.push(["examples", m.examples.join("\n")]);
    if (m.notes) rows.push(["notes", m.notes]);
    return rows;
  });
  // The spec's tables, in file order, each a block; [meta] is the entry block above.
  const blocks = $derived.by((): [string, [string, string][]][] => {
    try {
      return Object.entries(parseToml(toml))
        .filter(([name, t]) => name !== "meta" && Object.keys(t).length)
        .map(([name, t]) => [name || "spec", Object.entries(t).map(([k, v]) => [k, show(v, k)])]);
    } catch (e) {
      return [["spec", [["parse error", e instanceof Error ? e.message : String(e)]]]];
    }
  });
  const description = $derived.by(() => {
    const m = entry.meta;
    const head = `${entry.module.width}x${entry.module.height} module, 1/${entry.module.scan} scan, ${entry.chip.name}; ${m.status}, ${m.origin}, ${m.sources} vendor file${m.sources === 1 ? "" : "s"}.`;
    return m.notes ? `${head} ${m.notes}` : head;
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

<Head title="{entry.name} panel config" {description} path="/gallery/{encodeURIComponent(entry.name)}" />

<TitleRow title="{entry.name} panel config">
  {#snippet action()}
    <a href="/gallery">Gallery</a>
  {/snippet}
</TitleRow>

<section>
  <div class="blocks">
    <KeyValue title="entry" rows={meta} />
    {#each blocks as [name, rows] (name)}
      <KeyValue title={name} rows={rows} />
    {/each}
  </div>
</section>

<section>
  <h2>Downloads</h2>
  <p class="caption">Generated in the browser by the same code as <code>rxp config gen</code>; the files are byte-identical to the command's.</p>
  <div class="row">
    {#each data.formats.filter((f) => f.generate) as f (f.name)}
      <button onclick={() => downloadAs(f.name)} disabled={gen.busy || app.wasm === "failed"}>download as {f.name} (.{f.extension})</button>
    {/each}
    <button onclick={() => save(`${entry.name}.toml`, toml)}>download TOML</button>
    <button class="primary" onclick={() => go("/builder")}>open in Builder</button>
    {#if ops.card}
      <button onclick={() => go("/control")}>provision this card</button>
    {/if}
  </div>
  {#if app.wasm === "failed"}<p class="error">{app.wasmError}</p>{/if}
  {#if gen.error}<p class="error">{gen.error}</p>{/if}
  {#if gen.result}
    <p class="ok">{gen.result.format}: {gen.result.files.map((f) => `${f.name} (${f.bytes.length} bytes)`).join(", ")}</p>
    {#if gen.result.notes.length}<pre>{gen.result.notes.join("\n")}</pre>{/if}
  {/if}
</section>

<section>
  <h2>{entry.path}</h2>
  <pre class="toml">{toml}</pre>
</section>

<style>
  .toml {
    max-height: none;
  }
</style>
