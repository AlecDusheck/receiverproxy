# Version comparison

The five vendor E320/E120 gateware images compared at the decoded level:
tiles, LUT functions, block RAM, IO, clocks. Regenerable artefacts (not kept
in the repository; see [decode-method.md](decode-method.md)):
`analysis/fpga/summary_cross_image.txt`, `pll_dump.txt`, `final_*.tsv`.

## 1. The images

| file | family | PCB | version | header date |
|---|---|---|---|---|
| `E320_PCB6.1_LS0allDA_FPGA6.69_20220907` | LS0allDA | 6.1 | 6.69 | 2022-09-07 |
| `E320_PCB6.0_PWM_FPGA9.53_20221031` | PWM | 6.0 | 9.53 | 2022-10-31 |
| `E320_PCB6.0_Normal_FPGA13.39_20221101` | Normal | 6.0 | 13.39 | 2022-11-12 |
| `E320_PCB6.0_PWM_FPGA10.81_20230907` | PWM | 6.0 | 10.81 | 2023-09-07 |
| `E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH` | PWM | (none in name) | 16.53 | 2023-12-27 |

The version numbers are not one sequence. 10.81 is dated 2023-09-07, later
than 13.39 (2022-11) and earlier than 16.53 (2023-12). They are per product
line.

All five carry the same internal `Design name: lattice_lhf_lattice_lhf.ncd`
and the same `Bitstream CRC: 0x3474`; the latter is identical across five
images with different contents and so is not a content checksum.

## 2. Resource use

| metric | 6.69 LS | 9.53 PWM | 13.39 Normal | 10.81 PWM | 16.53 PWM |
|---|---|---|---|---|---|
| LUT4 with non-zero INIT | 21 371 | 22 392 | 22 208 | **23 734** | 23 199 |
| LUT utilisation (of 24 288) | 88.0 % | 92.2 % | 91.4 % | **97.7 %** | 95.5 % |
| LUT4 functional after tie-reduction | 18 839 | 19 403 | 19 098 | 20 349 | 20 170 |
| distinct effective INIT values | 2 227 | 2 125 | 2 148 | 2 147 | 2 129 |
| DFFs | 12 112 | 13 023 | 11 279 | **14 062** | 13 074 |
| CCU2 (carry) slices | 2 807 | 3 451 | 3 403 | 3 836 | 3 478 |
| DPRAM / RAMW slices | 178 / 89 | 40 / 20 | 118 / 59 | 40 / 20 | 36 / 18 |
| EBRs | 54 | 53 | 49 | 53 | 53 |
| DSP MULT18 / ALU54 | 24 / 12 | 26 / 13 | 26 / 13 | **28 / 14** | 24 / 12 |
| PLC2 tiles configured | 3 036 | 3 036 | 3 036 | 3 036 | 3 036 |
| PIO sites with a `BASE_TYPE` | 377 | 377 | 377 | 377 | 377 |
| routing arcs | 233 152 | 238 773 | 229 023 | **259 917** | 248 398 |

Utilisation ordered: 6.69 (88.0 %), 9.53 / 13.39 (about 92 %), 16.53
(95.5 %), 10.81 (97.7 %). Carry-chain slices 2 807 to 3 836; DFFs 12 112 to
14 062; arcs 233 k to 260 k.

Distributed RAM decreases across the images (178 DPRAM slices in 6.69, 118 in
13.39, 40 in 9.53/10.81, 36 in 16.53) while EBR usage rises. The counts are
measured from the decode; the reading that small storage migrated from
LUT-RAM into block RAM is inferred.

## 3. Board interface

The board interface is the same in all five images: the same 377 configured
PIO sites; byte-identical pin directions on 196 of 197 package pins (the
exception is `R7`, SPI flash `D5/MISO2`, an input only in 13.39); one
EHXPLLL with the same divider plan; the same 24 RGMII pins split 12 + 12; the
same `USRMCLK` / `OSCG` / `GSR` setup; all banks 3V3. The pinout in
[pinout.md](pinout.md) is valid for all five images.

