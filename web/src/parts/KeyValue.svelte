<script lang="ts">
  // Two columns: keys in text-2, values in monospace (`.kv` in app.css).
  let { title = "", rows }: { title?: string; rows: [string, string][] } = $props();
</script>

<div class="kv">
  {#if title}<h2>{title}</h2>{/if}
  <dl>
    {#each rows as [k, v] (k)}
      <dt>{k}</dt>
      <dd>{v}</dd>
    {/each}
  </dl>
</div>
