# The web UI, the daemon and the WASM module

The contract for `web/`, `crates/e120-server` and `crates/e120-wasm`. Everything
else is built against the shapes here; a shape that changes changes here first.

## 1. Overview and the two modes

The UI is a static site (Svelte 5, Vite, TypeScript, hand-written CSS) with
four screens: **Cards**, **Wall**, **Builder**, **Library**. It has two sources
of function:

- **WASM** (`crates/e120-wasm`): `e120-rcvbp` and `e120-canvas` compiled to
  `wasm32-unknown-unknown`. Generates, inspects and diffs configurations and
  validates wall layouts in the browser. No hardware, no network.
- **The daemon** (`crates/e120-server`, started by `e120 ui`): an HTTP server
  on `127.0.0.1:7120` that holds the raw Ethernet link and runs the CLI's
  command functions. Everything that touches the card goes through it.

On load the app requests `GET http://127.0.0.1:7120/api/v1/health` with a
1 s timeout.

| | daemon absent (standalone) | daemon present |
|---|---|---|
| Banner | shown: "The e120 daemon is not running. Install with `cargo install --path crates/e120-cli`, then run `e120 ui`." Dismissible per session. | not shown |
| Cards screen | hidden from the sidebar | enabled |
| Wall screen | editor, table, import/export | plus "provision this card" per receiver, "save as the daemon's wall" |
| Builder, Library | full, through WASM | full, through WASM; Builder gains "send to card" and "write to card" |
| Status bar | "standalone" | "iface en24 · 1 card: E120 16.53 128x64" and the running job |

The probe runs once at load and again when the user clicks "retry" in the
banner. When served by the daemon the API base is the page's own origin; a
`VITE_E120_API` build variable overrides it for `pnpm dev`.

## 2. The JSON API

Base: `http://127.0.0.1:7120/api/v1`. Bound to loopback only. Request and
response bodies are JSON (`Content-Type: application/json`) unless a route
says multipart. Numbers are JSON numbers; bytes are base64 strings; paths are
strings as the daemon's process sees them (absolute, or relative to the
directory `e120 ui` was started in).

**CORS**: `Access-Control-Allow-Origin: *`, all methods, headers
`Content-Type, X-Token`. The daemon listens on loopback only, but any page
open in the browser can reach loopback, so any page can drive the panel and
write its flash while the daemon runs. Starting with `--token TOKEN` requires
every request to carry `X-Token: TOKEN` (401 `{"error":"token required"}` or
`{"error":"bad token"}` otherwise). The built app reads the token from the
URL fragment `#token=...` once and keeps it in memory.

### Errors

Every non-2xx response is

```json
{"error": "provision: no response on en24 within 3s"}
```

`error` is the CLI's message verbatim: the anyhow chain rendered with `{:#}`
and the same command prefix `main.rs` adds (`provision`, `config write`,
`firmware install`...), without the leading `e120: `. Status codes:

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

```ts
// One receiving card, from e120_proto::DiscoveryInfo.
type Card = {
  controller: number;   // receiver index on the chain
  card_id: number;      // e.g. 0x03 for an E120, shown as hex
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

// A long operation.
type Job = {
  id: string;           // "j" + counter, unique for the daemon's lifetime
  kind: "provision" | "firmware/install" | "flash/snapshot" | "flash/restore" | "show/video" | "show/hold";
  state: "running" | "done" | "failed" | "cancelled";
  started: string;      // RFC 3339
  finished: string | null;
  lines: Line[];        // everything so far
  error: string | null; // set when state is "failed"
  result: GatedOutcome | Outcome | null;  // set when state is "done"
};
```

### Routes

`GET /health` → `{ version: string, iface: string, cards: Card[] }`.
`version` is `e120-server`'s `CARGO_PKG_VERSION`. `cards` is the last
discovery result; the daemon discovers once at startup (3 s) and on every
`POST /discover`. A failed discovery leaves `cards` as `[]`; the error is
logged and returned by the next `POST /discover`. Never opens the link
itself, so it is safe to poll.

`POST /discover` body `{ wait?: number }` (seconds, default 3) →
`{ cards: Card[] }`. Unlike `e120 discover`, no card is `{ "cards": [] }`
with 200, not an error. 409 while a job runs.

