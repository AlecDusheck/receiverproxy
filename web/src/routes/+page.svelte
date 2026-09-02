<script lang="ts">
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import { REPO } from "$lib/site";
</script>

<Head title="receiverproxy" description="Drive LED receiving cards over Ethernet without a sender card or vendor software. Panel configs, firmware, wall layout, live video." path="/" />

<TitleRow title="receiverproxy" />

<p>Drives a Colorlight E120 LED receiving card and its modules over raw Ethernet from a Mac or Linux machine: no sender card, no vendor software.</p>

<section>
  <h2>Install</h2>
  <pre>git clone {REPO}
cd receiverproxy
cargo install --path crates/cli
sudo chmod o+rw /dev/bpf*        # macOS
sudo setcap cap_net_raw,cap_net_admin+ep "$(which rxp)"   # Linux
rxp discover
rxp ui</pre>
</section>

<section>
  <h2>Pages</h2>
  <dl class="kv">
    <dt><a href="/panels">Panels</a></dt>
    <dd>panel specs and their generated files</dd>
    <dt><a href="/cards">Cards</a></dt>
    <dd>receiving-card models and firmware</dd>
    <dt><a href="/builder">Builder</a></dt>
    <dd>a spec as a form and as TOML; generate, inspect, diff</dd>
    <dt><a href="/wall">Wall</a></dt>
    <dd>the layout of several cards</dd>
    <dt><a href="/control">Control</a></dt>
    <dd>the card, through <code>rxp ui</code></dd>
    <dt><a href={REPO}>GitHub</a></dt>
    <dd>{REPO.replace("https://", "")}, MIT</dd>
  </dl>
</section>

<section>
  <h2>Tested</h2>
  <dl class="kv">
    <dt>card</dt>
    <dd>Colorlight E120, firmware 16.53</dd>
    <dt>panel</dt>
    <dd><a href="/panels/p25-128x64-sm16269s">P2.5 128x64 1/16 SM16269S</a></dd>
    <dt>host</dt>
    <dd>macOS</dd>
  </dl>
</section>

<style>
  dl.kv {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--s1) var(--s3);
    margin: 0;
  }
  dl.kv dd {
    margin: 0;
    font-family: var(--font);
    font-size: 13px;
  }
</style>
