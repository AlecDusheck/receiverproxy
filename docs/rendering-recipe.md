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
| boot | push packs ≥ 12 s after power-on | the card answers discovery before it has loaded its own parameters. |

## Open

* **Black and low greys render as noise.** A black frame overwrites a
  rendered pattern with per-pixel noise, so low values are being written and
  mis-encoded, not skipped. Threshold is between 64 and 128 of 255 (gray-128
  renders dim and uniform). Suspects: the 8→12-bit conversion, the word
  framing on the SH stream, a register governing low-grey handling.
* Row band order appears reversed on the rotated panel (`line_dir` /
  `reversed_lines`).
* The per-boot state that once looked like a "toggle" was the latch count
  (two per frame) plus pushing packs too early after power-on; with three
  latches and a settle it has not recurred.
