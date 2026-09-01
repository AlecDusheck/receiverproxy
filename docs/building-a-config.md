# Building a config for this panel, from parts we control

This is the reproducible recipe behind `firmware/derived/p25-128x64-fixed.rcvbp`
and the block-7 image installed on the card. Every byte is placed by code in
this repo; every byte we *changed* has a stated reason; bytes we do not yet
understand are carried verbatim from a named known-good source and flagged.

## The three layers

1. **`.rcvbp`** — the config *source* file: zlib-compressed TLV records
   (`docs/config-protocol.md` §8, parser in `crates/e120-rcvbp`). The card
   stores it at flash +0x8000 in block 7 but never reads it at boot.
2. **The compiled image** — block 7 bytes 0x0000–0x7FFF, a fixed-offset
   scatter of pack bodies (`docs/compiled-image-format.md`). This is what the
   card applies at boot. Built by `e120 compile-config`
   (`crates/e120-rcvbp/src/compiled.rs`, round-trip proven against the factory
   dump in unit tests).
3. **The EEPROM screen record** at 0x7F000 — geometry by value, set with
   `e120 screen-size --set WxH --commit`. Not reachable by page writes.

## The recipe

```sh
# 1. Compose the .rcvbp: seller's config (correct chip id + the only known
#    SM16269S register set) + the vendor-consensus pixel mapping.
e120 config-build \
  --base firmware/P2.5-32S-128X64-SM16269S-256X384I.rcvbp \
  --copy-from firmware/derived/donor-P2.5-320x160-2153-consensus.rcvbp \
  --copy 0a03 \
  --out firmware/derived/p25-128x64-fixed.rcvbp

# 2. Compile the block-7 boot image.
e120 compile-config \
  --rcvbp        firmware/derived/p25-128x64-fixed.rcvbp \
  --basic-pack   firmware/derived/basic-pack-single-module.bin \
  --chip-from    firmware/derived/p25-128x64-fixed.rcvbp \
  --mapping-from firmware/derived/p25-128x64-fixed.rcvbp \
  --out block7.bin
# Review the printed page diff before flashing.

# 3. Install and apply.
e120 restore-flash block7.bin --commit     # page 0xF0 refusing is expected
e120 screen-size --set 128x64 --commit
e120 reload-params --full                  # vendor's 0x77 apply; or power-cycle
```

Power-on note: with the chip-register page installed the card arms the panel
at boot **at full brightness** — the all-on state rails the 5.1 A supply limit.
Send `e120 brightness 25` (or stream black) promptly after power-on.

## Where each input comes from

| Input | Source | Confidence |
|---|---|---|
| `--base` (records incl. 0x84 chip regs) | the seller's config = the card's own shipped `.rcvbp`. Its 0x84 is the only SM16269S register table known to exist (none in the vendor corpus); matches a vendor `ChipSetting.dll` preset in 31/32 registers | placement proven; register semantics partial |
| consensus mapping (record 0x0a03) | `donor-P2.5-320x160-2153-consensus.rcvbp` — 34 of 49 unique vendor configs for exactly module 128x64 @ 1/16, across every chip family, share this byte-identical record. The seller's copy is a lone outlier differing in exactly the 2048 even-block entries (base 64+256k vs 128+256k) | high (consensus), entry semantics partial |
| `basic-pack-single-module.bin` | the vendor-computed basic pack from this card's factory flash (page 0 of the compiled image), with **five fields patched** for one 128x64 module at 1/16 scan: modules-in-line-dir 2→1 (+0x06), scan 8→16 (+0x09..0a BE), OneScanLen 256→128 (+0x0b..0c BE), MaxWidth 256→128 (+0x88..89 BE), MaxHeight 384→64 (+0x8a..8b BE). Field positions decoded instruction-by-instruction from `GetBasicParam` | patched fields high; ~200 bytes verbatim-carried, incl. an unexplained trailing dword `74 a9 51 a3` |
| everything else in block 7 | the factory dump (`firmware/card-dumps/primary-region.bin` at 0x70000) | carried verbatim, listed by `compile-config` output |

## Why the seller's config was wrong for this panel

The card shipped configured for a **256x384 wall of twelve of these modules at
1/8 scan** (the `256X384I` in the filename), with a modified mapping record.
The panel is **one 128x64 module at 1/16** (module spec: 1/16 duty). The
compiled boot pack carried the wall geometry, which garbled the raster from
every source — including the card's own test-pattern generator.

## What is still unverified

* The uninterpreted majority of record 0x01 (764 bytes, ~40 decoded).
* The unexplained basic-pack trailing dword (possibly a checksum — if a
  patched pack is ever rejected where the verbatim one is accepted, suspect it).
* Mapping-entry semantics (we use the consensus bytes, not a generator).
* Which "three classes" the 0x77 reload reloads.
* Padding rule for mapping tables shorter than their region (flagged by the
  builder when it applies).
