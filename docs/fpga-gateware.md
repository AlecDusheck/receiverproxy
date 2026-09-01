# The E120 FPGA gateware — overview

Reverse engineering of the Colorlight E120 receiving card's gateware, from the
vendor firmware images in `third-party/firmware/`, using open-source tooling
only (prjtrellis). No Lattice Diamond and no Colorlight software was executed.

**This is the summary. The detail is in [`docs/fpga/`](fpga/README.md)** — one
file per topic, indexed in [`docs/fpga/README.md`](fpga/README.md).

Every claim in this document set is tagged **HIGH** (read directly from the
bytes and cross-checked), **MEDIUM** (a strong pattern on one stated
assumption) or **NOT RESOLVED**. Nothing is inference dressed as fact.

---

## What the card is — HIGH

* **FPGA:** Lattice ECP5 `LFE5U-25F-6CABGA256`, IDCODE `0x41111043`. All
  eight IO banks at **3.3 V**.
* **Ethernet:** exactly **two RGMII gigabit ports**, proven pin by pin — 12
  pins each, 4 data + 1 control per direction, DDR throughout, no tri-state,
  no DQS.
* **Memory:** **none external.** No DQS group, no DDR DLL, no bidirectional
  bus. All buffering is on-chip in **53 of 56 block RAMs** (~954 Kbit).
* **Flash:** one SPI flash holding both the bitstream and the card's
  configuration. `CCLK.MODE USRMCLK` — the running design reads it.
* **LED side:** roughly **147 pins**, driving up to twelve HUB75E connectors
  (J1–J12). **96 of them are the serial RGB data lines** — 32 groups × 3
  colours, on the left and right edges. The HUB75 **control** signals (A–E,
  CLK, LAT, OE) are the top-edge pads, identified as a group but **not yet
  decomposed**.
* **Clocking:** one EHXPLLL from a 25 MHz reference on pin `P6`. The design is
  effectively **single-clock** — 98.9 % of flip-flops run on CLKOP at 125 MHz.
  CLKOS3 supplies the RGMII TXC skew. The fabric-generated global net is a
  **clock enable**, not a second domain.
* **Utilisation:** ~95 %. 20 170 functional LUT4s of 24 288; 13 074 flip-flops;
  the whole DSP row; essentially all the block RAM. **The part is full.**

→ [pinout.md](fpga/pinout.md), [resources.md](fpga/resources.md),
[block-ram.md](fpga/block-ram.md)

## The bitstream — HIGH

The `.hex` files are raw Lattice `.bit` images: 342-byte ASCII header,
preamble `BD B3` at `0x158`, a short command stream, then **7562 frames of
77 bytes** (74 data + 2 CRC big-endian + 1 dummy `0xFF`), then USERCODE, a
2048 × 9-bit EBR init block, and `DONE`.

The frame CRC is **CRC-16 poly `0x8005`, init 0, MSB-first**, accumulating
over the command stream since the last reset — frame 0's CRC covers everything
from `0x162`, later frames cover the preceding `0xFF` plus their 74 bytes.
With that model **all 7562 frames validate in all five vendor images**.

Decoding needs exactly one piece of wrangling: truncate the file at `0x8ED30`
so `ecpunpack` does not walk into the trailer and abort.

→ [bitstream-format.md](fpga/bitstream-format.md),
[decode-method.md](fpga/decode-method.md)

## Two traps that will bite anyone who repeats this — HIGH

1. **prjtrellis `word:` bit order varies per field.** The PLL's dividers are
   MSB-first; other fields are not. Always check `bits.db`.
2. **`PIOx.BASE_TYPE` names are not IO standards.** Many standards share one
   bit pattern and prjtrellis prints the alphabetically last match. Every
   `SSTL18`/`SSTL15` label in the decode is an artefact — all banks are 3.3 V.
   **Take pin direction from the routing graph, never from `BASE_TYPE`.**

3. **"No combinational source" means nothing on its own.** CCU2 carry travels
   on fixed, non-configurable wires, so **1012 of 6956 CCU2 LUTs in 16.53 have
   zero routed inputs**. An arc-only tracer reports every increment stage on
   the die as sourceless. One promising lead in this project was refuted by
   exactly this.
4. In pytrellis, resolve arc endpoints with `rg.globalise_net(row, col, name)`,
   not `id_at_loc` — the latter silently fails on 62 % of arcs.

And a limit worth knowing before planning work: **deep backward tracing is only
~93 % reliable and far worse at the die edge** (prjtrellis clamps out-of-range
span wires to the boundary), so only 45 of the 96 RGB pads resolve to a driver
cell. **Prefer forward reasoning or one-hop IOLOGIC signatures** — that is how
the 96 RGB pins were actually found.

## Which firmware the card is actually running — HIGH

