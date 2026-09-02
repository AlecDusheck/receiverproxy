# Rendering: the settings that make this panel display, and what pins each

One Eager P2.5-O16S-SMD1415-128x64-E module (1/16 duty, SM16269S drivers)
on hub J1 of a Colorlight E120, firmware 16.53. Every value below is in
`config/panels/p25-128x64-sm16269s.toml`, `config/chips/sm16269s-factory.toml`
or `e120_driver::Timing::default()`, and was fixed by a bench measurement
taken as described in [bench.md](bench.md). Claims this project made and later
withdrew are in [retracted-findings.md](retracted-findings.md); read that
before changing anything here.

Bench state as of 2026-09-01, configured from flash after a power-cycle:
black 0.466 A (LEDs off; boot current 0.41 A), grey 64 0.73 A, grey 128
0.98 A, white 2.64 A at the bench brightness, same-content control 0.432 A,
greys monotonic, every test pattern intact. Three of three power-cycles gave
the same picture and the same currents.

## The settings

| setting | value | where | what pins it | what happened when it was wrong |
|---|---|---|---|---|
| driver-chip id | `0x14C` (vendor name SM16169SH) | `[chip] library` → `sm16269s-factory.toml` | `factory.rs` record 0x84 equality; bench: the only id 16.53 arms the SM16269S outputs for | `0x0DE` (SM16169S, non-SH) and `0x0214` (the real SM16269S id, a stub in every vendor build) never arm; chip-control tails 2/4/8 and 3/5/7 under `0x14C` never arm. The 20-byte chip-control block is the protocol descriptor ([chip-control-block.md](chip-control-block.md)); only the SH pattern `1,5,6` works |
| record 0x01 `+0x02F` | `1` | `[record01_overrides]`, applied last in `spec/record01.rs` | `factory.rs` delta list `[0x023, 0x02F, 0x0C0..0x0C3]`; bench: with it cleared nothing displays | the config the card arrived with had `0`. `1` is the vendor `Reset()` default and 961 of 1146 corpus files. Meaning not resolved |
| grey depth | `12` | `[module] gray_bits` | `factory.rs` (`0x023`). Configured from flash, 12, 13, 14, 15 and 16 give the same picture and the same currents | the card arrived saying 14. "Only 12 renders" was measured through RAM pushes and is retracted. 12 stays only because it was measured most |
| frame order | brightness (0x0A), 64 row packets (0x55), 500 µs gap, 3 latch frames (0x0107) | `e120-driver/src/lib.rs Timing::default()` | `settings_default_to_the_measured_recipe`; latch count and gap by eye | one latch never starts the display; two render but decay into noise and back on a ~10 s period (the stale second buffer page); three hold. Without the gap the card latches before the last row is stored and the bottom row flickers (by eye; the 30 fps camera cannot see it) |
| raster | `rows`: one 0x55 packet per panel row, 128 px | the only layout `Wall::show` cuts | `pixel_rows_follow_the_fpp_layout` | double-width layouts put content in the wrong rows; deleted (in git before 2026-09-02) |
| pixel mapping | `block = 64` | `[mapping] block` | `the_reference_mapping_is_reproduced_by_the_block_knob`; the reference file's record 0x03 regenerates byte for byte | `block = 128` (one contiguous 128-slot run per row-half, the corpus majority) scrambles every column. The module's row-halves alternate every 64 columns ([panel-wiring.md](panel-wiring.md)) |
| phantom positions | gated (`gate_phantom_positions = true`) | `[mapping]`, `Block7Builder::void_line_columns` | `the_bench_spec_displaces_the_phantom_positions`; bench: black 0.466 A = LEDs off | ungated, an all-black frame drew ~24 % of white's LED current as a fixed pattern. See "The black floor" below |
| driver registers | the reference file's table | `config/chips/sm16269s-factory.toml` (record 0x84) | `factory.rs` | the vendor "Default Parameter" set (`sm16269.toml`) renders worse; the LEDSetting 2.2.6 set saturates the panel. A register-by-register sweep changed nothing at black |
| record 0x01 `+0x0FC` table, `+0x19A` lane map | inherited, non-zero | spec defaults | `factory.rs` | zeroing them (the SM16169 corpus value) breaks rendering on this card |
| serial clock | 8 | `[module] serial_clock` | the reference file's value; the chip default is 15 | not swept |
| firmware | 16.53 | `third-party/firmware/E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex` | `e120 discover` reports it | 9.53 arms nothing; 10.81 (as shipped) free-runs; Normal 13.39 leaves the panel dead at 0.44 A |
| EEPROM control area | `(0, 0, 128, 64)` | `e120 provision --position 0,0` → `eeprom::control_area` | `scripts/flash-review.py`; `e120 card screen-size` | erased to `startX = startY = 0xFFFF`, the card reports a healthy 128x64 and drops every pixel ([receiver-identity.md](receiver-identity.md)) |
| configure from flash | `arm_at_boot = true`; block-7 image via `flash restore-block`; EEPROM records rewritten; power-cycle | `[boot]`, `e120 provision` | three of three power-cycles render identically (black 0.73–0.77 A, white 1.74–1.76 A before the phantom gate; 0.466 A / 2.64 A after) | the same parameters pushed into RAM with `config send` after boot render on about one boot in three: the 34 unacknowledged packs do not all land. RAM pushes are for experiments only; push twice with `--gap-ms 25` |
| brightness | ≤ 40 until content is right | operator rule | PSU current | an armed panel showing unmodulated content draws ~4.5 A; at full brightness it rails the 5.1 A limit and browns out |
| colour order | `bgr` | `--order` default | `colour_order_reorders_the_channels` | — |
| pixels per packet | 497 | `e120-proto/src/pixel.rs MAX_PIXELS_PER_PACKET` | `pixel_rows_follow_the_fpp_layout`; CLTNic.dll hard-codes it | — |

