# The pixel write path

What the card does with a pixel-data Ethernet frame (type byte `0x55` at
frame offset 12), and what must hold for its pixel bytes to reach the buffer
the display raster reads.

Established:

* Every frame the card receives (`0x55`, `0x0A`, `0x0107`, `0x0600`, all
  types) enters through one of two block RAMs, one per Ethernet port. They
  are the design's only clock-domain-crossing memories (§2).
* Two large banked EBR arrays are the only candidate destination memories,
  each with one shared write-enable flip-flop and one shared address
  generator (§3). Which of them is the pixel buffer is not resolved.
* The memory that feeds the HUB75 pads is `EBR@4,25`, 512 x 36, whose entire
  write enable is one flip-flop, `Q6@9,27` (§3.1).
* There is no separate pixel-only memory path, no second frame buffer that a
  double-buffer swap could be stuck on, and no unwritten run-time table on the
  pixel side other than the one known on the control side.
* The gate that accepts or drops a `0x55` payload is not recoverable from the
  netlist by LUT-constant methods (§4).
* The card keeps only pixels inside its control area, a rectangle held in
  EEPROM; the `row` and `x-offset` fields of a `0x55` packet are absolute
  coordinates in the whole screen (§5). Measured: with the control area
  erased to `startX = startY = 0xFFFF` frames are accepted and nothing
  displays; restored to `(0, 0, 128, 64)`, the panel renders.

Artefacts (not kept in the repository; regenerate per
[decode-method.md](decode-method.md)): `analysis/fpga/ebr_map_16.53.txt`,
`analysis/fpga/ebr_map_10.81.txt`, `analysis/fpga/scripts/netlist/ebrmap.py`,
`analysis/fpga/negative_results_and_method.txt` (N5-N7).

## 1. Locating block-RAM connections

EBR bel pins are not set-arc sinks. Searching `drivenby` for a wire whose name
contains `EBR` returns zero results in all five images. The EBR pins
(`JADA0_EBR`, `JDIA0_EBR`, `JWEA_EBR`, `JCSA0_EBR`, `JCLKA_EBR`, ...) are
reached by a fixed, non-configurable connection from an ordinary CIB J-pin in
the adjacent `CIB_EBR` tile. The routed net stops at `JA1`, `JC0`, `JD0`,
`JCE0`, `JCLK0`, and those are the arc sinks.

