<script lang="ts">
  // Two columns: keys in text-2, values in monospace.
  let { title = "", rows }: { title?: string; rows: [string, string][] } = $props();
</script>

<div class="kv">
  {#if title}<h2>{title}</h2>{/if}
  <dl>
    {#each rows as [k, v] (k)}
      <dt>{k}</dt>
      <dd class="mono">{v}</dd>
    {/each}
  </dl>
</div>

<style>
  .kv {
    min-width: 0;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--s1) var(--s3);
    margin: 0;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
    white-space: pre-line;
  }
</style>
