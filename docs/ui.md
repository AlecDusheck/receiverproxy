# The web UI, the daemon and the WASM module

The contract for `web/`, `crates/daemon` and `crates/rcvbp-wasm`. Everything
else is built against the shapes here; a shape that changes changes here first.

## 1. Overview and the two modes

The UI is a SvelteKit site (Svelte 5, TypeScript, Tailwind 4) with the
prerendered **Panels** and **Cards** pages and the client-rendered
**Builder**, **Wall** and **Control** screens (the design is
[ui-design.md](ui-design.md); section 4 has the routes). It has two sources of function:

- **WASM** (`crates/rcvbp-wasm`): `rcvbp` and `wall` compiled to
  `wasm32-unknown-unknown`. Generates, inspects and diffs configurations and
  validates wall layouts in the browser. No hardware, no network.
- **The daemon** (`crates/daemon`, started by `rxp ui`): an HTTP server
  on `127.0.0.1:7120` that holds the raw Ethernet link and runs the CLI's
  command functions. Everything that touches the card goes through it, and
  every request carries the daemon's token.

On load the app requests `GET /api/v1/health` with a 1 s timeout, with the
token when it has one (section 2, "The token").

| | daemon absent (standalone) | daemon answers, no token (locked) | daemon present |
|---|---|---|---|
| Banner under the top bar | one line: "daemon not running: `cargo install --path crates/cli && rxp ui`" with "retry" and "dismiss" (under 640 px: "Control needs the desktop daemon: github.com/AlecDusheck/receiverproxy"); dismissed for the session, sessionStorage `rxp.install` | a token field and "connect"; "bad token" next to it when one was sent | nothing |
| Control pages | `/control` and its sub-pages show "daemon not running" and the install command | as absent | enabled; the selected card and the daemon's iface and version under the title |
| Wall | editor, table, import/export | as standalone | plus "provision" per receiver, "save as the daemon's wall", "show on the wall" |
| Panels, Builder | full, through WASM | full, through WASM | full, through WASM; a panel page gains "provision", the Builder "send to card" and "write to card" |

The probe runs once at load and again when the user clicks "retry" or
"connect" in the banner. Health answers `{ version }` alone without the
token; that answer is what tells the app it is locked. When served by the
daemon the API base is the page's own origin; a `VITE_RXP_API` build
variable overrides it for `pnpm dev`.

## 2. The JSON API

Base: `http://127.0.0.1:7120/api/v1` (`--port`, `--listen`). Request and
response bodies are JSON (`Content-Type: application/json`) unless a route
says multipart. Numbers are JSON numbers; bytes are base64 strings; paths are
strings as the daemon's process sees them (absolute, or relative to the
directory `rxp ui` was started in).

### The token

Every route except `GET /health` requires the daemon's token. `rxp ui`
generates one at start (32 random bytes, base64url) unless `--token TOKEN`
is given, and prints the URL `http://HOST:PORT/#token=TOKEN`, which it also
opens in the browser. A request presents the token in an `X-Token` header,
or as `?token=` in the query for `EventSource`, which cannot set headers.
Without it the answer is 401 `{"error":"token required"}`; with a wrong one
401 `{"error":"bad token"}`. `GET /health` answers `{ version }` without
the token and the full body with it, so the app can tell a daemon it is
locked out of from no daemon at all.

The built app reads `#token=` from the fragment once, stores it in
`sessionStorage` (`rxp.token`, one browser tab, gone when the tab closes),
removes it from the address bar with `history.replaceState`, and sends
`X-Token` on every request. A token typed into the banner's field is stored the same way.

**Network exposure**: the daemon binds `127.0.0.1` unless `--listen ADDR`
names another address (`0.0.0.0` for every interface); the printed URL uses
that address, or the first non-loopback IPv4 address when listening on
`0.0.0.0`. The token is the credential either way: on loopback it keeps
other pages open in the same browser from driving the panel and writing the
card's flash; on the network it keeps other machines out. **CORS** stays
open (`Access-Control-Allow-Origin: *`, all methods, headers `Content-Type,
X-Token`) because of it: an origin is not a credential, the token is. The
link is plain HTTP, so the token crosses the network in clear; use
`--listen` on networks you trust.

### Errors

Every non-2xx response is

```json
{"error": "provision: no response on en24 within 3s"}
```

`error` is the CLI's message verbatim: the anyhow chain rendered with `{:#}`
and the same command prefix `main.rs` adds (`provision`, `config write`,
`firmware install`...), without the leading `rxp: `. Status codes:

| code | when |
|---|---|
| 400 | body does not parse, a field is out of range, a layout fails `Canvas::validate` |
| 401 | token missing or wrong |
| 404 | unknown job id, unknown route |
| 409 | a hardware job is running (body names it: `{"error":"job j12 (provision) is running"}`) |
| 500 | the command returned an error |

### The commit gate

Every route that the CLI gates with `--commit` takes `"commit": boolean`.
With `commit: false` (the default) the daemon runs the command's dry run: the
same reads, the same plan lines, nothing written. The response or job result
carries the lines and `committed: false`. The UI always shows the dry run
first and asks before sending `commit: true`.

Gated: `config/write`, `provision`, `flash/restore`, `firmware/install`,
`PUT card/screen-size`. Not gated, because the CLI does not gate them:
`config/send` (RAM only), `card/reload`, `card/test-mode`, `card/set-layout`,
`brightness`, every `show/*`.

### Common shapes

The shapes below and every request body are Rust structs
(`crates/daemon/src/api.rs`, `jobs.rs`) from which `web/src/api/types.ts` is
generated (section 5, "Shared types"); the TypeScript here is the same
thing with comments.

