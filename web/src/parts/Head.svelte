<script lang="ts">
  // The head of a page: title, description, canonical URL, Open Graph. `path`
  // is the route, `/panels/x`. A client-only route (no content without the
  // daemon or a file) passes `noindex` and gets no canonical.
  import { SITE, title as full } from "$lib/site";
  let { title, description = "", path = "", noindex = false }: { title: string; description?: string; path?: string; noindex?: boolean } = $props();
  // The home page is the site itself; every other route names itself first.
  const tab = $derived(path === "/" ? title : full(title));
</script>

<svelte:head>
  <title>{tab}</title>
  {#if noindex}
    <meta name="robots" content="noindex" />
  {:else}
    <meta name="description" content={description} />
    <link rel="canonical" href="{SITE}{path}" />
    <meta property="og:type" content="website" />
    <meta property="og:site_name" content="receiverproxy" />
    <meta property="og:title" content={tab} />
    <meta property="og:description" content={description} />
    <meta property="og:url" content="{SITE}{path}" />
    <meta property="og:image" content="{SITE}/og.png" />
    <meta name="twitter:card" content="summary_large_image" />
  {/if}
</svelte:head>
