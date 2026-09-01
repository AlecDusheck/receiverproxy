# Resource inventory and clocking

What the design actually uses inside the LFE5U-25F, and how it is clocked.
Numbers are for `E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH` unless a
table spans versions. Cross-version detail is in
[version-diff.md](version-diff.md).

## 1. Device capacity

| resource | LFE5U-25F has | notes |
|---|---|---|
| PLC2 tiles | 3 036 | 8 LUT4 + 8 FF each |
| LUT4 | 24 288 | |
| block RAM (EBR) | 56 × 18 Kbit = 1 008 Kbit | rows 25 and 37, 7 column groups × 4 |
| DSP | 28 × MULT18X18D, 14 × ALU54B | one row, row 13, 7 groups |
| PLL (EHXPLLL) | 2 (this design uses 1) | |
| SERDES | none on `LFE5U` | the `VCIB_DCU*` tiles in the decode are inert |

## 2. Utilisation — HIGH

| metric | value | how counted |
|---|---|---|
| PLC2 tiles with configuration | **3 034 / 3 036** | any `word:`/`enum:` present |
| LUT4 INIT words written | 24 284 / 24 288 | `ecpunpack` emits one per LUT in a used tile |
| LUT4 with non-zero INIT | **23 199** | the tied-off ones read all-zero |
| LUT4 actually functional | **20 170** | after reducing each INIT by its constant-tied inputs |
| **LUT utilisation** | **~95 %** | the device is essentially full |
| DFFs (distinct slice `Q` sourcing an arc) | **13 074** | MEDIUM-HIGH — no enum marks "FF used" |
| CCU2 (carry-chain) slices | 3 478 | |
| Distributed-RAM slices (DPRAM / RAMW) | 36 / 18 | small; see [version-diff.md](version-diff.md) |
| Routing arcs set | **248 398** | 247 740 resolve to distinct sinks |
| Configured tiles | 4 132 | |
| DSP: MULT18 / ALU54 | **24 / 12** | whole DSP row populated |
| Block RAMs instantiated | **53** or **54** — see note | of 56 |

**EBR count discrepancy — MEDIUM.** Two independent passes gave 53 and 54.
The difference is a counting convention: prjtrellis spreads one EBR's bits over
2–3 adjacent `MIB_EBRn` tiles, so "instances" depends on how tiles are grouped.
Either way the design uses **essentially all** the block RAM
(≈954–972 Kbit of 1008 Kbit). Do not quote a precise figure without re-deriving
it; use `analysis/fpga/scripts/parse4.py`.

### The logic is massively replicated — HIGH

Only **2 129 distinct effective LUT INIT values** exist among 20 170 used
LUT4s, and the histogram is dominated by a handful:

| INIT | count |
|---|---|
| `0x3333` | 1 293 |
| `0x5555` | 1 247 |
| `0xC005` | 769 |
| `0xA003` | 687 |
| `0x900C` | 657 |
| `0x900A` | 639 |
| `0x5003` | 631 |
| `0x0001` | 408 |
| `0x8000` | 341 |

That is the signature of **one datapath slice instantiated many hundreds of
times** — HIGH. The obvious reading is a per-output-channel serialiser / PWM
engine, but that is MEDIUM: replication alone does not prove what is being
replicated. Full histogram: `analysis/fpga/lut_hist_16.53.txt`.

### DSP — HIGH on configuration, NOT RESOLVED on function

The entire DSP row (row 13, columns 4–68) is populated, configured as
`MULT18X18D` multipliers feeding `ALU54B` blocks, with input and pipeline
registers enabled (`REG_INPUTA_CLK CLK0`, `REG_PIPELINE_RST RST0`,
`GSR DISABLED`) — i.e. the full sysDSP complement in MAC configuration.

**What they compute is NOT RESOLVED.** The bitstream gives primitive modes and
register pipelining but no operand semantics. Per-channel gamma or brightness
scaling is the obvious role in an LED controller; there is no evidence for it
in the bitstream and it is not claimed here.

---

## 3. Clocking — resolved end to end

### Reference clock: pin `P6` — HIGH

Traced: `PLL0_LL: arc REFCLK1 ← JREFCLK1_3`, and `JREFCLK1_3@(2,49)`'s unique
fixed uphill is `JPADDIC_PIO@(0,47)` = package pin **`P6`** (R47C0 PIOC,
bank 6, dedicated function `LLC_GPLL0T_IN`). `P6` is configured as a genuine
input (`HYSTERESIS ON`, no `DRIVE`). The reference-select mux is also set:
`CIB_PLL2: arc PLLCSOUT_PLLREFCS ← CLK1_PLLREFCS`.

