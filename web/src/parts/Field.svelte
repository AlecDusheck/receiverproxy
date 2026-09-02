<script lang="ts">
  // One form row: label in the left column, the control(s) in the right.
  import type { Snippet } from "svelte";
  let { label, error = "", children }: { label: string; error?: string; children: Snippet } = $props();
</script>

<span class="label">{label}</span>
<div>
  <div class="row">{@render children()}</div>
  {#if error}<div class="error">{error}</div>{/if}
</div>
