# IO pinout and board architecture

Derived from the routing graph of
`E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH`. The full table is
`analysis/fpga/PINTABLE_16.53.txt` (197 package pins, one line each);
per-pin raw routing evidence for all five images is in
`analysis/fpga/final_*.tsv`.

**The pinout is a property of the board, not of one firmware.** Running the
same analysis over all five vendor images gives byte-identical direction flags
on **196 of 197 pins**. The single exception is `R7` (SPI flash `D5/MISO2`),
used as an input only in the Normal 13.39 build. — HIGH

---

## The `BASE_TYPE` trap

**Do not believe the IO-standard names prjtrellis prints.** — HIGH

Proven from `database/ECP5/tiledata/*/bits.db`:

* In `PICL0` / `PICR0` there are only **three** distinct `PIOA.BASE_TYPE` bit
  patterns: empty (30 names, including every `INPUT_*` and `NONE`),
  `{F1B4,F2B4}` (26 names — every single-ended `OUTPUT_*` **and** every
  `BIDIR_*`), and a six-bit pattern (28 differential names). prjtrellis prints
  the alphabetically last matching name, which for the middle pattern is
  `OUTPUT_SSTL18_II`. **So "OUTPUT_SSTL18_II" on a left/right pin means only
  "single-ended output driver enabled".**
* In `PIOT0` / `PICL1` the fuzzed `BASE_TYPE` patterns absorbed the `DRIVE`
  bits. `OUTPUT_LVTTL33 = {F15,F16,F17,F2,F7,F9}` while
  `DRIVE 8 = {F15,F16,F17,!F18,!F19}`. Consequently **a real `BIDIR_LVTTL33`
  pin at `DRIVE 4` decodes as `INPUT_LVTTL33`** — its bits are a strict subset
  match. This is why 18 apparent "inputs" had a driven `PADDO` *and* `PADDT`.
* **All eight banks are `BANK.VCCIO 3V3`** in all five images (35 of 35
  `BANKREF` tiles). `3V3` is the only VCCIO value with a unique bit, so this
  decode is reliable. **There is no 1.8 V or 1.5 V signalling anywhere on this
  board.** Every `SSTL18` / `SSTL15` / `HSUL` label in the decode is an
  artefact.

### The correct test for pin usage

Direction comes from the **routing graph**, never from `BASE_TYPE`:

| result | test |
|---|---|
| `OUT` | the CIB wire feeding `JPADDO<x>` is the sink of a set arc |
| `BIDIR` | the above, *and* the CIB wire feeding `JPADDT<x>` (tri-state) is driven |
| `IN` | the CIB wire fed by `JDI<x>` is the source of a set arc |
| `IN-DDR` / `OUT-DDR` | via IOLOGIC `JRXDATA0/1` / `JTXDATA0/1` with `MODE IDDRX1_ODDRX1` |
| `OUT-const0/1` | no routing, but the CIB input mux (`CIB.JA*MUX`/`JB*MUX`) ties the pad to a constant |

There is **no** A/C or B/D CIB-wire sharing (an early scare, and wrong). At a
left/right PIC site `(x=0, y=r)`: `JPADDOA ← JA0@(1,r)`,
`JPADDOB ← JA3@(1,r)`, `JPADDOC ← JA0@(1,r+2)`, `JPADDOD ← JA3@(1,r+2)`. On
the top edge PIOA uses the CIB at column X and PIOB at column X+1. Every PIO
has a private CIB node whose only fan-out is
`{J*_CIBTEST, JPADD*, JTXDATA0*}`, so the test above is exact. — HIGH

**Cross-check that closes the loop:** after correcting for the aliasing,
direction from routing and direction from electrical config agree on **all 197
pins with zero contradictions** — every routing-BIDIR pin has
`DRIVE 4 + HYSTERESIS ON`, every routing-OUT pin has a `DRIVE` +
`PULLMODE NONE`, every routing-IN pin has `HYSTERESIS ON` and no `DRIVE`.

The one place the "fill" reading *is* right: all 42 `OUTPUT_SSTL15D_II`, all 7
`OUTPUT_SSTL18D_*`, all 18 `OUTPUT_LVCMOS33D` and 47 of 49 `OUTPUT_SSTL15_II`
entries sit on **unbonded** pads. Those are Diamond's unused-pad fill. But of
the 57 **bonded** `OUTPUT_SSTL18_II` sites, **55 have a live fabric driver** —
they are real pins. — HIGH

---

## 1. Pin census — HIGH