```ts
// One receiving card, from colorlight::DiscoveryInfo.
type Card = {
  controller: number;   // receiver index on the chain
  card_id: number;      // the type byte, 0x64 for an E120, shown as hex
  model: string | null; // the config/cards model for card_id, null when none
  ver_major: number;    // 16
  ver_minor: number;    // 53
  cols: number;         // detected width
  rows: number;         // detected height
};

// What a command printed. `out` lines went to stdout in the CLI, `err` to
// stderr (progress, plans, warnings). Order preserved.
type Line = { stream: "out" | "err"; text: string };

// Result of a synchronous command.
type Outcome = {
  lines: Line[];
  files: string[];      // paths the command wrote (backups, dumps, generated files)
};

// Result of a gated command.
type GatedOutcome = Outcome & { committed: boolean };

// What a finished job produced.
type JobResult = GatedOutcome | Outcome;

// A long operation.
type Job = {
  id: string;           // "j" + counter, unique for the daemon's lifetime
  kind: "provision" | "firmware/install" | "flash/snapshot" | "flash/restore" | "show/video" | "show/hold";
  state: "running" | "done" | "failed" | "cancelled";
  started: string;      // RFC 3339
  finished: string | null;
  lines: Line[];        // everything so far
  error: string | null; // set when state is "failed"
  result: JobResult | null;  // set when state is "done"
};
```

A request field the routes below mark `?` may be left out; the daemon then
uses the default the route names.

### Routes

`GET /health` → `{ version: string, iface: string, cards: Card[] }` with
the token, `{ version: string }` without (never 401).
`version` is `daemon`'s `CARGO_PKG_VERSION`. `cards` is the last
discovery result; the daemon discovers once at startup (3 s) and on every
`POST /discover`. A failed discovery leaves `cards` as `[]`; the error is
logged and returned by the next `POST /discover`. Never opens the link
itself, so it is safe to poll.

`POST /discover` body `{ wait?: number }` (seconds, default 3) →
`{ cards: Card[] }`. Unlike `rxp discover`, no card is `{ "cards": [] }`
with 200, not an error. 409 while a job runs.

`GET /settings` → `{ iface: string, brightness: number, card: string | null }`.
`PUT /settings` body the same → the same.
`iface` applies to the next link opened. `brightness` (0-255) is the value
sent in sync frames by every following `show/*`. `card` names a model from
`config/cards/` (400 for an unknown name) and overrides the model the last
discovery gave; `null` follows discovery. Persisted in the daemon's settings
file (section 5).

`POST /brightness` body `{ value: number }` (0-255) → `{ value: number }`.
Sends the brightness and sync frames now (`rxp brightness`) and updates
`settings.brightness`.

`POST /show/image` → `Outcome`. Either JSON `{ path: string, fit?: Fit,
hold?: boolean }` or `multipart/form-data` with a `file` part and optional
`fit`, `hold` fields. `Fit` is `"stretch" | "contain" | "cover"`, default
`"stretch"` (what `rxp show image` does: `resize_exact`). `contain` and
`cover` are the `image` crate's `resize` and `resize_to_fill` with Lanczos3,
letterboxed in black; `sources::Fit` only applies to `VideoSource`. The
image is rendered onto the daemon's wall (`GET /wall`). `hold: false` sends three refreshes and returns;
`hold: true` starts a `show/hold` job that refreshes until cancelled or
replaced and the response is `{ id }` instead.

`POST /show/video` body `{ path: string, loop?: boolean, fps?: number,
fit?: Fit, layout?: Canvas }` → `{ id: string }` (job `show/video`).
Defaults: `loop` false, `fps` 30, `fit` `"contain"`, `layout` the daemon's
wall. `lines` carry the `N frames, F fps` progress line every 60 frames and
the final one.

`POST /show/pattern` body `{ name: "rgb" | "border" | "rows" | "gradient" | "white", hold?: boolean }` → as `show/image`.

`POST /show/fill` body `{ rgb: string, hold?: boolean }` → as `show/image`.
`rgb` is `RRGGBB`, `#` optional (`util::parse_color`).

`POST /show/blank` → `Outcome`. Three black refreshes on the wall.

`POST /config/gen` body `{ spec_toml: string }` → 

```ts
{
  name: string;                       // spec.name
  files: {
    rcvbp: string;                    // base64, <name>.rcvbp
    basic_pack: string;               // base64, 256 bytes
    block7: string | null;            // base64, 65536 bytes; null when the image could not be built
    sources_txt: string;              // the <name>-sources.txt text, not base64
  };
  sources: string[];                  // Generated.sources
  notes: string[];                    // Block7.notes, or the builder's error when block7 is null
}
```

No hardware. Chip library resolution is in section 5.

`POST /config/read` body `{ index?: number, page?: number, max_chunks?: number, wait?: number }` → `{ rcvbp: string, lines: Line[] }`. Base64 of the
file bytes `rxp config read` would save. Defaults as the CLI: index 0, page
the card model's parameter page, 64 chunks, 2 s. Read-only.

`POST /config/write` body `{ rcvbp: string, commit?: boolean, index?: number, wait?: number }` → `GatedOutcome`. The block backup goes to
`<data dir>/backups/block07-<unix seconds>.bin` and is listed in `files`.

`POST /config/send` body `{ spec_toml: string, chip_only?: boolean, gap_ms?: number }` → `Outcome`. RAM only, no gate, as `rxp config send`.

`POST /provision` body

