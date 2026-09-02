# e120

Drives a Colorlight E120 LED receiving card over raw Ethernet from a Rust CLI.
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
the frames we send; do not propose rewiring, moving ribbons or ports, or
changing the supply.

## Rig rules

- The supply is read and power-cycled through `scripts/psu.sh`; never change
  its voltage or current settings.
- Vendor software (`third-party/`, `vendor/`) is inspected, never executed.
- Configure from flash (`e120 provision`); pushing parameters to RAM is for
  experiments and is unreliable across boots.
- One continuous stream per measurement with a same-content control, averaged
  captures, current read from the supply: `scripts/bench.py`, described in
  `docs/bench.md`. A single photo or a single current reading proves nothing.

## Layout

| crate | owns |
|---|---|
| `e120-proto` | frame builders; byte-exact against the vendor sender, pinned by tests |
| `e120-net` | raw Ethernet transport (BPF on macOS, AF_PACKET on Linux) and pcap |
| `e120-rcvbp` | the `.rcvbp` format, the boot image, the TOML spec generator |
| `e120-canvas` | wall layout: panels, receivers, rotation |
| `e120-video` | frame sources: files via ffmpeg, raw rgb24 from a reader |
| `e120-driver` | `Wall`: the sink that paces and sends frames |
| `e120-cli` | the `e120` binary, one module per command group |

`docs/architecture.md` follows the path from spec to light and says where each
measured default lives. Keep it true when moving things.

## Working here

- Build, test and lint before calling anything done:
  `cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`.
- Byte-exact tests pin protocol frames and generated configs. A failing pinned
  value is a bug in the change, not in the pin.
- Timing defaults in `e120-driver` (frame order, latch count, latch gap, row
  gap) are measured; each carries the measurement in a comment. Change them
  only with a new measurement.
- Never run hardware commands unless the card is known to be on and the user
  asked; the card can be left in a state that needs a power-cycle.

## Rust

Clippy runs pedantic and nursery at deny. The rules below are in the same
spirit:

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
