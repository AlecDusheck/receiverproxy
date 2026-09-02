# The parameter path

How configuration reaches the card, what the card stores where, and every
constant the gateware is known or suspected to compare against.

Regenerable artefacts (not kept in the repository; see
[decode-method.md](decode-method.md)): `analysis/fpga/basic-pack-fields.tsv`
(the complete 256-byte pack field table), `parameter-transports.tsv` (every
wire transport plus the flash boot-image region map),
`constants-searched.tsv` (every constant, tagged FOUND / NOT FOUND / NEVER
SEARCHED).

## 1. Transports

### 1.1 Live raw Ethernet, RAM only

All frames use destination `11:22:33:44:55:66`, source `22:22:33:44:55:66`,
with the EtherType slot (frame offset 12-13) used as a packet type.

Two framing conventions coexist:

* Control and parameter frames put their payload at frame offset 14.
* FPP-derived pixel frames (`0x55`, `0x01`, `0x0A`) put their first data byte
  at frame offset 13, inside the second EtherType byte.

A pixel frame built with the control-frame convention degrades the card into
a 5 Hz strobe (measured on this bench).

The real-time push, in the vendor's order (`crates/cli/src/params.rs`,
matching `GetParamPacksBasic` @ `0x31f1e0` and `SendRealTimePacks` @
`0x32cf40`), is 41 packs; `rxp config send` sends it:

| order | type | sub | body | content |
|---|---|---|---|---|
| 1 | `0x05` | 1 | `0x100` | chip-register pack = record 0x84 verbatim; the pack that arms the drivers |
| 2 | `0x05` | 2 | `0x100` | data swap |
| 3 | `0x05` | 0 | `0x100` | basic parameter pack |
| 4 | `0x10` | 0 | `0x400` | void table |
| 5 | `0x17` | 0 | `0x300` | module positions (5-byte header) |
| 6-21 | `0x03` | 0-15 | `0x300` each | pixel sequence = record 0x03, the mapping |
| 22-25 | `0x1F` | 0-3 | `0x400` each | void line (8-byte header) |
| 26-33 | `0x32` | 0-7 | `0x400` each | anti-void line (8-byte header) |
| 34 | `0x18` | 0 | `0x400` | scan table |

Then the content stream: `0x55` pixel rows, `0x0A` brightness, `0x01 0x07`
display/latch. The vendor sender emits the latch twice per refresh on
firmware >= 13; the measured default for this card is brightness, rows, a
500 us gap, three latch frames ([../rendering.md](../rendering.md)).

There is no acknowledgement, no status word and no commit frame for
real-time packs (two full searches of the SDK found none). They take effect
on receipt; the only feedback is the panel and the supply current. Pushed
after boot, the packs do not all reliably land; the flash path below is the
reliable one.

### 1.2 Flash writes

Type `0x0600`, opcode at payload+3:

| opcode | operation |
|---|---|
| `0x44` | read |
| `0x23` | erase whole 64K block |
| `0x85` | write one 256-byte page |
| `0x79` | reload parameters without a power cycle (no data); `rxp card reload` |
| `0x77` | the vendor's post-save reload, with `01 01 01` at payload+8 (inferred); `rxp card reload --full` |

Block and page at payload+5/+6, data from payload+8. Only block `0x07`
(`0x70000`-`0x7FFFF`) carries parameters. Page `0xF0` (`0x7F000`) is the
EEPROM mirror holding the screen-size record and is reachable only through
the linear frame type `0x1900` ([../eeprom-map.md](../eeprom-map.md)).

Firmware lives in blocks `0x00`-`0x0A` with a golden copy at block `0x20`,
written with type `0x2600` opcode `0x62` after unlocking with type `0x2300`
payload[3] = `0xFF`.

### 1.3 Boot read

