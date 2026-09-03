# Rendering: what a working configuration needs

Every setting that decides whether a panel displays what is sent, what each
one does, and what goes wrong when it is wrong. The values are the ones a
P2.5-O16S-SMD1415-128x64-E module (1/16 duty, SM16269S drivers) on hub J1 of
a Colorlight E120 running firmware 16.53 needs; that pairing is the reference
module, and where a value was established by measurement rather than by the
vendor code it is marked as such. They live in
`config/panels/p25-128x64-sm16269s.toml`,
`config/chips/sm16269s.toml` and `driver::Timing::default()`.
The method for establishing them on another card is in [bench.md](bench.md).

## The settings

| setting | value | where | what pins it | what goes wrong with another value |
|---|---|---|---|---|
| driver-chip id | `0x14C` (vendor name SM16169SH) | `[chip] library` → `sm16269s.toml` | `day-one.rs` record 0x84 equality; measured as the only id 16.53 arms the SM16269S outputs for | `0x0DE` (SM16169S, non-SH) and `0x0214` (the SM16269S's own id, a stub in every vendor build) never arm the outputs. The 20-byte chip-control block is the protocol descriptor ([chip-control-block.md](chip-control-block.md)); of its tails only the SH pattern `1,5,6` renders, `2/4/8` and `3/5/7` never arm |
| record 0x01 `+0x02F` | `1` | `[record01_overrides]`, applied last in `crates/rcvbp/src/spec/record01.rs` | `day-one.rs` delta list `[0x023, 0x02F, 0x0C0..0x0C3]`; measured as required | at `0` nothing displays. `1` is the vendor `Reset()` default and the value in 961 of 1146 corpus files. Meaning not resolved |
| grey depth | `12` | `[module] gray_bits` | `day-one.rs` (`0x023`) | 12 through 16 render alike from flash on this chip; 12 is the value with measurements behind it, not a requirement |
| frame order | brightness (0x0A), 64 row packets (0x55), 500 µs gap, 3 latch frames (0x0107) | `crates/driver/src/lib.rs` `Timing::default()` | `settings_default_to_the_measured_recipe` | one latch never starts the display; two render but decay into noise and recover on a period of about 10 s (the stale second buffer page); three hold. Without the gap the card latches before the last row is stored and the bottom row flickers |
| raster | `rows`: one 0x55 packet per panel row | the only layout `Wall::show` cuts | `pixel_rows_follow_the_fpp_layout` | a double-width layout puts content in the wrong rows; the payload starting at frame offset 14 turns the panel into a 5 Hz strobe |
| pixel mapping | `block = 64` | `[mapping] block` | `the_reference_mapping_is_reproduced_by_the_block_knob`; the reference file's record 0x03 regenerates byte for byte | the block size must match how the module's shift chain visits its row-halves. `block = 128` (one contiguous 128-slot run per row-half, the corpus majority) scrambles every column on a module whose halves alternate every 64 columns ([panel-wiring.md](panel-wiring.md)) |
| phantom positions | gated (`gate_phantom_positions = true`) | `[mapping]`, `Block7Builder::void_line_columns` | `the_bench_spec_displaces_the_phantom_positions` | ungated, an all-black frame leaves a fixed lit pattern instead of a dark panel. See "The black floor" |
| driver registers | the chip library's table | `config/chips/sm16269s.toml` (record 0x84) | `day-one.rs` | the vendor "Default Parameter" set (`sm16269-defaults.toml`) renders worse and the LEDSetting 2.2.6 set saturates the panel. No register in the table changes the black floor |
| record 0x01 `+0x0FC` table, `+0x19A` lane map | inherited, non-zero | spec defaults | `day-one.rs` | zeroed (the SM16169 corpus value), rendering breaks |
| serial clock | 8 | `[module] serial_clock` | the reference file's value; the chip default is 15 | not swept |
| firmware | 16.53 | `third-party/firmware/E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex` | `rxp discover` reports the running version and is the only authority for it | the build must be a PWM build naming this chip family ([cards.md](cards.md) section 1). 9.53 arms nothing; 10.81 free-runs, displaying a buffer nothing drives; a Normal build leaves the panel dead |
| EEPROM control area | `(0, 0, 128, 64)` | `rxp provision --position 0,0` → `eeprom::control_area` | `scripts/flash-review.py`; `rxp card screen-size` | erased to `startX = startY = 0xFFFF` the window is empty: the card reports a healthy size to `discover`, accepts frames, and drops every pixel ([receiver-identity.md](receiver-identity.md)) |
| configure from flash | `arm_at_boot = true`; block-7 image via `flash restore-block`; EEPROM records rewritten; power-cycle | `[boot]`, `rxp provision` | repeated power-cycles render identically | the same parameters pushed into RAM with `config send` do not reliably land: the 34 packs are unacknowledged. RAM pushes are for experiments only; push twice with `--gap-ms 25` |
| brightness | ≤ 40 until content is right | operator rule | supply current | an armed panel showing unmodulated content draws several amps; at full brightness it can rail a small supply's limit and brown the card out |
| colour order | `bgr` | `--order` default | `colour_order_reorders_the_channels` | the channels come out permuted |
| pixels per packet | 497 | `crates/colorlight/src/pixel.rs` `MAX_PIXELS_PER_PACKET` | `pixel_rows_follow_the_fpp_layout`; CLTNic.dll hard-codes it | |

Experiment overrides, read once by `crates/ops/src/display.rs` into
`driver::Timing`: `RXP_LATCHES`, `RXP_LATCH_GAP_US`, `RXP_ROW_GAP_US`,
`RXP_FRAME_MS`. Nothing in `scripts/` sets them.

## Frame layout

Byte-exact against the vendor's own sender, CLTNic.dll, read statically
([pixel-protocol.md](pixel-protocol.md)): 21-byte row header, `0x55` at
frame offset 12, row / x-offset / count as big-endian u16 at 13 / 15 / 17,
`08 88` at 19–20, pixels from 21, at most 497 per packet; latch frame 112
bytes (`01 07`, brightness at 35 and gains at 38–40); brightness frame 77
bytes (`0A`, three bytes and `0xFF` at 13–16). The vendor sends latch,
brightness, rows, back to back; the order this card needs is brightness,
rows, gap, three latches. The proto tests pin the bytes.

## The black floor

With every other setting in place and the phantom positions ungated, an
all-black frame leaves a fixed lit pattern rather than a dark panel: red on
panel columns 0–63, blue everywhere, green between, drawing roughly a quarter
of white's LED current. The pattern is identical across cold starts, scales
only with the `0x0107` channel-gain bytes, and is invariant to every driver
register, the grey byte, the scan schedule, latch and page schemes, the
anti-void packs, the lane map and the inherited tables. It changes shape with
the load-length fields (CardScanLen / MaxPsc, basic-pack body `+0x0D`,
`+0x39`, `+0xE3`, `+0xE5`).

Cause: for an interleaved wiring like this the card emits `2 × width`
positions per line, and positions `width..2·width` carry no host pixels and
are driven with a fixed pattern. The type-`0x1F` void-line table (image
`0x1000`: 1024 per-line byte offsets, `0x1400`: 1024 per-column offsets,
`physical = a + table[a]`; decoded from
`ChangeVoidLineDataFromNormalToCustom` at `0x160160` and
`GetAntiVoidLineParam` at `0x1604d0`) displaces those positions off the
chain. `Block7Builder::void_line_columns` writes it, and
`mapping.gate_phantom_positions` (default on) enables it; with it enabled
black is LEDs-off. Every other candidate table in the compiled image was
checked and none differs from the vendor's
([compiled-image-format.md](compiled-image-format.md)).

Two things that look like the floor and are not:

* The gamma table. The host-built 8-bit → N-bit table is gamma 2.8 computed
  for the configured grey depth, byte-identical to the vendor formula, entry
  0 = `00 00 00`. The vendor tool cannot produce grey depth 12 for chip
  `0x14C`; its register formula gives a minimum of 13.
* The scan schedule. The boot image pairs a 12-bit grey byte with a 14-level
  scan table, so levels 12 and 13 (about 75 % of lit time) have no bits to
  read. The vendor's own 12-level table raises black current instead of
  lowering it; the 14-level table stays
  (`crates/rcvbp/src/image/scan_table.rs`, `GRAY`).

## Not resolved

* Row band order: `line_dir` and `reversed_lines` in the spec are the knobs
  for a module whose row bands come out reversed. Not measured.
* Serial clock 8 (the reference file's value) versus 15 (the chip default):
  not swept.
* Record 0x01 `+0x02F`: required at `1`; what the card does with it is not
  resolved.
