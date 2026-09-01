# The pixel write path — what happens to a type-`0x55` frame

**The question:** when the card receives a pixel-data Ethernet frame (type byte
`0x55` at frame offset 12), what does the gateware do with it, and what must
hold for those pixel bytes to land in the buffer the display raster reads?

**The short answer, stated honestly up front:**

* The **entry point is now identified in the bitstream** and it is not
  ambiguous: two block RAMs, one per Ethernet port, are the only
  clock-domain-crossing memories in the design. Every frame the card receives —
  `0x55`, `0x0A`, `0x0107`, `0x0600`, all of them — enters through one of those
  two, and nothing else. **HIGH.**
* The **two candidate destination memories are identified** — two large banked
  arrays of EBRs, each with a single shared write-enable flip-flop and a single
  shared address generator. **HIGH** that they exist and are so organised;
  **NOT RESOLVED** which of them is the pixel buffer.
* **The gate itself was not found in the netlist, and this document explains
  why that search cannot succeed** with the available method, rather than
  leaving it as an open action. **The netlist can neither confirm nor refute a
  row/offset window test.**
* What the netlist *does* rule out is worth having: there is **no separate
  pixel-only memory path**, **no second frame buffer that a double-buffer swap
  could be stuck on**, and **no unwritten run-time table on the pixel side**
  other than the one already known on the control side.
* The mechanism most consistent with all the bench evidence is documented
  outside the gateware, in the vendor's and FPP's own senders: **the `row` and
  `x-offset` fields of a `0x55` packet are absolute coordinates in the whole
  virtual display, and each receiver windows them against its own configured
  cabinet size and position.** §5.

Artefacts produced for this file:
`analysis/fpga/ebr_map_16.53.txt`, `analysis/fpga/ebr_map_10.81.txt`,
`analysis/fpga/scripts/netlist/ebrmap.py`.

---

## 1. Method — how the block RAMs were finally opened up