The compiled image at flash `0x70000` is a fixed-offset scatter of pack
bodies: no framing, no lengths, no terminators, no checksums beyond the basic
pack's own CRC-32. The region map is in
[flash-layout.md](flash-layout.md#6-the-compiled-boot-image-sits-at-absolute-flash-0x070000-high);
`crates/rcvbp/src/image/` reproduces it byte-exactly, verified against
`card-dumps/primary-region.bin`.

`CCLK.MODE = USRMCLK` and `OSC.MODE = OSCG` are set in all five images, so
the FPGA drives the configuration flash's SPI clock at runtime and can read
flash while the design is running. That the card reads block 0x07 at boot is
inferred from behaviour: the discovery reply tracks flash contents, erasing
block 0x07 changes it, and a configuration written to block 0x07 renders
identically on three of three power-cycles.

## 2. The 256-byte basic parameter pack

The full table is `analysis/fpga/basic-pack-fields.tsv`: 57 fields with body
offset, pack offset, size, endianness, meaning, source record byte and
confidence.

Offset convention: `body_off` is the offset into the 256-byte body, which
equals the boot-image offset because the body starts at image `0x0000`.
[../record-0x01-fields.md](../record-0x01-fields.md) uses
`pack_off = body_off + 4`, because on the wire the body is preceded by the
4-byte header `[0x05, 0x00, 0x00, sub]`. The TSV carries both columns.

Verified byte-for-byte against `card-dumps/primary-region.bin` at `0x70000`:

| body offset | field |
|---|---|
| `+0x00` | marker `0xA8` |
| `+0x01..0x03` | the `FF FF FF` head triple |
| `+0x04..0x05` | module geometry, order swapped by line direction: `[W, H/2]` for line_dir >= 2, `[H/2, W]` for line_dir < 2. Factory `20 80` = `[32, 128]`, line_dir 0 |
| `+0x07` | scan denominator (`0x10` = 16) |
| `+0x08` | grey bits (`0x0E` = 14) |
| `+0x09` | serial clock, BE (`0x0008`) |
| `+0x0B` | OneScanLen BE = 256 |
| `+0x0D` | CardScanLen BE = 512 factory / 256 for this spec |
| `+0x10` | colour byte `(swap<<6) | (s2<<4) | (s1<<2) | s0`; factory `0xC6` = swap 3, source (2,1,0) |
| `+0x1B` | chip id, or the literal escape `0xFE` when the id >= `0x100` |
| `+0x30..0x33` | the four current gains |
| `+0x48..0x4F` | luminance level split by colour percent as R, B, rest, G (u16 BE each), not in RGB order |
| `+0x91..0xA4` | the 20-byte `SChipControl` block, a chip-library constant ([../chip-control-block.md](../chip-control-block.md)) |
| `+0xE7..0xE8` | the full 16-bit chip id, big-endian, zero when it fitted the byte slot |
| `+0xFC` | CRC-32 (poly `0xEDB88320`) over `body[0x00..0xFC]` with bytes `0x1B`, `0xE7`, `0xE8` forced to zero |

The chip id is excluded from the pack checksum, so sweeping the chip id
needs no CRC recomputation.

Bytes not listed in the TSV are never written by `GetBasicParam` and stay
zero.

## 3. Stored versus recomputed

### Shipped precomputed by the host

The card is handed finished tables and does not derive them:

| table | boot-image offset | wire pack | size |
|---|---|---|---|
| Pixel mapping (record 0x03) | `0x3000` | type `0x03` x16 | 4096 x 3 B |
| Scan table (PWM level schedule) | `0x6000` | type `0x18` | `0x400` |
| Chip registers (record 0x84) | `0x0900` | `0x05`/sub 1 | `0x100` |
| Module positions | `0x0600` | type `0x17` | `0x300` |
| Data-swap / lane map | `0x0500` | `0x05`/sub 2 | `0x100` |
| Anti-void-line counters | `0x1800` | type `0x32` x8 | `0x1000` |
| Void / void-line tables | `0x0100`, `0x1000`, `0x6800` | `0x10`, `0x1F` | zeros in the factory image; the void-line column table gates phantom positions on this module ([../rendering.md](../rendering.md)) |
| Current segment | `0x0A00` | (none) | zeros for this chip id |

### Derived host-side and baked in as scalars

The card never sees the formula: OneScanLen, CardScanLen, module count,
modules-in-line-dir, `GetModuleInputCount`, screen extent in line dir, the
luminance current split, and the grey depth (derived from chip registers
`0x07`/`0x03`).

OneScanLen and CardScanLen are products of fields the card already has
(`+0x04..0x07`); shipping them is redundancy.

### Gamma

Record 0x01 carries a gamma float at `+0x01C` (2.8 on this card). The
corpus's gamma/calibration records (`0x07`, `0x86`, `0x8d`, `0x8e`, `0x8f`,
`0x91`, `0x95`, `0xcd`, `0xd8`, `0xda`) are all zero in an uncalibrated
profile, and a separate `0x85`-opcode "write gamma table" path exists. On an
uncalibrated card the gateware applies gamma from the scalar, or not at all.

