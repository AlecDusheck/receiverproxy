# Building a config for a panel

A panel is described once in `config/panels/<panel>.toml`; everything the
card consumes is generated from it and a chip library entry. Nothing is
copied from a donor file: every output byte is a vendor default, a spec
field, a chip-library value, or a documented literal, and the provenance
file names the source of each placement.

```sh
e120 config gen --spec config/panels/p25-128x64-sm16269s.toml --out-dir build
#   build/<name>.rcvbp             the config (17 records, the vendor's order)
#   build/<name>-basic-pack.bin    page 0 of the boot image (with its CRC-32)
#   build/<name>-block7.bin        the complete 64 KB boot image
#   build/<name>-provenance.txt    the source of every placed byte

e120 flash restore-block build/<name>-block7.bin --commit   # page 0xF0 refusing is expected
e120 card screen-size --set 128x64 --commit
e120 card reload --full                             # vendor's 0x77 apply; or power-cycle
e120 config send --spec config/panels/<panel>.toml    # or push the RAM packs directly
```

## The spec

`[module]` width, height, scan, line direction, data groups, optional serial
clock and gray override · `[screen]` the whole screen this card drives ·
`[chip] library` · `[color]` swap and R/G/B source · `[current]` gains and
percents · `[timing]` gamma, refresh, GCLK, minimum OE, luminance level, 8 ns
OE · `[mapping]` the two wiring knobs · `[boot]` arm at boot.

## The chip library (`config/chips/*.toml`)

Family id and sub-variant id, the vendor's default serial clock, the 20-byte
chip-control block the config carries, and the default register table with
the register field layout in comments. Record 0x84 is the table in order
with register 0x02 patched to `scan − 1`, as the vendor's loader does; the
gray depth is derived from registers 0x07/0x03 (`GetSupporttedGray`).

`sm16269s-factory.toml` is the library the bench panel renders with: family
`0x14C`, no sub-id, the register table read out of the reference file.
`sm16169sh.toml` is the same table with the reference file's reg 0x07 value.
`sm16269.toml` pairs `0x14C` with sub-id `0x14D` and the vendor tool's
"Default Parameter" table; `0x14D` is the vendor's SM16380SH id, not an
SM16269 variant, and that table renders worse on this panel
([chip-control-block.md](chip-control-block.md) §7, [rendering.md](rendering.md)).

Not every chip works this way. Non-SH parts such as SM16169S (0x00DE) have no
register table, no record 0x84, and carry their whole configuration in the
16-byte `SChipCustom` block of record 0x01. `sm16169s-vendor.toml` and
`sm16269s-vendor-0x214.toml` hold those; `chips.rs` loads them through
`chip_custom`, `chip_custom_scan_patch`, `chip_custom_ex`, `emit_record_84`
and `gray_bits` ([chip-libraries-non-sh.md](chip-libraries-non-sh.md)).

## What is derived, and from where

| Output | Source | Confidence |
|---|---|---|
| record 0x01 | vendor write-side defaults (`CHWParamRcvGeneral::Reset/ResetIS/ResetSwapData`), the spec, the chip library (family/sub id, chip control, reset serial clock), and 11 documented literals for bytes whose meaning is unresolved (`spec/record01.rs`) | high — the reference file regenerates byte-exact |
| record 0x03 (mapping) | geometry: pixel → (`row % scan`, `group·width + col`) with the vendor's reversed group order; reproduces the 34-config consensus | high |
| record 0x84 (chip registers) | chip library + `reg 0x02 = scan − 1` | high |
| other records (0x8a, 0x83/0x89, 0xca, 0xcd, 0x8f, 0x07, 0x86, 0x8e, 0x8d, 0x91/0x95/0xd8/0xda) | decoded loader defaults (`spec/records.rs`); 0x8a mirrors the screen size, 0xca the module geometry | high |
| basic pack (all 256 bytes) | `GetBasicParam` transcribed field by field from record 0x01, plus the CRC-32 trailer | high — factory pack byte-exact |
| boot image | every region generated (`image/`): gated zeros, data-swap, module positions, anti-void counters, mapping, scan table (bit-time solver), embedded `.rcvbp` | high — factory image byte-exact |

Pins (`crates/e120-rcvbp/tests/factory.rs`): the reference config regenerates
record for record from a spec; that spec reproduces the factory pack and the
factory image byte for byte; our single-module spec differs from the reference
only in the intended bytes.

## Why the reference config was wrong for this bench

It was compiled for a 256x384 wall (2x6 of these modules) — screen size,
module count and CardScanLen in the boot pack, an all-zero module-position
table (the wall exceeds the vendor's 64-tile cap), and a pixel mapping that is
a lone outlier against the corpus — and it carried the SM16169SH register set
with the sub-variant id unset, although the silicon is SM16269S.

## Limits

* Eleven record-0x01 bytes and a few small-record bytes are literals whose
  meaning is unresolved (provenance known); they are the reference file's values.
* The scan-table solver is transcribed for the default style, 16 segments,
  14-bit gray; other gray depths need their hand-coded vendor blocks.
* Module-position generation covers the plain grid (split segment 1).
* The current-exchange page (0xC00) is written as zeros; for one module the
  vendor's group-to-module map is all zero too (`GetCurrentExchangeParam`,
  [archive/black-floor.md](archive/black-floor.md) §2), so this matches.