```ts
{ spec_toml: string; firmware_path?: string;  // a config/firmware.toml name or a path
  position: [number, number];
  snapshot_dir?: string; commit?: boolean; wait?: number }
```

→ `{ id }` (job `provision`). The dry run is a job too because it discovers
the card. `snapshot_dir` defaults to `<data dir>/snapshots/<unix seconds>`.
The spec text is written to `<snapshot_dir>/spec.toml` first and the CLI
function runs on that file, so the sources report names a real path.

`POST /flash/snapshot` body `{ dir?: string, index?: number, wait?: number }` → `{ id }` (job `flash/snapshot`). `dir` defaults as above. Read-only.

`POST /flash/restore` body `{ dir: string, commit?: boolean, index?: number, wait?: number }` → `{ id }` (job `flash/restore`).

`POST /firmware/install` body `{ path: string, commit?: boolean, golden?: boolean, timeout?: number, chunk_delay_us?: number, wait?: number }` → `{ id }` (job `firmware/install`). Defaults as `rxp firmware install`; `path` is a `config/firmware.toml` name or a path, resolved and sha256-checked as the CLI does it.

`GET /card/screen-size?index=0&wait=3` → `{ width: number, height: number }`.
`PUT /card/screen-size` body `{ width: number, height: number, commit?: boolean, index?: number, wait?: number }` → `GatedOutcome & { width, height }` (the values read back).

`POST /card/reload` body `{ index?: number, full?: boolean }` → `Outcome`.

`POST /card/test-mode` body `{ n: number, index?: number }` → `Outcome`. `n` 0-255; 0 is off.

`POST /card/set-layout` body `{ panel_width: number, panel_height: number, index?: number }` → `Outcome`.

`GET /wall` → `Canvas` (section 4). Default when nothing is stored:
`Canvas::single(128, 64)`.
`PUT /wall` body `Canvas` → the stored `Canvas`. Validated with
`Canvas::validate`; a failure is 400 with the `LayoutError` text. Stored at
`<data dir>/wall.json`. Used by every `show/*` without an explicit `layout`.

`GET /jobs` → `Job[]`, newest first, at most the last 50.
`GET /jobs/{id}` → `Job`.
`DELETE /jobs/{id}` → `Job` (state `cancelled` once the worker has stopped; the call waits for that).
`GET /jobs/{id}/events` → `text/event-stream`:

```
event: line
data: {"stream":"err","text":"[1/5] snapshot: build/snapshot-1756771200"}

event: end
data: {"id":"j3","kind":"provision","state":"done", ...}   // the full Job
```

On connect the stream first replays every line already in `lines`, then
follows. `end` is sent exactly once, when the job leaves `running`, and the
stream closes. A comment line `: keepalive` goes out every 15 s.

### One link, one job

The daemon owns one `rawlink::Link` at a time. A job holds it from start to
end. While a job is `running`, every route that opens the link (`discover`,
`brightness`, `show/*`, `config/read`, `config/write`, `config/send`,
`provision`, `flash/*`, `firmware/*`, `card/*`) returns 409, with two
exceptions: a `show/*` request cancels a running `show/video` or `show/hold`
job and proceeds, and `DELETE /jobs/{id}` always works. `provision`,
`firmware/install`, `flash/*` are never cancelled implicitly. Cancellation is
polled between steps (`Progress::cancelled`, section 5); a flash write in
progress finishes its block before the job stops.

## 3. The WASM surface

`crates/rcvbp-wasm` is a `cdylib` (and an `rlib`, so `daemon --features ts`
can read its structs) over `rcvbp` and `wall` with
`wasm-bindgen` pinned to the installed CLI's version (`wasm-bindgen --version`
at the time of writing: 0.2.127; the crate and the CLI must match exactly).
`web/scripts/build-wasm.sh` emits `web/src/wasm/rcvbp_wasm.js` and
`rcvbp_wasm_bg.wasm` (`--target web`). Every function throws a JavaScript
`Error` whose `message` is the anyhow chain rendered with `{:#}`.