`GET /settings` → `{ iface: string, brightness: number }`.
`PUT /settings` body `{ iface: string, brightness: number }` → the same.
`iface` applies to the next link opened. `brightness` (0-255) is the value
sent in sync frames by every following `show/*`. Persisted in the daemon's
settings file (section 5).

`POST /brightness` body `{ value: number }` (0-255) → `{ value: number }`.
Sends the brightness and sync frames now (`e120 brightness`) and updates
`settings.brightness`.

`POST /show/image` → `Outcome`. Either JSON `{ path: string, fit?: Fit,
hold?: boolean }` or `multipart/form-data` with a `file` part and optional
`fit`, `hold` fields. `Fit` is `"stretch" | "contain" | "cover"`, default
`"stretch"` (what `e120 show image` does: `resize_exact`). `contain` and
`cover` are the `image` crate's `resize` and `resize_to_fill` with Lanczos3,
letterboxed in black; `e120_video::Fit` only applies to `VideoSource`. The
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
file bytes `e120 config read` would save. Defaults as the CLI: index 0, page
`FLASH_PAGE_BASIC_PARAM`, 64 chunks, 2 s. Read-only.

`POST /config/write` body `{ rcvbp: string, commit?: boolean, index?: number, wait?: number }` → `GatedOutcome`. The block backup goes to
`<data dir>/backups/block07-<unix seconds>.bin` and is listed in `files`.

`POST /config/send` body `{ spec_toml: string, chip_only?: boolean, gap_ms?: number }` → `Outcome`. RAM only, no gate, as `e120 config send`.

`POST /provision` body

```ts
{ spec_toml: string; firmware_path?: string; position: [number, number];
  snapshot_dir?: string; commit?: boolean; wait?: number }
```

→ `{ id }` (job `provision`). The dry run is a job too because it discovers
the card. `snapshot_dir` defaults to `<data dir>/snapshots/<unix seconds>`.
The spec text is written to `<snapshot_dir>/spec.toml` first and the CLI
function runs on that file, so the sources report names a real path.

`POST /flash/snapshot` body `{ dir?: string, index?: number, wait?: number }` → `{ id }` (job `flash/snapshot`). `dir` defaults as above. Read-only.

`POST /flash/restore` body `{ dir: string, commit?: boolean, index?: number, wait?: number }` → `{ id }` (job `flash/restore`).

`POST /firmware/install` body `{ path: string, commit?: boolean, golden?: boolean, timeout?: number, chunk_delay_us?: number, wait?: number }` → `{ id }` (job `firmware/install`). Defaults as `e120 firmware install`.

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

The daemon owns one `e120_net::Link` at a time. A job holds it from start to
end. While a job is `running`, every route that opens the link (`discover`,
`brightness`, `show/*`, `config/read`, `config/write`, `config/send`,
`provision`, `flash/*`, `firmware/*`, `card/*`) returns 409, with two
exceptions: a `show/*` request cancels a running `show/video` or `show/hold`
job and proceeds, and `DELETE /jobs/{id}` always works. `provision`,
`firmware/install`, `flash/*` are never cancelled implicitly. Cancellation is
polled between steps (`Progress::cancelled`, section 5); a flash write in
progress finishes its block before the job stops.

## 3. The WASM surface

`crates/e120-wasm` is a `cdylib` over `e120-rcvbp` and `e120-canvas` with
`wasm-bindgen` pinned to the installed CLI's version (`wasm-bindgen --version`
at the time of writing: 0.2.127; the crate and the CLI must match exactly).
`web/scripts/build-wasm.sh` emits `web/src/wasm/e120_wasm.js` and
`e120_wasm_bg.wasm` (`--target web`). Every function throws a JavaScript
`Error` whose `message` is the anyhow chain rendered with `{:#}`.

