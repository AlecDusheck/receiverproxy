<script lang="ts">
  // A vendor file (or a spec) read back into a spec through the WASM module;
  // the format is detected from the bytes. The result is handed to the Builder.
  import { goto } from "$app/navigation";
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import Drop from "$parts/Drop.svelte";
  import { app, handSpec } from "$lib/state.svelte";
  import { ops, type Imported } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { BUILDER_NAV } from "../nav";

  const imp = new Action<Imported & { file: string }>("import");
  function importFile(files: File[]) {
    const f = files[0]!;
    void imp.run(async () => ({ ...(await ops.pure.importSpec(new Uint8Array(await f.arrayBuffer()))), file: f.name }));
  }
  function toBuilder(text: string) {
    handSpec(text);
    void goto("/builder");
  }
</script>

<Head title="Import" noindex />

<TitleRow title="Import" />
<SubNav links={BUILDER_NAV} />

{#if app.wasm === "failed"}<p class="error">{app.wasmError}</p>{/if}

<Drop label="Vendor file or spec" disabled={app.wasm === "failed"} onfiles={importFile} />
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
