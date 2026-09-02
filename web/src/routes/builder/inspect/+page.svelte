<script lang="ts">
  // Inspect one .rcvbp; diff two.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import SubNav from "$parts/SubNav.svelte";
  import BuilderTools from "./BuilderTools.svelte";
  import { app } from "$lib/state.svelte";
  import { BUILDER_NAV } from "../nav";
</script>

<Head title="Inspect" noindex />

<TitleRow title="Inspect" />
<SubNav links={BUILDER_NAV} />

{#if app.wasm === "failed"}<p class="error">{app.wasmError}</p>{/if}

<BuilderTools />
