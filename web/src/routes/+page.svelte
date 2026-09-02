<script lang="ts">
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import { REPO } from "$lib/site";
</script>

<Head title="receiverproxy" description="Receiving card configs (rcvbp) for LED panels, supported cards with firmware, wall layout, and a desktop daemon that drives the cards over Ethernet without a sender card." path="/" />

<TitleRow title="receiverproxy" />

<p>Sets up and drives LED receiving cards.</p>

<dl class="kv">
  <dt><a href="/panels">Panel configs</a></dt>
  <dd>ready-made receiving card configs (rcvbp) for the listed panels, downloadable or customizable in the browser, and an importer for your own vendor file</dd>
  <dt><a href="/cards">Receiver cards</a></dt>
  <dd>the supported cards with their limits, firmware downloads and tested panels</dd>
  <dt><a href="/wall">Wall</a></dt>
  <dd>lay out cards and panels into a screen and get each card's position</dd>
  <dt><a href="/control">Control</a></dt>
  <dd>with the desktop daemon (<code>rxp ui</code>) the site drives the cards directly over Ethernet, provisions them, installs firmware and shows video, with no sender card and no vendor software</dd>
</dl>

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

<style>
  dl.kv {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--s2) var(--s3);
    margin: 0 0 var(--s5);
    max-width: 72ch;
  }
  dl.kv dd {
    margin: 0;
    font-family: var(--font);
    font-size: 13px;
  }
  @media (max-width: 640px) {
    dl.kv {
      grid-template-columns: 1fr;
      gap: var(--s1);
    }
    dl.kv dd {
      margin-bottom: var(--s2);
    }
  }
</style>
