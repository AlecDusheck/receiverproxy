<script lang="ts">
  // Provision the selected card: a spec from the library, an uploaded one,
  // or the one Panels or the Builder handed over; a firmware from the
  // manifest or "auto"; the position and the chain index. The dry run is a
  // job too and shows the plan; "commit" appears under it.
  //
  // `?provision=<index>` (the Wall's link) selects the receiver whose x,y is
  // the position and whose index the EEPROM writes address.
  import { page } from "$app/state";
  import { untrack } from "svelte";
  import ControlHead from "$parts/ControlHead.svelte";
  import Field from "$parts/Field.svelte";
  import FileDrop, { type Picked } from "$parts/FileDrop.svelte";
  import JobRunner from "$parts/JobRunner.svelte";
  import LibraryPicker from "$parts/LibraryPicker.svelte";
  import { app } from "$lib/state.svelte";
  import { ops } from "$api/ops";
  import { Action } from "$lib/action.svelte";
  import { manifest, type Manifest } from "$lib/firmware";

  // What Panels or the Builder handed over, if anything.
  let handed = "";
  try {
    handed = localStorage.getItem("rxp.builder.toml") ?? "";
  } catch {
    /* no storage */
  }
  let source = $state<"library" | "file" | "text">(handed ? "text" : "library");
  let specPath = $state("");
  let specFile = $state<Picked | null>(null);
  let spec = $state(handed);

  const fw = new Action<Manifest>("firmware manifest");
  void fw.run(() => manifest());
  const images = $derived(fw.result?.images ?? []);

  let firmware = $state("auto");
  let position = $state({ x: 0, y: 0 });
  let index = $state("");

  const from = untrack(() => page.url.searchParams.get("provision"));
  if (from !== null && Number.isInteger(Number(from))) {
    app.card = Number(from);
    index = from;
  }
  const rec = app.wall.receivers.find((q) => q.index === untrack(() => app.card));
  if (rec) {
    position = { x: rec.x, y: rec.y };
  }

  const chain = $derived(index.trim() === "" ? undefined : Number(index));
  const indexError = $derived(chain !== undefined && !(Number.isInteger(chain) && chain >= 0 && chain <= 0xfffe) ? "a whole number 0 to 65534" : "");
  const ready = $derived(!!spec.trim() && !indexError);
  const reason = $derived(!spec.trim() ? "no spec" : indexError ? `chain index: ${indexError}` : "");

  function take(p: Picked) {
    specFile = p;
    spec = new TextDecoder().decode(p.bytes);
  }
</script>

<ControlHead title="Provision">
  <section>
    <h2>Panel spec</h2>
    <div class="form">
      <Field label="from">
        <select bind:value={source}>
          <option value="library">the library</option>
          <option value="file">a .toml file</option>
          <option value="text">the text below</option>
        </select>
      </Field>
    </div>
    {#if source === "library"}
      <LibraryPicker bind:value={specPath} onpick={(s) => (spec = s.toml)} />
    {:else if source === "file"}
      <FileDrop label="panel spec .toml" accept=".toml" bind:picked={specFile} onpick={take} />
    {/if}
    <div class="form">
      <Field label="spec TOML" caption="{spec.split('\n').length} lines" wide><textarea rows="10" bind:value={spec} spellcheck="false"></textarea></Field>
    </div>
  </section>

  <section>
    <h2>Card</h2>
    <div class="form">
      <Field label="firmware" caption="auto ranks the manifest for this spec">
        <select bind:value={firmware}>
          <option value="auto">auto</option>
          <option value="">none</option>
          {#each images as i (i.name)}<option value={i.name}>{i.name}</option>{/each}
        </select>
      </Field>
      <Field label="position x" caption="the card's window on the screen"><input type="number" bind:value={position.x} min="0" /></Field>
      <Field label="position y"><input type="number" bind:value={position.y} min="0" /></Field>
      <Field label="chain index" caption="the card's place in the Ethernet chain; empty broadcasts, one card on the link" error={indexError}>
        <input bind:value={index} inputmode="numeric" class="mono" />
      </Field>
    </div>
    {#if fw.error}<p class="error">{fw.error}</p>{/if}

    <JobRunner
      label="provision"
      disabled={!ready}
      {reason}
      confirm="This writes firmware, flash block 7 and the EEPROM of {chain === undefined ? 'the card on the link' : `chain index ${chain}`}; power-cycle it afterwards."
      run={(commit) =>
        ops.card!.provision({
          spec_toml: spec,
          firmware_path: firmware || undefined,
          position: [position.x, position.y],
          index: chain,
          commit,
        })}
    />
  </section>
</ControlHead>