```ts
// web/src/wasm/e120_wasm.d.ts (generated) plus the shapes below in web/src/lib/types.ts

export default function init(): Promise<void>;   // loads the .wasm; call once

export function generate(spec_toml: string): Generated;
export function inspect(rcvbp: Uint8Array): Inspection;
export function diff(a: Uint8Array, b: Uint8Array): Diff;
export function libraries(): Libraries;
export function validate_layout(json: string): string;   // "ok" or the LayoutError text
export function layout_example(cols: number, rows: number, w: number, h: number): string; // Canvas JSON

type Generated = {
  name: string;
  rcvbp: Uint8Array;            // Rcvbp::to_file_bytes
  basic_pack: Uint8Array;       // 256 bytes
  block7: Uint8Array | null;    // 65536 bytes, or null with the reason in notes
  sources: string[];            // Generated.sources
  notes: string[];              // Block7.notes, plus "pages written: N: ..."
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
type Record01 = {               // every e120_rcvbp::record01::View accessor
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

type Libraries = {
  chips:  { path: string; name: string; toml: string }[];
  panels: { path: string; name: string; toml: string; mined: boolean }[];
};
```

`libraries()` returns the files embedded at build time with `include_dir`
from `config/chips/**/*.toml` and `config/panels/**/*.toml`, `path` relative
to the repository root (`config/chips/mined/icn2053.toml`), `name` from the
file's `name =` field (chip libraries name themselves; a panel spec without
one uses the file stem), `mined` true under `config/panels/mined/`. Order:
non-mined first, then mined, each alphabetical by path.

`generate` resolves `[chip].library` against the embedded set by exact path;
a path not in the set is an error `chip library config/chips/x.toml: not in
the embedded library`. The Builder's chip picker only offers embedded paths.

`inspect` accepts either `.rcvbp` form `Rcvbp::from_bytes` accepts
(compressed or the legacy inline stream).

## 4. The web app

```
web/
  package.json  vite.config.ts  tsconfig.json  index.html
  scripts/build-wasm.sh
  src/
    main.ts               mounts App, starts the daemon probe and the wasm load in parallel
    App.svelte            sidebar + content + status bar; hash router
    app.css               tokens, reset, form controls, layout
    lib/
      api.ts              fetch wrapper (base URL, token, {error} handling), sse(jobId, onLine, onEnd)
      wasm.ts             `ready: Promise<typeof import("../wasm/e120_wasm")>`; import of the generated module
      state.svelte.ts     the shared store (below)
      types.ts            Card, Line, Job, Canvas, Panel, Receiver, Generated, Inspection, Diff, Libraries
      layout.ts           Canvas helpers: snap, bounds, addReceiver, addPanel, validate (wasm)
      download.ts         save(name, bytes | text) through a Blob URL
      spec.ts             PanelSpec <-> TOML (parse the [table] form the generator accepts; emit the same order as config/panels/*.toml)
    parts/
      Sidebar.svelte  StatusBar.svelte  Banner.svelte  Field.svelte  Hex.svelte
    screens/
      Cards.svelte  Wall.svelte  Builder.svelte  Library.svelte
  src/wasm/               generated, gitignored
  dist/                   pnpm build output, gitignored, embedded by e120-server when present
```

