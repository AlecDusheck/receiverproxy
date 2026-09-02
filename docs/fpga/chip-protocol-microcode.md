# The driver-chip serial protocol: where it lives, and what selects it

Task: decode the microcode ROM as a chip-protocol program, find the runtime
protocol selector, settle GCLK, and describe the data-upload path.

**Headline result: the ROM is not the protocol, and the protocol is not
hard-wired per chip either. The LE/LAT command encoding is *parameter data*:
it is carried in the 20-byte `SChipControl` block of record 0x01 (`+0x0C4`),
which the host sends on every `send-params`.** That block decodes cleanly
against three independent open-source driver profiles and against the vendor's
own 29-file corpus.

Everything below is tagged **HIGH** (read from the bytes and cross-checked),
**MEDIUM** (a strong pattern on one stated assumption) or **NOT RESOLVED**.

Artefacts: `analysis/fpga/rom_*_decoded.txt`, `analysis/fpga/chip-control-corpus.tsv`,
`analysis/fpga/negative_results_and_method.txt` (§N10–N12),
`third-party/datasheets/SM16269_ZIGZZZAV10_datasheet_2025-08.pdf`.

---

## 0. Summary answers to the four questions

| # | question | answer |
|---|---|---|
| 1 | Which protocol does 16.53's ROM implement? | **None.** The ROM is byte-identical in 16.53, 13.39 (Normal), 9.53 (PWM) and 6.69 (LS0allDA). A block that does not change across the Normal/PWM/LS split cannot encode a chip-specific serial protocol. No LE-tail table, no guard word, no addressed-register structure is present in any build. **HIGH** |
| 2 | How is the protocol selected at runtime? | **By the chip id, but as *data*, not as a ROM jump.** The host tool's `ResetChipControl` jump table (indexed by `chipType − 0x10`) emits a 20-byte `SChipControl` descriptor per id into record 0x01 `+0x0C4`. It is **all-zero for exactly the non-S-PWM chips** and non-zero for every S-PWM chip, and it is the only chip-protocol-shaped payload in the pack. **HIGH that the id selects it and that the block is what varies; MEDIUM on the per-byte semantics in §2.1** |
| 3 | GCLK | The SM16269 has **no GCLK and no OE pin.** Pin 21 is **RCLK**, and the datasheet block diagram wires RCLK → 16-bit counter → PWM controller, so RCLK *is* the grey clock and also advances the row. `SChipControl[10..13]` is the card-side "scan cycle level"; ours is `0x0097` = 151, and it is **verified correct** for what we send (§4.2). Record `+0x031` `SetGClock` never reaches the pack at all. **HIGH** |
| 4 | Data upload | 16-bit words per output channel, **MSB first**, R/G/B as six parallel lanes, chip-minor / output-major nesting, with a **1-DCLK LE data-latch tail** (`SChipControl[5] = 0x01` in every S-PWM profile in the corpus). **MEDIUM-HIGH** |

**Single most likely reason data never reaches the SM16269S SRAM:** see §6.
Short version: **chip id `0x014C` is probably the wrong family entry for this
silicon, and `0x002F` is the one to try.** Every vendor `.rcvbp` in the corpus
that drives a 16169-family module uses `0x002F` + sub `0x008A`, and the two ids
carry *different* protocol descriptors (`[2][3][4]` = `1,5,6` vs `3,4,8`).

> **Read this file together with
> [`docs/chip-control-block.md`](../chip-control-block.md)**, which decodes the
> same 20 bytes from the vendor library's disassembly: jump table, per-byte
> write site, and the exact `GetScanCycleLevel` formula behind bytes 10–13.
> The two agree on structure and on bytes 10–13. They differ on bytes 1–6:
> that file marks them NOT RESOLVED (they are per-chip-id literals from
> `ResetChipControl`, and nothing in the library names them), while §2 here
> reads them as LE/LAT tail lengths on external evidence. **Where they
> conflict, `chip-control-block.md` is the harder source**: it is reading the
> code that writes the bytes. §2 here is the softer, cross-referenced reading
> of what those literals *mean*, and it is tagged MEDIUM for that reason.

---

## 1. The ROM is not a chip-protocol program (HIGH)

### 1.1 It is identical across incompatible chip families

`md5` of `analysis/fpga/rom_*_decoded.txt`:

