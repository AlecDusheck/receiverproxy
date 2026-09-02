# The driver-chip serial protocol: where it lives and what selects it

The LE/LAT command encoding the card emits to the driver chips is parameter
data: it is carried in the 20-byte `SChipControl` block of record 0x01
(`+0x0C4`), which the host sends on every `rxp config send` and which the
boot image holds. It is not in the microcode ROM, and it is not hard-wired
per chip in the gateware. This page gives the evidence for both negatives,
the decode of the block against three open-source driver profiles and the
vendor's 29-file corpus, the SM16269 datasheet facts on GCLK/RCLK, and the
data-upload path.

Artefacts: `analysis/fpga/rom_*_decoded.txt`,
`analysis/fpga/chip-control-corpus.tsv`,
`analysis/fpga/negative_results_and_method.txt` (§N10-N12),
`third-party/datasheets/SM16269_ZIGZZZAV10_datasheet_2025-08.pdf`. The
`analysis/fpga/` tree is not kept in the repository
([README.md](README.md#raw-artefacts)).

Companion: [`docs/chip-control-block.md`](../chip-control-block.md) decodes
the same 20 bytes from the vendor library's disassembly: jump table,
per-byte write site, and the exact `GetScanCycleLevel` formula behind bytes
10-13. The two pages agree on structure and on bytes 10-13. For bytes 1-6
that page records only the write sites (per-chip-id literals from
`ResetChipControl`, unnamed in the library); §2 here gives their meaning
from external evidence and marks it inferred. Where the two conflict,
`chip-control-block.md` is reading the code that writes the bytes and takes
precedence.

---

## 0. Summary

| subject | fact |
|---|---|
| The protocol in 16.53's ROM | None. The ROM is byte-identical in 16.53, 13.39 (Normal), 9.53 (PWM) and 6.69 (LS0allDA). A block that does not change across the Normal/PWM/LS split cannot encode a chip-specific serial protocol. No LE-tail table, no guard word, no addressed-register structure is present in any build |
| Runtime protocol selection | By the chip id, as data. The host tool's `ResetChipControl` jump table (indexed by `chipType - 0x10`) emits a 20-byte `SChipControl` descriptor per id into record 0x01 `+0x0C4`. It is all-zero for exactly the non-S-PWM chips and non-zero for every S-PWM chip, and it is the only chip-protocol-shaped payload in the pack. The per-byte semantics in §2.1 are inferred |
| GCLK | The SM16269 has no GCLK and no OE pin. Pin 21 is RCLK; the datasheet block diagram wires RCLK -> 16-bit counter -> PWM controller, so RCLK is the grey clock and also advances the row. `SChipControl[10..13]` is the card-side "scan cycle level"; the bench value `0x0097` = 151 is what the vendor formula gives for the bench registers (§4.2). Record `+0x031` `SetGClock` never reaches the pack |
| Data upload | 16-bit words per output channel, MSB first, R/G/B as six parallel lanes, output-major / chip-minor nesting, with a 1-DCLK LE data-latch tail (`SChipControl[5] = 0x01` in every S-PWM profile in the corpus). Inferred from the datasheet and two reference drivers |

Measured on the bench (firmware 16.53, [../rendering.md](../rendering.md)):
only chip id `0x014C` with the SH descriptor `[2][3][4] = 1, 5, 6` arms the
SM16269S outputs and renders. Tails `2/4/8` and `3/5/7` under `0x014C`,
and ids `0x0214`, `0x00DE` and `0x002F`, never arm.

---

## 1. The microcode ROM is not a chip-protocol program

### 1.1 Identity across incompatible chip families

`md5` of `analysis/fpga/rom_*_decoded.txt`:

| image | family | md5 |
|---|---|---|
| 6.69 LS0allDA | LS | `e4720fe550815b35836dcbcdb905d4ee` |
| 9.53 PWM | PWM | `e4720fe550815b35836dcbcdb905d4ee` |
| 13.39 Normal | Normal | `e4720fe550815b35836dcbcdb905d4ee` |
| 16.53 PWM `SM16386S_SM16269SH` | PWM | `e4720fe550815b35836dcbcdb905d4ee` |
| 10.81 PWM | PWM | `f876517ca06de39d1d942dbafe5fcbac` |

Four of five are bit-identical, spanning Normal, LS and PWM. A Normal build
emits a plain shift-register waveform and an S-PWM build emits a
command-and-register protocol; a single unchanged 351-entry block cannot be
both. 16.53, the build whose filename announces SM16269SH support, has the
same ROM to the bit.

10.81's only difference is a five-entry-longer prologue
(`1f 0a44 / 11 0430 / … / 00 0140 / 1f 0a43 / 1b 8011 / 1c 573f` where 16.53
has `00 8000`), and it writes the same 55 addresses. That is a different
initial value for one register, not a different protocol.

### 1.2 Protocol constants absent from the ROM

Every 16-bit immediate and every full 21-bit word of all five ROMs
(`analysis/fpga/bramdump_*.txt` decoded 4x9-bit -> 21-bit) was searched for:

| searched | result |
|---|---|
| SH guard/unlock words `0x00AA`, `0x01AA`, `0xF003`, `0x0055`, `0x0155` | 0 hits in all five images |
| SM16269S candidate config words `0x2408`, `0x3CE0`, `0x003F` | 0 hits |
| SH register stream `0x021F`, `0x0750`, `0x1630`, `0x1F0C`, `0x2200` | 0 hits |
| chip ids `0x014C 0x0187 0x0214 0x0215 0x00DE 0x00FD 0x013C 0x00FE` | 0 hits |

The tail lengths themselves (3, 5, 7, 11, 14, 16 …) are not evidence either
way: small integers are statistically meaningless in a 351-word block. The
guard words are the discriminating constants, and they are absent.

### 1.3 What the ROM is

Inferred, and not a lead for the protocol question. The 21 bits split as
5-bit opcode + 16-bit immediate, `0x1B`/`0x1F` carrying addresses and `0x1C`
data ([block-ram.md §3](block-ram.md#3-contents)). Entries 95-242 are a
pure `0x1C` run whose 296 concatenated bytes contain repeating 3-byte groups
with sequential 16-bit operands (`AF 847D / AF 84A6 / AF 84BB …`,
`EE 87F7 F5 / EE 87F8 1E / EE 87F9 06 …`) and ten occurrences of `02 51 BC`,
which reads as 8051 `LJMP 0x51BC`, plus `BF <k> <rel>` / `02 <addr16>`
pairs that read as `CJNE R7,#k,rel; LJMP`. That is the shape of an 8-bit
control-plane program with a byte-oriented dispatch on one register.

§1.1 settles that whatever it is, it is common to Normal, LS and PWM builds,
so it belongs to the card's control plane (flash/config/Ethernet
housekeeping), not to the LED serial protocol. The "8051 ruled out because
there are no ASCII strings" note in
[block-ram.md](block-ram.md#ruled-out-readings) is weak evidence; the 8051
reading is unsettled, not refuted.

### 1.4 The output stage did not change in 16.53

Pad-driver decode on 9.53, 10.81, 13.39 and 16.53
(`analysis/fpga/scripts/netlist/padlogic.py` extended to tag CCU2 chain
bits; the extension is described in §7):

* 9.53, 10.81 and 16.53 share one architecture: every classifiable top-edge
  control pad is `pad = G1 ? ¬(G2 ? legA : legB) : 0`, built from the same
  small set of LUT INITs (`0011000000100010`, `0010001000110000`,
  `0000110100001000`, `0000110000001010`, `0000101000001100`,
  `0001000001010101`, `0100010101000000`, `0000000011100100` …), with the two
  master bits in one slice. Only placement differs (16.53 `Q4/Q5@23,18`;
  10.81 `Q6/Q7@39,36`; 9.53 `Q6/Q7@17,6`).
* 13.39 (Normal) does not: its pad drivers are one-hot-ish INITs
  (`0000000000000001`, `0000000000000010`, `0000000000000100`,
  `0001000100000001` …) fed from combinational `F#` cells, with no
  blank/select master pair.

The PWM/Normal split is real and visible; 16.53 adds no protocol structure
over 9.53 (2022-10) or 10.81 (2023-09). A second chip protocol in 16.53 is
not visible as new output-stage logic, consistent with §2: the protocol is
data.

---

## 2. The protocol selector: `SChipControl`

`SChipControl` is record 0x01 `+0x0C4..0x0D7` (basic-pack body `+0x91`), 20
bytes, emitted by the vendor's `ResetChipControl` +
`SetGclkNumsOfChipControlByChipCustom`. The generator writes it verbatim
from `config/chips/*.toml` (`crates/rcvbp/src/spec/record01.rs:139`).

Survey across the 29 corpus files that carry a record 0x01
(`vendor/led-config-files/**`, `third-party/configs/`):

| chip id | file evidence | `[2] [3] [4]` | GCLK `[10:11]/[12:13]` | record 0x84 |
|---|---|---|---|---|
| `0x0098` (9930) | 4 files | `0 0 0` | 0/0 | none |
| `0x009E` (9935) | 3 files | `0 0 0` | 0/0 | none |
| `0x00A2` (2038) | 4 files | `0 0 0` | 0/0 | none |
| `0x00B8` (6047) | 1 file | `0 0 0` | 0/0 | none |
| `0x0085` (2153) | 2 files | `5 4 6` | 138/138 | empty |
| `0x00CF` (2163 / 6363) | 3 files | `5 4 6` | 74/74 | none |
| `0x00FD` (16380) | 2 files | `7 4 8` | 67/70 | none |
| `0x00E5` (3265 / 3264) | 4 files | `1 5 5` | 47 / 89 / 91 | 13 regs, `0x02..0x11` |
| `0x00BB` (16389) | 3 files | `1 5 6` | 138 / 33 | 32-33 regs, `0x02..0x22` |
| `0x00C2` (2065) | 2 files | `1 5 6` | 138/138 | 45-46 regs, `0x02..0xF5` |
| `0x014C` (bench panel) | reference file | `1 5 6` | 151/151 | 33 regs, `0x02..0x22,0xF0` |
| `0x002F` (MBI5153, sub `0x008A` SM16159) | 5 files, no rec 0x01 | `3 4 8` | 129 / 257 / 513 | none |

Byte 0 is always 0; byte 1 is `0x0E` = 14 in every non-zero block; byte 5 is
`0x01` and byte 6 is `0x03` in every non-zero block.

The `0x002F` files are the corpus entries whose names carry "16169" (P2.5
16S, two P3.91 16S including the E80 build, P6.67 6S, P8). The id is MBI5153
with sub-id SM16159 in the vendor name tables
([../chip-control-block.md §7](../chip-control-block.md#7-chip-names)); the
block comes from the `P3.91-64x64-16S-16169+2012-HL4.0-8.63-E80` file's
identity. `0x00DE` (SM16169S) does not appear with a record 0x01 in the
corpus.

### 2.1 Per-byte decode

Bytes 0-9 and 14-19 are per-chip-id literals written by
`CHWParamRcvGeneral::ResetChipControl()` from a jump table indexed by
`chipType - 0x10`; ids absent from the table get all twenty bytes zeroed.
Bytes 10-13 are recomputed at pack-build time by
`SetGclkNumsOfChipControlByChipCustom`
([../chip-control-block.md §1-2](../chip-control-block.md)). The meanings
below are what the literals encode, read from corpus patterns and the
open-source driver profiles in §2.2; the library does not name them.

```
[0]      0
[1]      0x0E = 14   pre-activation LE tail        (universal)
[2]      protocol variant / command-set selector: 7 / 5 / 3 / 1        inferred
[3]      register / CFG1 LE tail
[4]      second command LE tail (CFG2, or the addressed-write partner)
[5]      0x01 = 1    data-latch LE tail            (universal)
[6]      0x03 = 3    VSYNC LE tail                 (universal)
[7..9]   per-colour byte triple (R,G,B): 7F 7F 7F for 3265; 02 00 00
         for 16380; zero otherwise                                     inferred
[10..11] "scan cycle level", big-endian, then repeated at [12..13]
[12..13] (same value; only SM16380 0x00FD has the two differ)
[14..15] a further count: 8 / 16 / 5                                   not resolved
[16]     0 / 1 / 2                                                     not resolved
[17]     0
[18..19] (10,2) / (5,5) / (12,6) / (0,0)                               not resolved
```

### 2.2 Cross-checks

1. `0x00FD` = SM16380, `[1][3][4] = 14, 4, 8`. The open-source SM16380SC
   driver's command enum is `VSYNC=3, CFG1=4, CFG5=6, CFG2=8, CFG7=11,
   CFG4=12, PREACTIVE=14, CFG6=15, CFG3=16`. Bytes 1/3/4 are `PREACTIVE,
   CFG1, CFG2`; byte 6 is `VSYNC`. `0x00FD` has no record 0x84 at all,
   which is what the non-SH protocol needs: there the register is chosen by
   the tail length, not by an address byte, so there is no address/value
   table to send.
2. `0x00E5` = DP3265S, `[3] = [4] = 5`, and its record 0x84 carries 13
   registers at addresses `0x02..0x11`. The open-source DP3265S profile is
   13 addressed registers `0x01..0x0D` with tail `{5}` for every one. Both
   the count and the uniform tail match.
3. The block is all-zero for exactly the non-S-PWM chips (9930, 9935, 2038,
   6047), plain shift registers with no command protocol, and non-zero for
   every S-PWM chip.
4. GCLK ladder. Corpus values 33, 67, 129, 257, 513 are `(1024 >> n) + 1`
   or `+ 3`, the SM16380SC reference's `GCLK_PER_ROW = (1024 >> FMPWM) + 3`
   formula (FMPWM 3 -> 131, 2 -> 259, 1 -> 515, and 64 + 3 = 67,
   32 + 1 = 33), under a field the vendor names `SetGclkNums…`.

### 2.3 The chip id and the descriptor

The id selects a table in the host tool; what reaches the card is
`SChipControl` + the record 0x84 register stream. That is a mechanism-level
explanation for the negative in [chip-id.md](chip-id.md): the exhaustive
search found no id comparator because the id-dependent behaviour can be
carried in as data.

Measured ([chip-id.md §3](chip-id.md#3-measured-behaviour-by-id)):
`0x014C` renders and `0x0214` is dark. The `0x0214` pack differs from the
`0x014C` pack in the id and in the descriptor (all-zero `SChipControl`, no
register table), so that measurement does not separate "the gateware
branches on the id" from "the gateware acts on the descriptor". Which
mechanism produces the difference is not resolved; a `0x0214` pack that
differs from the `0x014C` pack only at the id bytes, diffed byte by byte
before sending, would settle it.

---

## 3. The protocol the bench card is told to speak

The bench config (`config/chips/sm16269s-factory.toml`; also `sm16269.toml`
and `sm16169sh.toml`, all `family_id = 0x014C`) sends:

```
00 0e 01 05 06 01 03 00 00 00 00 97 00 97 00 08 02 00 0a 02
   ^^ 14 pre-activation
      ^^ variant 1
         ^^ 5 register-write tail
            ^^ 6 second-command tail
               ^^ 1 data latch
                  ^^ 3 VSYNC
                              ^^^^^ ^^^^^ GCLK/RCLK per row = 151, 151
```

together with a record 0x84 of 33 addressed registers `0x02..0x22, 0xF0`.

The card emits the addressed-register ("SH") encoding: a 14-clock
pre-activation, then 16-bit words of the form `(addr << 8) | value` latched
with a 5-clock LE tail, plus a 3-clock VSYNC and a 1-clock data latch. This
is the shape the open-source SM16380SH / DP3265S / FM6373 / ICND1065L
profiles use. It is not the unaddressed SM16269S profile (tails 3 / 5 / 7,
no address byte, no pre-activation).

A non-SH profile is expressible in the parameter pack: `0x00FD` (SM16380)
and `0x002F` (MBI5153) both carry `[3][4] = 4, 8`, the non-SH CFG1/CFG2
tails, and both ship without a register record. Measured: under `0x014C`,
chip-control tails `2/4/8` and `3/5/7` never arm the SM16269S outputs; only
`1, 5, 6` renders ([../rendering.md](../rendering.md)).

Register writes land through the SH encoding: the gain register moves supply
current on this bench, and the panel renders. The part marked SM16269S
behaves as an SH part under this encoding, consistent with the firmware
being filed `SM16386S_SM16269SH`.

---

## 4. GCLK / RCLK

### 4.1 The chip has neither GCLK nor OE

From `third-party/datasheets/SM16269_ZIGZZZAV10_datasheet_2025-08.pdf`
(QSOP24 / QFN24, pp. 1-2):

* Pins are `GND, SDI, DCLK, LE, OUT0..OUT15, RCLK, SDO, REXT, VDD`. There
  is no OE pin and no GCLK pin.
* `RCLK` = 行扫时钟信号, "row-scan clock signal" (pin 21).
* The internal block diagram wires `RCLK` -> 16-bit counter -> PWM
  controller -> the sixteen SM-PWM processors. RCLK is both the grey clock
  and the row advance: the 16-bit counter is the grey counter and its
  rollover moves the row.
* `LE` = 数据锁存控制端；配合 DCLK 下达控制指令, "data-latch control; issues
  control instructions together with DCLK". That is the LE-tail command
  encoding, stated by the vendor.
* `SDI` -> 16-bit shift register -> SRAM (8 K) -> the PWM processors, 1-16
  scan, 16-bit grey, `f_DCLK` max 30 MHz (25 MHz at 3.3 V).
* The grey-output timing figure carries the note 附：GCLK 为 1 倍频, "GCLK is
  1x the frequency".

Consequence for HUB75: of the connector's `CLK / LAT / OE`, the chip
consumes `CLK -> DCLK` and `LAT -> LE`; `OE` is the only wire left for
`RCLK`. On this panel the HUB75 OE line carries a pulse train, not a
blanking level. The open-source `angyalr` driver does this: for the
`sm16269s` profile it sets `rclk_on_oe_pin` and forces the generic OE gating
off (`spwm_oe_style_is_dat_lat_only()`), driving OE from an independent
thread so it free-runs. The OE = RCLK wiring is inferred from the pin set;
the datasheet facts are read directly.

The sibling SM16169 has GCLK on the same pin 21. The card is told chip
`0x14C` = the vendor's "SM16169SH". Whether the gateware treats pin 21 as a
continuous grey clock (GCLK semantics) or a per-row strobe (RCLK semantics)
under that id is not resolved; for the SM16269 the two are the same signal
at different rates, so a rate error, not a presence error, is the failure
mode to look for.

<a id="42-the-count-is-a-pack-field-and-ours-is-already-correct"></a>
### 4.2 The count is a pack field

`SChipControl[10..13]` is the "scan cycle level", stored big-endian and
repeated. It is not a per-chip constant: chip `0x00BB` takes 138 with a
`2018` row driver and 33 with a `5958` row driver, and chip `0x00E5` takes
47 / 89 / 91 across three panels. SM16380 (`0x00FD`) is the only corpus
entry where the two halves differ (67 and 70).

It is computed. [../chip-control-block.md §2](../chip-control-block.md)
recovers the formula from `SSM16169SHChipCustomPlus::GetScanCycleLevel` and
verifies it: with `b` = register `0x07` red value and sub-id != `0x14D`,

```
A = (b & 0xC0) ? 2 : (b >> 5) + 1 ;  u = (b>>2)&1 ; v = b&3 ; n = (b>>3)&3
level = ceil( trunc(128 · 2^n) / A + (v + 10u + 12) + 1 )
```

For the reference file (`reg07 = 0x04`, sub-id `0x0000`) that is
151 = 0x97. The generated config has the same `reg07` and the same sub-id,
so `0x0097` is correct and self-consistent for what is sent. This field is
not a fault.

Recompute trap: `crates/panelspec/src/chips.rs` stores `chip_control` as a
literal from the TOML. Changing `reg 0x07` or the sub-id without
recomputing bytes 10-13 desynchronises the card's scan-cycle count from the
chip's own frequency-division setting: `reg07 = 0x44` with sub `0x14D` gives
`0x30`, not `0x97`; `sub_id = 0x14D` with `reg07 = 0x04` gives `0x5E`.
`config/chips/sm16269.toml` ships that combination (`sub_id = 0x14D` +
`reg07 = 0x44` + `0x97`), which the vendor tool never emits;
`sm16269s-factory.toml`, which the panel config uses, is consistent. A
register sweep that touches `0x07` or the sub-id needs `chips.rs` to compute
bytes 10-13, or at minimum to assert consistency.

Record 0x01 `+0x031` (`SetGClock`, default `0x14` = 20) is not a GCLK
divider: an exhaustive reference scan of the vendor library
([../chip-control-block.md §3](../chip-control-block.md)) shows it is read
only by a host-side grey-value display routine and never reaches the basic
pack. It cannot affect what the card emits.

### 4.3 What stops the clock

* An all-zero `SChipControl`, which is what chip ids with no vendor table
  produce. This is the most economical explanation of "`0x0214` -> panel
  dark at 0.5 A" (inferred).
* The control-group source block RAM `MIB_R25C4/C5 EBR0` starting empty
  (`WID = 1`, not initialised at configuration time;
  [output-stage.md §7.4](output-stage.md#74-the-control-group-source-ram-starts-empty-high)).
  Whatever the top-edge control pads emit in the "BRAM" phase of the 2:1 mux
  comes from that RAM.
* Nothing else. No pad in any of the five builds is driven from a global
  clock net, so every HUB75 clock-like output is fabric data and can be
  stopped by ordinary logic.

<a id="44-new-netlist-detail-on-the-control-pads-medium"></a>
### 4.4 The control pads in the netlist

Extending the pad-driver decode (§1.4) shows the top-edge group's two mux
legs are two distinct, separately clock-enabled register banks:

* the "BRAM" leg: `Q4@21,7`, `Q0@21,10`, `Q1@21,10`, `Q7@23,7`, `Q6@23,10`,
  `Q4@19,16`; all share `.CE = F0@7,4` and all take one input directly from
  the EBR at `(4,25)` (`JF0/JF4/JF5/JF7@4,25`, `JQ2@4,25`, `H06E0103@0,25`)
  plus a common companion `Q2@10,5`.
* the "counter" leg: `Q0@25,11`, `Q0@25,8`, `Q2/Q4/Q5@24,7`, `Q0/Q1@25,7`,
  `Q2/Q6/Q7@26,8`, `Q7@25,9`, `Q1@24,10`, `Q6@27,7`; all share
  `.CE = F3@26,8`; five of them are `MODE = CCU2` chain bits.

Five pads (`A3`, `B4`, `B11`, `E5`, `E10`) have the identical driver LUT
(`INIT 0011000000100010`, `pad = G2 ? ¬legBRAM : ¬legCTR`). Five identical
pads muxing a counter against a table is the signature of the HUB75 A-E
scan address lines, with the table leg supplying the scan table's line order
(`FieldTableToScanTable` writes an identity line order at `+0x3A0`,
[output-stage.md §3](output-stage.md#3-the-scan-table-high-and-a-genuinely-new-result)).
This is a finer reading of the 2:1 mux than the "test pattern vs live data"
option left open in
[output-stage.md §7.3](output-stage.md#73-what-the-21-mux-selects-between-high-that-it-is-counter-vs-bram).
Inferred: the group is real and the shape is right; the pin identities are
not proven and no waveform has been measured.

---

## 5. The data-upload path

From the datasheet plus two open-source reference implementations, and
consistent with `SChipControl[5] = 1` in every S-PWM corpus entry. Inferred.

* 16 bits per output channel, MSB first. The chip's input stage is a 16-bit
  shift register (datasheet block diagram); both reference drivers shift
  `for (bit = 15; bit >= 0; --bit)`.
* R, G, B are not serialised: `R1 G1 B1` (upper half) and `R2 G2 B2` (lower
  half) are six parallel lanes driven on every DCLK. The clock count is not
  multiplied by three.
* Nesting is output-major, chip-minor: for each scan row -> for each chip
  output 0..15 -> for each chip along the lane -> 16 bits. Reversing the
  chip order is documented in the reference bring-up notes to produce
  "scrambled 16-pixel rectangles".
* The data latch is 1 DCLK of LE, asserted on the final chip of the chain
  only, once per output-index group. `SChipControl[5] = 0x01` is that
  number, and it is `1` for all eight S-PWM chip families in the corpus.
* VSYNC is a 3-DCLK LE burst with the RGB lanes low, issued after the frame
  as additional clocks, not an overlay on the last three data clocks.
  `SChipControl[6] = 0x03` is that number, universally.
* The row address is advanced entirely outside the data path: A-E are
  binary, `A = bit0 … E = bit4`, and for an RCLK-style part the row is owned
  by the scan engine, not by the upload.

Against the card's own tables: record 0x03 maps every pixel to
`(line 0..15, slot 0..255)` and `OneScanLen = CardScanLen = 256`
([output-stage.md §2, §4](output-stage.md#2-scan-handling-high)). 256 slots
= 16 chips x 16 outputs on a 128-wide half at 1/16 scan, so the card's slot
index is the chip-output index, and one 256-slot pass is one full
16-bit-word sweep of the chain.

What starts an upload is not resolved. The `0x0107` latch frame from
Ethernet marks end-of-frame on the host side; nothing in the bitstream ties
it to the LE/VSYNC emission, and no waveform has been captured. Measured:
three `0x0107` frames after a 500 µs gap hold the display; one never starts
it; two render and decay ([../rendering.md](../rendering.md)).

---

## 6. Fault candidates for the data path and their status

| candidate | facts | status |
|---|---|---|
| `0x014C` is the wrong family entry and `0x002F` (+ sub `0x008A`) is the id for this silicon | every corpus `.rcvbp` whose name carries "16169" uses `0x002F`; none uses `0x14C`. The `0x14C` in the reference file came from a 256x384 wall config. The two ids carry different descriptors (`3, 4, 8` vs `1, 5, 6`). `IsHasGCLKRatioSetting()` is false for `0x14C` and true for `0x2F`; `GetGclkCount()` returns 0 for `0x14C` (the id falls past the table bound) and computes a value for `0x2F` | disproved. `0x002F` is MBI5153 with sub-id SM16159, not an SM16169. Measured: with `chip_control = 00 0e 03 04 08 01 03 00 00 00 00 81 00 81 00 10 00 00 00 00`, serial clock 10, it never arms the SM16269S. The chip library written for it (`sm16169-corpus.toml`) is removed |
| The SH (addressed-register) encoding is sent to a part that wants the unaddressed one | the SM16269 datasheet publishes one configurable word, the 6-bit current gain (`G5..G0` in bits 5:0), no register map and no address field; the card sends 33 addressed registers | disproved for this silicon. Measured: tails `2/4/8` and `3/5/7` never arm; `1, 5, 6` renders; the gain register moves supply current |
| Wrong RCLK regime: the chip holds a frame in its 8 K SRAM and self-scans it from RCLK while the card's A-E drive the row select; a rate mismatch shows the same SRAM row on every physical row | consistent with "gain writes work" and "brightness scales current", which never touch the row pointer | not the fault in the rendering configuration: `0x0097` is the vendor's own value for `reg07 = 0x04`, sub-id `0`, and the panel renders with it. Do not sweep bytes 10-13 in isolation |
| The recompute trap (§4.2) | a register sweep touching `0x07` or the sub-id without recomputing bytes 10-13 manufactures a fault | open as a tooling hazard; `sm16269.toml` ships the inconsistent combination |
| A second unresolved `SChipControl` field: `[14..15]`, `[16]`, `[18..19]` (bench `00 08 / 02 / 0a 02`) | byte 16 is not written by `ResetChipControl` for chip `0x14C`; the `02` in the reference file comes from a LEDVISION save path outside the reset path | not swept; not required for rendering |
| Un-flashed corrected config, `CardScanLen`, serial clock, register table, lane map | [output-stage.md §6](output-stage.md#6-reconciling-the-bench-facts) | see that table |

On a self-scanning S-PWM part two alignments have to hold: the bytes that
reach the scan buffer, and the chip's own row pointer against the card's row
select. Framebuffer correctness cannot fix the second. Both hold in the
rendering configuration.

---

## 7. Reproduction

```sh
sh analysis/fpga/scripts/repro.sh /tmp/rxp-trellis       # bitstreams -> .config
md5 analysis/fpga/rom_*_decoded.txt                       # §1.1
python3 analysis/fpga/chip_control_survey.py              # §2 corpus table
```

The pad-driver / CCU2-chain extension used in §1.4 and §4.4 is a small
addition to `analysis/fpga/scripts/netlist/padlogic.py`: build
`bit_of[(x,y,'Q#')] -> (chain, bit)` from maximal runs of adjacent
`SLICE?.MODE = CCU2` tiles along a row (carry runs horizontally,
`FCO -> HFIE0000@(x+1,y)`), then tag each pad-driver LUT input with it. Node
names contain commas, so a flop list is split on `;`, not `,`. The
extension is not checked in.

## Unresolved

* Which mechanism produces the `0x014C` vs `0x0214` difference: an id branch
  in the gateware, or the descriptor bytes (§2.3).
* `SChipControl[14..15]`, `[16]`, `[18..19]` (§2.1, §6).
* Whether the gateware treats pin 21 as GCLK or RCLK semantics under
  `0x14C` (§4.1).
* The identities of the top-edge control pads (§4.4); no waveform measured.
* What starts an upload, and how the `0x0107` frame maps to LE/VSYNC (§5).
* What the microcode ROM configures (§1.3; [open-questions.md](open-questions.md)).
