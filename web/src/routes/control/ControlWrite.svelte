<script lang="ts">
  // The selected card's provision, firmware, flash and card-state groups:
  // each a form with one button; gated ones run the dry run first and show
  // the confirm line with "commit" after it.
  import { untrack } from "svelte";
  import Field from "$parts/Field.svelte";
  import Lines from "$parts/Lines.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import type { GatedOutcome, Job, Outcome, SizeOutcome } from "$api/types";

  let { index, query }: { index: number; query: URLSearchParams } = $props();

  const jobLine = (j: Job) => `${j.kind} ${j.id}: ${j.state}${j.result && "committed" in j.result ? (j.result.committed ? ", written" : ", dry run") : ""}`;
  const dryRun = (j: Job | null) => j?.state === "done" && !!j.result && "committed" in j.result && !j.result.committed;
  const job = (start: Promise<{ id: string }>) => start.then((s) => ops.card!.follow(s.id));

  // Provision: the spec handed over by the Gallery or the Builder; the Wall's
  // "provision this card" names the receiver whose x,y is the position.
  let prov = $state({ spec_toml: "", firmware_path: "", x: 0, y: 0 });
  try {
    prov.spec_toml = localStorage.getItem("rxp.builder.toml") ?? "";
  } catch {
    /* no storage */
  }
  const from = untrack(() => query.get("provision") ?? String(index));
  const rec = app.wall.receivers.find((q) => q.index === Number(from));
  if (rec) {
    prov.x = rec.x ?? 0;
    prov.y = rec.y ?? 0;
  }
  const provision = new Action<Job>("provision");
  const runProvision = (commit: boolean) =>
    provision.run(() => job(ops.card!.provision({ spec_toml: prov.spec_toml, firmware_path: prov.firmware_path || undefined, position: [prov.x, prov.y], commit })));

  // Firmware
  let fw = $state("");
  const firmware = new Action<Job>("firmware install");
  const runFirmware = (commit: boolean) => firmware.run(() => job(ops.card!.firmwareInstall({ path: fw, commit })));

  // Flash
  let flashOp = $state<"snapshot" | "restore">("snapshot");
  let dir = $state("");
  const flash = new Action<Job>("flash");
  const runFlash = (commit: boolean) =>
    flash.run(() => job(flashOp === "snapshot" ? ops.card!.flashSnapshot({ dir: dir || undefined, index }) : ops.card!.flashRestore({ dir, commit, index })));

  // Card state
  type CardOp = "read screen size" | "write screen size" | "test mode" | "set layout" | "reload" | "full reload";
  let cardOp = $state<CardOp>("read screen size");
  let size = $state({ width: 128, height: 64 });
  let test = $state(0);
  let layout = $state({ w: 128, h: 64 });
  const card = new Action<Outcome | SizeOutcome | { width: number; height: number }>("card");
  const runCard = (commit: boolean) =>
    card.run(async () => {
      const c = ops.card!;
      switch (cardOp) {
        case "read screen size": {
          const r = await c.screenSize({ index });
          size = r;
          return r;
        }
        case "write screen size":
          return c.setScreenSize({ ...size, commit, index });
        case "test mode":
          return c.testMode({ n: test, index });
        case "set layout":
          return c.setLayout({ panel_width: layout.w, panel_height: layout.h, index });
        case "reload":
          return c.reload({ index });
        default:
          return c.reload({ index, full: true });
      }
    });
  const gatedCard = $derived(card.result && "committed" in card.result ? (card.result as GatedOutcome) : null);
</script>

