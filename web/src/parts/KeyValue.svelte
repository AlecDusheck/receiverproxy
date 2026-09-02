<script lang="ts" module>
  /** A value that is a link: the words are the link text. */
  export type Link = { href: string; text: string };
  export type Row = [string, string | Link];
</script>

<script lang="ts">
  // Two columns: keys in text-2, values in monospace (`.kv` in app.css); a
  // value may be a link.
  let { title = "", rows }: { title?: string; rows: Row[] } = $props();
</script>

<div class="kv">
  {#if title}<h2>{title}</h2>{/if}
  <dl>
    {#each rows as [k, v] (k)}
      <dt>{k}</dt>
      <dd>{#if typeof v === "string"}{v}{:else}<a href={v.href}>{v.text}</a>{/if}</dd>
    {/each}
  </dl>
</div>