| image | family | md5 |
|---|---|---|
| 6.69 LS0allDA | LS | `e4720fe550815b35836dcbcdb905d4ee` |
| 9.53 PWM | PWM | `e4720fe550815b35836dcbcdb905d4ee` |
| 13.39 Normal | Normal | `e4720fe550815b35836dcbcdb905d4ee` |
| **16.53 PWM `SM16386S_SM16269SH`** | PWM | `e4720fe550815b35836dcbcdb905d4ee` |
| 10.81 PWM | PWM | `f876517ca06de39d1d942dbafe5fcbac` |

Four of five bit-identical, spanning **Normal, LS and PWM**. A Normal build
emits a plain shift-register waveform and an S-PWM build emits a
command-and-register protocol; a single unchanged 351-entry block cannot be
both. And 16.53, the build whose *filename* announces new SM16269SH support,
did not change the ROM by one bit.

10.81's only difference is a five-entry-longer prologue
(`1f 0a44 / 11 0430 / … / 00 0140 / 1f 0a43 / 1b 8011 / 1c 573f` where 16.53
has `00 8000`), and it writes exactly the same 55 addresses. That is a
different initial value for one register, not a different protocol.

### 1.2 None of the protocol constants appear in it

Searched every 16-bit immediate and every full 21-bit word of all five ROMs
(`analysis/fpga/bramdump_*.txt` decoded 4×9-bit → 21-bit) for:

| searched | result |
|---|---|
| SH guard/unlock words `0x00AA`, `0x01AA`, `0xF003`, `0x0055`, `0x0155` | **0 hits in all five images** |
| SM16269S candidate config words `0x2408`, `0x3CE0`, `0x003F` | 0 hits |
| SH register stream `0x021F`, `0x0750`, `0x1630`, `0x1F0C`, `0x2200` | 0 hits |
| chip ids `0x014C 0x0187 0x0214 0x0215 0x00DE 0x00FD 0x013C 0x00FE` | 0 hits (prior work, re-confirmed) |

Searching for the *tail lengths* themselves (3, 5, 7, 11, 14, 16 …) is
statistically meaningless in a 351-word block and was **not** treated as
evidence either way. The guard words are the discriminating constants, and they
are absent.

### 1.3 What the ROM probably is: MEDIUM, and a dead end for this question

