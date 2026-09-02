<script lang="ts">
  // The receiving-card models in config/cards, prerendered at build time.
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  let { data } = $props();
  const hex = (n: number, w = 2) => "0x" + n.toString(16).padStart(w, "0");
</script>

<Head title="Cards" description="The receiving-card models under config/cards: vendor, protocol family, discovery id, limits, and how far each is tested; the firmware manifest is on each card's page." path="/cards" />

<TitleRow title="Cards" />

<p class="caption">One TOML file per card, embedded into the <code>receivers</code> crate; <code>rxp card models</code> prints the same list. {data.images} firmware images are in config/firmware.toml.</p>

<div class="scroll">
  <table>
    <thead>
      <tr><th>model</th><th>vendor</th><th>family</th><th>id</th><th class="num">max width</th><th class="num">max height</th><th class="num">hub ports</th><th>status</th><th class="num">panels tested</th></tr>
    </thead>
    <tbody>
      {#each data.cards as c (c.name)}
        <tr>
          <td><a href="/cards/{encodeURIComponent(c.name.toLowerCase())}">{c.name}</a></td>
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