| direction | TOP (b0/b1) | RIGHT (b2/b3) | BOTTOM (b8) | LEFT (b6/b7) | total |
|---|---|---|---|---|---|
| OUT, fabric-driven | 20 | 48 | 10 | 47 | **125** |
| OUT-DDR (ODDR data) | – | 5 | – | 5 | **10** |
| OUT-DDR clock (ODDR fed 1/0) | – | 1 | – | 1 | **2** |
| OUT tied to a constant | 3 | 2 | – | 1 | **6** |
| BIDIR (`PADDO` + `PADDT` both driven) | 29 | 1 | 1 | 3 | **34** |
| IN-DDR (IDDR) | – | 5 | – | 5 | **10** |
| IN (plain or dedicated) | 3 | 1 | 2 | 2 | **8** |
| unused | 1 (`A7`) | 1 (`N16`) | – | – | **2** |

**34 pins are genuinely bidirectional.** 20 of them share a single tri-state
enable whose root is one flip-flop, `Q2_SLICE@(25,2)`:
`A2 A3 A4 A5 B2 B3 B4 B11 B13 C3 C5 C6 C13 D5 D7 D11 E5 E6 E10 E11`.
— HIGH for "20 pins, one common OE". What that bus *is* — **NOT RESOLVED**;
see §4.

---

## 2. Ethernet: two RGMII Gigabit ports — HIGH

### Memory is ruled out, four independent ways

1. **No `DQSBUF`, `DDRDLL`, `DLLDEL` or `ECLKBRIDGE` configuration anywhere**
   in any of the five images — only tile *names* containing `DQS`
   (`PICL1_DQS0` etc.) hosting ordinary IO. A DDRx interface cannot exist
   without a DQS group.
2. **No bidirectional pin on the left or right edges except `T4`** (one pin).
   A DDRx DQ bus cannot exist without bidirectional pads.
3. All banks are 3.3 V.
4. The SSTL labels are decode artefacts (above).

### RGMII is proven positively

The 24 DDR/clock pins fall into two perfectly symmetric 12-pin groups:

| signal | **PHY-A (left edge)** | **PHY-B (right edge)** |
|---|---|---|
| RXC — dedicated clock input | **`J1`** R23C0A `PCLKT7_1` | **`M16`** R26C72C `PCLKT3_0` |
| RXD[3:0] + RX_CTL (IDDR) | `J2`, `K1`, `K2` (R23C0 B/C/D), `J3`, `K3` (R20C0 C/D) | `L16`, `L15`, `M15` (R26C72 A/B/D), `P16`, `R16` (R35C72 A/B) |
| TXC — ODDR clock output | **`L1`** R26C0A `PCLKT6_1` | **`J16`** R23C72A `PCLKT2_1` |
| TXD[3:0] + TX_CTL (ODDR) | `L2`, `M1`, `M2` (R26C0 B/C/D), `P1`, `R1` (R35C0 A/B) | `J15`, `K16`, `K15` (R23C72 B/C/D), `J14`, `K14` (R20C72 C/D) |
| RX clock domain | global net 1 (UL quadrant) | global net 0 (LR quadrant) |
| TXD launch clock | global net 2 = PLL **CLKOP** | global net 3 = PLL **CLKOP** |
| TXC launch clock | global net 1 = PLL **CLKOS3** | global net 2 = PLL **CLKOS3** |

Why this is RGMII and not something else — HIGH:

* 4 data + 1 control in each direction, **DDR on all ten**, **zero tri-state**,
  **zero DQS**. That is the RGMII signature exactly.
* The two TXC pins are proven to be **generated clocks**, not stubs: their CIB
  input muxes are tied to constants — `CIB.JA0MUX 1` (TXDATA0 = 1) with
  `JC0MUX 0` (TXDATA1 = 0) in `CIB_R26C1` and `CIB_R23C71`. An ODDR fed 1/0
  emits a clock. They also carry `DATAMUX_ODDR IOLDO`,
  `IOLOGIC MODE IDDRX1_ODDRX1` and `DRIVE 8`.
* **TXC runs on CLKOS3 while TXD runs on CLKOP** — a deliberate RGMII TXC/TXD
  skew of about +0.2 ns. HIGH for the mechanism; MEDIUM for the number, which
  depends on the VCO frequency.
* Each PHY's RX logic is clocked by *that PHY's own* RXC pad, through its own
  DCC onto its own global net — confirmed independently by the CMUX arcs and
  by the IOLOGIC clock muxes.

**Conclusion: exactly two Ethernet ports, RGMII, Gigabit class.** — HIGH

### PHY management

Six pins on left bank 6 use `IOLOGIC MODE IREG_OREG` (single-rate registered
IO): `N1`, `N4`, `P3`, `P4`, `R5` as outputs and **`T4` (R44C0B) as a
registered BIDIR** (`PADDO`, `PADDT` and `PADDI` all live).

