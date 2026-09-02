<script lang="ts">
  // The receiving-card models in config/cards, prerendered at build time.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  let { data } = $props();
  const hex = (n: number, w = 2) => "0x" + n.toString(16).padStart(w, "0");
  const href = (name: string) => `/cards/${encodeURIComponent(name.toLowerCase())}`;
</script>

<Head title="Cards" description="Receiving card models with firmware downloads and tested panels." path="/cards" />

<TitleRow title="Cards" />

<div class="scroll">
  <table>
    <thead>
      <tr><th></th><th>model</th><th>vendor</th><th>family</th><th>id</th><th class="num">max width</th><th class="num">max height</th><th class="num">hub ports</th><th>status</th><th class="num">panels tested</th></tr>
    </thead>
    <tbody>
      {#each data.cards as c (c.name)}
        <tr>
          <td class="pic">{#if c.image}<a href={href(c.name)}><img src="/{c.image}" alt="{c.vendor} {c.name}" /></a>{/if}</td>
          <td><a href={href(c.name)}>{c.name}</a></td>
          <td>{c.vendor}</td>
          <td class="mono">{c.family}</td>
          <td class="mono">{hex(c.id)}</td>
          <td class="num">{c.limits.max_width}</td>
          <td class="num">{c.limits.max_height}</td>
          <td class="num">{c.limits.hub_ports}</td>
          <td>{c.status}</td>
          <td class="num">{c.tested.length}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .pic {
    padding: var(--s1) var(--s2);
  }
  .pic a {
    display: block;
    line-height: 0;
  }
  img {
    height: 48px;
    width: auto;
    border: 1px solid var(--line);
  }
</style>