**Feedback:** `arc CLKFB ← JCLKFB3` and `CIB_PLL3: arc JCLK0 ← G_HPBX0200` —
feedback is taken from the *distributed* CLKOP global net (CLKOP-through-global
feedback). — HIGH

### PLL configuration — raw bits, HIGH

Exactly one PLL is used in all five images: `MIB_R50C2:PLL0_LL`, with the
divider/enable half in the adjacent `MIB_R50C3:BANKREF8`. Verbatim
(`analysis/fpga/pll_dump.txt` has all five images):

```
MODE EHXPLLL,  INT_LOCK_STICKY ENABLED
CLKI_DIV   (absent -> 0000000)     CLKFB_DIV  0000100
CLKOP_DIV  0000100   CLKOS_DIV  0000100   CLKOS2_DIV 0000100   CLKOS3_DIV 0000100
CLKOP_CPHASE  0000100   CLKOS_CPHASE  0000110
CLKOS2_CPHASE 0000101   CLKOS3_CPHASE 0000100
CLKOP_FPHASE  (absent)  CLKOS_FPHASE 111   CLKOS3_FPHASE 001
ICP_CURRENT 00101   LPF_RESISTOR 0010000
MFG_ENABLE_FILTEROPAMP 1   MFG_GMCREF_SEL 10   MFG_GMC_TEST 1110
CLKOP_ENABLE / CLKOS_ENABLE / CLKOS2_ENABLE / CLKOS3_ENABLE = ENABLED
```

### Bit order is MSB-first — HIGH

Not assumed. Three arguments, the third decisive:

1. `MFG_GMC_TEST 1110` = 14 and `MFG_GMCREF_SEL 10` = 2 MSB-first, which are
   Lattice's standard constants. LSB-first gives 7 and 1.
2. `ICP_CURRENT 00101` = 5 MSB-first, a normal Diamond value; LSB-first gives
   20, out of the usual range.
3. **LSB-first is physically impossible.** It gives `CLKFB_DIV = 17` and
   `CLKOP_DIV = 17`, hence f_VCO = 289 × f_REF, which would need f_REF between
   1.4 and 2.8 MHz to keep the VCO in its 400–800 MHz window. No such crystal,
   and this is a dedicated PLL pad.

