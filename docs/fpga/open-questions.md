# Unresolved

What is not known about the E120 gateware and card, with what is known
around each point and what would settle it. Negative results are recorded
with the point they bear on: a search that has failed is not worth
repeating.

The panel renders with the settings in [../rendering.md](../rendering.md);
the points below are what remains open behind that.

---

## Tier 1: the pixel path and its configuration

### 1.1 The pixel path in the gateware

**Not known:** which bank the raster reads, which the Ethernet writes, and
what gates either bank's write.

**Known:**

* Every Ethernet frame enters through one of exactly two block RAMs,
  `EBR@39,37` (left PHY RX clock) and `EBR@42,37` (right PHY RX clock),
  1024 × 9 each, the design's only clock-domain crossings. At 1024 bytes
  they cannot hold a maximum-size pixel packet, so the header is decoded and
  the payload consumed while the packet streams.
* The only two candidate destination memories are a Bank A of 8 EBRs
  (2048 × 9, shared `WEA = Q4@21,22`) and a Bank B of 12 EBRs (16 384 bits
  each, shared `WEA = Q4@44,26`). Both start uninitialised and are written
  at run time; the same two-bank shape is in 10.81.
* A double-buffer swap between two equal arrays does not exist: there is no
  third array and the two banks are structurally different, so there is
  nowhere for an un-swapped back buffer to live.
* The memory feeding the HUB75 pads is `EBR@4,25` (`= MIB_R25C4/C5 EBR0`,
  the uninitialised `WID = 1` block of
  [output-stage.md §7.4](output-stage.md)):
  `PDPW16KD`, 512 × 36, `WEAMUX = INV` so `WEA` is tied high, which makes
  `CSA0 ← Q6@9,27` the entire write enable of the output-stage buffer. 512
  entries is a scan/line buffer, not a frame buffer, so there is at least
  one more stage between a bank and the pads.
* The `row` and `x-offset` fields of a `0x55` packet are absolute
  coordinates in the whole virtual display; the card windows its own
  rectangle from the EEPROM control area
  ([../receiver-identity.md](../receiver-identity.md),
  [pixel-write-path.md §5](pixel-write-path.md)). A `0x07` discovery frame
  returns, in the `0x08` reply, `Data[21..24]` = cabinet width and height as
  the card believes them, `Data[38..41]` = received-packet counter,
  `Data[2..3]` = firmware version.

**Searches that fail; do not repeat:** any LUT-constant search for the
`0x55` type byte, the `08 88` marker, or a row-field comparator. The
positive control (the Ethernet SFD and the EtherType) fails the same way;
this design does not build constant comparisons out of LUT4s. A shallow
EBR-to-EBR dataflow search also fails: depth 3 finds zero edges chip-wide
(`negative_results_and_method.txt` N6–N9).

**What would settle it:** forward netlist recovery of what consumes
`DOA*`/`DOB*` of `EBR@39,37` and `EBR@42,37`, a localised search in
`x 38..46, y 30..45`, the one region of the die whose function is certain.
On hardware, a single lit pixel at (0,0) separates a scrambled raster from
a missing one; a uniform fill cannot.

### 1.2 The test-pattern selector byte

**Not known:** the meaning of each selector value.

**Known:** the frame is `33 00`, `0x09` at payload+5, selector at payload+6,
279 bytes; it matches `SetRcvCardTestMode` @ `0x3d54e0` exactly. The enum
lives in the UI layer; `ScrnTest.dll` yields only `NORMAL`/`RED`-family
strings with no numeric mapping. On 16.53 the selectors produce visibly
different displays (`rxp card test-mode <n>`, `rxp card test-sweep`).

**What would settle it:** a selector sweep with the wire otherwise quiet,
the supply current logged and each pattern captured.

### 1.3 The set of driver-chip ids the gateware recognises

**Not known:** the full set.