<section>
  <h2>Provision card {index}</h2>
  <p class="muted">Snapshot, firmware, EEPROM read, config, EEPROM write. The dry run discovers the card and prints the plan. Power-cycle the card afterwards.</p>
  <div class="form">
    <Field label="spec TOML" caption="from the Gallery or the Builder" wide><textarea rows="8" bind:value={prov.spec_toml} spellcheck="false"></textarea></Field>
    <Field label="firmware" caption="optional: a name from config/firmware.toml or a .hex path the daemon's process can read" wide><input bind:value={prov.firmware_path} class="mono" /></Field>
    <Field label="position x" caption="the card's window on the screen"><input type="number" bind:value={prov.x} min="0" /></Field>
    <Field label="position y"><input type="number" bind:value={prov.y} min="0" /></Field>
  </div>
  <div class="actions"><button onclick={() => runProvision(false)} disabled={provision.busy || !prov.spec_toml}>dry run</button></div>
  {#if provision.error}<p class="error">{provision.error}</p>{/if}
  {#if provision.result}
    <p class={provision.result.state === "done" ? "ok" : "muted"}>{jobLine(provision.result)}</p>
    <Lines lines={provision.result.lines} files={provision.result.result?.files ?? []} />
    {#if dryRun(provision.result)}
      <p class="confirm">This writes firmware, flash block 7 and the EEPROM of card {index}.</p>
      <button onclick={() => runProvision(true)} disabled={provision.busy}>commit</button>
    {/if}
  {/if}
</section>

<section>
  <h2>Firmware</h2>
  <div class="form">
    <Field label="image" caption="a name from config/firmware.toml or a .hex path the daemon's process can read" wide><input bind:value={fw} class="mono" /></Field>
  </div>
  <div class="actions"><button onclick={() => runFirmware(false)} disabled={firmware.busy || !fw}>dry run</button></div>
  {#if firmware.error}<p class="error">{firmware.error}</p>{/if}
  {#if firmware.result}
    <p class={firmware.result.state === "done" ? "ok" : "muted"}>{jobLine(firmware.result)}</p>
    <Lines lines={firmware.result.lines} files={firmware.result.result?.files ?? []} />
    {#if dryRun(firmware.result)}
      <p class="confirm">This programs the firmware bank of card {index}.</p>
      <button onclick={() => runFirmware(true)} disabled={firmware.busy}>commit</button>
    {/if}
  {/if}
</section>

<section>
  <h2>Flash</h2>
  <div class="form">
    <Field label="operation"><select bind:value={flashOp}><option value="snapshot">snapshot</option><option value="restore">restore</option></select></Field>
    <Field label="directory" caption={flashOp === "snapshot" ? "empty: under the daemon's data dir" : "a snapshot directory"} wide><input bind:value={dir} class="mono" /></Field>
  </div>
  <div class="actions"><button onclick={() => runFlash(false)} disabled={flash.busy || (flashOp === "restore" && !dir)}>{flashOp === "snapshot" ? "snapshot" : "dry run"}</button></div>
  {#if flash.error}<p class="error">{flash.error}</p>{/if}
  {#if flash.result}
    <p class={flash.result.state === "done" ? "ok" : "muted"}>{jobLine(flash.result)}</p>
    <Lines lines={flash.result.lines} files={flash.result.result?.files ?? []} />
    {#if flashOp === "restore" && dryRun(flash.result)}
      <p class="confirm">This writes every block of the snapshot to card {index}.</p>
      <button onclick={() => runFlash(true)} disabled={flash.busy}>commit</button>
    {/if}
  {/if}
</section>

<section>
  <h2>Card state</h2>
  <div class="form">
    <Field label="operation">
      <select bind:value={cardOp}>
        {#each ["read screen size", "write screen size", "test mode", "set layout", "reload", "full reload"] as o (o)}<option value={o}>{o}</option>{/each}
      </select>
    </Field>
    {#if cardOp === "write screen size" || cardOp === "read screen size"}
      <Field label="width" caption="pixels"><input type="number" bind:value={size.width} min="1" disabled={cardOp === "read screen size"} /></Field>
      <Field label="height" caption="pixels"><input type="number" bind:value={size.height} min="1" disabled={cardOp === "read screen size"} /></Field>
    {:else if cardOp === "test mode"}
      <Field label="mode" caption="0-255, 0 is off"><input type="number" bind:value={test} min="0" max="255" /></Field>
    {:else if cardOp === "set layout"}
      <Field label="panel width" caption="RAM only"><input type="number" bind:value={layout.w} min="1" /></Field>
      <Field label="panel height"><input type="number" bind:value={layout.h} min="1" /></Field>
    {/if}
  </div>
  <div class="actions"><button onclick={() => runCard(false)} disabled={card.busy}>{cardOp === "write screen size" ? "dry run" : "run"}</button></div>
  {#if card.error}<p class="error">{card.error}</p>{/if}
  {#if card.result}
    {#if "width" in card.result}<p class="ok">screen size {card.result.width}x{card.result.height}</p>{/if}
    {#if "lines" in card.result}<Lines lines={card.result.lines} files={card.result.files} />{/if}
    {#if gatedCard && !gatedCard.committed}
      <p class="confirm">This writes the screen size to the EEPROM of card {index}.</p>
      <button onclick={() => runCard(true)} disabled={card.busy}>commit</button>
    {/if}
  {/if}
</section>
