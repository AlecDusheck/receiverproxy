<script lang="ts">
  // The top of every Control page: the title, the sibling pages, the selected
  // card. Without the daemon, the install command instead of the content.
  import type { Snippet } from "svelte";
  import Head from "./Head.svelte";
  import TitleRow from "./TitleRow.svelte";
  import SubNav from "./SubNav.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";

  let { title, action, children }: { title: string; action?: Snippet; children: Snippet } = $props();
  const NAV: [string, string][] = [
    ["/control", "Cards"],
    ["/control/show", "Show"],
    ["/control/mirror", "Mirror"],
    ["/control/provision", "Provision"],
    ["/control/firmware", "Firmware"],
    ["/control/flash", "Flash"],
    ["/control/card", "Card state"],
  ];
  const cards = $derived(app.health?.cards ?? []);
  const label = (c: (typeof cards)[number]) => `${c.controller}: ${c.model ?? "card 0x" + c.card_id.toString(16).padStart(2, "0")} ${c.ver_major}.${c.ver_minor} ${c.cols}x${c.rows}`;
</script>

<Head {title} noindex />

<TitleRow {title} {action} />
<SubNav links={NAV} />

{#if !ops.card}
  <p>daemon not running</p>
  <pre>cargo install --path crates/cli
rxp ui</pre>
{:else}
  <div class="row mb-4">
    <label>card <select bind:value={app.card} aria-label="selected card">
        {#each cards as c (c.controller)}<option value={c.controller}>{label(c)}</option>{/each}
        {#if !cards.some((c) => c.controller === app.card)}<option value={app.card}>{app.card}: no card answered</option>{/if}
      </select></label>
    {#if app.health}<span class="caption">iface {app.health.iface}, daemon {app.health.version}</span>{/if}
  </div>
  {@render children()}
{/if}

<style>
  label {
    display: flex;
    gap: var(--s2);
    align-items: center;
  }
</style>
