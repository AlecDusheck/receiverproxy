<script lang="ts">
  // The screen's title row: name on the left, the primary action on the right.
  import type { Snippet } from "svelte";
  let { title, action }: { title: string; action?: Snippet } = $props();
</script>

<div class="title">
  <h1>{title}</h1>
  {#if action}<div class="row">{@render action()}</div>{/if}
</div>

<style>
  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--s2) var(--s3);
    min-height: 32px;
    margin-bottom: var(--s4);
  }
</style>
