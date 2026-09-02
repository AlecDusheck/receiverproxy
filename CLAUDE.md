# receiverproxy

Drives a Colorlight E120 LED receiving card over raw Ethernet from a Rust CLI, `rxp`.
Read `README.md` for what the tool does and `docs/README.md` for the notes
behind it; read `docs/retracted-findings.md` before drawing a conclusion from a
measurement.

## Hardware on the bench

One E120 (ECP5 gateware, firmware 16.53) driving one P2.5 128x64 module with
SM16269S drivers, on a bench supply, photographed by a webcam. The panel spec
is `config/panels/p25-128x64-sm16269s.toml`; it is the only spec that has been
driven. Everything under `config/*/mined/` is a vendor default taken from the
config corpus, not a measurement.

The hardware works. If pixels are wrong, the fault is in configuration or in
the frames sent; do not propose rewiring, moving ribbons or ports, or
changing the supply.

## Rig rules

- The supply is read and power-cycled through `scripts/psu.sh`; never change
  its voltage or current settings.
- Vendor software (`third-party/`, `vendor/`) is inspected, never executed.
- Configure from flash (`rxp provision`); pushing parameters to RAM is for
  experiments and is unreliable across boots.
- One continuous stream per measurement with a same-content control, averaged
  captures, current read from the supply: `scripts/bench.py`, described in
  `docs/bench.md`. A single photo or a single current reading proves nothing.

## Layout

receiverproxy is the project, `rxp` is the command (`receiverproxy` is the same binary); crates are named for their role.

| crate | owns |
|---|---|
| `colorlight` | frame builders; byte-exact against the vendor sender, pinned by tests |
| `rawlink` | raw Ethernet transport (BPF on macOS, AF_PACKET on Linux) and pcap |
| `panelspec` | the vendor-neutral panel description: `PanelSpec`, the chip library, the loader hook, `config/chips` and `config/panels` embedded (`embedded`) |
| `receivers` | card models as data: `config/cards/*.toml` embedded, `models()`, `by_id`, `by_name`; the firmware manifest `config/firmware.toml` (`firmware::manifest`, `image`, sha256 `verify`) |
| `rcvbp` | the `.rcvbp` format, the boot image, the record generator over a `PanelSpec` |
| `wall` | wall layout: panels, receivers, rotation |
| `sources` | frame sources: files via ffmpeg, raw rgb24 from a reader |
| `driver` | `Wall`: the sink that paces and sends frames |
| `ops` | the commands as functions (`Ctx` + `Progress` sink), one module per command group; shared by the binary and the daemon |
| `cli` | the `rxp` binary (`receiverproxy` is the same program, `src/receiverproxy.rs` includes `main.rs`): the clap tree, `Stdio` printing, `rxp ui` |
| `daemon` | the daemon behind `rxp ui`: the JSON API, jobs, the embedded web app (`docs/ui.md`) |
| `rcvbp-wasm` | `rcvbp` and `wall` for the browser: generate, inspect, diff, layouts, the embedded `config/` libraries served as `libraries()`; built by `web/scripts/build-wasm.sh` |
| `demos` | `rxp-demo`: effects that use what LEDs physically are; the example of driving a wall from outside the CLI |

`web/` is the SvelteKit app: `pnpm build` is the site (adapter-cloudflare, receiverproxy.com), `pnpm build:embed` writes `web/build-static`, which `daemon` embeds when it exists at compile time; its contract is `docs/ui.md`.

`docs/architecture.md` follows the path from spec to light and says where each
measured default lives. Keep it true when moving things. Adding a receiving
card, a panel or a vendor: `docs/cards.md`.

## Working here

- Build, test and lint before calling anything done:
  `cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`.
- Byte-exact tests pin protocol frames and generated configs. A failing pinned
  value is a bug in the change, not in the pin.
- Timing defaults in `driver` (frame order, latch count, latch gap, row
  gap) are measured; each carries the measurement in a comment. Change them
  only with a new measurement.
- Never run hardware commands unasked, and never while the card may be off;
  a command can leave the card in a state that needs a power-cycle.

## Rust

Clippy runs pedantic and nursery at deny. The rules below follow the same
line:

- If a design needs a paragraph to justify it, it is probably the wrong
  design. Step back.
- Do the work in the type: a slice, an iterator or a newtype beats a comment
  saying what a `Vec<u8>` holds.
- No allocation in a per-frame or per-packet path; reuse buffers.
- Errors carry the failing subject first (`flash: block 7 verify failed`) and
  are returned, not printed, until the CLI boundary.
- One caller, no abstraction. Three callers, one abstraction.
- Comments say what the code cannot: the measurement, the vendor function and
  offset, the test that pins it. One or two lines. No narration.
- Public surface is what a second binary would need and nothing more.

## Prose

README, docs and help text state facts a reader can check. No enthusiasm, no
roadmaps, nothing invented, no mention of how the text was produced.