**`E320_PCB6.0_PWM_FPGA10.81_20230907`, not 16.53.** Confirmed three ways: the
header date, a per-block match of exactly `1.000000` outside the reserved
span, and 10.81's uniquely different EBR ROM present in both primary dumps.

Most of the analysis targets 16.53 because that is the firmware the project
intends to run. **Check which image a claim refers to before acting on it.**

→ [flash-layout.md](fpga/flash-layout.md)

## The chip id — NOT RESOLVED in the gateware, answered on the bench

The card takes a 16-bit driver-chip id at parameter-pack byte `+0x1B`
(the escape `0xFE` when the id ≥ `0x100`) with the full id big-endian at
`+0xE7..+0xE8`, **excluded from the pack CRC-32**.

An exhaustive search found **no constant comparator against any chip id** —
and none against the Ethernet SFD or ethertype either. **This design does not
build constant comparisons out of LUT4s**, so that search cannot succeed and
must not be repeated. The surviving hypothesis is a register file compared
data-vs-data. (The `R27C44_Q0..Q3` "mode field" lead that once looked concrete
is **refuted** — it is an ordinary CCU2 accumulator.)

The bench settles what the netlist could not: the gateware **does** branch on
the id, and **`0x014C` is very likely correct while `0x0214` is not** —
MEDIUM-HIGH.

→ [chip-id.md](fpga/chip-id.md),
[parameter-path.md](fpga/parameter-path.md)

## Version differences — HIGH

The board interface is **frozen** across all five images: identical pin
directions on 196 of 197 pins, the same PLL divider plan, the same RGMII
split. Only phases and logic change.

The real family split is **PWM vs Normal/LS**, and it shows up as IO-cell
register usage: `IOLOGIC*.MODE = IREG_OREG` appears **96 times** in Normal
13.39 and LS 6.69 but only **10** times in the three PWM builds. **96 = 32
serial RGB groups × 3 colour lines** — exactly the E120 spec's "32 groups of
serial RGB data" — which is both the most likely explanation for the panel
being dead on 13.39 and the fastest route to a classified HUB75 pin list.

Version numbers are not one sequence: 10.81 is dated *after* 13.39.

→ [version-diff.md](fpga/version-diff.md)

---

## What this means for getting a test pattern on the panel

The bench facts triangulate tightly:

| fact | what it establishes |
|---|---|
| brightness scales current | the card parses our frames, the scan engine runs, OE/current modulation works, the drivers are armed and sinking current — **HIGH** |
| `0x014C` gives per-pixel noise at 2.8–4 A | individual pixels are individually addressable: the chain loads, the latch fires, the PWM engines run. A panel showing per-pixel noise under a uniform white fill is **displaying buffer contents that are not our content** — **HIGH** |
| dead on Normal 13.39, responds on PWM builds | SM16269S is a PWM-class self-scanning driver and only PWM gateware speaks its protocol. **16.53 is the right build; stop chasing other families** — **HIGH** |
| our frames are byte-exact FPP and on the wire | the host encoder is not the problem — **HIGH** |
| the card's own test pattern also fails | the fault is at or below the card's raster stage — **but the selector enum is unknown and a background streamer may have been overwriting the framebuffer, so this is the weakest link in the chain** — NOT RESOLVED |

> **The driver protocol is right, the drivers are armed, and the raster is
> being scanned. What is wrong is which bytes reach the scan buffer.**

### Do these, in order

1. **Flash `build/p25-128x64-sm16269s-block7.bin`** (it lands at flash
   `0x070000`), fix the screen-size record at `0x7F000`, `reload-params
   --full`, `send-params`. The corrected config — right CardScanLen (256, not
   512), right module positions, right serial clock, double latch — **has
   never actually been on the card.** Every scrambled-content result predates
   it.
2. **Keep sending chip id `0x014C`**, not `0x0214`.
   `config/panels/p25-128x64-sm16269s.toml` already points at
   `config/chips/sm16269.toml` (family `0x14C`, sub `0x14D`), which is
   correct.
3. **Kill every background `fill --hold` streamer**, confirm the wire is
   quiet, and **press the card's physical test button.** That bypasses the
   host, the Ethernet stack and the `0x33` command path entirely. If the
   button lights the panel, the output stage is proven good and everything
   left is the data path — and it settles the one fact the whole diagnosis
   currently hangs on.
4. **Send exactly one lit pixel** at (0,0), then one row, then one column. A
   scrambled raster and a missing raster are indistinguishable under a uniform
   fill and completely different under one pixel.

Full ranked hypotheses with per-hypothesis experiments:
[output-stage.md §6](fpga/output-stage.md#6-reconciling-the-bench-facts).
Everything still unresolved, tiered by impact, with what would settle each:
[open-questions.md](fpga/open-questions.md).