```ts
// web/src/wasm/rcvbp_wasm.d.ts (generated by wasm-bindgen, every result `any`);
// the shapes below are `crates/rcvbp-wasm/src/api.rs`, generated into
// web/src/api/types.ts, and what web/src/lib/wasm.ts types the module with.

export default function init(): Promise<void>;   // loads the .wasm; call once

export function gallery(): Entry[];
export function formats(): Format[];
export function generate(spec_toml: string, format: string): Generated;
export function import(bytes: Uint8Array, format?: string): Imported;   // the glue exports it as `_import`; lib/wasm.ts maps it back
export function inspect(rcvbp: Uint8Array): Inspection;
export function diff(a: Uint8Array, b: Uint8Array): Diff;
export function libraries(): Libraries;
export function validate_layout(json: string): string;   // "ok" or the LayoutError text
export function layout_example(cols: number, rows: number, w: number, h: number): string; // Canvas JSON

// One embedded panel spec (panelspec::embedded::specs), in embedding order.
type Entry = {
  path: string;                 // "config/panels/mined/icn2053.toml"
  name: string;                 // spec.name
  meta: Meta;                   // the spec's [meta] table, defaults filled in
  module: { width: number; height: number; scan: number };
  chip: { library: string; name: string; family_id: number };   // [chip].library, the library's name and family_id
  formats: string[];            // registry formats with generate: true
};
type Meta = {                   // panelspec::Meta
  pitch_mm?: number;
  status: "tested" | "generates";
  origin: "bench" | "mined";
  sources: number;              // vendor files the values came from
  agreement?: number;           // 0..1, share of the module class's files that agree
  examples: string[];           // a few source file names
  vendors: string[];
  notes?: string;
};

// rcvbp::Format, one per registered Codec (rcvbp::formats()); what
// `rxp config formats` prints.
type Format = {
  name: string;                 // "rcvbp", the value `generate` and `--format` take
  vendor: string;               // "Colorlight"
  extension: string;            // "rcvbp", without the dot
  generate: boolean;            // Codec::generate is implemented
  import: boolean;              // a file can be read back into a spec
};

type Generated = {
  name: string;
  files: { name: string; bytes: Uint8Array }[];   // <name>.<extension>, <name>-basic-pack.bin (256 bytes), and <name>-block7.bin (65536 bytes) when it builds
  sources: string[];            // Generated.sources
  notes: string[];              // Block7.notes, plus "pages written: N: ..."; the builder's error when block 7 is absent
};

type Imported = {
  spec_toml: string;            // PanelSpec::to_toml, what the Builder edits
  unresolved: string[];         // fields the file did not determine, by name
  format: string;               // the registry format read, or "spec" for a TOML spec passed through
};

type Inspection = {
  version: number;              // Rcvbp.version
  cabinet: [number, number] | null;   // Rcvbp::geometry (record 0x0aca)
  records: RecordInfo[];
};
type RecordInfo = {
  offset: number;               // Record.offset in the blob
  type: string;                 // "0x0a01"
  id: number;                   // low byte, 0x01
  length: number;               // payload bytes
  nonzero: number;              // non-zero payload bytes
  empty: boolean;               // Record::is_empty_table
  description: string;          // config.rs describe_record, "" when unknown
  fields: Record01 | null;      // decoded when id == 0x01 and length >= 764
};
type Record01 = {               // every rcvbp::record01::View accessor
  module_width: number; module_height_stored: number; scan: number;
  serial_clock: number; gray: number; luminance_level: number;
  max_width: number; max_height: number; grid: [number, number];
  line_dir: number; split_segment: number; segments: number; min_oe: number;
  hr_style: number; hr_scan_style: number; chip_id: number;
  swap_ramp: Uint8Array;        // 64 bytes at +0x19A
  chip_custom: Uint8Array;      // 16 bytes at +0x06A
};

type Diff = {
  a_records: number; b_records: number;
  only_a: string[]; only_b: string[];      // "0x0a84"
  records: { type: string; len_a: number; len_b: number; offsets: number[] }[];  // all differing offsets, not the CLI's first 16
};

type Libraries = { chips: LibraryChip[]; panels: LibraryPanel[] };
type LibraryChip = { path: string; name: string; toml: string };
type LibraryPanel = { path: string; name: string; toml: string; mined: boolean };
```

`libraries()` returns the files `panelspec::embedded` holds, built in from
`config/chips/**/*.toml` and `config/panels/**/*.toml`, `path` relative
to the repository root (`config/chips/mined/icn2053.toml`), `name` from the
file's `name =` field (chip libraries name themselves; a panel spec without
one uses the file stem), `mined` true under `config/panels/mined/`. Order:
non-mined first, then mined, each alphabetical by path.

`generate` resolves `[chip].library` against the embedded set by exact path;
a path not in the set is an error `chip library config/chips/x.toml: not in
the embedded library`. The Builder's chip picker only offers embedded paths.
`format` must name a registry entry; otherwise the error is `format x:
unknown; known formats: rcvbp`. The files are the ones `rxp config gen
--format` writes, named as it names them, so the two are byte-identical
(`generate_matches_the_cli_byte_for_byte`). `gallery()` parses every
embedded spec and its chip library; `formats()` is `rcvbp::formats()`, the
same table `rxp config formats` prints.

`import` is `rxp config import` in memory: `Codec::import` of the codec
named by `format`, or, without it, of the one whose signature the bytes
start with (`rcvbp::detect`; the error names the known formats). Chip
libraries are chosen by the file's chip id from the embedded set
(`panelspec::embedded::chip_by_family`). `unresolved` lists what the file
does not carry (`meta`, `mapping.gate_phantom_positions`, `boot.arm_at_boot`
for every `.rcvbp`) and whatever the regenerated file would still differ
in, by record and offset; a chip id with no embedded library leaves
`[chip].library` empty and says so. Bytes no codec recognises that parse
as a panel spec come back unchanged as format `spec`, so the Builder's
drop target takes a spec file too. The spec is named `<w>x<h>-<scan>s-<chip
library stem>`; the CLI names it after the file instead.

`inspect` accepts either `.rcvbp` form `Rcvbp::from_bytes` accepts
(compressed or the legacy inline stream).

## 4. The web app

A SvelteKit app (Svelte 5, TypeScript, Tailwind 4 through `@tailwindcss/vite`)
built twice from one source: `pnpm build` with `@sveltejs/adapter-cloudflare`
is the site at receiverproxy.com (`wrangler.jsonc`: worker `receiverproxy`,
the `ASSETS` binding, custom domains `receiverproxy.com` and
`www.receiverproxy.com`; `pnpm deploy` runs `wrangler deploy`), and
`pnpm build:embed` sets `ADAPTER=static` in `svelte.config.js` so
`@sveltejs/adapter-static` writes `web/build-static`, the copy `daemon`
embeds (section 6).