Apparent IO-standard differences between 13.39/6.69 and the PWM builds in
3-6 IO sites are `BASE_TYPE` name differences, a degenerate decode
([pinout.md](pinout.md#the-base_type-trap)), and not electrical. The one
electrically real difference is `MIB_R50C4.PIOA`, the EFB0/GSR pin:
`DRIVE 16` in 6.69, `DRIVE 4` in every other image. 6.69 is the PCB 6.1
image; a board-revision correlation is inferred from n = 1.

## 4. Netlist similarity

Placement-independent LUT-function multisets give Jaccard 0.64-0.75 for every
pair, including the closest (9.53 vs 16.53, 0.748; 10.81 vs 16.53, 0.717).
No two images are re-places of one netlist; every version is a different
design. LUT-function multiset overlap proves dissimilarity; a high score
would not have proved identity.

## 5. Family split: IO-cell register usage

The PWM, Normal and LS families separate on IO-cell register usage:

| | `IOLOGIC*.MODE = IREG_OREG` | `IOLOGIC*.CEOMUX = 1` |
|---|---|---|
| **13.39 Normal**, **6.69 LS0allDA** | **96** | 176 / 153 |
| **9.53, 10.81, 16.53 PWM** | **10** | 81 |

The Normal and LS builds register about 96 more output pins inside the IO
cell. The counts are measured; that the PWM builds moved that logic into the
fabric is inferred. The 96 IO registers map onto the 96 RGB data pads
([output-stage.md](output-stage.md)).

This IO-register difference is the candidate explanation for the bench
result that the panel is completely dead on Normal 13.39 (measured: 0.44 A)
and responds on the PWM builds. Nothing was traced from those 96 IO registers
to a specific panel signal, so it is not proof.

13.39 has the fewest EBRs (49) and the lowest DFF count (11 279) of the five.

## 6. PLL differences

Across all five images the dividers, output enables, charge-pump current and
loop filter are identical. Only `CPHASE` / `FPHASE` of CLKOS, CLKOS2 and
CLKOS3 differ:

| field | 6.69 | 9.53 | 10.81 | 16.53 | 13.39 Normal |
|---|---|---|---|---|---|
| `CLKOS_CPHASE` | 0000110 | 0000110 | 0000110 | 0000110 | **0000111** |
| `CLKOS_FPHASE` | 111 | 111 | 111 | 111 | **001** |
| `CLKOS2_CPHASE` | 0000101 | 0000101 | 0000101 | 0000101 | **0001001** |
| `CLKOS2_FPHASE` | (0) | (0) | (0) | (0) | **111** |
| `CLKOS3_CPHASE` | **0001000** | 0000100 | 0000100 | 0000100 | 0000100 |
| `CLKOS3_FPHASE` | **111** | 001 | 001 | 001 | (0) |

9.53, 10.81 and 16.53 have byte-identical PLL configuration. 6.69 differs
only in CLKOS3 phase; 13.39 differs in CLKOS, CLKOS2 and CLKOS3 phase.

Phase sets the launch-edge relationship between the data, shift-clock and
latch outputs. CLKOS3 is the RGMII TXC skew clock
([resources.md](resources.md)); inferred: 6.69's CLKOS3 difference is a PHY
timing change, and 13.39's three-way phase change is the panel-side retiming
that goes with its 96 IO registers. Neither was traced.

## 7. Initialised ROM

The one initialised block RAM is byte-identical across 6.69, 9.53, 13.39 and
16.53. Only 10.81 differs, by a five-entry-longer prologue. Adding
SM16386S/SM16269SH support in 16.53 changed nothing in it. See
[block-ram.md](block-ram.md).

## 8. 10.81 as an outlier

10.81 is the largest design of the five, the only one using all 28
multipliers and all 14 ALU54s, the only one with a different BRAM ROM, and
has the most routing arcs. It is dated between 13.39 and 16.53. Reading it as
a separate product line is inferred.

## 9. Structural enum fingerprint

Non-PLC2 `(tiletype, key, value)` triples:

| image | distinct triples | unique to it |
|---|---|---|
| 13.39 Normal | 1 108 | 36 |
| 10.81 | 1 102 | 5 |
| 9.53 | 1 083 | 27 |
| 6.69 | 1 083 | 15 |
| **16.53** | 1 106 | **1** |

953 triples (86-88 %) are common to all five. The unique sets:

* **13.39 (36)**: almost all `IOLOGIC*.CEMUX = CE` and
  `IOLOGIC*.OUTREG.REGSET = SET` at PICL/PICR/PICT sites (section 5), plus
  one EBR configured x2 in `EBR_CMUX_LR_25K`.
* **9.53 (27)**: entirely `ALU54_7.*` DSP-ALU register/opcode settings: one
  extra DSP ALU wired differently.
* **6.69 (15)**: the EFB0 pin drive, `PICL1_DQS3 PIOD` output class, IOLOGIC
  GSR flags.
* **10.81 (5)**: an extra 36-bit `PDPW16KD` and two
  `MULT18_0.REG_PIPELINE_RST`.
* **16.53 (1)**: a single `MULT18_5.REG_INPUTA_CLK NONE`.

16.53 is unique in exactly one enum triple: what distinguishes it from
9.53/10.81 lives almost entirely in LUT functions and routing, not in
primitive configuration.

## 10. Command-stream difference

The control-register operand at file offset `0x16A` is `0x40000000` in 6.69
and `0x40000020` in the other four. Bit 5 is in the SPI-mode area of ECP5
control register 0.

## 11. Unresolved

* Which build is right for an SM16269S panel, from the decode alone. 16.53
  is the only published image whose name mentions the SM16269 family, and the
  bench shows PWM builds respond where Normal does not, but nothing in the
  decode identifies a driver-chip protocol. On the bench, 16.53 renders
  ([../rendering.md](../rendering.md)).
* What the LS0allDA family is. Only the name and the resource profile are
  known.
* Why 10.81's ROM prologue is five entries longer.
* The meaning of control-register bit 5.