(An earlier calibration used `EBRn.WID 110000000` = 3 matching `.bram_init 3`
to argue LSB-first. **Do not lean on it** — across 54 EBRs that field only
takes three values, so it is not a clean index and the agreement may be
coincidence. The bit order genuinely varies per field; see
[bitstream-format.md](bitstream-format.md#6-the-word-bit-order-trap--high-that-it-exists).)

### Derived frequencies

Fields are stored as *value − 1*, so MSB-first gives `CLKI_DIV = 1`,
`CLKFB_DIV = CLKOP_DIV = CLKOS_DIV = CLKOS2_DIV = CLKOS3_DIV = 5`:

* **f_CLKOP = 5 × f_REF, f_VCO = 25 × f_REF** — HIGH
* The RGMII TXD ODDRs are clocked by CLKOP, so for Gigabit RGMII
  **CLKOP = 125 MHz, f_REF = 25 MHz, f_VCO = 625 MHz** — MEDIUM-HIGH. This
  rests on the ports being gigabit, which the RGMII pinout supports strongly
  but the bitstream cannot prove absolutely.
* Phases relative to CLKOP: CLKOS ≈ +207° (CPHASE 6, FPHASE 7),
  CLKOS2 ≈ +72°, **CLKOS3 ≈ +9° ≈ +0.2 ns** — CLKOS3 is the RGMII TXC skew
  clock.
* **CLKOS2 is enabled but is not routed to any DCC.** Whether it is used at
  all is **NOT RESOLVED**.

### Global clock network — HIGH for the arcs

From set arcs in `LMID_0`, `RMID_0`, `BMID_0V` and the `CMUX_*` tiles:

| DCC | input | nets driven | fan-out |
|---|---|---|---|
| `LDCC8` | PLL **CLKOP** | UL2, UR3, LL2, LR2 | **1953 — the main system clock** |
| `LDCC6` | PLL **CLKOS** | UL0, UR0, LL0 | ~23 |
| `LDCC4` | PLL **CLKOS3** | UR2, LL1 | the two RGMII TXC pins |
| `LDCC3` | pad `J1` `PCLKT7_1` (left RXC) | UL1, LR1 | left PHY RX domain |
| `RDCC2` | pad `M16` `PCLKT3_0` (right RXC) | UR1, LR0 | right PHY RX domain |
| `LDCC2` | input arc not set (default) | UL3, LL3 | |
| `BDCC0` | **fabric FF `Q1_SLICE@(25,48)`** via `G_JBLQPCLKCIB0` — used as a clock **enable**, not a clock | UL9, UR15, LL9, LR9 | 370 / 294 |
| corner DCCs (`DCCTL`/`DCCTR`/`DCCBL`) | CIB-routed | nets 4, 10, 11, 14 | 374 / 294 / 203 / 95 |

**Ten of the 16 primary global nets are in use** (0, 1, 2, 3, 4, 9, 10, 11,
14, 15).

`BDCC0` is notable: a fabric-generated signal re-buffered onto the global
network with a fan-out of several hundred, also feeding `DCS0` / `DCS1` (so
dynamic clock select is instantiated). It is **used as a clock enable, not as
a clock** — see the correction below.

### Clock domain summary — corrected

**The design is effectively single-clock — HIGH.** 98.9 % of flip-flops are on
PLL CLKOP in 16.53 (12 589 of 12 725), and the same holds in 10.81 and 13.39.

| domain | source | drives |
|---|---|---|
| **system, 125 MHz** (MEDIUM-HIGH on the frequency) | PLL CLKOP | ~1953 loads — **98.9 % of all flip-flops**, plus RGMII TXD |
| RGMII TXC | PLL CLKOS3 | 2 pins |
| PHY-A RX | pad `J1` | left PHY receive path |
| PHY-B RX | pad `M16` | right PHY receive path |
| internal oscillator | `OSC.MODE OSCG`, `OSC.DIV 9` → `G_LDCC2CLKI ← G_JOSC` in all five builds | on a global net; loads NOT RESOLVED |

> **Correction — HIGH.** `BDCC0` (the fabric-generated clock re-buffered onto
> the global network) is **not a second clock domain**. It is distributed on a
> global net and used as a **clock enable**: `G_HPBX0900` appears as `.CE` on
> output-stage flip-flops. **There is no slow LED clock domain** — the LED
> output stage runs at CLKOP and is gated down with enables.
>
> Relatedly: **no pad anywhere is driven from a global clock net.** The HUB75
> DCLK is fabric-generated *data*, not a routed clock.

### Edge clocks — HIGH for the arcs, MEDIUM for the reading

* `ECLK_L`: `W2_JECLK0/1 ← W2_SYNCECLK0/1` and
  `S1W2_JECLK0/1 ← S1W2_SYNCECLK0/1` — ECLKSYNC used, ECLKI source left at
  default.
* `ECLK_R`: `E2_JECLKI0 ← G_JURQECLKCIB0` (global net 15 via `JCLK0@(71,24)`)
  and `E2_ECLKI1 ← G_JLRQECLKCIB1` (global net 9 via `JCLK1@(71,27)`), plus
  the SYNCECLK paths and `S1E2_JECLK1 ← S1E2_JNEIGHBORECLK1`.

Both edge-clock trees are live and are fed **from the fabric-generated clock
(nets 9 / 15), not from the PLL**.

### Other primitives — HIGH

* `OSC.MODE OSCG`, `OSC.DIV 9` — the internal oscillator is enabled.
* `CCLK.MODE USRMCLK` — the fabric drives the configuration-flash clock at
  runtime, i.e. the running design reads the SPI flash.
* `GSR.GSRMODE ACTIVE_LOW` — global set/reset in use.

---

## 4. Output registration — MEDIUM-HIGH

The RGMII outputs are unambiguously registered/DDR from PLL CLKOP.

For the ~125 plain LED-side outputs, **only 12 have any IOLOGIC at all** —
they are driven from fabric LUT/FF outputs through ordinary routing, **not**
through the IO registers. Tracing each output's net to its root gives ~85
roots at slice `F*` / `Q*` outputs and ~50 at routing wires whose driver
prjtrellis does not model (the CIB constant / VLO-VHI paths are the likely
gap).

So: **the LED outputs are combinational at the pad, with the pipelining done
in the fabric**, on the CLKOP domain plus the fabric-generated global nets
9/15.

This is also the clearest structural difference between the PWM and Normal
firmware families — the Normal/LS builds push ~96 more outputs through IO-cell
registers. See [version-diff.md](version-diff.md).
