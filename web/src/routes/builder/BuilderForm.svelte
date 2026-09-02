<script lang="ts">
  // The Builder's form by section; every edit calls `onchange`, which the
  // Builder debounces into the TOML pane.
  import Field from "$parts/Field.svelte";
  import type { PanelSpec } from "$lib/spec";
  import type { Libraries } from "$api/types";

  let { spec = $bindable(), libs, onchange }: { spec: PanelSpec; libs: Libraries | null; onchange: () => void } = $props();

  const hex = (n: number, w = 2) => "0x" + (Number.isFinite(n) ? n : 0).toString(16).padStart(w, "0");
  let overrideOffset = $state("");
  let overrideValue = $state("");
  const offsetOk = $derived(/^0x[0-9a-fA-F]{1,3}$/.test(overrideOffset));
  const valueOk = $derived(overrideValue.trim() !== "" && Number.isInteger(Number(overrideValue)) && Number(overrideValue) >= 0 && Number(overrideValue) <= 255);
  function addOverride() {
    spec.overrides.push({ offset: overrideOffset, value: Number(overrideValue) });
    overrideOffset = "";
    overrideValue = "";
    onchange();
  }
  function removeOverride(i: number) {
    spec.overrides.splice(i, 1);
    onchange();
  }
  const range = (v: number, lo: number, hi: number) => (Number.isFinite(v) && v >= lo && v <= hi ? "" : `${lo}-${hi}`);
</script>

<div class="form" oninput={onchange} onchange={onchange}>
  <Field label="name" wide><input bind:value={spec.name} class="mono" /></Field>

  <h2>Module</h2>
  <Field label="width" caption="pixels" error={range(spec.module.width, 1, 65535)}><input type="number" bind:value={spec.module.width} min="1" /></Field>
  <Field label="height" caption="pixels" error={range(spec.module.height, 1, 65535)}><input type="number" bind:value={spec.module.height} min="1" /></Field>
  <Field label="scan" caption="1-255, 1/scan duty" error={range(spec.module.scan, 1, 255)}><input type="number" bind:value={spec.module.scan} min="1" max="255" /></Field>
  <Field label="line_dir" caption="0-255"><input type="number" bind:value={spec.module.line_dir} min="0" max="255" /></Field>
  <Field label="data_groups" caption="1-255"><input type="number" bind:value={spec.module.data_groups} min="1" max="255" /></Field>
  <Field label="serial_clock" caption="1-31, empty: chip default"><input type="number" bind:value={spec.module.serial_clock} min="1" max="31" placeholder="chip default" /></Field>
  <Field label="gray_bits" caption="12-16, empty: default"><input type="number" bind:value={spec.module.gray_bits} min="1" max="16" placeholder="default" /></Field>

  <h2>Screen</h2>
  <Field label="width" caption="pixels"><input type="number" bind:value={spec.screen.width} min="1" /></Field>
  <Field label="height" caption="pixels"><input type="number" bind:value={spec.screen.height} min="1" /></Field>

  <h2>Chip</h2>
  <Field label="library" caption={spec.chip.library || "config/chips"} mono>
    <select bind:value={spec.chip.library} disabled={!libs}>
      <option value="">choose</option>
      <optgroup label="libraries">
        {#each (libs?.chips ?? []).filter((c) => !c.path.includes("/mined/")) as c (c.path)}<option value={c.path}>{c.name.replace(/\s*\(mined\)/, "")}</option>{/each}
      </optgroup>
      <optgroup label="mined">
        {#each (libs?.chips ?? []).filter((c) => c.path.includes("/mined/")) as c (c.path)}<option value={c.path}>{c.name.replace(/\s*\(mined\)/, "")}</option>{/each}
      </optgroup>
    </select>
  </Field>

  <h2>Colour</h2>
  <Field label="swap" caption="0-255"><input type="number" bind:value={spec.color.swap} min="0" max="255" /></Field>
  <Field label="source" caption="channel index per output, 0-2" wide>
    {#each [0, 1, 2] as i (i)}<input type="number" bind:value={spec.color.source[i]} min="0" max="2" aria-label="source {i}" />{/each}
  </Field>

  <h2>Current</h2>
  <Field label="gains" caption="R G B vR, 0-63" wide>
    {#each [0, 1, 2, 3] as i (i)}<input type="number" bind:value={spec.current.gains[i]} min="0" max="63" aria-label="gain {i}" />{/each}
  </Field>
  <Field label="percent" caption="R G B, 0-1" wide>
    {#each [0, 1, 2] as i (i)}<input type="number" bind:value={spec.current.percent[i]} step="0.01" min="0" max="1" aria-label="percent {i}" />{/each}
  </Field>

  <h2>Timing</h2>
  <Field label="gamma"><input type="number" bind:value={spec.timing.gamma} step="0.1" min="0" /></Field>
  <Field label="refresh_hz" caption="Hz"><input type="number" bind:value={spec.timing.refresh_hz} step="1" min="1" /></Field>
  <Field label="gclock" caption={`0-255, ${hex(spec.timing.gclock)}`}><input type="number" bind:value={spec.timing.gclock} min="0" max="255" /></Field>
  <Field label="min_oe" caption="seconds"><input type="number" bind:value={spec.timing.min_oe} step="0.00001" min="0" /></Field>
  <Field label="luminance_level" caption="0-65535"><input type="number" bind:value={spec.timing.luminance_level} min="0" max="65535" /></Field>
  <Field label="oe_8ns"><input type="checkbox" bind:checked={spec.timing.oe_8ns} /></Field>

  <h2>Mapping</h2>
  <Field label="reversed_groups"><input type="checkbox" bind:checked={spec.mapping.reversed_groups} /></Field>
  <Field label="reversed_lines"><input type="checkbox" bind:checked={spec.mapping.reversed_lines} /></Field>
  <Field label="block" caption="columns, empty: module width"><input type="number" bind:value={spec.mapping.block} min="1" placeholder="module width" /></Field>
  <Field label="gate_phantom_positions"><input type="checkbox" bind:checked={spec.mapping.gate_phantom_positions} /></Field>

  <h2>Boot</h2>
  <Field label="arm_at_boot" caption="chip registers on the boot page" wide><input type="checkbox" bind:checked={spec.boot.arm_at_boot} /></Field>

  <h2>Record 0x01 overrides</h2>
  {#each spec.overrides as ov, i (ov.offset)}
    <Field label={ov.offset} caption={`byte, ${hex(ov.value)}`}>
      <input type="number" bind:value={ov.value} min="0" max="255" />
      <button onclick={() => removeOverride(i)}>remove</button>
    </Field>
  {/each}
  <Field label="add" caption="offset 0x000-0x2FB, value 0-255" wide>
    <input placeholder="0x02F" bind:value={overrideOffset} class="mono short" aria-label="offset" />
    <input placeholder="1" bind:value={overrideValue} class="mono short" aria-label="value" />
    <button disabled={!offsetOk || !valueOk} onclick={addOverride}>add</button>
  </Field>
</div>

<style>
  .short {
    width: 88px;
  }
</style>
