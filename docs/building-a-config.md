# Building a config for a panel

A panel is described once in `config/panels/<panel>.toml`. Everything the
card consumes is generated from that file and a chip library in
`config/chips/`. No donor file is involved: each output byte is a vendor
default, a spec field, a chip-library value or a documented literal, and the
generated sources file names the origin of every placed byte.

## Commands and outputs

```sh
e120 config gen --spec config/panels/p25-128x64-sm16269s.toml --out-dir build
#   build/<name>.rcvbp             the config (17 records, in the vendor's order)
#   build/<name>-basic-pack.bin    page 0 of the boot image, with its CRC-32
#   build/<name>-block7.bin        the complete 64 KB boot image
#   build/<name>-sources.txt       the origin of every placed byte

e120 flash restore-block build/<name>-block7.bin --commit   # page 0xF0 refuses; expected
e120 card screen-size --set 128x64 --commit
e120 card reload --full                                    # vendor opcode 0x77 apply; or power-cycle
e120 config send --spec config/panels/<panel>.toml         # RAM packs only, no flash
```

`e120 provision --spec <spec> --commit` runs the flash path end to end
([provisioning.md](provisioning.md)). Configure from flash; `config send`
pushes RAM packs and lands on about one boot in three ([rendering.md](rendering.md)).

## The spec

| table | fields |
|---|---|
| `[module]` | width, height, scan, line direction, data groups, optional serial clock and gray override |
| `[screen]` | the whole screen this card drives |
| `[chip]` | `library`, a file in `config/chips/` |
| `[color]` | swap and R/G/B source |
| `[current]` | gains and percents |
| `[timing]` | gamma, refresh, GCLK, minimum OE, luminance level, 8 ns OE |
| `[mapping]` | the two wiring knobs (`block`, `gate_phantom_positions`) |
| `[boot]` | arm at boot |

## The chip library (`config/chips/*.toml`)

A library holds the family id and sub-variant id, the vendor's default
serial clock, the 20-byte chip-control block the config carries, and the
default register table with the register field layout in comments. Record
0x84 is that table in order with register 0x02 patched to `scan - 1`, as the
vendor's loader does. The gray depth is derived from registers 0x07 and 0x03
(`GetSupporttedGray`).

| library | contents |
|---|---|
| `sm16269s-factory.toml` | family `0x14C`, no sub-id, the register table read out of the reference file. The library the bench panel renders with |
| `sm16169sh.toml` | the same table with the reference file's reg 0x07 value |
| `sm16269.toml` | `0x14C` with sub-id `0x14D` and the vendor tool's "Default Parameter" table. `0x14D` is the vendor's SM16380SH id, not an SM16269 variant; this table renders worse on the bench panel ([chip-control-block.md](chip-control-block.md) section 7, [rendering.md](rendering.md)) |
| `sm16169s-vendor.toml`, `sm16269s-vendor-0x214.toml` | non-SH parts, see below |

Non-SH parts such as SM16169S (`0x00DE`) have no register table and no
record 0x84; their whole configuration is the 16-byte `SChipCustom` block of
record 0x01. `chips.rs` loads them through `chip_custom`,
`chip_custom_scan_patch`, `chip_custom_ex`, `emit_record_84` and `gray_bits`
([chip-libraries-non-sh.md](chip-libraries-non-sh.md)).

## Derivation of each output

| Output | Source | Pinned by |
|---|---|---|
| record 0x01 | vendor write-side defaults (`CHWParamRcvGeneral::Reset/ResetIS/ResetSwapData`), the spec, the chip library (family/sub id, chip control, reset serial clock), and 11 documented literals for bytes whose meaning is unresolved (`spec/record01.rs`) | the reference file regenerates byte-exact |
| record 0x03 (mapping) | geometry: pixel to (`row % scan`, `group * width + col`) with the vendor's reversed group order; `[mapping] block` selects the run length ([panel-wiring.md](panel-wiring.md)) | `block = 128` reproduces the 34-config consensus; `block = 64` reproduces the reference file |
| record 0x84 (chip registers) | chip library, with `reg 0x02 = scan - 1` | reference file equality |
| other records (0x8a, 0x83/0x89, 0xca, 0xcd, 0x8f, 0x07, 0x86, 0x8e, 0x8d, 0x91/0x95/0xd8/0xda) | decoded loader defaults (`spec/records.rs`); 0x8a mirrors the screen size, 0xca the module geometry | reference file equality |
| basic pack (all 256 bytes) | `GetBasicParam` transcribed field by field from record 0x01, plus the CRC-32 trailer | factory pack byte-exact |
| boot image | every region generated (`image/`): gated zeros, data-swap, module positions, anti-void counters, mapping, scan table (bit-time solver), embedded `.rcvbp` | factory image byte-exact |

Tests in `crates/e120-rcvbp/tests/factory.rs`:

| test | asserts |
|---|---|
| `the_reference_config_is_regenerated_record_for_record` | the reference config regenerates record for record from a spec |
| `the_reference_config_reproduces_the_factory_pack_byte_for_byte` | that spec reproduces the factory pack |
| `the_factory_image_rebuilds_from_erased_flash_and_its_own_parts` | that spec reproduces the factory image |
| `our_panel_differs_from_the_reference_only_where_intended` | the single-module spec differs from the reference only in the intended bytes |
| `the_bench_spec_displaces_the_phantom_positions` | the void-line column table gates positions `width..2*width` |
| `the_scan_table_is_invariant_to_the_load_width_for_this_chip` | scan table independent of load width |
| `a_single_module_screen_gets_a_module_position_table` | one module gets a position table |
| `the_default_block_gives_the_vendor_consensus_table` | `block = 128` is the corpus consensus |
| `the_reference_mapping_is_reproduced_by_the_block_knob` | `block = 64` is the reference file's record 0x03 |
| `a_scan_that_does_not_divide_the_module_is_refused` | invalid scan is an error |

The factory pack and image tests need the card's factory flash dump, which is
kept outside the repository; they skip without it.

## The reference config and the bench panel

`third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp` is compiled for
a 256x384 wall (2x6 of these modules). Its screen size, module count and
CardScanLen (512) in the boot pack are the wall's; its module-position table
is all zero because the wall exceeds the vendor's 64-tile cap; it carries the
SM16169SH register set with the sub-variant id unset. Its pixel mapping
(`block = 64`) is the correct wiring for this module; the corpus consensus
(`block = 128`) scrambles every column ([panel-wiring.md](panel-wiring.md)).
The single-module spec keeps the mapping and the register table and replaces
the wall geometry: CardScanLen 256, one module, a generated position table.

## Limits

* Eleven record-0x01 bytes and a few small-record bytes are literals whose
  meaning is not resolved (source known); they carry the reference file's
  values.
* The scan-table solver is transcribed for the default style, 16 segments,
  14-bit gray. Other gray depths need their hand-coded vendor blocks.
* Module-position generation covers the plain grid (split segment 1).
* The current-exchange page (`0xC00`) is written as zeros. The vendor's
  `GetCurrentExchangeParam` builds a hub-data-group to module-index map, one
  byte per group; with a single module every group maps to module 0, so the
  vendor's output is also all zeros.
