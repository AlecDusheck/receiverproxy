<script lang="ts">
  // The head of a prerendered page: title, description, canonical URL and
  // the Open Graph title and description. `path` is the route, `/gallery/x`.
  import { SITE, title as full } from "$lib/site";
  let { title, description, path }: { title: string; description: string; path: string } = $props();
  // The home page is the site itself; every other route names itself first.
  const tab = $derived(path === "/" ? title : full(title));
</script>

<svelte:head>
  <title>{tab}</title>
  <meta name="description" content={description} />
  <link rel="canonical" href="{SITE}{path}" />
  <meta property="og:type" content="website" />
  <meta property="og:site_name" content="receiverproxy" />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta property="og:url" content="{SITE}{path}" />
</svelte:head>
