<script lang="ts">
  // The selected card under the drawing: position, size, its panels, the
  // provision line for it; with a daemon, the provision form prefilled.
  import { goto } from "$app/navigation";
  import KeyValue from "$parts/KeyValue.svelte";
  import { app, handSpec } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { cardPanels, provisionLine } from "$lib/wall";

  let { index, spec }: { index: number; spec: string } = $props();
  const card = $derived(app.wall.receivers.find((r) => r.index === index));
  const panels = $derived(cardPanels(app.wall, index));
  const rows = $derived.by((): [string, string][] =>
    card ? [["card", String(card.index)], ["position", `${card.x ?? 0},${card.y ?? 0}`], ["size", `${card.width} x ${card.height}`], ["panels", String(panels.length)]] : [],
  );

  const prov = new Action<void>("provision");
  const provision = () =>
    prov.run(async () => {
      const lib = await ops.pure.libraries();
      const p = lib.panels.find((q) => q.path === spec);
      if (!p) throw new Error(`${spec}: not in the panel library`);
      handSpec(p.toml);
      await goto(`/control/provision?provision=${index}`);
    });
</script>

{#if card}
  <section class="selected">
    <KeyValue {rows} />
    <div class="scroll">
      <table>
        <thead><tr><th class="num">panel</th><th class="num">card x</th><th class="num">card y</th><th class="num">x</th><th class="num">y</th><th class="num">width</th><th class="num">height</th><th>rotation</th><th>flip x</th><th>flip y</th></tr></thead>
        <tbody>
          {#each panels as p, i (i)}
            <tr>
              <td class="num">{i}</td>
              <td class="num">{p.receiver_x ?? 0}</td>
              <td class="num">{p.receiver_y ?? 0}</td>
              <td class="num">{p.x}</td>
              <td class="num">{p.y}</td>
              <td class="num">{p.width}</td>
              <td class="num">{p.height}</td>
              <td>{p.rotation ?? "none"}</td>
              <td>{p.flip_x ? "on" : "off"}</td>
              <td>{p.flip_y ? "on" : "off"}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if spec}
      <pre>{provisionLine(spec, card.x ?? 0, card.y ?? 0)}</pre>
      <p class="caption">provision one card at a time: the write reaches every card on the chain</p>
      {#if ops.card}
        <div class="row">
          <button onclick={provision} disabled={prov.busy}>provision</button>
        </div>
        {#if prov.error}<p class="error">{prov.error}</p>{/if}
      {/if}
    {/if}
  </section>
{/if}

<style>
  .selected {
    display: flex;
    flex-direction: column;
    gap: var(--s3);
    max-width: 960px;
  }
  pre {
    max-width: max-content;
  }
  p {
    margin: 0;
  }
</style>
