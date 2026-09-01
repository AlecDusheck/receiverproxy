# The compiled parameter image at flash 0x70000 — format

Recovered by correlating the factory compiled block (primary-region.bin
0x70000..0x78000) with the factory .rcvbp stored at 0x78000 on the same dump.
Round-trip proof: `compile_rcvbp.py factory-config.rcvbp` reproduces the
factory block with **0 byte diffs** over all 0x8000 bytes.

## Headline findings

1. **The factory .rcvbp on the card is byte-identical, record for record, to
   `firmware/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`.** Every shared record
   (0x0a01, 0x0a03, 0x0a84, 0x0a8a, 0x0aca, … all 15) is equal; the target
   file merely adds two all-zero tables (0x0ad8, 0x0ada). The card shipped
   configured FOR THIS PANEL. Therefore the compiled block on the card is
   already the correct compiled form of the target config — synthesizing a
   "new" one yields the identical 32KB. (Confidence: high — direct byte
   comparison.)

2. **Page 0 of the image is the vendor's complete basic parameter pack**, the
   0x104-byte type-0x05 pack minus its 4-byte header (marker 0xA8 at +0,
   chip-id escape 0xFE at +0x1B, chip id 0x014C at +0xE7..E8 — three
   independent anchors to the §7.3/§21.2 wire-pack layout, all at a constant
   −4 shift). This is the pack with every unresolved accessor **computed by
   the vendor's own code** for this exact panel. It can be sent on the wire
   verbatim: `05 00 00 <subidx>` + page0. (Confidence: high on the
   identification; the correct sub-index byte for sending remains the known
   open question — 0x02 per the table, 0x00 per the empirically-arming pack.)

## What page 0 says about how this panel is really driven

Decoded via the §21.2 table (pack offset = page offset + 4):

| field | value | meaning |
|---|---|---|
| module W/H pair | 0x20, 0x80 | module is 128×32, two of them (count byte = 2) → 128×64 |
| GetScanMode | **8** | the compiled scan denominator is 8, NOT the 16 in record 0x0aca |
| GetOneScanLen | **0x100 = 256** | one scan line is 256 clocks — the 256-wide fold is real |
| GetCardScanLen | 0x200 = 512 | |
| GetColorSwap | 0xC6 | |
| gray level | 0x0E | |
| RgbSelValue | 0x10 | |
| chip id | 0x014C | SM16269S |
| current percent | 0x20 | |
| pack+0x54..0x74 | 0x10..0x1F, 0x00..0x0F | 32-entry scan-line remap, second half first |
| pack+0xB4..0xD4 | 0x20..0x3F | remap continues: entries 32..63 |
| pack+0x100 dword | 74 A9 51 A3 | unresolved computed field (NOT the §14 CRC of the page — tested) |

## Image layout (page = 256 bytes, 128 pages)

| pages | content | how to generate |
|---|---|---|
| 0x00 | basic-pack body (see above) | template (computed fields incl. +0xFC dword) |
| 0x01–0x04 | written zeros | zeros |
| 0x05 | bytes 0x00–0x40 = record 0x01 payload[0x19A..0x1DB] (a 0x40..0x7F ramp); then zeros; three 0x01 bytes at page+0xEA/0xF0/0xF6 whose source is NOT RESOLVED (pattern matches record 0x0aca payload[4..], 6-byte stride) | template |
| 0x06–0x08 | written zeros | zeros |
| 0x09 | 0xFF | never written (erased flash shows through) |
| 0x0A–0x0C | written zeros | zeros |
| 0x0D–0x0F | 0xFF | never written |
| 0x10–0x17 | written zeros | zeros |
| 0x18–0x27 | gamma LUTs: **two** copies of the 1024-entry u16-BE identity ramp 0x2000,0x2001,…,0x23FF | generated (exact; factory gamma records are all zero → identity ramp; which copy is which table is NOT RESOLVED) |
| 0x28–0x2F | 0xFF | never written |
| 0x30–0x5F | **pixel mapping table**: record 0x0a03 payload minus its 2-byte count header; 4096 entries × 3 bytes; per entry `(flag, lo, hi)` → `(flag, hi, lo)` (the u16 goes LE→BE) | generated (proved: 0 diffs across all 4096 entries) |
| 0x60 | a u32-BE table of small values (0x00–0x0F), data to +0xC0; source NOT RESOLVED (not found in any record; plausibly derived scan order) | template |
| 0x61–0x62 | written zeros | zeros |
| 0x63 | zeros, then at +0xA0 a 0x00..0x0F ramp, at +0xC1 a 31-byte rising curve ending 0x2F; source NOT RESOLVED | template |
| 0x64–0x67 | 0xFF | never written |
| 0x68–0x7F | written zeros | zeros |

The FF/zero split is meaningful: the vendor's save path erases block 7 (whole
block → 0xFF) and then writes only the pages its writers cover; FF pages were
simply never written. Reproducing the FF pages matters only for byte-exact
flash comparison, not (as far as we know) for function.

Confidence: layout/derivations marked "generated" are high (byte-exact proof);
"template" pages are exact copies so equally safe for THIS config, but cannot
yet be recomputed for a config whose gating records differ.

## Generator

`compile_rcvbp.py <in.rcvbp> <out.bin> [--full-block <out64k.bin>]` — refuses
any input whose records 0x0a01/84/8a/ca/8e/83/89 differ from the factory copy
(template pages could then be stale) or that adds non-zero records. For the
target SM16269S file it runs clean and the output is byte-identical to the
factory block. `--full-block` additionally embeds the input file, u32-LE
length-prefixed, at +0x8000 (the vendor clamps at 0x6FFC bytes) with 0xFF
elsewhere — a drop-in 64KB block-7 image.

## Writing to the card (NOT done here)

The vendor path (docs §13.5): erase block 7 (type 0x06, opcode 0x23, addrHi
0x07 — kills BOTH the compiled image and the .rcvbp at +0x8000, so always
write both back), then 256-byte page writes (opcode 0x85, addrHi 0x07,
addrLo = page). The repo's `write-config` / `restore-flash` already speak
this. Page 0xF0 is EEPROM-redirected and unwritable — expected to fail,
use `e120 screen-size --set … --commit` for geometry. Since the card already
holds these exact bytes, no write is needed today; `sm16269s-block7.bin`
exists as a restore artifact.

## Artifacts (this directory)

- `factory-compiled-raw.bin`, `factory-config.rcvbp` — extracted ground truth
- `compile_rcvbp.py` — generator (round-trip proven, 0 diffs)
- `sm16269s-compiled.bin`, `sm16269s-block7.bin` — generated for the target
  config (identical to factory compiled block, as expected)
- `factory-basic-pack-body.bin` — page 0, the vendor's complete basic pack body
- `basic-pack-payload-sub00.bin` / `basic-pack-payload-sub02.bin` — ready
  wire payloads (`05 00 00 <sub>` + body) for `e120 raw-send`
- `region-6000.bin` — the unresolved page-0x60 table for later study
