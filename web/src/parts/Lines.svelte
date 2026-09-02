<script lang="ts">
  // Command output as the CLI printed it; `err` lines in grey. Newest at the
  // bottom, kept in view until the user scrolls up.
  import type { Line } from "../api/types";
  let { lines, files = [] }: { lines: Line[]; files?: string[] } = $props();
  let el = $state<HTMLPreElement | null>(null);
  let follow = true;

  const atBottom = (p: HTMLPreElement) => p.scrollHeight - p.scrollTop - p.clientHeight < 4;
  $effect(() => {
    void lines.length;
    void files.length;
    if (el && follow) el.scrollTop = el.scrollHeight;
  });
</script>

{#if lines.length || files.length}
  <pre class="lines" bind:this={el} onscroll={() => (follow = atBottom(el!))}>{#each lines as l, i (i)}<span class={l.stream}>{l.text}</span>{"\n"}{/each}{#each files as f (f)}wrote {f}{"\n"}{/each}</pre>
{/if}
