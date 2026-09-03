<script lang="ts">
  // A "?" that shows one line of explanation, on hover or on click.
  let { text }: { text: string } = $props();
  let open = $state(false);
</script>

<span class="hint">
  <button type="button" title={text} aria-expanded={open} aria-label="what this is" onclick={() => (open = !open)}>?</button>
  {#if open}<span class="text">{text}</span>{/if}
</span>

<style>
  .hint { white-space: nowrap; }
  button {
    width: 16px;
    height: 16px;
    padding: 0;
    margin-left: 4px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--bg-2);
    color: var(--text-2);
    font-size: 11px;
    line-height: 1;
    cursor: help;
  }
  .text {
    display: block;
    margin-top: 4px;
    color: var(--text-2);
    font-size: 11px;
    white-space: normal;
  }
</style>
