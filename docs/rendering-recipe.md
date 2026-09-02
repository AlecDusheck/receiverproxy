# How the panel came to render, and what each part does

2026-09-01. Every item below was found by measurement on this bench and is
pinned in `config/panels/p25-128x64-sm16269s.toml`; the method is in
[bench-measurement.md](bench-measurement.md).

| what | value | why it matters |
|---|---|---|
| chip id | `0x14C` (vendor "SM16169SH") | the only identity firmware 16.53 brings the SM16269S outputs up for. The vendor's own SM16169S (0x0DE, non-SH, tails 2/4/8) and the SM16269S entry (0x0214, a stub in every build) never arm; nor do tails 2/4/8 or 3/5/7 under 0x14C. The chip-control block *is* the protocol (`docs/chip-control-block.md`, `docs/fpga/chip-protocol-microcode.md`) and only the SH pattern `1,5,6` works here. |
| grey depth | 12–16 all render identically | **Retracted as a cause (evening):** from flash, 12, 13, 14, 15 and 16 give the same picture and the same currents. The "only 12 renders" result came from RAM-push runs, which land on ~1 boot in 3; the real enablers were `+0x02F = 1`, the frame order and configuring from flash. Kept at 12 in the spec only because it is what was measured most. |
| `+0x02F` | **1** | the vendor Reset() default and 961/1146 corpus files; the inherited config had it cleared and with it cleared nothing displays. Meaning not resolved. |
| frame order | brightness, rows, **500 µs gap**, **3 latches** | one latch never starts the display; two renders but decays into noise and back on a ~10 s period (the stale second buffer page); three hold. Without the gap the card latches before the last row packet is stored and the bottom row flickers (judged by eye; invisible to the 30 fps camera). Both `image` and `play` do this; `E120_LATCHES`, `E120_LATCH_GAP_US`, `E120_ROW_GAP_US`, `E120_FRAME_MS` override for experiments. |
| raster | `rows` (64 packets of 128 px) | the double-width layouts place content in the wrong rows. |
| mapping | `block = 64` | the module's row-halves interleave every 64 columns; `block = 128` collapses the picture. |
| registers | `config/chips/sm16269s-factory.toml` | the inherited set; the vendor "Default Parameter" set renders worse and the LEDSetting 2.2.6 set saturates the panel. |
| `+0x0FC` table, `+0x19A` lane map | inherited (non-zero) | zeroing them (as the 16169 corpus has) breaks rendering on this card. |
| firmware | 16.53 | 9.53 arms nothing; 10.81 free-runs. Install via `e120 upgrade install` (SDRAM self-program, blocks 0–2 and 8) **plus** `flash-firmware --from-block 3 --to-block 7`: 16.53 write-protects its header/trailer sectors from the host path. |
| EEPROM | control area `0,0,128,64` | check with `scripts/flash-review.py` after any flash operation; restore with `scripts/eeprom-restore.py` (one record at a time, broadcast index, paced). |
| boot | **configure from flash** (`arm_at_boot = true`, `restore-flash` the block-7 image, then `eeprom-restore`) | three of three power-cycles render identically (black 0.73–0.77 A, white 1.74–1.76 A, control returns). Pushing the same parameters into RAM with `send-params` after boot renders on roughly one boot in three: the 34 unacknowledged packs are evidently not all landing. Use RAM pushes for experiments only, and prefer pushing twice with `--gap-ms 25`. |

## The black floor — solved

An all-black frame used to leave ~24 % of white's LED current as a fixed
pattern (red on panel columns 0–63, blue everywhere), reproducible across
cold starts and invariant to every driver register, the grey depth, the scan
schedule, latch/page schemes, the anti-void packs, the lane map and the
inherited tables. What finally moved it was the load length, and what
explained it was the **void-line table** (`docs/black-floor.md`): one byte
per line position, `physical = a + table[a]`, decoded from the vendor library.

The card emits `2 × width` positions per line for this interleaved wiring;
positions `width..2·width` carry nothing of ours and were being driven with a
fixed pattern. Displacing them off the chain through the void-line column
table (`mapping.gate_phantom_positions`, default on; `Block7Builder::
void_line_columns`) gives, from flash: **black 0.466 A = LEDs off**, boot
current 0.41 A, greys monotonic (64 → 0.73 A, 128 → 0.98 A, white 2.64 A),
every pattern intact, and the same-content control returning (0.432 A).

## Open

* Row band order reads reversed on the rotated panel (`line_dir` /
  `reversed_lines` in the spec are the knobs).
* The flicker the user perceived is not measurable with the 30 fps camera
  (2.4 % frame-to-frame against an 8–14 % camera reference); it may have been
  the floor's per-pixel structure mixed into content. Re-assess by eye now
  that black is black.