Routes are hash fragments: `#/cards`, `#/wall`, `#/builder`, `#/library`.
The default is `#/cards` when the daemon answered, else `#/builder`.
`#/builder?panel=<path>` opens a library spec; `#/cards?provision=<index>`
opens the provision form for a receiver (the Wall's "provision this card"
sets `position` from the receiver's `x,y`).

### Shared state (`state.svelte.ts`)

One module of `$state` runes, imported by every screen:

```ts
daemon:   "probing" | "absent" | "present";
health:   Health | null;          // the last GET /health
settings: { iface: string; brightness: number } | null;
wall:     Canvas;                 // the editor's document; loaded from GET /wall when present, else localStorage "e120.wall", else single 128x64
job:      Job | null;             // the job the status bar follows (last started from this page)
status:   { kind: "idle" | "busy" | "error"; text: string };  // status bar right side
wasm:     "loading" | "ready" | "failed";
banner:   boolean;                // standalone banner visible
```

`api.ts` sets `status` to `busy` while a request is in flight and to `error`
with the `error` text when one fails; screens show the same text next to the
control that caused it. Starting a job sets `job` and opens its SSE; the
status bar shows `kind`, the last line, and a cancel button; `end` sets
`idle` or `error`.

### The layout JSON (`e120-canvas`)

The Wall edits exactly the structure `e120 show ... --layout` reads, serde
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

Import/export is the JSON above, pretty-printed as `e120 card layout-example`
prints it. When the daemon is present "save" is `PUT /wall`; export is always
a file download.

## 5. The daemon crate and the CLI refactor

### `crates/e120-commands`: the commands as functions

`e120-cli` cannot both provide the command library and depend on
`e120-server` (cargo rejects the package cycle), so the command modules
live in their own crate, `e120-commands`. `e120-cli` keeps clap, `main.rs`
and the `Stdio` sink; its output stays byte for byte the same (checked by
running every offline command against the previous binary). `e120-server`
depends on `e120-commands`; `e120-cli` depends on both.

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
`PanelSpec` has `parse(text: &str)`, `generate_with(&self, chip)` and
`chip_library(&self, load)`; the file-based `load` and `generate` stay for
the CLI. `ChipLibrary::parse(text)` likewise. `e120_rcvbp::image::compile`
builds the block-7 image `e120 config gen` writes. The loader is passed by
the caller: the CLI passes `read_library`, the WASM crate the embedded map,
the daemon "embedded `config/chips` map, then the filesystem relative to
the working directory" (`e120_server::state::load_library`).

### `crates/e120-server`

axum (`http1`, `json`, `multipart`, `query`, `tokio` features only), tokio
(`rt-multi-thread`, `macros`, `sync`, `time`), tokio-stream, tower-http
`cors`, include_dir, serde, base64, dirs, mime_guess.

```
src/lib.rs        pub fn run(opts: Options) -> Result<()>; Options { port, open, token, iface: Option<String>, data_dir: Option<PathBuf> }; pub fn router(state)
src/state.rs      AppState: settings, wall, cards, jobs, the link holder (a job or a command's subject, for the 409 text), data dir, token; command() and start_job(); load_library
src/routes.rs     one handler per route in section 2; the commit gate; Body/Qs extractors that turn a bad body into 400 {"error"}
src/jobs.rs       Job, Handle (lines + broadcast + done watch + cancel flag), Sink (impl Progress), Lines (a command's sink), spawn_blocking runner, SSE
src/assets.rs     include_dir!("$CARGO_MANIFEST_DIR/../../web/dist") behind build.rs's cfg(web_dist); every non-/api path
src/store.rs      settings.json and wall.json under the data dir
src/error.rs      ApiError { status, message } -> {"error": message}
tests/api.rs      the router without a link: health, config/gen against gen_config, the commit gate (flash/restore dry run), 409 with a fake job, CORS, token, wall
```

The link holder is a `Mutex<Option<Holder>>`; a command takes it for its
duration on the blocking pool, a job from start to finish, and the 409 body
names the holder (`job j3 (provision) is running`, `discover is running`).
A `show/*` route cancels a running `show/video` or `show/hold` job and
waits for it before taking the link. Job ids are handed out only once the
link is free, so a 409 does not consume one.

Data dir: `dirs::config_dir()/e120` (`~/Library/Application Support/e120`
on macOS, `~/.config/e120` on Linux); `--data-dir` overrides. Holds
`settings.json`, `wall.json`, `backups/`, `snapshots/`.

Static files: when `web/dist/index.html` exists at compile time the whole
directory is embedded and served at `/` with the right MIME types and
`index.html` for unknown non-API paths. Otherwise `/` returns
`text/plain` `build the web app: cd web && pnpm install && pnpm build, then
rebuild e120`. A rebuild of `e120-server` is needed after `pnpm build`
(`build.rs` emits `rerun-if-changed=../../web/dist`).

`e120 ui [--port 7120] [--no-open] [--token TOKEN] [--data-dir DIR]` in
`e120-cli` builds `Options` and calls `e120_server::run`, which owns its
tokio runtime; `--no-open` skips opening the browser. `--iface` typed on
the command line replaces the saved `settings.iface`; otherwise the saved
one (default `en24`) applies. It prints one line: `e120 ui:
http://127.0.0.1:7120` (with `#token=...` appended when a token is set),
then discovers for 3 s before serving.

## 6. Build and run

```sh
# once
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127    # the version crates/e120-wasm/Cargo.toml pins for the wasm-bindgen crate
cd web && pnpm install

# the wasm module (rerun after changes to e120-rcvbp, e120-canvas, e120-wasm, config/)
web/scripts/build-wasm.sh
#   cargo build -p e120-wasm --release --target wasm32-unknown-unknown
#   wasm-bindgen --target web --out-dir web/src/wasm target/wasm32-unknown-unknown/release/e120_wasm.wasm

# development: the app at http://localhost:5173, API from a running daemon
cd web && pnpm dev                                      # API base: same origin as the page (proxied to 7120 by vite.config.ts)
VITE_E120_API=http://127.0.0.1:7121/api/v1 pnpm dev     # or an explicit base

# production
cd web && pnpm build          # -> web/dist
cargo build --release -p e120-cli   # embeds web/dist
cargo install --path crates/e120-cli
e120 ui                       # opens http://127.0.0.1:7120
e120 ui --iface en24 --port 7120 --no-open --token secret
```

`pnpm check` runs `svelte-check`; `cargo build --workspace && cargo test
--workspace && cargo clippy --workspace --all-targets -- -D warnings` covers
the three Rust crates as for the rest of the workspace. `e120-wasm` is a
workspace member and must also pass
`cargo clippy -p e120-wasm --target wasm32-unknown-unknown -- -D warnings`.

## 7. Design rules for the UI

No component library, no CSS framework, no icon font, no emoji. System font
stack, native controls, `prefers-color-scheme`. Plain short prose; errors
verbatim from the API or WASM.

**Layout.** A 160 px left sidebar with the four screen names and, at the
bottom, the daemon state. A content pane that scrolls on its own. A 28 px
status bar across the bottom: left the interface and cards, right the job or
status text and a cancel button while a job runs. The banner, when shown,
sits above everything and pushes the layout down.

**Spacing.** One scale, in `app.css` as `--s1: 4px`, `--s2: 8px`, `--s3:
12px`, `--s4: 16px`, `--s5: 24px`, `--s6: 32px`. Controls are 28 px tall;
form rows are a two-column grid, label 160 px, gap `--s2`; sections are
separated by `--s5`.

**Type.** `font: 13px/1.45 system-ui, -apple-system, "Segoe UI", Roboto,
sans-serif`. Headings 15 px/600 for a screen, 13 px/600 for a section. Code,
hex, TOML and paths in `ui-monospace, SFMono-Regular, Menlo, Consolas,
monospace` at 12 px. No other sizes.

**Colour.** Take it from the system where a value exists: `Canvas`,
`CanvasText`, `Field`, `FieldText`, `ButtonFace`, `ButtonText`,
`AccentColor`, `AccentColorText`, `GrayText`. Own tokens only for what the
system has no name for, defined for both schemes:

| token | light | dark | use |
|---|---|---|---|
| `--line` | `#d0d0d0` | `#3a3a3a` | borders, table rules |
| `--muted` | `#f3f3f3` | `#1e1e1e` | sidebar, status bar, table header |
| `--error` | `#b3261e` | `#f2857a` | error text, invalid field border |
| `--ok` | `#1b7f3b` | `#6cc57c` | done state |
| `--busy` | `AccentColor` | `AccentColor` | running job, progress |

Wall editor canvas: receivers as 1 px `--line` boxes with the index at top
left, panels filled `--muted` with a 1 px `CanvasText` border, the selected
one `AccentColor`, rotation shown as a small arrow, flips as a mirrored
arrow. The grid at 1 px `--line` at 50 % opacity.

**States.** Every screen and every control that talks to the daemon or WASM
is in one of three states, and shows it the same way:

- idle: default rendering;
- busy: the control is `disabled`, the status bar shows the running text,
  the cursor is `progress`; a job also shows its last line in the status bar;
- error: the text in `--error` directly under the control that failed, and
  in the status bar until the next action; never a modal.

Confirmation before a commit is a second button, `Write to card`, that
appears only after a dry run finished, next to its plan; not a dialog.

Downloads name files as the CLI does: `<name>.rcvbp`,
`<name>-basic-pack.bin`, `<name>-block7.bin`, `<name>-sources.txt`,
`wall.json`.