```
web/
  package.json  svelte.config.js  vite.config.ts  tsconfig.json  wrangler.jsonc
  scripts/build-wasm.sh
  src/
    app.html              the document: charset, viewport, color-scheme
    tokens.css            the colour tokens of ui-design.md, light and dark; the only file that writes a colour
    app.css               tokens.css, then Tailwind with the tokens as the theme (`@theme inline`: colours, the two font stacks, `--spacing: 4px`, the four type sizes; the default palette, sizes, radii and shadows cleared), then the component classes: controls, forms, tables, key-value blocks, drop target
    routes/
      +layout.svelte      top bar (parts/TopBar.svelte: Panels, Cards, Builder, Wall, Control, GitHub), the daemon banner (parts/Banner.svelte), content (960 px, the Wall unbounded), footer (package.json version); `onMount` reads the token from the address bar and starts the probe (`ops.start`)
      +page.svelte        `/`, prerendered: one sentence, the install commands, the pages, what is tested
      panels/             `/panels`, prerendered: +page.server.ts loads config/panels/**/*.toml through lib/server/config.ts and the cards whose [[tested]] name each spec; the table (module drawing, title from lib/panel.ts, status, formats, tested with), the filters (text, vendor, chip, scan, status) and the sort
      panels/[name]/      `/panels/<name>`, one prerendered page per spec (`entries`), sections Downloads, Module, Wiring, Timing, TOML: the download buttons (WASM, on click), open in Builder, provision (daemon), the spec as key-value blocks, the TOML
      gallery/, gallery/[name]/  the old addresses: prerendered 301 redirects to /panels (Cloudflare `_redirects`; a refresh page in the static build)
      cards/              `/cards`, prerendered from config/cards/*.toml: photo, model, vendor, family, id, limits, status, panels tested
      cards/[model]/      `/cards/<name>`, sections Photo and identity, Limits, Memory map, Tested panels, Firmware (the manifest as a table, each image name a download link)
      builder/            `/builder` (+layout.ts: `ssr = false` for the sub-pages too): +page.svelte (the two panes, generate, the card actions), BuilderForm.svelte; import/ (`/builder/import`: the drop target, the unresolved list, open in Builder); inspect/ (`/builder/inspect`: BuilderTools.svelte, inspect and diff)
      wall/               `/wall` (+layout.ts: `ssr = false`): +page.svelte (the drawing, WallCanvas.svelte, import, the daemon actions); layout/ (`/wall/layout`: WallTables.svelte, the same document as tables)
      control/            `/control` (+layout.ts: `ssr = false`): the discovered cards (a row selects `app.card`) and brightness; show/, provision/, firmware/, flash/, card/: one page per action group, each headed by parts/ControlHead.svelte (title, sibling links, the selected card; the install command without the daemon); job.ts (start and follow a job, the dry-run test)
      sitemap.xml/        +server.ts, prerendered: every prerendered page with lastmod the build date; the client-only routes are noindex and absent
      robots.txt/         +server.ts, prerendered
    api/
      types.ts            generated from the Rust structs (section 5, "Shared types"); never edited
      ops.ts              the one interface: `ops.pure` (WASM), `ops.card` (daemon, null when absent), `ops.start`, `ops.probe`, `ops.connect`
      daemon.ts           the transport: base URL, token, request/call ({error} handling), sse(jobId, onLine, onEnd); nothing runs at import
      mock.ts             the canned daemon behind `VITE_RXP_MOCK=1`
    lib/
      server/config.ts    the build-time loader: panels(), cards(), firmware() from the repository's config/ with smol-toml, the field names of the files; `FORMATS` is the codec registry by hand, pinned by tests/config.test.ts
      site.ts             the origin (canonical URLs, the sitemap), the repository URL, the title form `<route> · receiverproxy`
      token.ts            the token: splitFragment (pure), sessionStorage, loadToken(replace)
      wasm.ts             `ready(): Promise<WasmModule>`, loaded on the first call and only in the browser; the generated glue typed with api/types.ts, or a stub when it is not built; a build missing a function throws a rebuild message for it
      state.svelte.ts     the shared store (below); handSpec(toml) hands a spec to the Builder and the Control provision form (localStorage `rxp.builder.toml`)
      action.svelte.ts    Action<T>: one action's idle/busy/done/error state
      panel.ts            panelTitle(entry): "P2.5 128x64 1/16 SM16269S"; chipLabel(name)
      layout.ts           Canvas helpers: snap, bounds, addReceiver, addPanel, the JS validate and example
      error.ts            errText(e)
      download.ts         save(name, bytes | text) through a Blob URL
      spec.ts             PanelSpec <-> TOML (parse the [table] form the generator accepts; emit the same order as config/panels/*.toml; tables the form does not edit, [meta] among them, pass through)
    parts/
      Head.svelte (title, description, canonical, Open Graph, og:image /og.png; `noindex` for the client-only routes)  TopBar.svelte  Banner.svelte  TitleRow.svelte (title, primary action)  SubNav.svelte (a row of text links: sibling pages or a page's sections)  ControlHead.svelte
      Module.svelte (a module as a dot grid in the line token, at its aspect)  JobLines.svelte (a job's lines as they arrive, cancel, the final state)
      Field.svelte  KeyValue.svelte  Drop.svelte  Hex.svelte  Lines.svelte
  src/wasm/               generated, gitignored
  tests/token.test.ts     node --test: the fragment handling of token.ts
  tests/config.test.ts    node --test: the loader against config/ and the crate's format list
  .svelte-kit/cloudflare/ pnpm build output, gitignored, what wrangler deploys
  build-static/           pnpm build:embed output, gitignored, embedded by daemon when present
```

`$lib` is `src/lib`, `$api` is `src/api`, `$parts` is `src/parts`
(`kit.alias`). Routes and parts call `api/ops.ts` and nothing else in
`api/` or `lib/wasm.ts`. `ops.pure` is always there (its functions load the
WASM module on first use; `validateLayout` and `layoutExample` use the JS
forms in `lib/layout.ts` until it is loaded). `ops.card` is the daemon's
operations while `app.daemon` is `"present"` and `null` otherwise, so a
route that needs the card tests `ops.card` and hides the control when it is
null.

