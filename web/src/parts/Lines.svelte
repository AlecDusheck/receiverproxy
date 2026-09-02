<script lang="ts">
  // Command output as the CLI printed it; `err` lines in grey.
  import type { Line } from "../lib/types";
  let { lines, files = [] }: { lines: Line[]; files?: string[] } = $props();
</script>

{#if lines.length || files.length}
  <pre class="lines">{#each lines as l, i (i)}<span class={l.stream}>{l.text}</span>{"\n"}{/each}{#each files as f (f)}wrote {f}{"\n"}{/each}</pre>
{/if}
