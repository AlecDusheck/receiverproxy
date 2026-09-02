<script lang="ts">
  // The selected gallery entry beneath the table: the spec as key-value
  // blocks, download-as buttons per output format, open in Builder, and,
  // with a daemon, provision this card.
  import KeyValue from "../parts/KeyValue.svelte";
  import { ops, type Entry, type Format, type Generated } from "../api/ops";
  import { handSpec } from "../lib/state.svelte";
  import { Action } from "../lib/action.svelte";
  import { parseToml } from "../lib/spec";
  import { save } from "../lib/download";

  let { entry, toml, formats }: { entry: Entry; toml: string; formats: Format[] } = $props();

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

  const gen = new Action<Generated & { format: string }>("generate");
  function downloadAs(f: Format) {
    void gen.run(async () => {
      const g = await ops.pure.generate(toml, f.name);
      for (const file of g.files) save(file.name, file.bytes);
      return { ...g, format: f.name };
    });
  }
  function go(hash: string) {
    handSpec(toml);
    location.hash = hash;
  }
</script>

<section class="entry">
  <h2>{entry.name}</h2>
  <div class="blocks">
    <KeyValue title="entry" rows={meta} />
    {#each blocks as [name, rows] (name)}
      <KeyValue title={name} rows={rows} />
    {/each}
  </div>
  <div class="actions">
    {#each formats.filter((f) => f.generate) as f (f.name)}
      <button onclick={() => downloadAs(f)} disabled={gen.busy || !toml}>download as {f.name} (.{f.extension})</button>
    {/each}
    <button class="primary" onclick={() => go("#/builder")} disabled={!toml}>open in Builder</button>
    {#if ops.card}
      <button onclick={() => go("#/cards")} disabled={!toml}>provision this card</button>
    {/if}
  </div>
  {#if gen.error}<p class="error">{gen.error}</p>{/if}
  {#if gen.result}
    <p class="ok">{gen.result.format}: {gen.result.files.map((f) => `${f.name} (${f.bytes.length} bytes)`).join(", ")}</p>
    {#if gen.result.notes.length}<pre>{gen.result.notes.join("\n")}</pre>{/if}
  {/if}
</section>

<style>
  .entry {
    margin-top: var(--s4);
    padding-top: var(--s3);
    border-top: 1px solid var(--line);
  }
  .blocks {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--s4) var(--s5);
  }
</style>
