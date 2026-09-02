<script lang="ts">
  import Head from "$parts/Head.svelte";
  import TitleRow from "$parts/TitleRow.svelte";
  import { REPO } from "$lib/site";
</script>

<Head
  title="receiverproxy"
  description="A command-line tool and a web app that drive a Colorlight E120 LED receiving card over raw Ethernet, without a sender card or vendor software."
  path="/"
/>

<TitleRow title="receiverproxy" />

<p>
  receiverproxy is a command-line tool, <code>rxp</code>, and this web app; they drive a Colorlight E120 LED receiving card and its modules over raw Ethernet, generate and flash the module configuration, and put images, video and live streams on the panel.
</p>
<p>The card is driven from a Mac or Linux machine with an Ethernet port, without a sender card, vendor software or an account.</p>

<section>
  <h2>Install</h2>
  <pre>git clone {REPO}
cd receiverproxy
cargo install --path crates/cli
rxp discover</pre>
  <p class="caption">Rust stable through rustup; raw Ethernet needs privileges on the interface (<code>sudo chmod o+rw /dev/bpf*</code> on macOS, <code>setcap cap_net_raw,cap_net_admin+ep</code> on Linux). <code>rxp ui</code> serves this app with the card actions.</p>
</section>

<section>
  <h2>Pages</h2>
  <dl class="kv">
    <dt><a href="/gallery">Gallery</a></dt>
    <dd>every panel spec under config/panels, with its generated files</dd>
    <dt><a href="/cards">Cards</a></dt>
    <dd>the receiving-card models under config/cards and the firmware manifest</dd>
    <dt><a href="/builder">Builder</a></dt>
    <dd>a panel spec as a form and as TOML; generate, inspect and diff files in the browser</dd>
    <dt><a href="/wall">Wall</a></dt>
    <dd>the layout JSON for several cards, as a drawing and as tables</dd>
    <dt><a href={REPO}>Source</a></dt>
    <dd>{REPO.replace("https://", "")}, MIT</dd>
  </dl>
</section>

<section>
  <h2>Tested</h2>
  <p>One Colorlight E120 on firmware 16.53 driving one P2.5 128x64 module with SM16269S drivers, on macOS. Every other card and module in the tables is a vendor default, not a measurement.</p>
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
