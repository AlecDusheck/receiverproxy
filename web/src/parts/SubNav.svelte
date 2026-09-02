<script lang="ts">
  // A row of text links under the title: sibling pages (the current one in
  // weight) or a page's sections (`#anchor` links).
  import { page } from "$app/state";
  let { links }: { links: [string, string][] } = $props();
  const current = $derived(page.url.pathname.replace(/\/$/, "") || "/");
</script>

<nav class="row">
  {#each links as [href, label] (href)}
    <a {href} class={{ active: href === current }} aria-current={href === current ? "page" : undefined}>{label}</a>
  {/each}
</nav>

<style>
  nav {
    gap: var(--s2) var(--s4);
    margin: calc(-1 * var(--s2)) 0 var(--s4);
  }
  a.active {
    color: var(--text);
    font-weight: 600;
    text-decoration: none;
  }
</style>
