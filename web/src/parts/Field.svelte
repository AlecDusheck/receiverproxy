<script lang="ts">
  // One form field: label above the control, the unit or range in the
  // caption (monospace for a path), an invalid value's message under the
  // control in err.
  import type { Snippet } from "svelte";
  let { label, caption = "", error = "", wide = false, mono = false, children }: { label: string; caption?: string; error?: string; wide?: boolean; mono?: boolean; children: Snippet } = $props();
</script>

<div class={["field", { wide }]}>
  <span class="name">{label}</span>
  <span class="row">{@render children()}</span>
  {#if caption}<span class={["caption", { mono }]}>{caption}</span>{/if}
  {#if error}<span class="error">{error}</span>{/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--s1);
    min-width: 0;
  }
  .name {
    font-size: 12px;
    color: var(--text-2);
  }
  .error {
    font-size: 12px;
  }
  .caption.mono {
    font-size: 11px;
    overflow-wrap: anywhere;
  }
</style>