Method (`ebrmap.py`): find every driven `JA#`/`JB#`/`JC#`/`JD#`/`JCE#`/
`JCLK#`/`JLSR#` wire, follow `fixed_dn` one hop to identify the EBR pin, then
walk the set-arc graph back to the driving cell. This is a one-hop
identification followed by a backward walk, the pattern
[decode-method.md §6](decode-method.md#6-how-far-backward-tracing-can-be-trusted-high)
recommends, not a deep blind trace.

Result: 53 of 56 EBR sites have at least one driven input pin in 16.53, the
same 53 the utilisation count gives. For each, the pin set, clock, control
sources and the die location of the address and data generators are recorded
in the EBR map. The same holds for 10.81.

Pin-to-CIB mapping, verified at `CIB_R25C4`:

| CIB pin | EBR pin |
|---|---|
| `JA0`, `JA1` | `ADB5`, `ADA5` |
| `JB0` | `DIB1` |
| `JC0` | `ADA4` |
| `JD0` | `DIA0` |
| `JCE0` | `OCEA` |
| `JCLK0` | `CLKA` |
| `JLSR0` | `RSTA` |
| `JQ0` (output) | `DOB0` |
| `JF0` (output) | `DOA0` |

## 2. The receive path: two block RAMs, one per Ethernet port

Of the 53 instantiated EBRs in 16.53, exactly two have `CLKA != CLKB`:

```
EBR@39,37   CLKA = G_HPBX0100 (global net 1, LR quadrant)   CLKB = G_HPBX0200 (net 2 = PLL CLKOP)
EBR@42,37   CLKA = G_HPBX0000 (global net 0, LR quadrant)   CLKB = G_HPBX0200 (net 2 = PLL CLKOP)
```

Both sit at `y = 37`, `x = 39` and `x = 42`, in the lower-right (LR)
quadrant. From the DCC table in
[resources.md](resources.md#global-clock-network-high-for-the-arcs):

| DCC | source | nets driven |
|---|---|---|
| `LDCC3` | pad `J1` `PCLKT7_1`, left PHY RXC | UL1, LR1 |
| `RDCC2` | pad `M16` `PCLKT3_0`, right PHY RXC | UR1, LR0 |

* `EBR@39,37` is written on the left PHY's recovered receive clock.
* `EBR@42,37` is written on the right PHY's recovered receive clock.
* Both are read on PLL CLKOP, the single system clock.

They are the design's only clock-domain-crossing memories. The RGMII receive
logic is the only logic on a PHY RX clock, and every byte from the wire
arrives in that domain, so these two block RAMs are the sole entry point for
every Ethernet frame the card accepts, `0x55` pixel rows included.

Shape, identical for both:

```
ADA# = 10 driven, ADB# = 10 driven      ->   1024 entries
DIA# =  9 driven                        ->   9 bits: 8 data + 1 flag
CEA, CEB, OCEA driven; no WEA/WEB       ->   write-enable is the port CE,
                                             not a write-enable pin
CEA and OCEA share one LUT              ->   push and output-register enable
                                             move together
```

`EBR@39,37`: `CEA = OCEA = F6@23,42 = !C&D`, `CEB = F1@42,39 = !B&C`.
`EBR@42,37`: `CEA = OCEA = F1@42,23 = !C&D`, `CEB = F2@42,34 = C&!D`.

An asynchronous FIFO: write side gated by a two-term condition on the RX
clock, read side gated by a two-term condition on CLKOP.

The same signature is present in 10.81, the factory image
([flash-layout.md §8](flash-layout.md#8-identity-of-the-dumped-firmware)):
two EBRs, at `(4,37)` and `(10,25)`, with `ADA# = 10`, `DIA# = 9`,
`CEA`+`CEB`+`OCEA` driven, no WE, and `CLKA != CLKB`. Placement moved; the
structure did not. The global-net numbers differ between builds and the 10.81
DCC table is not derived, so for 10.81 the statement is "two crossing FIFOs of
the same shape", not "this one is the left PHY".

### 2.1 Packet size against FIFO depth

1024 x 9 bits holds 1024 bytes. A maximum-size Colorlight pixel packet is
`21 + 3 x 497` = 1512 bytes, which does not fit, so the card cannot
store-and-forward a pixel frame: the header is decoded and the payload
consumed while the packet is arriving. A 128-pixel row is `21 + 3 x 128` =
405 bytes, inside a single FIFO occupancy.

Consequences (architectural inference, not netlist evidence):

* there is no "whole packet buffered, validated, then committed" step, and
  no place for a whole-packet accept/reject decision; the decision is made
  from the header, byte by byte;
* a row/offset window test on the header is the shape of decision this
  architecture forces: it can be made from the first 8 payload bytes and then
  gate the following payload bytes as they stream past;
* a "double buffer that never swaps" would need a second identical bank and a
  swap flop feeding an address MSB; §3 shows neither exists.

## 3. The destination: two banked memories

Grouping the 53 EBRs by their shared write-enable flip-flop gives two large
regular arrays and a tail of singletons and pairs. In 16.53:

### Bank A: 8 EBRs

```
EBR@8,37  10,25  10,37  13,25  13,37  15,37  17,25  19,25
WEA  <- Q4@21,22      WEB  <- Q4@6,10
CSA0 <- Q2@16,20      CSB0 <- Q4@16,23
RSTA/RSTB <- Q4@15,38
ADA# = 11, ADB# = 11, DIA# = 8..9        ->  2048 x 9 per block
address generator cells clustered at x 13..16, y 21..23
data-in generator cells at x 8..15, y 16..22
```

Eight blocks x 2048 x 9 = 147 456 bits.

### Bank B: 12 EBRs

```
EBR@44,25  44,37  46,25  48,25  48,37  51,37  53,37  55,37  57,37  60,25  62,25  64,25
WEA <- Q4@44,26   (one flop for all twelve)
CSA0 <- Q6@45,22 for eight of them; four use small LUTs at x 44..46
address generator cells clustered at x 25..45, y 22..35  (median x 43, y 24)
aspect ratios: ADA13/DIA2 (8192x2), ADA12/DIA4 (4096x4), ADA10/DIA16 (1024x16)
```

Every member uses exactly 16 384 of its 18 432 bits, whatever its aspect
ratio. Twelve blocks x 16 384 = 196 608 bits.

`196 608 = 8192 x 24`, the size of a 128 x 64 panel at 24 bits per pixel.
This is a coincidence: the equivalent bank in 10.81 has 13 members
(`WEA <- Q4@43,29`), 212 992 bits, and the mixed aspect ratios mean the bank
is not a plain 8192 x 24 array. The total is exact; its interpretation is not
resolved.

### What the bank structure establishes

* There is no second, idle frame buffer. Bank A and Bank B are the only
  multi-EBR arrays in the design, they differ from each other in depth, width
  and control, and every remaining EBR is a singleton or a pair. A back
  buffer that never becomes the front buffer has nowhere to live; the
  double-buffer-swap reading is excluded.
* Both banks are written at run time and start empty. Only one EBR in the
  design carries a `.bram_init` block
  ([block-ram.md §1](block-ram.md#1-exactly-one-initialised-bram)), and it is
  in neither bank. A bank that is never written scans whatever the SRAM
  powered up as: structured content that never goes black.
* The same two-bank architecture is present in 10.81.
* Which bank the raster reads and which the Ethernet writes is not resolved.
  The memory that feeds the pads is in neither bank (§3.1), so at least one
  more stage sits between a bank and the output.

### 3.1 The RAM that feeds the output stage: `EBR@4,25`

[output-stage.md §7.3](output-stage.md#73-what-the-21-mux-selects-between-high-that-it-is-counter-vs-bram)
traces the pads' 2:1 mux to block-RAM data out at `JQ5@5,25 <- JDOB13_EBR`
and `JQ2@4,25 <- JDOB2_EBR`. Both J-pins belong to one EBR instance, the bel
at `x = 4, y = 25`, whose configuration spans CIB columns 4-6: `MIB_R25C4/C5
EBR0`, the `WID = 1` block that is uninitialised at configuration time
([output-stage.md §7.4](output-stage.md#74-the-control-group-source-ram-starts-empty-high)).

```
EBR@4,25   PDPW16KD, PDPW16KD.DATA_WIDTH_R = 36, REGMODE_A/B = OUTREG
           ADA# = 9 driven          ->  512 entries
           DIA# = 18, DIB# = 14     ->  a 36-bit wide write port
           WEAMUX = INV             ->  WEA is TIED HIGH, not routed
           CSA0 <- Q6@9,27          ->  the ONLY routed gate on the write
           CLKA = CLKB = G_HPBX0200 = PLL CLKOP
           address generators at x 4..11,  y 23..27   (local)
           data-in  generators at x 4..10,  y 24..26   (local)
```

With `WEA` strapped high, `CSA0` is the entire write enable of the memory
that drives the HUB75 pads, and it is one flip-flop: `Q6@9,27`. Its fan-in is
a five-level cone of ordinary counter and state-machine flops around
`x 7..13, y 27..36`: no constant and no parameter-shaped register within five
levels. `Q6@9,27` is the first signal to observe if internal state becomes
observable (JTAG readback, a debug build, or a simulation of the recovered
region).

512 x 36 bits is not a frame buffer. Inferred: a line/scan buffer, one scan
address's worth of serial data for the output stage, matching
`CardScanLen = 256` and 36 bits of parallel colour lanes.

## 4. Why the `0x55` decode is not recoverable from the netlist

The decode of the type byte, the row field, the offset field and the count
field is a constant comparison plus a magnitude comparison. This design does
not build constant comparisons out of LUT4s: the Ethernet SFD `0xD5` and the
EtherType are as invisible to an exhaustive LUT INIT search as any driver-chip
id ([parameter-path.md §4](parameter-path.md#4-constants-the-card-compares-against),
[chip-id.md](chip-id.md)).

Two further searches return nothing, for reasons that generalise (N6 and N7):

* EBR-to-EBR dataflow (does the RX FIFO's output reach a bank's data-in?): a
  bounded backward cone of depth 3 from every EBR data-in pin finds zero EBR
  data-out wires anywhere in the design. The pipeline depth between any two
  memories exceeds three levels of logic, and deeper walks meet the
  93 %-reliable, edge-clamped backward-trace limit.
* Forward tracing from the RX FIFO outputs: the fan-out spreads across shared
  span wires and does not terminate.

The netlist gives where a packet enters and where the two candidate
destinations are; it does not give what decides whether a `0x55` payload is
written or dropped. LUT-constant searches for the `0x55` type byte, the
`08 88` marker or a row-field comparator cannot succeed and are not to be
repeated.

What would settle it in the gateware: recovering the netlist around the RX
FIFO read ports, what consumes `DOA*`/`DOB*` of `EBR@39,37` and `EBR@42,37`,
and what the byte counter that walks the header feeds. That is forward
netlist recovery on a small region (`x 38..46`, `y 30..45`) whose function is
certain.

## 5. Windowing: the control area

### 5.1 `row` and `x-offset` are screen coordinates

Both independent senders address the whole screen and carry no per-receiver
addressing in a `0x55` packet:

* FPP (`ColorLight-5a-75.cpp`). `Init()` loops
  `for (row = 0; row < m_rows; row++)` over the whole display's height and
  packetises `m_rowSize` = the whole display's width x 3, splitting at 497
  pixels. Every packet goes to the one destination MAC `11:22:33:44:55:66` on
  one socket. There is no receiver index in a `0x55` packet and no
  per-receiver stream; a wall of N receivers all see the same bytes.
* CLTNic ([../pixel-protocol.md §1.6](../pixel-protocol.md#16-row-field-and-screen-number)).
  The transmitted row field is `base(screenNo) + y`, with
  `base = (n-1) << 12` for screen numbers `<= 9`: a global row address space
  with a screen selector in its high bits.

Each receiver therefore windows the stream: it keeps the pixels whose `row`
falls inside its vertical extent and whose `x-offset` falls inside its
horizontal extent, and discards the rest.

### 5.2 The window is the EEPROM control area

The window is the 42-byte record at EEPROM `0x02`: `startX`, `startY`,
`endX`, `endY`, big-endian `u16`, end coordinates not sizes
([../receiver-identity.md](../receiver-identity.md)). It is not in the
`.rcvbp`, not in record 0x01 and not in the compiled parameter image; it
lives in the EEPROM and its flash mirror at `0x07F000`. `rxp provision
--position x,y` writes it. The pixel-keep rule, row in `[startY, endY)` and
column in `[startX, endX)`, is inferred from the record's shape and FPP's
global addressing.

Measured (firmware 16.53):

* control area `startX = startY = 0xFFFF`, `endX = 128`, `endY = 64` (the
  state a block 0x07 erase followed by `rxp card screen-size --set` leaves):
  frames are accepted, the received-packet counter advances, the supply
  current changes with the stream, `discover` reports 128x64, nothing
  displays;
* control area `(0, 0, 128, 64)`: the panel renders.

### 5.3 The layout frame (type `0x02`)

FPP's receiver-layout packet:

```
0x02 - Write receiver layout
  Data[0..2]              receiver index
  then 20 bytes per receiver, for up to 64 receivers:
    Data[7..8]    this receiver's width   (MSB, LSB)
    Data[9..10]   this receiver's height
    Data[13..14]  NEXT receiver's x offset
    Data[15..16]  NEXT receiver's y offset
    Data[17..18]  total display width
    Data[19..20]  total display height
```

`crates/colorlight/src/discovery.rs::set_layout` builds this frame with a
98-byte payload: header and one 20-byte record. FPP's sibling packet `0x11`
carries `3 + 64 x 20 = 1283` data bytes, a 64-receiver table. `rxp card
set-layout` sends the frame on request. `driver` sends it only when
`Settings::announce_layout` is set, off by default. Measured: the layout frame
blanks a provisioned card, which takes its control area from EEPROM
([../rendering.md](../rendering.md)).

### 5.4 The discovery reply (type `0x08`)

`Data[n]` is frame offset `13 + n`.

| field | contents |
|---|---|
| `Data[2..3]` | firmware version major, minor; `rxp discover` prints it |
| `Data[21..22]` | cabinet width as the card holds it: `endX` (payload 20-21) |
| `Data[23..24]` | cabinet height: `endY` (payload 22-23) |
| payload 16-19 | `startX`, `startY` |
| `Data[38..41]` | received-packet count |
| `Data[46..49]` | uptime in ms |
| `Data[85]` | receiver card number |

`discover` reports `endX`/`endY` as a size, which is right only while
`startX = startY = 0`. With the control area erased it reports 128x64 while
the window is empty.

### 5.5 Consistency with measurements under the empty window

| measurement | consistent with windowing |
|---|---|
| brightness (`0x0A`) and latch (`0x0107`) act on the panel | yes; neither carries coordinates, so neither is windowed |
| streaming shifts supply current and the RX FIFOs churn | yes; frames are received and the latch frames fire |
| all-black and all-white draw the same current | yes; every write is discarded and the bank keeps its power-on contents. Measured interleaved: black and white differ by 0.001 A against a within-condition spread of 0.033 A. A 0.15 A white-over-black difference from two sequential readings does not exist; it is drift ([../retracted-findings.md](../retracted-findings.md)) |
| the stored configuration is byte-identical to the reference `.rcvbp` | yes; the control area is outside the `.rcvbp`, so a correct configuration and an empty window coexist |
| four host raster layouts all fail | yes; reordering rows cannot help when every row index is rejected |

The reference `.rcvbp` was compiled for a 256 x 384 wall. It does not carry a
cabinet position; the empty window came from the EEPROM erase, not from the
file.

## 6. Unresolved

* Which bank the raster reads and which the Ethernet writes, and what gates
  either bank's write. Known: Bank A (8 x 2048 x 9, `WEA = Q4@21,22`), Bank B
  (12 x 16 384 bits, `WEA = Q4@44,26`), both uninitialised; the output buffer
  `EBR@4,25` is in neither. What would settle it: the forward recovery of §4
  from the RX FIFO read ports, or the write-address generators of the banks
  traced back to the Ethernet path.
* The fan-in of `Q6@9,27`, the output buffer's write enable, beyond five
  logic levels.
* Whether the card accepts a 98-byte `0x02` frame as a one-receiver table or
  only a 64-receiver table of FPP's `0x11` length. Known: the 98-byte frame
  has an effect (it blanks a provisioned card).
* The 10.81 DCC table, and therefore which of 10.81's two FIFOs belongs to
  which PHY.