Every previous attempt in this project to reason about block RAM stopped at
"53 EBRs are instantiated but none was traced to a logical buffer"
([block-ram.md §4](block-ram.md#4-the-other-block-rams),
[negative result N5](../../analysis/fpga/negative_results_and_method.txt)).
The obstacle was a decode detail, not a hard limit:

> **EBR bel pins are not set-arc sinks.** Searching `drivenby` for a wire whose
> name contains `EBR` returns **zero** results in all five images. The EBR pins
> (`JADA0_EBR`, `JDIA0_EBR`, `JWEA_EBR`, `JCSA0_EBR`, `JCLKA_EBR`, …) are
> reached by a **fixed, non-configurable** connection from an ordinary CIB
> J-pin in the adjacent `CIB_EBR` tile. The routed net stops at `JA1`, `JC0`,
> `JD0`, `JCE0`, `JCLK0`… and *those* are the arc sinks.

So the recipe is: find every driven `JA#`/`JB#`/`JC#`/`JD#`/`JCE#`/`JCLK#`/
`JLSR#` wire, follow `fixed_dn` one hop to learn which EBR pin it actually is,
then walk the set-arc graph back to the driving cell. `ebrmap.py` does this.
It is a *one-hop* identification followed by a backward walk, i.e. the pattern
[decode-method.md §6](decode-method.md#6-how-far-backward-tracing-can-be-trusted--high)
recommends, not a deep blind trace.

Result: **53 of 56 EBR sites have at least one driven input pin** in 16.53 —
the same 53 the utilisation count gives — and every one now has its pin set,
its clock, its control sources and the die location of its address and data
generators recorded. Same for 10.81.

The pin→CIB mapping is worth recording because it is not documented anywhere
obvious (verified at `CIB_R25C4`):

| CIB pin | EBR pin |
|---|---|
| `JA0`,`JA1` | `ADB5`, `ADA5` |
| `JB0` | `DIB1` |
| `JC0` | `ADA4` |
| `JD0` | `DIA0` |
| `JCE0` | `OCEA` |
| `JCLK0` | `CLKA` |
| `JLSR0` | `RSTA` |
| `JQ0` (output) | `DOB0` |
| `JF0` (output) | `DOA0` |

---

## 2. The receive path: exactly two block RAMs, one per Ethernet port — HIGH

Across all 53 instantiated EBRs in 16.53, **exactly two have `CLKA ≠ CLKB`**:

```
EBR@39,37   CLKA = G_HPBX0100 (global net 1, LR quadrant)   CLKB = G_HPBX0200 (net 2 = PLL CLKOP)
EBR@42,37   CLKA = G_HPBX0000 (global net 0, LR quadrant)   CLKB = G_HPBX0200 (net 2 = PLL CLKOP)
```

Both sit at `x = 39` and `x = 42`, `y = 37` — the **lower-right (LR) quadrant**.
Cross-referencing the DCC table in
[resources.md](resources.md#global-clock-network--high-for-the-arcs):

| DCC | source | nets driven |
|---|---|---|
| `LDCC3` | pad `J1` `PCLKT7_1` — **left PHY RXC** | UL1, **LR1** |
| `RDCC2` | pad `M16` `PCLKT3_0` — **right PHY RXC** | UR1, **LR0** |

So:

* **`EBR@39,37` is written on the LEFT PHY's recovered receive clock.**
* **`EBR@42,37` is written on the RIGHT PHY's recovered receive clock.**
* Both are read on **PLL CLKOP**, the single system clock.

They are the design's **only** clock-domain-crossing memories. Since the RGMII
receive logic is the only thing in the design that runs on a PHY RX clock, and
since every byte that arrives on the wire arrives in that domain, **these two
block RAMs are the sole entry point for every Ethernet frame the card
accepts** — the `0x55` pixel rows included. — **HIGH.**

Their shape is identical and distinctive:

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

That is a textbook asynchronous FIFO: write side gated by a two-term condition
on the RX clock, read side gated by a two-term condition on CLKOP.

**The same signature appears in 10.81**, the build the card is actually running
([flash-layout.md §5.1](flash-layout.md)) — two EBRs, at `(4,37)` and
`(10,25)`, with `ADA# = 10`, `DIA# = 9`, `CEA`+`CEB`+`OCEA` driven, no WE, and
`CLKA ≠ CLKB`. Placement moved; the structure did not. (The specific global-net
numbers differ between builds and the 10.81 DCC table was **not** re-derived,
so for 10.81 the claim is "two crossing FIFOs of the same shape", not "this one
is the left PHY".) — **HIGH.**

### 2.1 A consequence that matters for the bench — HIGH

**1024 × 9 bits is 1024 bytes.** A maximum-size Colorlight pixel packet is
`21 + 3 × 497` = **1512 bytes**, which does not fit. So the card cannot be
doing store-and-forward on pixel frames: the header must be decoded and the
payload consumed **while the packet is still arriving**.

Our 128-pixel rows are `21 + 3 × 128` = **405 bytes**, comfortably inside a
single FIFO occupancy, so this is not a fault mode we are hitting. But it does
mean:

* there is no "whole packet buffered, then validated, then committed" step, and
  therefore **no place for a whole-packet accept/reject decision** — the
  decision has to be made from the header, early, byte by byte;
* a **row/offset window test on the header is exactly the shape of decision
  this architecture forces**, because it can be made from the first 8 payload
  bytes and then used to gate the following payload bytes as they stream past.

That is an architectural argument, not evidence, and it is labelled as such.
But it does say the "screen assigned / window" hypothesis is the kind of gate
this hardware *can* implement, while a "double buffer that never swaps" would
need structure (a second identical bank, a swap flop feeding an address MSB)
that §3 shows is **not present**.

---

## 3. The destination: two banked memories — HIGH that they exist

Grouping the 53 EBRs by their shared write-enable flip-flop reveals two large,
regular arrays and a long tail of ones and twos. In 16.53:

### Bank A — 8 EBRs

```
EBR@8,37  10,25  10,37  13,25  13,37  15,37  17,25  19,25
WEA  <- Q4@21,22      WEB  <- Q4@6,10
CSA0 <- Q2@16,20      CSB0 <- Q4@16,23
RSTA/RSTB <- Q4@15,38
ADA# = 11, ADB# = 11, DIA# = 8..9        ->  2048 x 9 per block
address generator cells clustered at x 13..16, y 21..23
data-in generator cells at x 8..15, y 16..22
```

Eight blocks × 2048 × 9 = **147 456 bits**.

### Bank B — 12 EBRs

```
EBR@44,25  44,37  46,25  48,25  48,37  51,37  53,37  55,37  57,37  60,25  62,25  64,25
WEA <- Q4@44,26   (one flop for all twelve)
CSA0 <- Q6@45,22 for eight of them; four use small LUTs at x 44..46
address generator cells clustered at x 25..45, y 22..35  (median x 43, y 24)
aspect ratios: ADA13/DIA2 (8192x2), ADA12/DIA4 (4096x4), ADA10/DIA16 (1024x16)
```

Every member uses **exactly 16 384 of its 18 432 bits**, whatever its aspect
ratio. Twelve blocks × 16 384 = **196 608 bits**.

`196 608 = 8192 × 24` — which is a 128 × 64 panel at 24 bits per pixel exactly.
**Do not read anything into that.** The equivalent bank in 10.81 has **13**
members (`WEA <- Q4@43,29`), giving 212 992 bits, which breaks the coincidence;
and the mixed aspect ratios mean the bank is *not* a plain 8192 × 24 array
anyway. The arithmetic is recorded because it is the sort of thing a later
reader will compute and be misled by. — the total is **HIGH**, the
interpretation is **NOT RESOLVED**.

### What the bank structure rules in and out

* **HIGH — there is no second, idle frame buffer.** Bank A and Bank B are the
  only multi-EBR arrays in the design, they are structurally different from
  each other (different depth, different width, different control), and every
  remaining EBR is a singleton or a pair. There is nowhere for a "back buffer
  that never becomes the front buffer" to live. **The double-buffer-swap
  hypothesis is dead.**
* **HIGH — both banks are written at run time and start empty.** Only one EBR
  in the whole design carries a `.bram_init` block
  ([block-ram.md §1](block-ram.md#1-exactly-one-initialised-bram--high)), and it
  is neither bank. A bank that is never written scans whatever the SRAM powered
  up as. That is a mechanism for structured garbage that never goes black,
  and it is consistent with the bench.
* **HIGH — the same two-bank architecture is present in 10.81**, so this is a
  property of the design, not of the build we cannot run.
* **NOT RESOLVED — which bank the raster reads and which the Ethernet writes.**
  See §3.1: the memory that feeds the pads is in **neither** bank, so there is
  at least one more stage between a bank and the output.

### 3.1 The RAM that feeds the output stage is `EBR@4,25`, and its write gate
### is a single flip-flop — HIGH

[output-stage.md §7.3](output-stage.md#73-what-the-21-mux-selects-between--high-that-it-is-counter-vs-bram)
traced the pads' 2:1 mux to block-RAM data out at `JQ5@5,25 ← JDOB13_EBR` and
`JQ2@4,25 ← JDOB2_EBR`. Both J-pins belong to **one** EBR instance: the bel at
`x = 4, y = 25`, whose configuration spans CIB columns 4–6. That is
`MIB_R25C4/C5 EBR0` — the same block
[§7.4](output-stage.md#74-the-control-group-source-ram-starts-empty--high)
already identified as `WID = 1`, uninitialised at configuration time. The two
readings now agree and the map fills the block in:

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

Because `WEA` is strapped high, **`CSA0` is the entire write enable of the
memory that drives the HUB75 pads**, and it is one flip-flop: `Q6@9,27`. Its
fan-in is a five-level cone of ordinary counter and state-machine flops around
`x 7..13, y 27..36` — no constant, no parameter-looking register, nothing that
resolves in five levels. `Q6@9,27` is therefore **the single most interesting
named signal in the design for this fault**, and it is the obvious first target
if anyone gets a way to observe internal state (JTAG readback, a debug build,
or a Diamond-side simulation of the recovered region).

512 × 36 bits is not a frame buffer. It is a **line/scan buffer** — one scan
address's worth of serial data for the output stage, which matches
`CardScanLen = 256` and 36 bits of parallel colour lanes far better than it
matches 8192 pixels. That is a shape argument, not evidence. — MEDIUM.

---

## 4. Why the `0x55` decode itself could not be recovered — and why not to retry

The decode of the type byte, the row field, the offset field and the count
field is a **constant comparison plus a magnitude comparison**. This project
has already established, with a positive control that failed, that:

> **This design does not build constant comparisons out of LUT4s.** The
> Ethernet SFD `0xD5` and the EtherType are as invisible to an exhaustive LUT
> INIT search as any driver-chip id.
> ([parameter-path.md §4](parameter-path.md#4-constants-the-card-compares-against),
> [chip-id.md](chip-id.md))

Two further attempts were made this session and both failed for reasons that
generalise; they are recorded in
`analysis/fpga/negative_results_and_method.txt` as N6 and N7:

* **EBR-to-EBR dataflow** (does the RX FIFO's output reach a bank's data-in?):
  a bounded backward cone of depth 3 from every EBR data-in pin found **zero**
  EBR data-out wires anywhere in the design. The pipeline depth between any two
  memories exceeds three levels of logic, and going deeper runs straight into
  the 93 %-reliable / edge-clamped backward walk.
* **Forward tracing from the RX FIFO outputs**: the fan-out explodes across
  shared span wires and did not terminate in the time available.

So the honest position is: **the netlist can tell you where the packet enters
and where the two candidate destinations are; it cannot currently tell you what
decides whether a given `0x55` payload is written or dropped.** Searching
harder with LUT-constant methods is *known* to be futile, and that is a
finding, not a gap.

### What would settle it in the gateware

Recovering the netlist around the RX FIFO **read port**: what consumes
`DOA*`/`DOB*` of `EBR@39,37` and `EBR@42,37`, and what the byte counter that
walks the header feeds. That is a forward netlist-recovery task on a *small,
localised* region (`x 38..46`, `y 30..45`), which is far more tractable than
any chip-wide search — and it is the one region of the die whose function is
now certain.

---

## 5. The mechanism the evidence actually points at — and it is not in the FPGA

This is documentary, not bitstream, evidence, and it is tagged accordingly.

### 5.1 The `row` and `x-offset` fields are absolute display coordinates — HIGH

Both independent senders agree, and neither has any notion of per-receiver
addressing on the wire:

* **FPP** (`ColorLight-5a-75.cpp`, the copy in this session's scratchpad).
  `Init()` loops `for (row = 0; row < m_rows; row++)` over the **whole
  display's** height and packetises `m_rowSize` = the **whole display's**
  width × 3, splitting at 497 pixels. Every packet goes to the one destination
  MAC `11:22:33:44:55:66` on one socket. There is no receiver index in a `0x55`
  packet and no per-receiver stream. A wall of N receivers all see the same
  bytes.
* **CLTNic** (`docs/pixel-protocol.md` §1.6). The transmitted row field is
  `base(screenNo) + y`, with `base = (n-1) << 12` for screen numbers ≤ 9. The
  row field is explicitly a **global** address space with a screen selector in
  its high bits.

Each receiver therefore has to **window** the stream: keep the pixels whose
`row` falls inside its own vertical extent and whose `x-offset` falls inside
its own horizontal extent, and discard the rest. There is no other way a wall
can work from a single broadcast stream.

### 5.2 The card's window is configuration, and it has its own packet — HIGH

FPP documents a packet type this project has been sending but never verified:

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

and the discovery reply (`0x08`) hands the same information back:

```
  Data[2..3]    firmware version major, minor
  Data[21..22]  cabinet width   as the card believes it
  Data[23..24]  cabinet height  as the card believes it
  Data[38..41]  received packet count
  Data[46..49]  uptime in ms
  Data[85]      receiver card number
```

(`Data[n]` is frame offset `13 + n`.) FPP's author notes that cabinet **x/y
offsets** are in the reply too but that his guessed positions did not survive
testing.

`crates/e120-proto/src/discovery.rs::set_layout` already builds this frame and
`e120-driver` sends it before the first video frame — but with a **98-byte
payload**, i.e. room for the header and one 20-byte record only. FPP's sibling
packet `0x11` carries `3 + 64 × 20 = 1283` data bytes, which is exactly a
64-receiver table. **A short `0x02` may simply be rejected on length.** That is
a hypothesis, not a finding; it is cheap to test.

### 5.3 Why this fits every bench fact

| bench fact | fits? |
|---|---|
| brightness (`0x0A`) and latch (`0x0107`) work | yes — neither carries coordinates, so neither is windowed |
| streaming shifts supply current, panel garbage changes | yes — frames are received, the RX FIFOs churn, the latch frames still fire |
| all-black never darkens the panel; all-white differs | yes — if the writes are discarded the bank keeps its power-on contents; the *difference* between black and white would then come from the burst cadence, not the payload. **This is the weakest link: a fully-windowed-out stream should make black and white identical.** Partial overlap would explain a difference; so would a second effect. |
| config now byte-identical to the seller's `.rcvbp` | yes, and it is the point — the seller's file was compiled for a **256 × 384 wall**, so its notion of where this cabinet sits is not (0,0) in a 128 × 64 display |
| four different host raster layouts all fail | yes — reordering rows cannot help if every row index is rejected |

### 5.4 What this does *not* explain

If the window rejected everything, black and white should look **identical**,
and evidence 3 says they do not. Either the window is partially overlapping
(some rows land), or a second mechanism is in play. Recorded as the one loose
end in this reading.

---

## 6. The next experiment

**Read the card's own answer instead of guessing it.**

Send a `0x07` discovery frame (284 bytes, receiver index at frame offset 16)
with the wire otherwise quiet, capture the `0x08` reply, and dump it. From one
capture you get, all at once:

1. **`Data[21..24]` — the cabinet width and height the card believes it has.**
   If that is not `0x0080 0x0040`, the window hypothesis is confirmed on the
   spot and the fix is a configuration fix, not a gateware fix.
2. **`Data[2..3]` — the firmware version the card reports.** This settles
   16.53-vs-10.81 and, per
   [open-questions.md §3.2](open-questions.md#32-where-does-the-reported-firmware-version-come-from),
   also settles which flash bank actually boots.
3. **`Data[38..41]` — the received-packet counter.** Take a reading, send
   exactly *K* pixel packets, take a second reading. If the counter advances by
   *K*, the packets are **accepted and counted** and the fault is downstream of
   acceptance. If it does not, they are being dropped at the frame level and
   the whole windowing theory is wrong.
4. The bytes around `Data[19..34]`, where FPP could not pin the cabinet
   **offsets**, become readable by differencing: capture, push a `0x02` layout
   with a deliberately odd offset such as `(37, 21)`, capture again, and diff.
   Two captures locate the offset fields exactly.

It is read-only, needs no flash write, costs one frame, and it converts the
central hypothesis from an argument into a measurement. Everything else —
row-base sweeps, layout-packet length experiments — should wait for it.

If the counter *does* advance and the cabinet size *is* 128 × 64, then the
window theory is refuted and the next move is the localised netlist recovery
around `EBR@39,37` / `EBR@42,37` described in §4.
