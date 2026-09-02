# The LED output stage

What the E120 emits to the driver chips: the connector, the driver register
file, the scan arithmetic, the scan table, the pixel mapping, the output
stage as it appears in the 16.53 netlist, and the measurements that bear on
each. Artefacts: `analysis/fpga/sm16269-register-map.tsv` (the
33-register file, both vendor tables side by side, per-field bit meanings,
grey-depth derivation), `analysis/fpga/output-stage-arithmetic.tsv`
(OneScanLen / CardScanLen / mapping / scan-table numbers for the reference
module, with verification results), `analysis/fpga/minoe_corpus_survey.py`
(the 370-file corpus survey), `analysis/fpga/output_stage_16.53.txt`,
`analysis/fpga/rgb96_pins.txt`, `analysis/fpga/led_pin_classification_16.53.txt`,
`analysis/fpga/pad_driver_logic_16.53.tsv`, `analysis/fpga/build_comparison.txt`,
`analysis/fpga/negative_results_and_method.txt`. The `analysis/fpga/` tree is
not kept in the repository ([README.md](README.md#raw-artefacts)).

Bench panel throughout: one P2.5 128x64 module, 1/16 duty, SM16269S drivers,
on hub J1, firmware 16.53. Every timing statement in this file is derived
from tables and netlists; no waveform has been measured (no
scope, no logic analyser).

## 0. Where the driver protocol lives

Colorlight ships a different FPGA bitstream per LED driver chip family
(`third-party/README.md`, corroborated by the firmware filenames and the
vendor download-page structure). There is no runtime setting that changes
the family: the Normal / PWM / LS split is gateware.

Within a family the chip id in the parameter pack selects a protocol
descriptor. The id is a mode selector into gateware that may or may not
implement the chip, not a tuning parameter. Measured: the panel is dead on
Normal 13.39 (0.44 A) and responds on PWM builds; that is a protocol-level
result. How the id reaches the card and what it selects is in
[chip-id.md](chip-id.md) and [chip-protocol-microcode.md](chip-protocol-microcode.md).

## 1. The connector and the driver chip

### Connector

* The panel connector is HUB75E, 16-pin. The card has twelve, J1 to J12.
  The reference module is driven from J1. Signal set (vendor spec p. 4): `RD1 GD1 BD1` /
  `RD2 GD2 BD2` (two RGB data groups), `A B C D E` (five address lines),
  `CLK`, `LAT`, `OE`, `GND`.
* The E120 drives 24 groups of parallel RGB data or 32 groups of serial RGB
  data (vendor spec p. 2) = 12 hubs x 2 groups.

### Driver register file

* The SM16269 / SM16169SH register file is 33 registers, each carrying three
  values (R, G, B); the chip is configured per colour channel. Record 0x84 is
  the flat `(reg, R, G, B)` quad stream in library order, zero-padded to 256
  bytes.
* Register `0x02` is patched at load time to `(scan - 1) & 0x3F` = 15. The
  driver chip is told the scan depth: this chip family runs its own line
  counter.
* Grey depth is derived from the registers, not declared:

  ```
  g     = 128 << ((reg07 >> 3) & 3)
  m     = (reg03 < 0x40) ? 64 : 32
  total = m * g
  ```

  Reference configuration: `reg07 = 0x44` gives `g = 128`; `reg03 = 0x3F`
  gives `m = 64`; `total = 8192`, 14-bit. This matches basic-pack
  `+0x08 = 0x0E` in the reference file and the Eager module datasheet's
  "Gray Grade: 14 bits" and "Refresh Frequency >= 3840 Hz". The spec in
  `config/panels` declares `gray_bits = 12`; from flash, 12 to 16 render
  identically on this chip ([../rendering.md](../rendering.md)).

### Register field meanings

From LEDVISION's `ChipSetting.dll` dialogs IDD20173/IDD20174, recorded in
`config/chips/*.toml`. These are dialog labels, not a datasheet; inferred.

| register | field |
|---|---|
| `0x03[7:6]` | low-grey high-refresh level; `< 0x40` selects the x64 grey multiplier |
| `0x07[7:5]` | frequency-division factor - 1; `[4:3]` line grey; `[2:0]` blanking coarse/fine |
| `0x13[3]` | low-grey-high-refresh enable; `[4]` photo optimisation |
| `0x16[5:0]` | per-colour current gain 0-63 (the dialog's sliders) |
| `0x18[1:0]` | blanking ghost level |
| `0x19[2:0]` | blanking level |
| `0x1a[2:0]` | first-line dark compensation |
| `0x20[7:6]` | small period num |
| `0x0a[7]` | standby mode, inverted (0 = on) |
| `0xf0[7:4]` | black-screen standby (`0xA` = on) |

### S-PWM structure

The `m x g` factorisation is the S-PWM (scrambled-PWM) structure. A 14-bit
grey value is not emitted as one 8192-count burst; it is emitted as 64
interleaved sub-periods of 128 counts each ("line grey" x "grey
multiplier", with `0x20[7:6]` "small period num" as a further subdivision
control). That is how a driver reaches >= 3840 Hz visual refresh from a
~60 Hz frame rate, and why the module datasheet claims 14-bit grey at 1/16
duty. The arithmetic is in `chips.rs`; the "64 interleaved sub-periods"
reading is inferred.

### Serial protocol

The driver's serial protocol is described to the card by the 20-byte
`SChipControl` block of record 0x01 (`+0x0C4`), whose accessor is
`SetGclkNumsOfChipControlByChipCustom`. Value for the reference configuration,
identical in both chip libraries:

```
00 0e 01 05 06 01 03 00 00 00 00 97 00 97 00 08 02 00 0a 02
```

Byte 1 = `0x0E` = 14 is the pre-activation LE tail; bytes 10..13 =
`0x0097` = 151 are the scan-cycle level (GCLK/RCLK count per row), computed
from register `0x07`. The full decode (pre-activation tail 14, register tail
5, second-command tail 6, data-latch tail 1, VSYNC tail 3) is in
[chip-protocol-microcode.md](chip-protocol-microcode.md) and
[../chip-control-block.md](../chip-control-block.md). Facts from the SM16269
datasheet (`third-party/datasheets/SM16269_ZIGZZZAV10_datasheet_2025-08.pdf`):
a register write is 16 bits, MSB first, on the RGB lanes; the chip has no
GCLK and no OE pin; pin 21 is `RCLK`, both the grey clock and the row
advance, so the HUB75 OE wire carries a pulse train.

There is no SM16269S / SM16169SH datasheet in this repository;
`third-party/datasheets/` holds the E120 spec, the Eager module spec and the
SM16269 datasheet. Facts about the MBI/ICN/SM family in general are not
applied to this part. Not resolved from any document here: OE polarity on
this part, and the LAT-to-CLK phase relationship.

<a id="2-scan-handling-high"></a>
## 2. Scan handling

All numbers in `analysis/fpga/output-stage-arithmetic.tsv`.

The record stores half the module height (32, not 64) because a 128x64
HUB75 module is driven as two vertical halves on the two RGB groups
(`RD1/GD1/BD1` upper, `RD2/GD2/BD2` lower). Then:

```
OneScanLen  = W x (H/2) / scan  = 128 x 32 / 16 = 256
              clocks per scan address, per data group, per module
CardScanLen = OneScanLen x modulesInLineDir = 256
              (256x384 reference wall: 512, for 2 modules chained)
```

At 1/16 scan, 4 of the 64 rows are lit per address, 2 rows in each half. Per
address, per data group, the card shifts 2 rows x 128 columns = 256 pixels,
which is OneScanLen. The five address lines A-E can express 1/32; four are
needed here.

`CardScanLen` = 512 is wrong for a single module: that value belongs to the
2-modules-in-line-dir wall the reference file was compiled for. With it the
card shifts 512 clocks per address into a 256-pixel chain, a first-order
raster corruption. The generated config carries 256.

<a id="3-the-scan-table-high-and-a-genuinely-new-result"></a>
## 3. The scan table

`GetScanTable` @ `0x1eabc0` calls `CalScanTalbeDefault` @ `0x14d710`,
transcribed in `crates/rcvbp/src/image/scan_table.rs`. It computes:

1. `InitFieldTable16Segment` picks which grey level gets the full 16-slot
   segment, and takes the other 13 levels from a hand-coded per-grey-depth
   block (only 14-bit is transcribed; other depths need their own vendor
   blocks).
2. `FromSegmentToFrameTime` @ `0x1d0a70` assigns each enabled slot a bit time
   `2^level * minOE / segments`, snapped to 8-unit quanta with the vendor's
   round-half-away-from-zero.
3. `FieldTableToScanTable` @ `0x1d1c00` buckets slots by `slot % nSeg`,
   highest level first, and emits `(level, 24-bit BE value/8)` entries, with
   per-bucket `(start, end)` pairs at `+0x3C0`, the scan mode at `+0x39E`,
   the segment count at `+0x39F` and an identity line order at `+0x3A0`.

Verified: the generated scan table is byte-identical to the factory image's
(`card-dumps/primary-region.bin` at `0x76000` vs
`build/p25-128x64-sm16269s-block7.bin` at `0x6000`), and every one of the
`0xE7` 24-bit value fields is zero in both.

With `minOE = 1e-4`, every snapped bit time rounds to zero. The scan table
as shipped is a level sequence, not a timing table: 152 entries carrying a
level and no duration, plus segment bucketing and line order.

The all-zero bit-time field is the vendor's normal output for modern
configs, not a defect: across 370 vendor `.rcvbp` files
(`analysis/fpga/minoe_corpus_survey.py`, all parsed), `minOE = 1e-4` appears
in 93 files in total and in 24 of the 29 that use the modern 764-byte record
0x01. For a chip that generates its own grey, the card only sequences
levels; the durations live in the chip's PWM engine. The most economical
reading of how the gateware consumes it (inferred): per scan address, walk
the bucket for that segment, and for each `(level, value)` entry drive the
corresponding grey plane / OE window; with zero values the window
degenerates and the chip's internal S-PWM supplies the modulation.

The generated image pairs a 12-bit grey byte with the 14-level table.
Substituting the vendor's own 12-level table raises the black current rather
than lowering it, so the 14-level table stays
([../rendering.md](../rendering.md)).

## 4. Pixel mapping and module positions

### Record 0x03

4096 entries = `W x (H/2)`, one per pixel of one half. For pixel `i` in
raster order over the 128x32 half (`crates/rcvbp/src/spec/mapping.rs`):

```
row    = i / 128
col    = i % 128
line   = row % scan                          -> 0..15,  the SCAN ADDRESS
group  = (storedH/scan - 1) - row/scan       -> reversed_groups = true (vendor default)
groups = storedH / scan                      -> 2
slot   = (col / blk) * (groups * blk) + group * blk + col % blk
                                             -> 0..255, the POSITION IN THAT
                                                ADDRESS'S 256-CLOCK SHIFT-OUT
```

Each entry is `(line u8, slot u16 LE)`; in the boot image the u16 is flipped
to BE and the 4096 entries are sliced into 16 packs of 256. The table is
"framebuffer pixel -> (which of the 16 scan addresses, which of the 256
serial slots)".

`blk` is `[mapping] block` in the panel spec. Two wirings:

| `blk` | slot formula | where it holds |
|---|---|---|
| `W` = 128 (generator default) | `group * W + col`; each data group one contiguous 128-slot run | the vendor corpus consensus: byte-exact against the 34-config consensus for 128x64 @ 1/16, and 1039 of 1517 corpus tables from geometry alone. It scrambles every column on a module whose halves alternate |
| 64 | the chain alternates between the two row-halves every 64 columns | the reference module ([../panel-wiring.md](../panel-wiring.md)); the reference file's record 0x03 regenerates byte for byte. Pinned by `the_reference_mapping_is_reproduced_by_the_block_knob` in `crates/rcvbp/tests/factory.rs` |

`reversed_groups` (234 of 241 two-group vendor configs) means the last
row-group is shifted out first, the standard consequence of a chain that
fills from the far end.

The second half of the module is not covered by record 0x03; it is driven
from the same 4096-entry map on the second RGB group. The arithmetic requires
it; inferred, not independently confirmed.

### Module positions

The screen (MaxW x MaxH) tiled by the 16x16 grid unit, count at `+0x005`,
10-byte entries from `+0x016`: `[outer idx, inner idx, x BE, y BE, w BE,
h BE]`. Four direction variants; line_dir 0/1 compact dropped tiles, 2/3
leave positional holes. The index byte order is inferred.

The vendor emits all zeros when the tile count exceeds 64. The reference
file's 256x384 wall (384 tiles) has an all-zero table; a single module gets a
real one (count `0x20`, 32 entries).

### Data-swap / lane map

Record 0x01 carries four 16-byte swap blocks plus a 64-byte lane ramp; the
generator writes plain identity ramps (`0x00..0x0F`, `0x10..0x3F`,
`0x40..0x7F`). The reference file regenerates byte-exactly from that, so
identity is the vendor tool's output for this module type, and the panel
renders with it. Measured: with the `+0x19A` lane map zeroed, rendering
breaks ([../rendering.md](../rendering.md)).
[../rcvbp-format.md](../rcvbp-format.md) records that swap block 0
(`+0x05A..0x069`) is wholly reordered between a 32S and a 64S variant of the
same module, so it is scan-dependent. Whether identity is the correct block
0 for 1/16 on this module is not resolved beyond "it renders".

## 5. Physical output in the bitstream

* Only 12 of the ~125 LED-side outputs have any IOLOGIC in the PWM builds;
  they are driven from fabric LUT/FF outputs through ordinary routing, not
  through the IO registers. The pipelining is in the fabric, on the CLKOP
  domain plus a fabric-generated global clock (`BDCC0`, fan-out ~660). See
  [resources.md §4](resources.md#4-output-registration).
* The logic is massively replicated: 20 170 used LUT4s but only 2 129
  distinct effective INIT values, with the top handful appearing 600-1300
  times each. That is one datapath slice instantiated hundreds of times,
  consistent with a per-channel serialiser/PWM engine; replication alone does
  not prove what is replicated.
* `IOLOGIC*.MODE = IREG_OREG` appears 96 times in the Normal 13.39 and
  LS0allDA 6.69 builds and 10 times in each of the three PWM builds. 96 = 32
  serial RGB groups x 3 colour lines, the E120 spec's "32 groups of serial RGB
  data". Reading (inferred): Normal builds register all 96 serial data lines
  inside the IO cell because a Normal build generates the greyscale itself and
  clocks data out at bit-plane rates; PWM builds moved that into the fabric
  because the per-line data rate is far lower when the chip does the PWM. §7.1
  confirms the 96 sites are the RGB data pins.

## 6. Measured behaviour

Method: [../bench.md](../bench.md). The settings that make the panel render:
[../rendering.md](../rendering.md).

### Measurements and what each establishes

| measurement | establishes |
|---|---|
| The `test_mode()` frame (`crates/colorlight/src/discovery.rs`) matches `CReceiverOP::SetRcvCardTestMode` @ `0x3d54e0`: type `33 00`, `0x09` at payload+5, selector at payload+6, 279-byte frame | the host's test-mode command is byte-correct |
| On 10.81 all nine test selectors give flat current and indistinguishable output; on 16.53 the selectors give visibly different displays (`rxp card test-mode <n>`, `rxp card test-sweep`) | the built-in generator is inert on 10.81 only. The generator bypasses the host, so a fault visible in test mode is at or below the card's raster stage |
| The physical test button (E120 spec item 5: four monochrome fields plus scan patterns) does nothing when pressed | it is not a diagnostic; the generator is reached over the wire |
| Pixel frames are byte-exact FPP and verifiably on the wire | the host encoder is not a fault source |
| Brightness scales supply current | the card receives and parses the frames, the scan engine runs, OE/current modulation is under control, the driver chips are powered, armed and sinking current |
| An SM16269S panel is dead on Normal 13.39 and responds on PWM builds | SM16269S is a PWM-class (S-PWM, self-scanning, register-configured) driver and only PWM-family gateware speaks its protocol. A Normal build emits a plain shift-register waveform with card-generated bit-plane OE modulation; an S-PWM chip that never receives its register writes stays in its power-on state. 16.53 is the build |
| Chip id `0x014C` renders with the settings in rendering.md and drives the outputs even under a wrong configuration; `0x0214` leaves the panel dark | the card acts on the chip id or on the id-selected descriptor bytes (`SChipControl`, record 0x84); no id comparator is found in the LUT netlist ([chip-id.md](chip-id.md)). `0x014C` (vendor name SM16169SH) is the id 16.53 arms the SM16269S outputs for; `0x0214` (SM16269S's own id) is a dead id in every vendor build and produces an all-zero `SChipControl` and no register table |
| Per-pixel structure under a uniform white fill at `0x014C` | the serial chain loads, the latch fires, the PWM engines run, the current sinks work. On 10.81 the content shown is not host content: the panel changes with no traffic on the wire ([../bench.md](../bench.md), idle test) |
| The EEPROM screen-size record at flash `0x7F000` can read a different size from the one the stored configuration was compiled for | the card's two notions of its own size are independent and can disagree |
| With the EEPROM control area erased (`startX = startY = 0xFFFF`) frames are accepted, the packet counter advances, current changes, nothing displays, and `discover` still reports a healthy size | the card windows its own rectangle from the control area; `0x55` row frames carry an absolute row index and pixel offset ([../receiver-identity.md](../receiver-identity.md)) |

`0x014D` is SM16380SH in the vendor name tables, not an SM16269 sub-variant
([../chip-control-block.md §7](../chip-control-block.md#7-chip-names)).

### Diagnostic notes

* A single lit pixel at (0,0) separates addressing from ordering: a
  scrambled raster and a missing raster look identical under a fill and
  different under one pixel.
* `CardScanLen` set for more modules than are attached is on its own a
  guaranteed raster corruption.
* The card's built-in generator is reached with `rxp card test-sweep`, but
  only with the wire quiet: a concurrent streamer's frames overwrite the
  framebuffer the generator writes into, and its `0x0107` latch frames keep
  firing. Kill every streamer and confirm no frames on the wire first.
* Two register tables for this chip both derive 14-bit grey but differ at
  `reg 0x07` (`0x44`, frequency division 3, against `0x04`, division 1),
  `reg 0x0a`, `reg 0xf0` and `0x0b`/`0x0c`/`0x11`/`0x1b`/`0x1c`/`0x1f`.
  Changing `reg 0x07` requires recomputing `SChipControl[10..13]`
  ([chip-protocol-microcode.md §4.2](chip-protocol-microcode.md#42-the-count-is-a-pack-field)).
* All-zero scan-table bit times with minOE are the vendor norm: 24 of 29
  modern-format configs, and byte-identical to the factory image.
* The data-swap and lane-map identities for 1/16 are not resolved; the
  search space is large and there is no consensus data. The panel renders
  with the identity mapping.
* An all-black frame that leaves a fixed lit pattern is the black floor:
  the card emits `2 x width` positions per line for an interleaved wiring
  and positions `width..2*width` carry no host pixels. It is gated through
  the void-line column table
  ([../rendering.md](../rendering.md#the-black-floor)).

## 7. The output stage in the netlist

Traced from the pads backward.

<a id="71-the-96-rgb-data-pins-are-identified-high"></a>
### 7.1 The 96 RGB data pins

The 96 IOLOGIC sites in 13.39 and 6.69 are byte-identical between those two
builds, and mapping them to package pins gives exactly the 96 left- and
right-edge pads that the pin census classifies as plain fabric-driven outputs
in 16.53 (47 LEFT OUT + 48 RIGHT OUT + 1 LEFT BIDIR). Zero discrepancy in
either direction. 96 = 32 serial RGB groups x 3 colour lines. The list is
`analysis/fpga/rgb96_pins.txt`.

`T4` is one of the 96 RGB pins, not MDIO. There is no MDC/MDIO group
anywhere in this design ([pinout.md](pinout.md#phy-management-none)).

### 7.2 Two master control bits

Counting how often each signal feeds a pad-driver LUT across all 197 pads,
two flip-flops in one slice dominate: `Q5@23,18` (23 pads) and `Q4@23,18`
(20 pads), with duplicates `Q5@39,18` / `Q4@39,18` for the lower half.
Everything else feeds 1-3 pads.

Normalising the pad LUTs against them gives, for 17 of 21 classifiable pads:

```
pad = 0                                 when Q4@23,18 = 0
pad = NOT( Q5@23,18 ? dA : dB )         otherwise
```

* `Q4@23,18` is a global synchronous BLANK.
* `Q5@23,18` is a 2:1 source select.
* The outputs are active-low.

The 96 RGB pads are not gated by this; the blanked group is the top-edge
control pads.

<a id="73-what-the-21-mux-selects-between-high-that-it-is-counter-vs-bram"></a>
### 7.3 The 2:1 mux legs: counter and block RAM

* One leg is always a CCU2 counter at `x = 24..26, y = 7..11`, sharing
  `.CE = F3@26,8`.
* The other leg always terminates on block RAM data out, verified:
  `JQ5@5,25 <- JDOB13_EBR`, `JQ2@4,25 <- JDOB2_EBR`, with probes
  `A3 -> JDOB2`, `A11 -> JDOB13`, `A2 -> JDOB4`, `E6 -> JDOA5`.

Whether the mux selects "test pattern vs live data" or a within-frame
command/data time-multiplex (SM16xxx configuration words vs pixel data) is
not resolved. Both readings fit the structure. The finer decode in
[chip-protocol-microcode.md §4.4](chip-protocol-microcode.md#44-new-netlist-detail-on-the-control-pads-medium)
reads five identical pads as the A-E scan address lines muxing a counter
against the scan table's line order.

<a id="74-the-control-group-source-ram-starts-empty-high"></a>
### 7.4 The control-group source RAM starts empty

Every build contains exactly one `.bram_init` block, and it targets the EBR
with `WID = 3`. The block RAM feeding the top-edge control group is
`MIB_R25C4/C5` EBR0, `PDPW16KD`, `WID = 1`, not initialised.

That RAM comes up empty at configuration time and is written at run time. A
card with no valid parameters loaded scans a buffer of nothing.

No 256-byte parameter-pack store was located. LUT-RAM is ruled out as the
store in 16.53: only 18 blocks of 16x4, against 59 and 89 in 13.39 and 6.69,
too small and too fragmented.

### 7.5 The build difference

The one large output-stage difference between the Normal/LS family and the
PWM family is the `IREG_OREG` count. In 13.39 and 6.69 all 96 RGB pads go
through the IO output register, clocked by PLL CLKOP, with LSR tied on all 96
and CE tied on 72. In the PWM builds only 10 do. Details in
`analysis/fpga/build_comparison.txt`.

### 7.6 Clocking of the output stage

* No pad anywhere is driven from a global clock net. HUB75 DCLK is
  fabric-generated data, not a routed clock.
* The design is single-clock. 98.9 % of flip-flops are on PLL CLKOP in 16.53
  (12 589 of 12 725); the same holds in 10.81 and 13.39.
* There is no slow LED clock domain. The fabric-divided clock distributed on
  a global net is a clock enable, not a clock: `G_HPBX0900` appears as `.CE`
  on output-stage flops. `BDCC0` is not a second clock domain.
* `G_LDCC2CLKI <- G_JOSC` in all five builds: the internal `OSCG` oscillator
  is on a global net.

## Unresolved

* Which top-edge pad carries which HUB75 control signal (A-E, CLK, LAT, OE).
  The RGB data group is identified (§7.1) and the control group is
  identified as a group (§7.2); it is not decomposed.
* Whether the scan counter's terminal count is 16.
* Whether the FPGA drives HUB75 directly or through buffers. All banks are
  3.3 V, so every FPGA IO is 3.3 V; the board has driver ICs visible in the
  vendor photo, and the bitstream says nothing about the far side of a pad.
* The on-wire waveform. Not measured; it needs a logic analyser or a scope
  on the HUB75 header, which a supply and a camera cannot substitute for.
* OE polarity, and whether OE free-runs as the grey clock. `IsChipHasOE`,
  `Is8nsOeEnable`, `Get8nsOeEnableInfo`, `GetMinOE`, `HR_SetMinOE` exist in
  the vendor SDK and bottom out in a chip-library sub-object not resolvable
  through vtable ambiguity. Nothing in `libCLTDevice` selects OE behaviour
  or a GCLK mode, consistent with it being a gateware property that
  configuration cannot change.
* Whether the 2:1 mux is a test/live select or a command/data
  time-multiplex (§7.3).
* The data-swap / lane-map block 0 for 1/16 on this module, beyond the
  measured "identity renders".
* Serial clock 8 vs 15: not swept.