* `T4` is a registered bidirectional control pin — HIGH.
* `T4` is **MDIO** and one of the others **MDC**, the rest PHY resets/straps —
  MEDIUM.

---

## 3. SPI configuration flash — HIGH

Bank 8 (bottom edge) is the boot SPI flash, and it is **live at runtime**:

* `CCLK.MODE USRMCLK` — the fabric drives the flash clock after configuration.
* `T6` = `D7/IO7` as a registered BIDIR; `T7` = `D1/MISO` as an input;
  `CSN`, `WRITEN`, `HOLDN`, `D0`, `D2`, `D3`, `D4`, `D6` as outputs.
* 13 bank-8 pins in total.

This is how the running design reads its stored configuration back out of the
same flash it booted from — see [parameter-path.md](parameter-path.md).

---

## 4. The LED side — 147 pins, structure NOT RESOLVED

After removing 24 RGMII pins, 6 PHY-management pins and 13 bank-8 SPI pins,
roughly **147 pins remain on the LED side** (≈52 top, ≈44 left, ≈51 right), of
which 32 are bidirectional. — HIGH

**That is about ten times a single HUB75 port.** An early worry in this
analysis — "only 7 output pins, too few for HUB75" — was an artefact of the
`BASE_TYPE` trap and is **wrong**. Discard it.

One striking structural feature — HIGH: a contiguous run of **14 top-edge pins
at `DRIVE 8` / `SLEWRATE FAST`** spanning `R0C27`…`R0C44`:

```
C7  B7  A8   E8 D8   C8 B8   B9 C9   D9 E9   A9   B10 C10
```

These are the only pins on the top edge with that drive/slew combination. 14
is exactly a HUB75E port's signal count (6 RGB + 5 address + CLK + LAT + OE),
which is suggestive — but suggestive is all it is. **MEDIUM at best**, and it
does not survive as evidence on its own because the left and right edges carry
~95 more LED-side outputs with no comparable grouping.

### What is NOT RESOLVED

* **Which pins form which physical connector.** Nothing in the bitstream ties
  a pad to a connector. 147 does not factor cleanly into HUB75E ports under
  any obvious sharing scheme (14/port → 10.5; 6 data + OE per port with
  shared A–E/CLK/LAT → 19.3), so no port count is claimed here.
  *Resolving this needs continuity-buzzing the PCB, or a clear photo of the
  hub connector pinout traced to the BGA.*
* **What the 34 bidirectional pins are.** They are real (out-enable driven
  from fabric, `HYSTERESIS ON` input buffers) and 20 share one OE flip-flop.
  Readback from the LED driver chain is *plausible* given the firmware is
  named for SM16386S/SM16269SH (chips with status/error readback), but that is
  speculation and is flagged as such.
  *Resolving this needs a scope on the hub connector during a chip-register
  write, watching for the card driving then releasing a line.*
* **Six pins strapped to constants** via the CIB input mux at
  `DRIVE 16 / SLEWRATE FAST`: `A15`, `M6`, `K12` (constant 0), `E12`, `E13`
  (constant 1), plus one more. HIGH that they are static level outputs;
  their meaning is NOT RESOLVED (enables or mode straps).

---

## 5. Board architecture implied by the pinout — HIGH unless noted

```
                    +-------------------------------+
  RJ45 #1  ===RGMII==|  LEFT  bank 6/7               |
  (PHY-A)   12 pins  |    + 6 PHY mgmt (MDIO/MDC?)   |
                     |    + ~44 LED-side outputs     |
                     |                               |
                     |        LFE5U-25F              |==  ~52 LED-side  ==  TOP bank 0/1
                     |        CABGA256               |     (29 BIDIR, 20 OUT,
                     |        3.3 V, all banks       |      incl. the 14-pin DRIVE 8 run)
                     |                               |
  RJ45 #2  ===RGMII==|  RIGHT bank 2/3               |
  (PHY-B)   12 pins  |    + ~51 LED-side outputs     |
                     +-------------------------------+
                                   |
                            BOTTOM bank 8
                            13 pins, SPI flash
                            (boot + runtime, USRMCLK)
```

* Two gigabit Ethernet ports (in and out / daisy-chain).
* One SPI flash holding both the bitstream and the card's configuration.
* No external RAM of any kind — all buffering is in the FPGA's 53 block RAMs.
* Everything else — ~147 pins — goes to the LED hub connector(s).
* No dedicated LED/button pins were identified. The six constant-strapped
  outputs are the only candidates, and their function is NOT RESOLVED.