The prerendered routes (`/`, `/panels`, `/panels/<name>`, `/cards`,
`/cards/<name>`, `sitemap.xml`, `robots.txt`, and the `/gallery` redirects)
set `prerender = true` and load their data in `+page.server.ts` from
`config/` at build time; no WASM runs at build. Each sets its title (`<subject>
· receiverproxy`, at most 60 characters), a description of at most 155
characters, canonical URL and Open Graph title, description and image
through `parts/Head.svelte`; a panel page's title is `<panelTitle> panel`.
`/builder`, `/wall`, `/control` and their sub-pages set `ssr = false` and
`prerender = false` in a `+layout.ts`, render on the client, where the WASM
module and the daemon are, and carry `<meta name="robots" content="noindex">`.
`/builder?panel=<path>` opens a library spec; `/control/provision?provision=<index>`
selects a receiver and sets `position` from its `x,y` (the Wall's
"provision" link). A panel page's "open in Builder" and "provision", and
`/builder/import`, hand the spec over through `handSpec` and `goto`.

### Shared state (`state.svelte.ts`)

One module of `$state` runes, imported by every screen:

```ts
daemon:   "probing" | "absent" | "locked" | "present";   // locked: health answered without iface and cards
tokenError: string;               // "bad token" when a token was sent and health stayed minimal
health:   Health | null;          // the last full GET /health
settings: { iface: string; brightness: number; card: string | null } | null;
wall:     Canvas;                 // the editor's document; loaded from GET /wall when present, else localStorage "rxp.wall", else single 128x64
job:      Job | null;             // the job last started from a page; its lines arrive over SSE
card:     number;                 // the receiver index the Control pages act on
wasm:     "unloaded" | "loading" | "ready" | "failed";
install:  boolean;                // the install banner is visible (dismissed for the session)
```

An error is shown next to the control that caused it, verbatim. Starting a
job sets `job` and opens its SSE; `parts/JobLines.svelte` shows the lines as
they arrive and a cancel button where the action was, then the final state.

### The layout JSON (`wall`)

The Wall edits exactly the structure `rxp show ... --layout` reads, serde
names as written:

```ts
type Rotation = "none" | "cw90" | "ccw90" | "rot180";
type Receiver = { index: number; x?: number; y?: number; width: number; height: number };  // x, y default 0
type Panel = {
  receiver: number;               // Receiver.index this panel hangs off
  receiver_x?: number; receiver_y?: number;   // where on the receiver's window, default 0
  x: number; y: number;           // the source rectangle in the screen image
  width: number; height: number;  // the panel's own pixels, before rotation
  rotation?: Rotation;            // default "none"
  flip_x?: boolean; flip_y?: boolean;   // default false
};
type Canvas = { width: number; height: number; receivers: Receiver[]; panels: Panel[] };
```

A receiver's `x,y` is the card's `provision --position`; the card keeps the
window `x,y .. x+width,y+height` of every frame. `Canvas::validate` rejects a
receiver or panel past the canvas, a panel naming an undefined receiver, and
a panel whose rotated size (`width x height`, swapped for `cw90`/`ccw90`)
does not fit at `receiver_x, receiver_y` inside its receiver. The Wall calls
`validate_layout` on every change and shows the text under the canvas. The
editor snaps positions to the smallest panel size in the document, or 16 px
when there is none. `layout_example(cols, rows, w, h)` is
`Canvas::cards(w, h, cols, rows)`: one receiver per panel, receivers at the
panel's `x,y`.

Import/export is the JSON above, pretty-printed as `rxp card layout-example`
prints it. When the daemon is present "save" is `PUT /wall`; export is always
a file download.

## 5. The daemon crate and the CLI refactor

### `crates/ops`: the commands as functions

`cli` cannot both provide the command library and depend on
`daemon` (cargo rejects the package cycle), so the command modules
live in their own crate, `ops`. `cli` keeps clap, `main.rs`
and the `Stdio` sink; its output stays byte for byte the same (checked by
running every offline command against the previous binary). `daemon`
depends on `ops`; `cli` depends on both.

```rust
/// The former global flags (iface, width, height, order, brightness).
pub struct Ctx { pub iface: String, pub width: u16, pub height: u16, pub order: ColorOrder, pub brightness: u8 }

/// Where a command's lines go. The CLI's `Stdio` prints `out` with println!
/// and `err` with eprintln!; the daemon's sink appends to the job.
pub trait Progress {
    fn out(&mut self, line: &str);
    fn err(&mut self, line: &str);
    /// A line that replaces the previous transient one (`show video`'s
    /// `N frames, F fps`). Default: `err`. `Stdio` redraws it with `\r`
    /// when stderr is a terminal and drops it otherwise.
    fn transient(&mut self, line: &str) { self.err(line) }
    fn clear_transient(&mut self) {}
    /// True once the caller wants the command to stop; polled between steps.
    fn cancelled(&self) -> bool { false }
}
pub struct Stdio;

/// Maps a spec's `[chip].library` path to the library's TOML text.
pub type Loader<'a> = &'a dyn Fn(&str) -> Result<String>;
/// The CLI's loader: the file, relative to the working directory.
pub fn read_library(path: &str) -> Result<String>;
/// `bail!("cancelled")` once `p.cancelled()`.
pub fn check(p: &dyn Progress) -> Result<()>;
```

