# Handoff

Current state of the P2.5 128x64 SM16269S panel bring-up on the E120, and
what remains. Read with [`docs/building-a-config.md`](docs/building-a-config.md).

## The rig

Mac + AX88179B on `en24` (raw Ethernet via `/dev/bpf`; `sudo chmod o+rw
/dev/bpf*` after every reboot). Card: E120, firmware 16.53 — the only
SM16269-family gateware Colorlight publishes; do not chase other builds.
Panel: one Eager P2.5-O16S-SMD1415-128x64-E module (1/16 duty, 14-bit gray,
SM16269S read off the silicon) on hub J1. PSU: KA3005P 5 V / 5.1 A limit —
read and power-cycle only, never change settings. Camera on the panel.

**Power rule:** the armed panel showing unmodulated content draws ~4.5 A;
at full brightness it rails the limit and browns out. Boot with
`scripts/safe-boot.sh`, keep brightness ≤ 40 until content is right, and
never flash while `ka3005p status` shows `CH1: Cc`. With the hub connector
unplugged the card alone is safe at any time.

## What is established

* **Wire protocol** — correct and verified (data starts at frame offset 13;
  latch frame sent twice as FPP does for firmware ≥ 13). The old shifted
  frames caused the 5 Hz strobe.
* **Config generation** — `e120 gen-config --spec panels/<panel>.toml`
  produces the `.rcvbp`, the basic pack, and the complete boot image from
  erased flash, with a provenance line per byte. Record 0x01 is decoded
  byte-for-byte (`docs/record-0x01-fields.md`), the boot image region by
  region (`docs/compiled-image-format.md`); the factory image rebuilds
  byte-exact under test.
* **Why the seller's config failed** — it was compiled for a 256x384 wall
  (2x6 modules): wrong screen size, module count and CardScanLen, an
  all-zero module-position table, and a pixel mapping that is an outlier
  against 34 known-good vendor configs for this geometry.
* **Bench facts** — chip pack arms the drivers; brightness scales current;
  content changes the panel but has rendered scrambled under every config
  tried so far *except* one clean reload of a corrected config, which gave a
  bounded content region on a true-black background (the first "off means
  off" state). Those tests predate the corrected serial clock, scan-line
  length, module positions and double latch now in the generator.

## Open items

1. **Flash and test the generated config** (`build/p25-128x64-sm16269s-block7.bin`
   from the spec; `boot.arm_at_boot = false` so the card boots dark). Then
   `send-params --spec ...` to arm, `test rgb --hold`, `strobe.sh`, photos.
2. **Last decode gaps** (agents were running at handoff): the scan-table
   bit-time solver (`FromSegmentToFrameTime`; the carried table was computed
   for a 512-wide load, ours is 256), `ExchangeChipRegisterWhenColorChanged`
   (may permute record 0x84 per colour swap), the pixel-mapping generator
   (so record 0x03 is computed, not reused), and the module-position index
   byte order (medium confidence).
3. Once content renders: `image` / `play` are already wired; then enable
   `boot.arm_at_boot` so the panel comes up from flash.

## Assets

`firmware/card-dumps/primary-region.bin` (day-one dump, ground truth),
`firmware/derived/` (template config, reference pack, consensus donor, the
pinned single-module pack), `analysis/record01-fieldmine/` (corpus
statistics), the LEDVISION extract and `libCLTDevice.asm` in the old session
scratchpad `/private/tmp/claude-501/-Users-amd-e120/261c3dad-.../scratchpad/`
(re-extract per `docs/vendor-sdk-analysis.md` if gone).
