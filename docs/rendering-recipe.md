# How the panel came to render, and what each part does

2026-09-01. Every item below was found by measurement on this bench and is
pinned in `config/panels/p25-128x64-sm16269s.toml`; the method is in
[bench-measurement.md](bench-measurement.md).

| what | value | why it matters |
|---|---|---|
| chip id | `0x14C` (vendor "SM16169SH") | the only identity firmware 16.53 brings the SM16269S outputs up for. The vendor's own SM16169S (0x0DE, non-SH, tails 2/4/8) and the SM16269S entry (0x0214, a stub in every build) never arm; nor do tails 2/4/8 or 3/5/7 under 0x14C. The chip-control block *is* the protocol (`docs/chip-control-block.md`, `docs/fpga/chip-protocol-microcode.md`) and only the SH pattern `1,5,6` works here. |
| grey depth | **12** | inherited 14. At 14 and 16 pixel data never reaches the chips' SRAM at all; at 12 patterns render; 13/11/10/8 drift. |
| `+0x02F` | **1** | the vendor Reset() default and 961/1146 corpus files; the inherited config had it cleared and with it cleared nothing displays. Meaning not resolved. |
| frame order | brightness, rows, **3 latches** | one latch never starts the display; two renders but decays into noise and back on a ~10 s period; three or four hold for as long as measured. Both `image` and `play` use this. |
| raster | `rows` (64 packets of 128 px) | the double-width layouts place content in the wrong rows. |
| mapping | `block = 64` | the module's row-halves interleave every 64 columns; `block = 128` collapses the picture. |
| registers | `config/chips/sm16269s-factory.toml` | the inherited set; the vendor "Default Parameter" set renders worse and the LEDSetting 2.2.6 set saturates the panel. |
| `+0x0FC` table, `+0x19A` lane map | inherited (non-zero) | zeroing them (as the 16169 corpus has) breaks rendering on this card. |
| firmware | 16.53 | 9.53 arms nothing; 10.81 free-runs. Install via `e120 upgrade install` (SDRAM self-program, blocks 0–2 and 8) **plus** `flash-firmware --from-block 3 --to-block 7`: 16.53 write-protects its header/trailer sectors from the host path. |
| EEPROM | control area `0,0,128,64` | check with `scripts/flash-review.py` after any flash operation; restore with `scripts/eeprom-restore.py` (one record at a time, broadcast index, paced). |
| boot | **configure from flash** (`arm_at_boot = true`, `restore-flash` the block-7 image, then `eeprom-restore`) | three of three power-cycles render identically (black 0.73–0.77 A, white 1.74–1.76 A, control returns). Pushing the same parameters into RAM with `send-params` after boot renders on roughly one boot in three: the 34 unacknowledged packs are evidently not all landing. Use RAM pushes for experiments only, and prefer pushing twice with `--gap-ms 25`. |

## Open

### Black is not off — a gain-scaled per-pixel floor

An all-black frame leaves ~0.3 A of LED current at gain 12 as per-pixel
speckle. What is established about it (all on the flash-configured card):

* it scales with the sync frame's three **channel-gain** bytes (0.47 A = off
  at 0, 0.71 at 4, 0.75 at 12, 0.86 at 40, 1.08 at 120) and those bytes are
  the only brightness control — the "master" byte at data[21] is inert;
* the grey response above it is monotonic with no cliff (72 → 136 → white);
* it is **not** the stale second buffer page: writing both pages per frame
  (`E120_WRITES=2`) leaves it unchanged, although that experiment did show the
  two-latch decay was page alternation;
* no single driver register moves it: 0x03, 0x07, 0x0B, 0x0C, 0x0F, 0x11,
  0x14, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1E, 0x22, 0xF0 each tried at the
  vendor-default or LEDSetting-2.2.6 value; the chip's 13-bit mode
  (0x03 = 0x40) neither;
* `[current] percent` (the inherited 0.1) at 0 or 0.02: no change;
* single camera frames of it correlate ~0.6 with each other and ~0.88 with
  the average, so it has both a static and a flickering component.

What remains: how the card turns input 0 into the word it shifts out at grey
depth 12 — a gamma/LUT with a non-zero origin, or a word-width mismatch
between the 12-bit pack setting and the registers' 14-bit derivation — which
is a question for the vendor library (`GetBasicParam`, the grey-table
builders) rather than the bench. Knobs left in for it: `E120_LATCHES`,
`E120_WRITES`, `E120_SYNC_GAIN`.

### Geometry

Row band order reads reversed on the rotated panel (`line_dir` /
`reversed_lines` in the spec are the knobs).