Each command function takes `&Ctx` and, when it prints or polls
cancellation, `&mut dyn Progress`; every `println!`/`eprintln!` became
`p.out(..)`/`p.err(..)`. `util::warn` and `util::hexdump` take the sink.
Commands that only send frames (`display::brightness`, `screen::reload`,
`screen::test_mode`, `screen::set_layout`, `params::send_params`,
`display::probe`) take no sink. Functions the daemon needs a value from
return it; they still emit the CLI's lines through the sink, so the CLI
wrappers print nothing extra:

| function | returns | notes |
|---|---|---|
| `capture::discover` | `Vec<DiscoveryInfo>` | one `p.out` line per card as it answers; empty is not an error (the CLI adds the "no response" one) |
| `config::generate(&spec, label, load)` | `GenOutputs { name, rcvbp, basic_pack, block7: Option<Vec<u8>>, sources, notes, report, paths }` | in memory, no files; a block-7 build failure is `block7: None` with the error as the last note |
| `config::gen_config(path, out_dir, load, p)` | the same, `paths` filled | the CLI's four files; fails as before when block 7 cannot be built |
| `flash::read_config` | `Vec<u8>` (file bytes) | `flash::save_config(bytes, out, p)` writes, prints the path and warns, for the CLI |
| `screen::screen_size` | `(u16, u16)` read back | prints `WxH` or the dry-run line itself |
| `provision::provision(ctx, &provision::Args {..}, load, p)` | `()` | the six former parameters as a struct |
| `display::play_on`, `show_pattern_on`, `show_solid_on`, `show_frame` | `()` | take a `Canvas` (the daemon's wall); the `_on`-less forms load `--layout` or the single panel |
| `display::image_frame(&img, &canvas, fit)` | `Frame` | stretch / contain (letterboxed) / cover, Lanczos3 |
| `params::send_generated(ctx, &spec, &g, ..)` | `()` | `send_params` for a spec already generated |

`display::show_frame` polls `p.cancelled()` in its `hold` loop, `play_on`
per frame, `provision` between its five steps, `upgrade::install` per chunk
and per poll, `flash::read_blocks` (hence `restore::snapshot`) per block.
`panelspec::PanelSpec` has `parse(text: &str)`, `load(path)` and
`chip_library(&self, load)`; `rcvbp::spec::generate(&spec, &chip)` builds
the records and `rcvbp::image::compile(&model.memory.boot_image, ..)` the
block-7 image `rxp config gen` writes, laid out for a `receivers`
card model (the daemon's `card` setting, the discovered card, else
`receivers::default_model()`). `ChipLibrary::parse(text)` likewise. The loader is passed by
the caller: the CLI passes `read_library`, the WASM crate
`panelspec::embedded`, the daemon "`panelspec::embedded`, then the
filesystem relative to the working directory" (`daemon::state::load_library`).

### `crates/daemon`

axum (`http1`, `json`, `multipart`, `query`, `tokio` features only), tokio
(`rt-multi-thread`, `macros`, `sync`, `time`), tokio-stream, tower-http
`cors`, include_dir, serde, base64, dirs, mime_guess.

```
src/lib.rs        pub fn run(opts: Options) -> Result<()>; Options { port, listen: Ipv4Addr, open, token: Option<String>, iface: Option<String>, data_dir: Option<PathBuf> }; the random token; pub fn router(state)
src/ifaces.rs     first_non_loopback_v4 (getifaddrs), the host in the printed URL when listening on 0.0.0.0
src/state.rs      AppState: settings, wall, cards, jobs, the link holder (a job or a command's subject, for the 409 text), data dir, token; command() and start_job(); load_library
src/routes.rs     one handler per route in section 2; the token layer (X-Token or ?token=) on every route but health; the commit gate; Body/Qs extractors that turn a bad body into 400 {"error"}
src/jobs.rs       Job, Handle (lines + broadcast + done watch + cancel flag), Sink (impl Progress), Lines (a command's sink), spawn_blocking runner, SSE
src/assets.rs     include_dir!("$CARGO_MANIFEST_DIR/../../web/build-static") behind build.rs's cfg(web_dist); every non-/api path: `<path>.html` for a prerendered route (`index.html` for `/`), else fallback.html
src/store.rs      settings.json and wall.json under the data dir
src/error.rs      ApiError { status, message } -> {"error": message}
tests/api.rs      the router without a link: health with and without the token, 401 on the other routes, config/gen against gen_config, the commit gate (flash/restore dry run), 409 with a fake job, CORS, wall
```

The link holder is a `Mutex<Option<Holder>>`; a command takes it for its
duration on the blocking pool, a job from start to finish, and the 409 body
names the holder (`job j3 (provision) is running`, `discover is running`).
A `show/*` route cancels a running `show/video` or `show/hold` job and
waits for it before taking the link. Job ids are handed out only once the
link is free, so a 409 does not consume one.

Data dir: `dirs::config_dir()/receiverproxy` (`~/Library/Application Support/receiverproxy`
on macOS, `~/.config/receiverproxy` on Linux); `--data-dir` overrides. Holds
`settings.json`, `wall.json`, `backups/`, `snapshots/`. `rxp firmware fetch`
writes its `firmware/` cache under the same directory, `--data-dir` or not.

Static files: when `web/build-static/index.html` exists at compile time the
whole directory is embedded and served at `/` with the right MIME types: a
prerendered route from `<path>.html`, any other non-API path from
`fallback.html`, the client-rendered shell. Otherwise `/` returns `text/plain` `build the web app: cd web
&& pnpm install && pnpm build:embed, then rebuild rxp`. A rebuild of
`daemon` is needed after `pnpm build:embed` (`build.rs` emits
`rerun-if-changed=../../web/build-static`).

`rxp ui [--port 7120] [--listen 127.0.0.1] [--no-open] [--token TOKEN]
[--data-dir DIR]` in `cli` builds `Options` and calls `daemon::run`, which
owns its tokio runtime; `--no-open` skips opening the browser. `--iface`
typed on the command line replaces the saved `settings.iface`; otherwise
the saved one (default `en24`) applies. It prints one line: `rxp ui:
http://127.0.0.1:7120/#token=...` (the token given, or the generated one),
then discovers for 3 s before serving.

## 6. Build and run

```sh
# once
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127    # the version crates/rcvbp-wasm/Cargo.toml pins for the wasm-bindgen crate
cd web && pnpm install

# the wasm module (rerun after changes to rcvbp, wall, rcvbp-wasm, config/)
web/scripts/build-wasm.sh
#   cargo build -p rcvbp-wasm --release --target wasm32-unknown-unknown
#   wasm-bindgen --target web --out-dir web/src/wasm target/wasm32-unknown-unknown/release/rcvbp_wasm.wasm

# development: the app at http://localhost:5173, API from a running daemon
cd web && pnpm dev                                      # API base: same origin as the page (proxied to 7120 by vite.config.ts)
VITE_RXP_API=http://127.0.0.1:7121/api/v1 pnpm dev     # or an explicit base

# the site: adapter-cloudflare into web/.svelte-kit/cloudflare, deployed by wrangler
cd web && pnpm build
pnpm deploy                  # wrangler deploy, web/wrangler.jsonc: receiverproxy.com and www.receiverproxy.com

# the embedded copy: adapter-static into web/build-static, then the daemon
cd web && pnpm build:embed   # ADAPTER=static vite build
cargo build --release -p cli   # embeds web/build-static
cargo install --path crates/cli
rxp ui                       # prints and opens http://127.0.0.1:7120/#token=<random>
rxp ui --iface en24 --port 7120 --no-open --token secret
rxp ui --listen 0.0.0.0      # reachable from the network; the token is the credential
```

The daemon serves the static build's files at `/`: a prerendered route
from `<path>.html`, everything else that is not `/api` from `fallback.html`,
the client-rendered fallback. `pnpm check` runs `svelte-kit sync` and
`svelte-check`; `pnpm test` runs the node tests under `web/tests/`;
`cargo build --workspace && cargo test --workspace && cargo clippy
--workspace --all-targets -- -D warnings` covers the three Rust crates as
for the rest of the workspace. `rcvbp-wasm` is a workspace member and must
also pass `cargo clippy -p rcvbp-wasm --target wasm32-unknown-unknown -- -D
warnings`.

### Shared types

`web/src/api/types.ts` is generated, not written: every request and
response struct in `crates/daemon/src/api.rs` and `jobs.rs`, the WASM
result structs in `crates/rcvbp-wasm/src/api.rs`, `wall`'s layout types
and `sources::{Fit, Pattern}` derive `ts_rs::TS` behind a `ts` feature in
each crate, and

```sh
cargo test -p daemon --features ts
```

writes them, one `export type` each in the order of this document. The
test compares what it renders with the committed file: when they differ it
rewrites the file and fails, so a struct change is a two-step edit (run,
commit). Optional request fields are `Option<T>` in Rust and `t?: T` in
TypeScript; `u64` fields are `number`; the WASM byte fields are
`Uint8Array`. The web app and `lib/wasm.ts` import these types; there is
no hand-written copy.

## 7. Design rules for the UI

The design is [ui-design.md](ui-design.md): principles, layout, type,
spacing, the colour tokens, components, states and the review checklist.
What follows is how the code meets it.

**Layout.** `+layout.svelte`: the 44 px top bar (`parts/TopBar.svelte`,
links wrap on a narrow screen), the daemon banner (`parts/Banner.svelte`,
only when it applies), the content 960 px wide (the Wall's drawing scales
to the width it has), the footer. Every screen starts with
`parts/TitleRow.svelte` (title, primary action) and, where it has sibling
pages or sections, `parts/SubNav.svelte`. Under 640 px forms are one column
(`.form`), every table scrolls inside `.scroll`, and the banner shows the
repository instead of the install command.

**Spacing and type.** `app.css` holds the scale as `--s1: 4px` to `--s5:
24px`, the font stacks as `--font` and `--mono`, 32 px controls, 28 px table
rows, and the form as `parts/Field.svelte`: label above the control, the
range or unit in the caption, the message in `--err` under it.

**Colour.** `tokens.css` defines the nine tokens of ui-design.md for both
schemes (plus `--accent-text`, the primary button's text) and nothing else
writes a colour; components use `var(--token)`. The Wall drawing reads the
tokens from its element's computed style: receivers as 1 px `--line` boxes
with `card N x,y` at top left, panels filled `--bg-2` with a 1 px `--text`
border, the selected one `--accent`, rotation as a small arrow, flips as a
mirrored arrow, the grid at 1 px `--line` at 50 % opacity.

**States.** `lib/action.svelte.ts` gives every action the four states of
ui-design.md: `run` sets busy, done keeps the result where the action was,
error keeps the message verbatim under the action. Controls are `disabled`
while their action is busy, the label unchanged. A job's progress is its
lines, shown where it was started (`parts/JobLines.svelte`).

Confirmation before a commit is one line above a second button, `commit`,
that appears only after a dry run finished, under its plan; not a dialog.

Downloads are named by the module (`Generated.files`) and `wall.json`.