The 21 bits split as 5-bit opcode + 16-bit immediate, `0x1B`/`0x1F` carrying
addresses and `0x1C` data (see [block-ram.md §3](block-ram.md#3-contents-medium)).
Entries 95–242 are a pure `0x1C` run whose 296 concatenated bytes contain
repeating 3-byte groups with sequential 16-bit operands (`AF 847D / AF 84A6 /
AF 84BB …`, `EE 87F7 F5 / EE 87F8 1E / EE 87F9 06 …`) and ten occurrences of
`02 51 BC`, which reads as 8051 `LJMP 0x51BC`, plus `BF <k> <rel>` /
`02 <addr16>` pairs that read as `CJNE R7,#k,rel; LJMP`. That is suggestive of
an 8-bit control-plane program with a byte-oriented dispatch on one register.

**This was not pursued and should not be, for this question.** Whatever it is,
§1.1 settles that it is common to Normal, LS and PWM builds, so it belongs to
the card's control plane (flash/config/Ethernet housekeeping), not to the LED
serial protocol. Recorded so nobody re-derives it. The earlier "8051 ruled out
because there are no ASCII strings" note in
[block-ram.md](block-ram.md#it-reads-as-a-register-write-script-medium) is
**weak evidence and should be read as unsettled**, not as a refutation.

### 1.4 The output stage did not change in 16.53 either (HIGH)

Re-ran the pad-driver decode on 9.53, 10.81, 13.39 and 16.53
(`analysis/fpga/scripts/netlist/padlogic.py` extended to tag CCU2 chain bits;
the extension is not checked in, results below).

* **9.53, 10.81 and 16.53 share one architecture**: every classifiable
  top-edge control pad is `pad = G1 ? ¬(G2 ? legA : legB) : 0`, built from the
  same small set of LUT INITs (`0011000000100010`, `0010001000110000`,
  `0000110100001000`, `0000110000001010`, `0000101000001100`,
  `0001000001010101`, `0100010101000000`, `0000000011100100` …), with the two
  master bits in one slice. Only placement moved (16.53 `Q4/Q5@23,18`;
  10.81 `Q6/Q7@39,36`; 9.53 `Q6/Q7@17,6`).
* **13.39 (Normal) does not**: its pad drivers are one-hot-ish INITs
  (`0000000000000001`, `0000000000000010`, `0000000000000100`,
  `0001000100000001` …) fed from combinational `F#` cells, with no
  blank/select master pair.

**So the PWM/Normal split is real and visible, but 16.53 added no new
protocol structure over 9.53 (2022-10) or 10.81 (2023-09).** If 16.53 speaks a
second chip protocol, it is not visible as new output-stage logic, which is
consistent with §2: the protocol is *data*.

---

## 2. The protocol selector: `SChipControl` (HIGH)

`SChipControl` is record 0x01 `+0x0C4..0x0D7` (basic-pack body `+0x91`), 20
bytes, emitted by the vendor's `ResetChipControl` +
`SetGclkNumsOfChipControlByChipCustom`. Our generator writes it verbatim from
`config/chips/*.toml` (`crates/e120-rcvbp/src/spec/record01.rs:139`).

Surveyed across the 29 corpus files that carry a record 0x01
(`vendor/led-config-files/**`, `third-party/configs/`):

| chip id | file evidence | `[2] [3] [4]` | GCLK `[10:11]/[12:13]` | record 0x84 |
|---|---|---|---|---|
| `0x0098` (9930) | 4 files | `0 0 0` | 0/0 | none |
| `0x009E` (9935) | 3 files | `0 0 0` | 0/0 | none |
| `0x00A2` (2038) | 4 files | `0 0 0` | 0/0 | none |
| `0x00B8` (6047) | 1 file | `0 0 0` | 0/0 | none |
| `0x0085` (2153) | 2 files | `5 4 6` | 138/138 | empty |
| `0x00CF` (2163 / **6363**) | 3 files | `5 4 6` | 74/74 | none |
| `0x00FD` (**16380**) | 2 files | `7 4 8` | **67/70** | **none** |
| `0x00E5` (**3265 / 3264**) | 4 files | `1 5 5` | 47 / 89 / 91 | **13 regs, `0x02..0x11`** |
| `0x00BB` (16389) | 3 files | `1 5 6` | 138 / 33 | 32–33 regs, `0x02..0x22` |
| `0x00C2` (2065) | 2 files | `1 5 6` | 138/138 | 45–46 regs, `0x02..0xF5` |
| **`0x014C`** (our panel) | factory file | **`1 5 6`** | **151/151** | **33 regs, `0x02..0x22,0xF0`** |
| `0x002F` (16169 corpus) | 5 files, no rec 0x01 | `3 4 8` | 129 / 257 / 513 | none |

Byte 0 is always 0; **byte 1 is `0x0E` = 14 in every non-zero block**; **byte 5
is `0x01` and byte 6 is `0x03` in every non-zero block**.

### 2.1 The decode

> Bytes 0–9 and 14–19 are per-chip-id **literals** written by
> `CHWParamRcvGeneral::ResetChipControl()` from a jump table indexed by
> `chipType − 0x10`; ids absent from the table get all twenty bytes zeroed.
> Bytes 10–13 are then recomputed at pack-build time by
> `SetGclkNumsOfChipControlByChipCustom`. All of that is established in
> [`docs/chip-control-block.md`](../chip-control-block.md) §1–2 from the
> disassembly, at HIGH confidence, and supersedes anything below that
> disagrees. What follows is the *meaning* of those literals, which the
> library does not name.

```
[0]      0
[1]      0x0E = 14   pre-activation LE tail        (universal)          HIGH
[2]      protocol variant / command-set selector: 7 / 5 / 3 / 1         MEDIUM
[3]      register / CFG1 LE tail                                        HIGH
[4]      second command LE tail (CFG2, or the addressed-write partner)  HIGH
[5]      0x01 = 1    data-latch LE tail            (universal)          HIGH
[6]      0x03 = 3    VSYNC LE tail                 (universal)          HIGH
[7..9]   per-colour byte triple (R,G,B): 7F 7F 7F for 3265; 02 00 00
         for 16380; zero otherwise                                      MEDIUM
[10..11] "scan cycle level", big-endian, then repeated at [12..13]       HIGH
[12..13] (same value; only SM16380 0x00FD has the two differ)           HIGH
[14..15] a further count: 8 / 16 / 5                                    NOT RESOLVED
[16]     0 / 1 / 2                                                      NOT RESOLVED
[17]     0
[18..19] (10,2) / (5,5) / (12,6) / (0,0)                                NOT RESOLVED
```

### 2.2 Why the decode is credible: four independent cross-checks

1. **`0x00FD` = SM16380, `[1][3][4] = 14, 4, 8`.** The open-source SM16380SC
   driver's command enum is literally
   `VSYNC=3, CFG1=4, CFG5=6, CFG2=8, CFG7=11, CFG4=12, PREACTIVE=14, CFG6=15,
   CFG3=16`. Bytes 1/3/4 are exactly `PREACTIVE, CFG1, CFG2`; byte 6 is
   exactly `VSYNC`. And `0x00FD` has **no record 0x84 at all**, precisely what
   the non-SH protocol needs, because there the register is chosen by the tail
   length, not by an address byte, so there is no address/value table to send.
2. **`0x00E5` = DP3265S, `[3] = [4] = 5`, and its record 0x84 carries exactly
   13 registers at addresses `0x02..0x11`.** The open-source DP3265S profile is
   13 addressed registers `0x01..0x0D` with tail `{5}` for every one. Both the
   count and the uniform tail match.
3. **The block is all-zero for exactly the non-S-PWM chips** (9930, 9935, 2038,
   6047), chips that are plain shift registers and have no command protocol at
   all, and non-zero for every S-PWM chip. That is what a "serial-protocol
   descriptor" field would look like and nothing else would.
4. **GCLK ladder.** Corpus values 33, 67, 129, 257, 513 are exactly
   `(1024 >> n) + 1` or `+ 3`, which is the SM16380SC reference's
   `GCLK_PER_ROW = (1024 >> FMPWM) + 3` formula
   (FMPWM 3 → 131, 2 → 259, 1 → 515, and 64 + 3 = 67, 32 + 1 = 33). Getting a
   clean `2ⁿ ± small` ladder out of what the vendor names
   `SetGclkNums…` is not a coincidence.

### 2.3 What this means for the chip id

**The gateware may not need to compare the chip id at all.** The id selects a
table *in the host tool*; what reaches the card is `SChipControl` + the record
0x84 register stream. That is a complete, mechanism-level explanation for the
long-standing negative in [chip-id.md](chip-id.md): the exhaustive search found
no id comparator because the id-dependent behaviour is carried in as data.

Caveat, and it is a real one: [chip-id.md §8](chip-id.md#8-what-this-means-for-the-bench)
records that on this bench **only the id was changed** between `0x014C` (panel
responds) and `0x0214` (panel dark) and the behaviour changed anyway. If that
is literally true (nothing else in the pack differed, `SChipControl` included)
then the gateware *does* branch on the id as well, and both mechanisms are
live. **Which of the two produced the `0x14C` vs `0x0214` difference is NOT
RESOLVED**, and it is worth re-running the sweep with the pack diffed
byte-by-byte to settle it, because the two explanations lead to completely
different courses of action.

`0x00DE` (SM16169S) and `0x002F` do not appear with a record 0x01 in the
corpus; `0x002F`'s block comes from
the identity the `P3.91-64x64-16S-16169+2012-HL4.0-8.63-E80` corpus file carries (a chip library for it, `sm16169-corpus.toml`, was tried and removed: the id is MBI5153, not SM16169, and it never arms this panel; see docs/chip-control-block.md).

---

## 3. Which protocol our card is being told to speak (HIGH)

Our config (`config/chips/sm16269.toml`, `sm16269s-factory.toml`,
`sm16169sh.toml`, all `family_id = 0x014C`) sends:

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

together with a record 0x84 of **33 addressed registers `0x02..0x22, 0xF0`**.

So the card is emitting the **addressed-register ("SH") encoding**: a
14-clock pre-activation, then 16-bit words of the form `(addr << 8) | value`
latched with a 5-clock LE tail, plus a 3-clock VSYNC and a 1-clock data latch.
This is the same shape the open-source SM16380SH / DP3265S / FM6373 / ICND1065L
profiles use. It is **not** the unaddressed SM16269S profile (tails 3 / 5 / 7,
no address byte, no pre-activation).

**A non-SH profile does not exist in any of the five E120 builds**, because
there is no per-chip protocol logic in the builds at all, but it *is*
expressible in the parameter pack: `0x00FD` (SM16380) and `0x002F`
(SM16169 family) both carry `[3][4] = 4, 8`, the non-SH CFG1/CFG2 tails,
and both ship **without** a register record. To make this card speak non-SH you
would set `chip_control[2..5] = 3, 4, 8` (or `7, 4, 8`) and drop record 0x84.
Whether the gateware honours that without a matching chip id is **NOT
RESOLVED** and is the single cheapest experiment in this document.

The bench says register writes **do** land (the gain register moves supply
current), so the SH encoding is being accepted by this silicon. Read literally,
that means the part marked SM16269S behaves as an SH part, which is consistent
with the firmware being filed `SM16386S_SM16269SH` and inconsistent with
treating "our silicon is non-SH" as established.

---

## 4. GCLK / RCLK (HIGH)

### 4.1 The chip has neither GCLK nor OE

From `third-party/datasheets/SM16269_ZIGZZZAV10_datasheet_2025-08.pdf`
(QSOP24 / QFN24, pp. 1–2):

* Pins are `GND, SDI, DCLK, LE, OUT0..OUT15, RCLK, SDO, REXT, VDD`.
  **There is no OE pin and no GCLK pin.**
* `RCLK` = 行扫时钟信号, "row-scan clock signal" (pin 21).
* The internal block diagram wires **`RCLK` → 16-bit counter → PWM controller
  → the sixteen SM-PWM processors**. So RCLK is *both* the grey clock and the
  row advance: the 16-bit counter is the grey counter and its rollover moves
  the row.
* `LE` = 数据锁存控制端；配合 DCLK 下达控制指令, "data-latch control;
  issues control instructions together with DCLK". That is the LE-tail command
  encoding, stated by the vendor.
* `SDI → 16-bit shift register → SRAM (8 K) → the PWM processors`, 1–16 scan,
  16-bit grey, `f_DCLK` max 30 MHz (25 MHz at 3.3 V).
* The grey-output timing figure carries the note **附：GCLK 为 1 倍频**,
  "GCLK is 1× the frequency".

Consequence for HUB75: of the connector's `CLK / LAT / OE`, the chip consumes
`CLK → DCLK` and `LAT → LE`; **`OE` is the only wire left for `RCLK`**. So on
this panel the HUB75 OE line must carry a *pulse train*, not a blanking level.
The open-source `angyalr` driver does exactly this: for the `sm16269s`
profile it sets `rclk_on_oe_pin` and forces the generic OE gating off
(`spwm_oe_style_is_dat_lat_only()`), driving OE from an independent thread so
it free-runs. **HIGH on the datasheet facts, MEDIUM-HIGH on the OE = RCLK
wiring inference.**

The sibling SM16169 has **GCLK** on the same pin 21. The card is being told
chip `0x14C` = the vendor's "SM16169SH". Whether the gateware treats pin 21 as
a continuous grey clock (GCLK semantics) or a per-row strobe (RCLK semantics)
under that id is **NOT RESOLVED**, but for the SM16269 the two are
the same signal at different rates, so a *rate* error, not a *presence* error,
is the failure to look for.

### 4.2 The count is a pack field, and ours is already correct

`SChipControl[10..13]` is the "scan cycle level", stored big-endian and
repeated. It is **not** a per-chip constant: chip `0x00BB` takes 138 with a
`2018` row driver and 33 with a `5958` row driver, and chip `0x00E5` takes
47 / 89 / 91 across three panels. SM16380 (`0x00FD`) is the only corpus entry
where the two halves differ (67 and 70).

**It is computed, not chosen**, and
[`docs/chip-control-block.md`](../chip-control-block.md) §2 recovers the exact
formula from `SSM16169SHChipCustomPlus::GetScanCycleLevel` and verifies it:
with `b = ` register `0x07` red value and sub-id ≠ `0x14D`,

```
A = (b & 0xC0) ? 2 : (b >> 5) + 1 ;  u = (b>>2)&1 ; v = b&3 ; n = (b>>3)&3
level = ceil( trunc(128 · 2ⁿ) / A + (v + 10u + 12) + 1 )
```

and for the reference file (`reg07 = 0x04`, sub-id `0x0000`) that is exactly
**151 = 0x97**. Our generated config has the same `reg07` and the same sub-id,
so **`0x0097` is correct and self-consistent for what we send. This field is
not the fault.**

> **Footgun, and it bites two of the experiments listed in
> [output-stage.md §6](output-stage.md#6-reconciling-the-bench-facts).**
> `crates/e120-rcvbp/src/chips.rs` stores `chip_control` as a **literal** from
> the TOML. Change `reg 0x07` (hypothesis #5) or the sub-id and the vendor
> would recompute bytes 10–13 (`reg07 = 0x44` with sub `0x14D` gives `0x30`,
> not `0x97`) while we would keep sending `0x97`, desynchronising the card's
> scan-cycle count from the chip's own frequency-division setting.
> `config/chips/sm16269.toml` already ships that impossible combination
> (`sub_id = 0x14D` + `reg07 = 0x44` + `0x97`); `sm16269s-factory.toml`, which
> the panel config actually uses, is consistent.

Record 0x01 `+0x031` (`SetGClock`, default `0x14` = 20) is a red herring: an
exhaustive reference scan of the vendor library
([chip-control-block.md §3](../chip-control-block.md)) shows it is read only by
a host-side grey-value display routine and **never reaches the basic pack at
all**. It is not a GCLK divider and cannot affect what the card emits. (HIGH)

### 4.3 What would stop the clock

* An all-zero `SChipControl`, which is what chip ids with no vendor table
  produce, and is the most economical explanation of "`0x0214` → panel dark at
  0.5 A". **(MEDIUM)**
* The control-group source block RAM `MIB_R25C4/C5 EBR0` starting empty
  (`WID = 1`, not initialised at configuration time; see
  [output-stage.md §7.4](output-stage.md#74-the-control-group-source-ram-starts-empty-high)).
  Whatever the top-edge control pads emit in the "BRAM" phase of the 2:1 mux
  comes from that RAM.
* Nothing else was found. **No pad in any of the five builds is driven from a
  global clock net** (re-confirmed), so every HUB75 clock-like output is fabric
  data and can be stopped by ordinary logic.

### 4.4 New netlist detail on the control pads (MEDIUM)

Extending the pad-driver decode (§1.4) shows the top-edge group's two mux legs
are two distinct, separately clock-enabled register banks:

* **the "BRAM" leg**: `Q4@21,7`, `Q0@21,10`, `Q1@21,10`, `Q7@23,7`,
  `Q6@23,10`, `Q4@19,16`; all share `.CE = F0@7,4` and all take one input
  directly from the EBR at `(4,25)` (`JF0/JF4/JF5/JF7@4,25`, `JQ2@4,25`,
  `H06E0103@0,25`) plus a common companion `Q2@10,5`.
* **the "counter" leg**: `Q0@25,11`, `Q0@25,8`, `Q2/Q4/Q5@24,7`,
  `Q0/Q1@25,7`, `Q2/Q6/Q7@26,8`, `Q7@25,9`, `Q1@24,10`, `Q6@27,7`; all share
  `.CE = F3@26,8`; five of them are genuinely `MODE = CCU2` chain bits.

Five pads (**`A3`, `B4`, `B11`, `E5`, `E10`**) have the *identical* driver
LUT (`INIT 0011000000100010`, `pad = G2 ? ¬legBRAM : ¬legCTR`). Five identical
pads muxing a counter against a table is the signature of the **HUB75 A–E scan
address lines**, with the table leg supplying the scan table's line order
(`FieldTableToScanTable` writes an identity line order at `+0x3A0`, see
[output-stage.md §3](output-stage.md#3-the-scan-table-high-and-a-genuinely-new-result)).
That is a better reading of the 2:1 mux than the "test pattern vs live data"
option left open in [output-stage.md §7.3](output-stage.md#73-what-the-21-mux-selects-between-high-that-it-is-counter-vs-bram).
**MEDIUM: the group is real and the shape is right; the pin identities are
not proven and no waveform has ever been measured.**

---

## 5. The data-upload path: MEDIUM-HIGH

From the datasheet plus the two open-source reference implementations, and
consistent with `SChipControl[5] = 1` in every S-PWM corpus entry:

* **16 bits per output channel, MSB first.** The chip's input stage is a
  16-bit shift register (datasheet block diagram); both reference drivers shift
  `for (bit = 15; bit >= 0; --bit)`.
* **R, G, B are not serialised**: `R1 G1 B1` (upper half) and `R2 G2 B2`
  (lower half) are six parallel lanes driven on every DCLK. The clock count is
  not multiplied by three.
* **Nesting is output-major, chip-minor**: for each scan row → for each chip
  output 0..15 → for each chip along the lane → 16 bits. Reversing the chip
  order is documented in the reference bring-up notes to produce "scrambled
  16-pixel rectangles".
* **The data latch is 1 DCLK of LE**, asserted on the final chip of the chain
  only, once per output-index group. `SChipControl[5] = 0x01` is that number,
  and it is `1` for all eight S-PWM chip families in the corpus.
* **VSYNC is a 3-DCLK LE burst with the RGB lanes low**, issued after the
  frame, additional clocks, not an overlay on the last three data clocks.
  `SChipControl[6] = 0x03` is that number, universally.
* **The row address is advanced entirely outside the data path**: A–E are
  binary, `A = bit0 … E = bit4`, and for an RCLK-style part the row is owned by
  the scan engine, not by the upload.

Against the card's own tables: record 0x03 maps every pixel to
`(line 0..15, slot 0..255)` and `OneScanLen = CardScanLen = 256`
([output-stage.md §2, §4](output-stage.md#2-scan-handling-high)). 256 slots =
16 chips × 16 outputs on a 128-wide half at 1/16 scan, so the card's slot index
is the *chip-output* index, and one 256-slot pass is one full 16-bit-word
sweep of the chain, arithmetically consistent.

**What starts an upload is NOT RESOLVED.** The `0x0107` latch frame from
Ethernet marks end-of-frame on the host side; nothing in the bitstream ties it
to the LE/VSYNC emission, and no waveform has been captured.

---

## 6. The single most likely reason data never reaches the SRAM

Ranked, with the reasoning made explicit so it can be attacked.

**#1: Chip id `0x014C` is the wrong family entry for this silicon; the id the
vendor actually uses for 16169-family modules is `0x002F` (+ sub `0x008A`).**
*For, and it is a stack of independent facts:*

* **The corpus.** Every vendor `.rcvbp` whose name carries "16169" (P2.5 16S,
  two P3.91 16S including the E80 build, P6.67 6S, P8) uses id `0x002F`, and
  **none** of them uses `0x14C`. Our `0x14C` came from the previous owner's
  256 × 384 wall config, which is not evidence about this panel.
* **The two ids carry different protocol descriptors.** `0x002F` gets
  `[2][3][4] = 3, 4, 8`; `0x014C` gets `1, 5, 6`. On the §2 reading those are
  *different command encodings*: `4, 8` is the SM16380 non-SH CFG1/CFG2 tail
  pair, `5, 6` is the addressed-register pair. Even without that reading, the
  bytes differ, they are the only chip-protocol bytes in the pack, and they
  are selected by the id.
* **The vendor library treats the two ids asymmetrically on exactly the GCLK
  question.** From [chip-control-block.md §3](../chip-control-block.md):
  `IsHasGCLKRatioSetting()` is **false for `0x14C` and true for `0x2F`**, and
  `GetGclkCount()` returns **0 for `0x14C`** (the id falls past the table
  bound) while it computes a value for `0x2F`. For a part whose grey clock and
  row advance are the same RCLK pin, "this id has no GCLK ratio and a GCLK
  count of zero" is a pointed thing to be true.
* **The symptom fits.** The SM16269 is not a shift register with a latch: it
  holds a whole frame in its 8 K SRAM and self-scans it, advancing its own row
  pointer from RCLK, while the card's A–E lines drive the panel's row select
  independently. Wrong RCLK regime → every physical row shows the same SRAM
  row. That is "no pixel data displays and every scan line shows identical
  content", and it coexists happily with "gain writes work" and "brightness
  scales current", which are command-channel and current-source behaviour that
  never touch the row pointer.

*Against:* the bench has `0x014C` responding and `0x0214` dark, so `0x14C` is
at least *a* live id; and the reference file's wall presumably worked on `0x14C` at
some point (unverified).
*Experiment (one line, RAM only, and the cheapest decisive test left):*
point the panel config at the 0x002F identity (tried 2026-09-01: it does not arm the SM16269S; the id is MBI5153,
sub `0x008A`, `chip_control = 00 0e 03 04 08 01 03 00 00 00 00 81 00 81 00 10
00 00 00 00`, serial clock 10), `send-params`, photograph. Then, since that
profile ships **without** a register record in the corpus, repeat with record
0x84 suppressed.

**#2: The card is emitting the addressed-register ("SH") encoding to a part
that wants the unaddressed one.** *For:* the SM16269 datasheet publishes
exactly one configurable word, the 6-bit current gain (`G5..G0` in bits 5:0),
and **no register map and no address field at all**, while we send 33 addressed
registers `0x02..0x22,0xF0`. *Against, and it is strong:* the gain register
demonstrably moves supply current on this bench, which an addressed write
should not achieve on a part that decodes commands by tail length alone. Either
the silicon really is an SH variant, or the "gain works" observation needs
re-checking against a quiet wire.
*Experiment:* the same one as #1: `0x002F` carries `4, 8`, the non-SH tails.
The two hypotheses are distinguished only by *which* byte did the work, so run
the experiment first and attribute afterwards.

**#3: The `chip_control` recompute trap, which the register-sweep experiments
trigger.** Not a fault in itself, but it will manufacture one.
[output-stage.md §6](output-stage.md#6-reconciling-the-bench-facts) hypothesis
#5 says "bisect the differing registers, starting with `0x07`". Register `0x07`
red is the **input to the scan-cycle-level formula** (§4.2). Change it without
recomputing `chip_control[10..13]` and the card's scan-cycle count no longer
matches the chip's frequency division. Same for the sub-id.
`config/chips/sm16269.toml` already ships a combination the vendor would never
emit. **Fix `chips.rs` to compute bytes 10–13 rather than store them, or at
minimum assert consistency, before sweeping registers.**

**#4: A second unresolved `SChipControl` field** (`[14..15]`, `[16]`,
`[18..19]`; ours `00 08 / 02 / 0a 02`). Byte 16 in particular is *not written
by `ResetChipControl` for chip `0x14C` at all*; the `02` we send is carried
over from whatever LEDVISION last wrote, and is unexplained. Sweep after #1–#3.

**#5: Everything already ranked in
[output-stage.md §6](output-stage.md#6-reconciling-the-bench-facts)**:
un-flashed corrected config, `CardScanLen`, serial clock, register table, lane
map. Those are valid, and its #2 ("the results predate the corrected
config") is the cheapest thing to eliminate before any of this.

**Refuted here:** "the GCLK/RCLK-per-row count is wrong". `0x0097` = 151 is
exactly what the vendor's own formula produces for our `reg07 = 0x04` and
sub-id `0x0000` (§4.2). Do not sweep it in isolation.

### What this changes about the existing diagnosis

The standing one-liner,
*"the driver protocol is right, the drivers are armed, and the raster is being
scanned; what is wrong is which bytes reach the scan buffer"*,
should be amended. On a self-scanning S-PWM part there is a **second**
alignment that has to hold and that no amount of framebuffer correctness can
fix: the chip's own row pointer against the card's row select. Everything the
bench has observed is consistent with the bytes being right and that alignment
being wrong.

---

## 7. Reproducing this

```sh
sh analysis/fpga/scripts/repro.sh /tmp/e120-trellis       # bitstreams -> .config
md5 analysis/fpga/rom_*_decoded.txt                       # §1.1
python3 analysis/fpga/chip_control_survey.py              # §2 corpus table
```

The pad-driver / CCU2-chain extension used in §1.4 and §4.4 is a small
addition to `analysis/fpga/scripts/netlist/padlogic.py`: build
`bit_of[(x,y,'Q#')] → (chain, bit)` from maximal runs of adjacent
`SLICE?.MODE = CCU2` tiles along a row (carry runs horizontally,
`FCO → HFIE0000@(x+1,y)`), then tag each pad-driver LUT input with it. One
argument-parsing trap: node names contain commas, so a flop list must be
split on `;`, not `,`.
