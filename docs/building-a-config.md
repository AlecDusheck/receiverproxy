# Building a config for a panel

Configs are generated from a declarative panel spec — nothing is patched by
hand. Every byte in the outputs comes from a spec field, a decoded formula,
or a named template record, and the provenance file says which.

```sh
e120 gen-config --spec panels/p25-128x64-sm16269s.toml --out-dir build
#   build/<name>.rcvbp             the config source (records)
#   build/<name>-basic-pack.bin    page 0 of the boot image
#   build/<name>-block7.bin        the complete 64 KB boot image
#   build/<name>-provenance.txt    the source of every placed byte

e120 restore-flash build/<name>-block7.bin --commit   # page 0xF0 refusing is expected
e120 screen-size --set 128x64 --commit
e120 reload-params --full                             # vendor's 0x77 apply; or power-cycle
```

## The spec

See `panels/p25-128x64-sm16269s.toml`. Sections: `[module]` (width, height,
scan, gray bits, serial clock, line direction), `[screen]` (the whole screen
this card drives — MaxWidth/MaxHeight), `[chip]` (vendor id, optional
register donor), `[color]` (swap index and R/G/B source), `[current]` (gains,
percents), `[timing]` (gamma, refresh, GCLK), `[template]` (the config whose
non-derived records are reused, the reference basic pack and block), `[boot]`
(whether to install the chip page so the card arms at power-on).

## What is derived, and from where

| Output | Source | Confidence |
|---|---|---|
| record 0x01 geometry, scan, clocks, gray, chip id, colour, gains, gamma, refresh, screen size | spec → offsets in `docs/record-0x01-fields.md` (vendor loader/serializer, instruction level; corpus-validated) | high |
| record 0x01 remaining bytes | template record; every byte named or classified in the field dictionary; ~49% constants | carried |
| record 0x03 (pixel mapping) | template — the vendor-consensus mapping 34 known-good 128x64/16 configs share | high (consensus), not generated |
| record 0x84 (chip registers) | template — the only SM16269S register set known (matches a vendor preset 31/32) | carried; colour permutation on install NOT RESOLVED |
| basic pack: module dims, module count, scan, gray, serial clock, OneScanLen, CardScanLen, colour byte, gains, chip-custom block, screen size, chip id | formulas from `GetBasicParam`, each reproducing the factory bytes (pinned by tests) | high |
| basic pack remaining ~100 bytes | reference pack (vendor-computed for this chip/clock) | carried |
| image regions | generated (`docs/compiled-image-format.md`): zeros where the vendor's gates fail, data-swap, module positions, anti-void counters, mapping | high (factory rebuilds byte-exact) |
| scan table (0x400 bytes) | reference block; solver untranscribed, input is width-dependent | carried, flagged |

Tests pin the generator to reality: the spec for our panel reproduces the
hand-derived pack byte-for-byte; a 2-wide screen reproduces the seller's
factory pack; the factory image rebuilds from erased flash.

## Why the seller's config was wrong

The card shipped configured for a 256x384 wall (2x6 of these modules): screen
size, module count and CardScanLen in the boot pack, an all-zero module
position table (the wall exceeds the vendor's 64-tile cap), and a pixel
mapping that is a lone outlier against the vendor corpus. The panel is one
128x64 module at 1/16. Earlier notes claiming a 1/8-scan boot pack were a
field-offset misread; scan was always 16.

## Limits (honest list)

* Mapping and chip-register records are reused, not generated; the spec
  refuses module geometry the template cannot express.
* The scan table is carried; if the panel shows scan-timing symptoms with a
  correct mapping, this is the first suspect.
* The chip block's colour-swap register permutation is not transcribed.
* Module-position index bytes: row/column assignment is medium confidence.