The gamma LUT is not a boot-time ROM: the BRAM sweep found no 256- or
1024-point ramp anywhere in the one initialised block RAM. Where gamma is
applied is not resolved.

### Runtime flash reads

Given `USRMCLK`, all of the boot-image regions above are read at minimum on
reset. Total live table about 29 KB (`0x100` basic + `0x100` swap + `0x300`
positions + `0x100` chip + `0x3000` mapping + `0x400` scan table + `0x1000`
anti-void). Against 53 instantiated EBRs, about 119 KB of on-chip RAM, with
no external DRAM, that fits. "Read once into BRAM, not streamed per frame" is
a capacity argument, inferred.

## 4. Constants the card compares against

Full table: `analysis/fpga/constants-searched.tsv`.

| status | constants |
|---|---|
| **FOUND** | only bitstream-framing constants: IDCODE `0x41111043`, preamble `BD B3`, CRC-16 poly `0x8005`. These are hard IP, not fabric logic |
| **NOT FOUND** | every driver-chip id (`0x014C`, `0x0214`, `0x0187`, `0x0215`, `0x00DE`, `0x00FD`, `0x013C`), the `0xFE` escape, every packet type and EtherType, the Ethernet SFD `0xD5`, and the card MAC |
| **NEVER SEARCHED** | the basic-pack marker `0xA8`, the pixel-row magic `08 88`, the pack CRC-32 polynomial |

Search coverage: every LUT4 INIT in all five images corrected for
constant-tied inputs, plus CCU2 carry chains, one-hot decoder groups, and the
microcode ROM. Method and detail in [chip-id.md](chip-id.md).

### Interpretation of the negative

The positive control failed: constants that must be compared (the EtherType,
the SFD) are as invisible as the chip id. The correct statement is "this
design does not build constant comparisons out of LUT4s", not "the chip id is
never compared". The surviving hypothesis is a BRAM or LUT-RAM register file
loaded by the packet parser and compared data-vs-data, which matches the
design's 117 CCU2 XNOR comparators (inferred).

Measurement shows the gateware branches on the chip id: `0x014C` arms the
outputs and `0x0214` leaves the panel dark
([../rendering.md](../rendering.md)). The LUT-level negative is a search
limitation, not a property of the design.

### Parameter store location

Not identified. `R27C44_Q0..Q3`, a 4-bit field with no combinational source
feeding a 10-bit decoded mode bundle read by 734 LUTs, is not the parameter
store: it is an ordinary 8-bit CCU2 accumulator that appears sourceless
because CCU2 carry travels on fixed, non-configurable wires
([chip-id.md](chip-id.md)). Finding the store turns "which chip ids does the
gateware recognise" into "which stored byte feeds the mode selector". See
[open-questions.md](open-questions.md).
