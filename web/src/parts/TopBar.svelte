<script lang="ts">
  // 44 px: the project name, the pages as text links, the repository at the
  // right. On a narrow screen the links wrap to a second row.
  import { page } from "$app/state";
  import { REPO } from "$lib/site";

  const current = $derived(page.url.pathname.split("/")[1] || "home");
  const items: [string, string][] = [["panels", "Panels"], ["cards", "Cards"], ["builder", "Builder"], ["wall", "Wall"], ["control", "Control"]];
</script>

<header>
  <a class="brand" href="/" aria-current={current === "home" ? "page" : undefined}>receiverproxy</a>
  <nav>
    {#each items as [id, label] (id)}
      <a href="/{id}" class={{ active: current === id }} aria-current={current === id ? "page" : undefined}>{label}</a>
    {/each}
  </nav>
  <a class="repo" href={REPO}>GitHub</a>
</header>

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--s4);
    min-height: 44px;
    padding: 0 var(--s4);
    background: var(--bg-2);
    border-bottom: 1px solid var(--line);
  }
  a {
    color: var(--text);
    text-decoration: none;
    line-height: 44px;
  }
  .brand {
    font-weight: 600;
  }
  nav {
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--s3);
  }
  nav a.active {
    color: var(--accent);
    font-weight: 600;
  }
  .repo {
    margin-left: auto;
  }
</style>