Experiment overrides, read once by `e120-cli/src/display.rs` into
`e120_driver::Timing`: `E120_LATCHES`, `E120_LATCH_GAP_US`, `E120_ROW_GAP_US`,
`E120_FRAME_MS`. Nothing in `scripts/` sets them.

## Frame layout

Byte-exact against the vendor's own sender, CLTNic.dll, read statically
([pixel-protocol.md](pixel-protocol.md)): 21-byte row header, `0x55` at
frame offset 12, row / x-offset / count as big-endian u16 at 13 / 15 / 17,
`08 88` at 19–20, pixels from 21, at most 497 per packet; latch frame 112
bytes (`01 07`, brightness at 35 and gains at 38–40); brightness frame 77
bytes (`0A`, three bytes and `0xFF` at 13–16). The vendor sends latch,
brightness, rows, back to back; the measured order for this card is
brightness, rows, gap, three latches. The proto tests pin the bytes.

## Firmware 16.53

The card shipped running `E320_PCB6.0_PWM_FPGA10.81_20230907`; the notes said
16.53 because a restore had been assumed to install it. `e120 discover`
reports the running version; do not take it from notes.

Install: `e120 provision --firmware …16.53….hex --commit` does both halves.
By hand:

```
e120 flash snapshot --dir build/snapshot-<time>     # primary bank + golden bank (block 0x20, never written)
e120 firmware install <hex> --commit                # SDRAM self-program: blocks 0-2 and 8
e120 firmware write <hex> --backup <snapshot>/primary-region.bin --from-block 3 --to-block 7 --commit
```

16.53 write-protects blocks 0–2 and 8 from the host page-write path and its
self-program path writes only those, so a complete install needs both. The
verify step reports about 4042 differing bytes: all of them are in block 0x07
between `0x7F000` and `0x7FFFF`, the parameter tail the card writes for
itself; blocks 0x00–0x06 and 0x08–0x0A verify exactly. Flashing firmware
erases block 0x07, so the config and the EEPROM records must be rewritten
afterwards (`provision` does; by hand, `flash restore-block` then
`scripts/eeprom-restore.py`), then power-cycle. `e120 card set-layout` was
needed once after a firmware change: `discover` reported "detected size
1544x128" until it was re-sent.

What 16.53 changed, measured: on 10.81 the panel changed with no network
traffic (three photos five seconds apart, every streamer killed, mean
absolute difference 29–37 levels of 255, mean brightness 226 → 200 → 235); on
16.53 the same test gives 1.6–1.8 (camera noise) and 189/189/189. On 10.81
the card's nine built-in test selectors gave flat current and indistinguishable
output; on 16.53 they differ visibly. Immediately after the install content
still did not render and black and white drew the same current (0.001 A
apart against a 0.033 A within-condition spread); the settings above are
what fixed that.

## The black floor

With everything else in place, an all-black frame left ~24 % of white's LED
current as a fixed pattern: red on panel columns 0–63, blue everywhere, green
between. It was identical across cold starts (correlation 0.997–0.999),
scaled only with the `0x0107` channel-gain bytes (gain sweep 0/4/12/40/120
gave 0.47/0.71/0.75/0.86/1.08 A at black), and was invariant to every driver
register, the grey byte, the scan schedule, latch and page schemes, the
anti-void packs, the lane map and the inherited tables. It changed shape with
the load-length fields (CardScanLen / MaxPsc, basic-pack body `+0x0D`,
`+0x39`, `+0xE3`, `+0xE5`).

Two candidates were run down before the cause:

* the gamma table: the host-built 8-bit → N-bit table on this card's flash
  (block 9) is gamma 2.8 computed for 14-bit grey, byte-identical to the
  vendor formula, entry 0 = `00 00 00`. Not the floor. The vendor tool
  cannot even produce grey depth 12 for chip `0x14C` (minimum 13 from the
  register formula). Decode in
  [archive/grey-mapping.md](archive/grey-mapping.md);
* the scan schedule: the boot image pairs a 12-bit grey byte with a 14-level
  scan table, so levels 12 and 13 (about 75 % of lit time) have no bits to
  read. Tested 2026-09-01 by generating the vendor's own 12-level table: black
  went from 0.75 A to 0.90 A, the wrong way. The 14-level table stays
  (`image/scan_table.rs`, `GRAY`).

The cause: for this interleaved wiring the card emits `2 × width` positions
per line, and positions `width..2·width` carry nothing of ours and were driven
with a fixed pattern. The type-`0x1F` void-line table (image `0x1000`: 1024
per-line byte offsets, `0x1400`: 1024 per-column offsets, `physical = a +
table[a]`; decoded from `ChangeVoidLineDataFromNormalToCustom` at
`0x160160` and `GetAntiVoidLineParam` at `0x1604d0`) displaces those
positions off the chain. `Block7Builder::void_line_columns` writes it;
`mapping.gate_phantom_positions` (default on) enables it. Result from flash:
black 0.466 A = LEDs off, control 0.432 A, greys monotonic, patterns intact.
The full decode, the verification of every other candidate table against the
factory image and the two probe experiments are in
[archive/black-floor.md](archive/black-floor.md).

## Open

* Row band order reads reversed on the rotated panel; `line_dir` and
  `reversed_lines` in the spec are the knobs. Not measured.
* A faint flicker seen by eye is not measurable with the 30 fps camera
  (2.4 % frame to frame against an 8–14 % camera reference). It may have been
  the floor's per-pixel structure mixed into content; re-assess by eye now
  that black is black.
* Serial clock 8 (the reference file) versus 15 (chip default): not swept.