**Known:** `0x014C` arms the SM16269S outputs on 16.53; `0x0214`, `0x00DE`
and `0x002F` do not ([chip-id.md §3](chip-id.md#3-measured-behaviour-by-id),
[../rendering.md](../rendering.md)). Under `0x014C`, chip-control tails
`2/4/8` and `3/5/7` never arm; only `1, 5, 6` renders. Whether the card
branches on the id or on the id-selected descriptor bytes is not resolved
([chip-protocol-microcode.md §2.3](chip-protocol-microcode.md#23-the-chip-id-and-the-descriptor)).
The id is excluded from the pack CRC-32.

**Searches that fail; do not repeat:** 16-bit and 8-bit compare-to-constant
in LUT4s, the `0xFE` escape test, 4-to-16 decoders on a chip-id nibble,
CCU2 carry-chain compare-to-constant, chip-id values in the microcode ROM,
and any constant present in 16.53 but not the older images. The positive
control fails too: the Ethernet SFD `0xD5` and the ethertype are equally
invisible, so this design does not build constant comparisons out of LUT4s.

**What would settle it:** the empirical sweep,
`scripts/bench.py run --boot --spec …` per candidate id.

### 1.4 The data-swap / lane map for 1/16 on this module

**Not known:** whether the identity ramps the generator writes are the
correct block 0 for this module at 1/16.

**Known:** the generator writes identity ramps and the reference file
regenerates byte-exactly from them; the panel renders with them.
[../rcvbp-format.md](../rcvbp-format.md) records that swap block 0 was
wholly reordered between 32S and 64S variants of the same module, so it is
scan-dependent.

**What would settle it:** a vendor `.rcvbp` for a 1/16 128×64 module of this
family with a non-identity block 0, or bisecting the block on hardware.

### 1.5 The serial clock: 8 or 15

**Not known:** which value is right for this chip on this module.

**Known:** `config/panels/*.toml` carries 8 (the reference file's wall
config); `config/chips/sm16269.toml` gives the vendor default for this chip
as 15. The pack carries the value three times (`+0x09`, `+0x2C`, `+0x2E`)
and it also feeds the scan-table line time. The panel renders at 8. Not
swept.

**What would settle it:** a one-line spec change and a `bench.py run`.

### 1.6 Record 0x01 `+0x02F`

**Not known:** what the byte controls.

**Known:** with `+0x02F = 0` nothing displays; with `1` the panel renders. `1` is the vendor `Reset()` default and
the value in 961 of 1146 corpus files
([../rendering.md](../rendering.md)).

**What would settle it:** the vendor setter for that offset in the SDK, or a
sweep of the other values on hardware.

---

## Tier 2: gateware structure

### 2.1 Top-edge pad to HUB75 control signal assignment

**Not known:** which top-edge pad is A, B, C, D, E, CLK, LAT or OE, and
which pins belong to which of the twelve HUB75E connectors.

**Known:** the 96 RGB data pins are identified: the `IREG_OREG` signature
in the Normal/LS builds maps exactly onto the 96 left/right-edge pads, with
zero discrepancy
([output-stage.md §7.1](output-stage.md),
`analysis/fpga/rgb96_pins.txt`). The control group is identified as a
group: the top-edge pads, sharing a global synchronous blank (`Q4@23,18`)
and a 2:1 source select (`Q5@23,18`), active-low.
`analysis/fpga/pad_driver_logic_16.53.tsv` holds the per-pad driver logic.

**What would settle it:** continuity testing of the PCB from the J1
connector to the BGA, or a photo of the connector traced out. In the
netlist, a scan address line should be driven by a small counter and CLK by
a toggling flop.

### 2.2 The 34 bidirectional pins

**Not known:** what they carry.

**Known:** they are real (out-enable driven from fabric, `HYSTERESIS ON`
input buffers), and 20 of them share a single OE flip-flop
`Q2_SLICE@(25,2)`. Readback from the LED driver chain is plausible, since
SM16386S/SM16269SH have status/error readback; that is inferred.

**What would settle it:** a scope on the hub connector during a chip-register
write, watching for the card driving then releasing a line.

### 2.3 The microcode ROM's targets

**Not known:** what the ROM configures.

**Known:** 351 used entries of 512, 5-bit opcode + 16-bit immediate,
addressing spaces `0x0Axx`–`0x0Dxx`, `0x80xx`–`0x87xx`, `0xA0xx`, `0xB8xx`.
Byte-identical across four of the five builds. Ruled out: gamma/brightness
LUT, Lattice Mico8, 8051/PicoBlaze, any chip id, scan table
([block-ram.md](block-ram.md)).

**What would settle it:** netlist recovery around the ROM's read port: what
the 16-bit immediate fans out to and what decodes the 5-bit opcode.

### 2.4 The parameter store

**Not known:** where the 256-byte pack lands (BRAM, LUT-RAM or a flop file).

**Known:**

* LUT-RAM in 16.53 is ruled out: only 18 blocks of 16×4, against 59 and 89
  in 13.39 and 6.69. Too small and too fragmented to hold the tables.
* `R27C44_Q0..Q3` is not the store. It is an ordinary 8-bit CCU2
  accumulator; it looks sourceless only because CCU2 carry travels on
  fixed, non-configurable wires. 1012 of 6956 CCU2 LUTs in 16.53 have zero
  routed inputs, so "no combinational source" is the normal appearance of
  every increment stage on the die. See
  [chip-id.md §6](chip-id.md).
* The block RAM feeding the top-edge control pads is `MIB_R25C4/C5` EBR0 =
  `EBR@4,25`, `PDPW16KD`, 512 × 36, `WID = 1`, not initialised at config
  time: a run-time-written table feeding the output stage directly.
* `analysis/fpga/ebr_map_16.53.txt` records the driven pins, clock, write
  gate and generator locations of all 53 instantiated block RAMs. A
  256-byte pack wants a small, singly-written, CLKOP-clocked block whose
  address generator is not part of either large bank; several candidates in
  the map fit. EBR pins are not set-arc sinks
  ([pixel-write-path.md §1](pixel-write-path.md)).

**What would settle it:** tracing a candidate's write path back to the
Ethernet RX FIFOs. Finding the store turns "which chip ids does the gateware
recognise" into "which stored byte feeds the mode selector".

### 2.5 Where gamma is applied

**Not known:** the stage that applies gamma.

**Known:** record 0x01 carries a gamma float at `+0x01C` (2.8 in the reference file); the
corpus's gamma/calibration records are all zero in an uncalibrated profile;
a separate `0x85`-opcode "write gamma table" path exists; the host-built
table on this card's flash (block 9) is gamma 2.8 for 14-bit grey
([../rendering.md](../rendering.md)). It is not a boot-time ROM: the one
initialised block RAM holds no 256- or 1024-point ramp. Candidate: the 24
MULT18 / 12 ALU54 DSP blocks, all populated in MAC configuration, operands
unknown.

**What would settle it:** see 2.6.

### 2.6 The DSP blocks' function

**Not known:** what the DSP row computes.

**Known:** the whole DSP row is populated as MULT18X18D feeding ALU54B with
input and pipeline registers enabled. Per-channel gamma or brightness
scaling is the obvious role in an LED controller; there is no evidence for
it.

**What would settle it:** tracing the multiplier operand nets back to a
buffer read port and a coefficient source.

---

## Tier 3: flash and boot

### 3.1 Primary bank or golden bank at boot

**Not known:** which bank the card configures from. It matters for every
flashing operation.

**Known:** two readings survive the evidence
([flash-layout.md](flash-layout.md#which-bank-the-card-boots-not-resolved)):

* (A) The card boots the golden bank at block 0x20. Against: golden's EBR
  init block is not 10.81's, yet the card reports 10.81; and there is no
  jump command or second preamble anywhere.
* (B) `0x030000`–`0x07FFFF` is not the boot flash at all: the card's
  firmware redirects host access in that range to a separate parameter
  store, as it does for `0x07F000`.

After `rxp firmware install` (SDRAM self-program, blocks 0x00–0x02 and
0x08) followed by `rxp firmware write --from-block 3 --to-block 7` of
16.53, `rxp discover` reports 16.53 ([../rendering.md](../rendering.md)).

Ruled out: that the loader skips `0x030000`–`0x07FFFF`. Skipping 320 KB out
of a single continuous `LSC_PROG_INCR_RTI` of 7562 frames is not
expressible in this format, the frames there are CRC-valid frame data, and
there is no jump command. `third-party/README.md`'s "the bitstream is not
contiguous / those contents are padding" is wrong as stated; its practical
rules about which regions the host may write are correct.

**What would settle it:** the ECP5 sysCONFIG usage guide (whether control
register `0x40000020` disables CRC checking; whether ECP5 falls back to
golden automatically without an explicit jump), plus a read of the flash
above `0x2B0000`. The reported firmware version (3.2) is a live probe of
which bitstream is configured.

<a id="32-where-does-the-reported-firmware-version-come-from"></a>
### 3.2 Source of the reported firmware version

**Not known:** the register or logic that produces the version number.

**Known:** it is not an ASCII string, not USERCODE (`0x00000000` in all
five images and all three dumps), and not a fixed-offset literal: all five
images searched for their own version in six encodings give an empty
intersection of hit offsets. `GetRCVTypeVersionDesp` formats `%d.%02d` from
receiver-info reply bytes, so the number is produced by the running
gateware as a register value, synthesised into fabric LUTs, scrambled by
placement, not recoverable by byte search. Consequence: the version the
card reports is the version of whichever bitstream is actually configured.

**What would settle it:** netlist recovery of the source of bytes 2–3 of
the discovery reply.

### 3.3 Two unidentified flash regions

**Not known:** what they hold.

**Known:**

* `0x030000`–`0x033FFF`: 4096 × 4-byte BE entries, 4091 of them
  `FFFFFF00`. The shape of a 4096-entry gamma or calibration LUT with almost
  no information in it.
* `0x040000`–`0x04FFFF`: 64 KB of the constant word `99 99 99 08`.

Neither corresponds to any region in
[../compiled-image-format.md](../compiled-image-format.md), and blocks
0x03/0x04 are unassigned in the vendor library's flash address table. Block
0x03 is not wholly erased: only `0x034000`-`0x03FFFF` reads `0xFF`.

**What would settle it:** a vendor flash-write call site that targets those
addresses, from static analysis of the SDK.

### 3.4 The 8-byte end marker and control-register bit 5

**Not known:** their meaning.

**Known:** the marker at `0xAFFF8` is per-image (`…E0 89 5B A0` for 16.53,
`…C5 99 12 FD` for 10.81) and 13.39 uses a different container length.
Control register 0 is `0x40000020` in four images and `0x40000000` in 6.69.

**What would settle it:** the ECP5 sysCONFIG control-register documentation
for bit 5, and the Diamond bitstream writer's definition of the trailer.

### 3.5 The ASCII header's `Bitstream CRC: 0x3474`

**Not known:** what it covers.

**Known:** it is identical in all five images despite completely different
contents, so it is not a content checksum.

**What would settle it:** the Diamond header writer's definition of the
field.

---

## Tier 4: minor

### 4.1 The LS0allDA firmware family

**Not known:** what the family is. Only the name and the resource profile
are known.

**What would settle it:** a vendor release note naming the family.

### 4.2 10.81's ROM prologue

**Not known:** why it is five entries longer. It is the only ROM difference
among the five builds.

**What would settle it:** decoding the ROM's opcode set (2.3).

### 4.3 `CLKOS2`

**Not known:** whether it is used. The PLL enables it, but it is not routed
to any DCC.

**What would settle it:** a routing-graph search for any consumer of the
PLL's `CLKOS2` output.

### 4.4 The fabric-generated global net `BDCC0`

**Not known:** its specific role (presumably the LED shift or pixel-rate
gate).

**Known:** it is not a clock. The design is single-clock (98.9 % of flops on
PLL CLKOP) and `G_HPBX0900` appears as `.CE` on output-stage flops, so this
net is a clock enable.

**What would settle it:** tracing the net's fan-out to the output-stage
flops it enables.

### 4.5 The six constant-strapped output pins

**Not known:** their function.

**Known:** `A15`, `M6`, `K12` (constant 0), `E12`, `E13` (constant 1) and
one more, all at `DRIVE 16 / SLEWRATE FAST`. They are static level outputs.

**What would settle it:** continuity from the BGA balls to what they drive
on the PCB.

### 4.6 Exact EBR instance count

**Not known:** whether 53 or 54 is the right count for 16.53. The
difference is a tile-grouping convention, not a disagreement about
utilisation. Do not quote a precise figure without re-deriving it.

**What would settle it:** one stated grouping convention alongside the
count.

### 4.7 The 2:1 output mux

**Not known:** whether the mux selects "internal test pattern vs live pixel
data" or a within-frame command/data time-multiplex (SM16xxx configuration
words vs pixel data). Both readings fit.

**Known:** one leg is a CCU2 counter (`x = 24..26, y = 7..11`), the other is
block RAM data out. If it is test-pattern-vs-data, the select bit
`Q5@23,18` is the card's test-mode control and 1.2 becomes answerable from
the netlist.

**What would settle it:** tracing the driver of `Q5@23,18` to the `0x33`
command path, or a scope on one RGB data line during `rxp card test-mode`.

### 4.8 Membership of the blanked top-edge set

**Not known:** the exact membership (20–23 pads) and the driver logic of 4
of 21 classifiable pads, which do not fit the normalised truth table.

**Known:** `Q4@23,18` blanks the top-edge control group but not the 96 RGB
pads. Per-pad data is in `analysis/fpga/pad_driver_logic_16.53.tsv`.

**What would settle it:** re-deriving the truth tables of the four
unexplained pads.
