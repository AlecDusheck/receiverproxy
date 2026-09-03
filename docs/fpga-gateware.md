# The E120 FPGA gateware: overview

The Colorlight E120 receiving card's gateware, decoded from the vendor
firmware images in `third-party/firmware/` with open-source tooling only
(prjtrellis). Neither Lattice Diamond nor any Colorlight software was
executed.

This page is the summary. The detail is in [`docs/fpga/`](fpga/README.md),
one file per topic, indexed in [`docs/fpga/README.md`](fpga/README.md).
Claims read directly from the bytes and cross-checked are stated plainly;
claims resting on one stated assumption are marked "inferred"; what is not
determined is listed in [Unresolved](#unresolved) and in
[open-questions.md](fpga/open-questions.md).

---

## The device

* FPGA: Lattice ECP5 `LFE5U-25F-6CABGA256`, IDCODE `0x41111043`. All eight
  IO banks at 3.3 V.
* Ethernet: two RGMII gigabit ports, identified pin by pin: 12 pins each,
  4 data + 1 control per direction, DDR throughout, no tri-state, no DQS.
* Memory: none external. No DQS group, no DDR DLL, no bidirectional bus. All
  buffering is on-chip in 53 of 56 block RAMs (about 954 Kbit).
* Flash: one SPI flash holding both the bitstream and the card's
  configuration. `CCLK.MODE USRMCLK`: the running design reads it.
* LED side: about 147 pins, driving up to twelve HUB75E connectors (J1–J12).
  96 of them are the serial RGB data lines, 32 groups × 3 colours, on the
  left and right edges. The HUB75 control signals (A–E, CLK, LAT, OE) are the
  top-edge pads, identified as a group but not individually.
* Clocking: one EHXPLLL from a 25 MHz reference on pin `P6`. The design is
  single-clock: 98.9 % of flip-flops run on CLKOP at 125 MHz. CLKOS3 supplies
  the RGMII TXC skew. The fabric-generated global net is a clock enable, not
  a second domain.
* Utilisation: about 95 %. 20 170 functional LUT4s of 24 288; 13 074
  flip-flops; the whole DSP row; nearly all the block RAM.

Detail: [pinout.md](fpga/pinout.md), [resources.md](fpga/resources.md),
[block-ram.md](fpga/block-ram.md).

## The receive path

Exactly two block RAMs have `CLKA ≠ CLKB`: `EBR@39,37` on the left PHY's
receive clock and `EBR@42,37` on the right PHY's. They are the design's only
clock-domain crossings, 1024 × 9 each, gated by port CE rather than a
write-enable pin. Every Ethernet frame the card accepts enters through one of
those two. At 1024 bytes they cannot hold a maximum-size pixel packet, so the
header is decoded and the payload consumed while the packet is still
streaming.

Downstream there are two banked memories and no third: 8 EBRs of 2048 × 9
sharing one write-enable flop, and 12 EBRs of 16 384 bits sharing another.
Both start empty and are written at run time. The RAM that feeds the HUB75
pads is neither: it is `EBR@4,25`, 512 × 36, whose write enable is a single
flip-flop, `Q6@9,27` (`WEA` is strapped high, so `CSA0` is the whole gate).

There is no un-swapped back buffer: no third array exists, and the two banks
are structurally different. A double-buffer swap cannot explain a dead panel.

Detail: [pixel-write-path.md](fpga/pixel-write-path.md).

## The bitstream

The `.hex` files are raw Lattice `.bit` images: 342-byte ASCII header,
preamble `BD B3` at `0x158`, a short command stream, then 7562 frames of
77 bytes (74 data + 2 CRC big-endian + 1 dummy `0xFF`), then USERCODE, a
2048 × 9-bit EBR init block, and `DONE`.

The frame CRC is CRC-16 poly `0x8005`, init 0, MSB-first, accumulating over
the command stream since the last reset: frame 0's CRC covers everything
from `0x162`, later frames cover the preceding `0xFF` plus their 74 bytes.
With that rule all 7562 frames validate in all five vendor images.

Decoding needs one workaround: truncate the file at `0x8ED30` so `ecpunpack`
does not walk into the trailer and abort.

Detail: [bitstream-format.md](fpga/bitstream-format.md),
[decode-method.md](fpga/decode-method.md).

## Decode traps

1. prjtrellis `word:` bit order varies per field. The PLL's dividers are
   MSB-first; other fields are not. Check `bits.db` for each field.
2. `PIOx.BASE_TYPE` names are not IO standards. Many standards share one bit
   pattern and prjtrellis prints the alphabetically last match. Every
   `SSTL18`/`SSTL15` label in the decode is an artefact; all banks are 3.3 V.
   Pin direction comes from the routing graph, not from `BASE_TYPE`.
3. "No combinational source" means nothing on its own. CCU2 carry travels on
   fixed, non-configurable wires, so 1012 of 6956 CCU2 LUTs in 16.53 have
   zero routed inputs. An arc-only tracer reports every increment stage on
   the die as sourceless.
4. In pytrellis, resolve arc endpoints with `rg.globalise_net(row, col, name)`,
   not `id_at_loc`, which silently fails on 62 % of arcs.

Limit: deep backward tracing is about 93 % reliable and worse at the die edge
(prjtrellis clamps out-of-range span wires to the boundary), so only 45 of
the 96 RGB pads resolve to a driver cell. Forward reasoning and one-hop
IOLOGIC signatures are more reliable; the 96 RGB pins were identified that
way.

## Firmware images

An E120's day-one image is `E320_PCB6.0_PWM_FPGA10.81_20230907`, identified
three ways from a flash dump: the header date, a per-block match of exactly
`1.000000` outside the reserved span, and 10.81's uniquely different EBR ROM.
A `flash restore` of a day-one dump therefore reinstalls 10.81, whatever the
card ran before; `rxp discover` reports the running version and is the only
authority for it. The analysis targets 16.53; each claim in `docs/fpga/`
names the image it refers to.

Detail: [flash-layout.md](fpga/flash-layout.md).

## The chip id

The card takes a 16-bit driver-chip id at parameter-pack byte `+0x1B` (the
escape `0xFE` when the id ≥ `0x100`) with the full id big-endian at
`+0xE7..+0xE8`, excluded from the pack CRC-32.

In the netlist, there is no constant comparator against any chip id, and
none against the Ethernet SFD or ethertype either (exhaustive search). This
design does not build constant comparisons out of LUT4s, so that search
cannot succeed. The surviving hypothesis is a register file compared
data-vs-data. The `R27C44_Q0..Q3` "mode field" is an ordinary CCU2
accumulator, not a comparator.

Measured: the gateware branches on the id. `0x014C` arms the SM16269S
outputs and renders; `0x0214` and `0x00DE` never arm
([rendering.md](rendering.md)).

Detail: [chip-id.md](fpga/chip-id.md),
[parameter-path.md](fpga/parameter-path.md).

## Version differences

The board interface is frozen across all five images: identical pin
directions on 196 of 197 pins, the same PLL divider plan, the same RGMII
split. Only phases and logic change.

The family split is PWM vs Normal/LS, visible as IO-cell register usage:
`IOLOGIC*.MODE = IREG_OREG` appears 96 times in Normal 13.39 and LS 6.69 but
only 10 times in the three PWM builds. 96 = 32 serial RGB groups × 3 colour
lines, the E120 spec's "32 groups of serial RGB data". Measured: an SM16269S panel
is dead on Normal 13.39 and responds on PWM builds.

Version numbers are not one sequence: 10.81 is dated after 13.39.

Detail: [version-diff.md](fpga/version-diff.md).

---

## Measured behaviour and what it establishes

| fact | what it establishes |
|---|---|
| dead on Normal 13.39, responds on PWM builds | SM16269S is a PWM-class self-scanning driver and only PWM gateware speaks its protocol; 16.53 is the build |
| the frames are byte-exact against CLTNic.dll and on the wire | the host encoder is not a fault source |
| `0x014C` arms the drivers; with `+0x02F = 1`, the measured frame order and booting from flash, content renders | the driver protocol is selected by the chip id in the pack, not by a table in the gateware |
| an all-black frame draws a fixed pattern until the positions `width..2·width` are displaced through the void-line column table | the card emits `2 × width` positions per line for this wiring and fills the upper half from a fixed source; the void-line remap gates it |
| the physical test button does nothing when pressed | test patterns are reached over the wire with `rxp card test-mode <n>`, and the card's built-in generator is inert on 10.81 |
| on firmware 10.81 the panel changes with no traffic on the wire; on 16.53 it holds still | what 10.81 shows is a buffer nothing is driving, so no before/after comparison on 10.81 means anything |

What the output stage's behaviour establishes is in
[output-stage.md §6](fpga/output-stage.md#6-measured-behaviour); read it
against [rendering.md](rendering.md).

## Unresolved

* Which top-edge pad carries which HUB75 control signal, and which pads
  belong to which of the twelve connectors.
* Where the 256-byte parameter pack is stored in the fabric.
* Which of the two banked memories the raster reads and which the Ethernet
  writes.
* What the microcode ROM and the DSP row compute.
* Which flash bank the card boots.

Each with what is known and what would settle it:
[open-questions.md](fpga/open-questions.md).
