<script lang="ts">
  import { ops } from "../api/ops";
  import { app } from "../lib/state.svelte";
  import { header } from "../lib/spec";
  import type { Libraries } from "../api/types";

  let libs = $state<Libraries | null>(null);
  let q = $state("");
  let open = $state<string | null>(null);
  void ops.pure.libraries().then((l) => (libs = l));

  const match = (s: { path: string; name: string; toml: string }) => {
    const t = q.trim().toLowerCase();
    return !t || s.path.toLowerCase().includes(t) || s.name.toLowerCase().includes(t) || header(s.toml).toLowerCase().includes(t);
  };
  const chips = $derived(libs?.chips.filter(match) ?? []);
  const panels = $derived(libs?.panels.filter(match) ?? []);
</script>

<h1>Library</h1>
<p class="muted">Chip libraries and panel specs embedded in the WASM module from config/chips and config/panels. Files under mined/ are vendor defaults taken from the config corpus, not measurements.</p>

<div class="row" style="margin-bottom: var(--s4)">
  <input type="search" placeholder="search name, path, comment" bind:value={q} style="width: 320px" />
  {#if libs}<span class="muted">{chips.length} chips, {panels.length} panels</span>{/if}
</div>

{#if app.wasm === "failed"}
  <p class="error">{app.wasmError}</p>
{:else if !libs}
  <p class="muted">loading</p>
{/if}

{#snippet entry(s: { path: string; name: string; toml: string; mined?: boolean }, kind: "panel" | "chip")}
  <tr>
    <td><button class="link" onclick={() => (open = open === s.path ? null : s.path)}>{s.name}</button></td>
    <td class="mono">{s.path}</td>
    <td>{s.mined ? "mined" : ""}</td>
    <td><a href="#/builder?{kind}={encodeURIComponent(s.path)}">use in Builder</a></td>
  </tr>
  {#if open === s.path}
    <tr>
      <td colspan="4">
        {#if header(s.toml)}<pre class="head">{header(s.toml)}</pre>{/if}
        <pre>{s.toml}</pre>
      </td>
    </tr>
  {/if}
{/snippet}

{#if libs}
  <section>
    <h2>Panel specs</h2>
    <table>
      <thead><tr><th>name</th><th>path</th><th></th><th></th></tr></thead>
      <tbody>{#each panels as s (s.path)}{@render entry(s, "panel")}{/each}</tbody>
    </table>
  </section>
  <section>
    <h2>Chip libraries</h2>
    <table>
      <thead><tr><th>name</th><th>path</th><th></th><th></th></tr></thead>
      <tbody>{#each chips as s (s.path)}{@render entry(s, "chip")}{/each}</tbody>
    </table>
  </section>
{/if}

<style>
  .link {
    background: none;
    border: 0;
    padding: 0;
    height: auto;
    color: AccentColor;
    text-decoration: underline;
  }
  .head {
    white-space: pre-wrap;
    margin-bottom: var(--s2);
    background: none;
  }
</style>
