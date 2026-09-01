# The E120 FPGA gateware

Reverse engineering of the Colorlight E120 receiving-card gateware, from the
vendor firmware images in `third-party/firmware/`.

Every claim below carries a confidence tag:

* **HIGH** — read directly out of the bitstream or the database, and
  cross-checked.
* **MEDIUM** — a strong pattern resting on one stated assumption.
* **NOT RESOLVED** — we could not determine it. What was ruled out is stated.

Nothing in this document is an inference dressed up as a fact. Where a
question could not be answered it says so.

Raw dumps are deliberately kept out of this file; paths to them are given
in [Artefacts](#artefacts).

---

## 1. Device and bitstream format

### 1.1 The part — HIGH

All five `.hex` files in `third-party/firmware/` are raw **Lattice Diamond
`.bit` bitstreams** for a

> **Lattice ECP5 LFE5U-25F-6CABGA256, IDCODE `0x41111043`**

The `.hex` extension is a misnomer — the files are binary, not Intel HEX. The
part is stated in plain ASCII in the file header and confirmed by the
`VERIFY_ID` command in the stream.

Each file is exactly **721 024 bytes**: the command stream followed by `0xFF`
padding out to the flash page, plus an 8-byte trailer.

### 1.2 File layout — HIGH

Verified independently for this document (script:
`scratchpad/trellis/repro.sh`, CRC check re-derived from scratch).

| Offset | Length | Contents |
|---|---|---|
| `0x000` | 342 | ASCII header, `\xFF\x00` then `Lattice Semiconductor Corporation Bitstream\n…` |
| `0x156` | 2 | `FF FF` |
| `0x158` | 2 | preamble `BD B3` |
| `0x15A` | 4 | `FF FF FF FF` |
| `0x15E` | 4 | `3B 00 00 00` — `LSC_RESET_CRC` |
| `0x162` | 8 | `E2 00 00 00 41 11 10 43` — `VERIFY_ID`, IDCODE `0x41111043` |
| `0x16A` | 8 | `22 00 00 00 40 00 00 20` — `LSC_WRITE_COMP_DIC` / control register 0 = `0x40000020` |
| `0x172` | 4 | `46 00 00 00` — `LSC_INIT_ADDRESS` |
| `0x176` | 4 | `82 91 1D 8A` — `LSC_PROG_INCR_RTI`, flags `0x91`, **`0x1D8A` = 7562 frames** |
| `0x17A` | 582 274 | frame data, 7562 × 77 bytes |
| `0x8E3FC` | — | `0xFF` padding |
| `0x8E408` | 8 | `C2 80 00 00 00 00 00 00` — `ISC_PROGRAM_USERCODE`, USERCODE `0x00000000` |
| `0x8E412` | 8 | `F6 00 00 00 00 00 18 00` — `LSC_EBR_ADDRESS`, address **`0x1800`** |
| `0x8E41A` | 4 | `B2 D0 01 00` — `LSC_EBR_WRITE` |
| `0x8E41E` | 2304 | EBR init payload, 2048 × 9-bit words |
| `0x8ED1E` | — | `B8 28`, then `5E 00 00 00` — program `DONE` |
| `0x8ED24`.. | — | `0xFF` padding to `0xAFFFC`, then an 8-byte trailer `00 00 00 01 E0 89 5B A0` |

The header's `Rows: 7562 / Cols: 592 / Bits: 4476704` matches the frame
geometry: 7562 frames × 592 bits (74 bytes) = 4 476 704 bits.

The only structural difference between the five images at this level is the
control-register value at `0x16A`: **`0x40000000` in the 6.69 (LS0allDA)
image, `0x40000020` in the other four.** — HIGH

### 1.3 Frame and CRC spec — HIGH

Each of the 7562 frames is **77 bytes**:

```
74 bytes frame data | 2 bytes CRC (big-endian) | 1 byte 0xFF inter-frame dummy
```

The CRC is **CRC-16 with polynomial `0x8005`, initial value 0, no input or
output reflection, no final XOR, MSB-first**, i.e.:

```python
def crc16(data, crc=0):
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x8005) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc
```

The CRC accumulates over **the command stream since the last CRC reset**, and
is reset after every frame's CRC field. Concretely:

* **Frame 0** — the CRC covers everything from `0x162` (the first byte after
  `LSC_RESET_CRC`) through the frame's own 74 data bytes. A previous note in
  this project said frame 0 also validated with an empty prefix; it does not.
  This was rechecked by brute-forcing the seed and by scanning prefix start
  offsets: only a prefix starting at `0x15F`–`0x162` (the leading zero bytes
  do not affect the CRC) reproduces frame 0's stored CRC `0x2C8E`.
* **Frames 1..7561** — the CRC covers the preceding frame's `0xFF` dummy byte
  followed by the 74 data bytes.

With that model **all 7562 frames validate in all five vendor images.**

The frame data area therefore ends at `0x17A + 7562·77 = 0x8E3FC` (an earlier
note in this project put it at `0x8ED24`, which is the end of the *EBR* block,
not the frame area).

---

## 2. Decode method — how to reproduce

Tooling is [prjtrellis](https://github.com/YosysHQ/prjtrellis), the
open-source ECP5 bitstream database. No vendor software is used.

```sh
brew install prjtrellis          # 1.4; provides ecpunpack + pytrellis
sh scratchpad/trellis/repro.sh   # or the three steps below
```

1. The `.hex` file **is** a `.bit` file. No header stripping is required —
   `ecpunpack` parses the ASCII header itself.
2. **The only wrangling needed** is to cut the trailing padding. `ecpunpack`
   walks past the `DONE` command into the `0xFF` fill, reaches the 8-byte
   trailer at `0xAFFFC`, hits the byte `0x00`, and aborts with
   `Bitstream Parse Error: unsupported command 0x00 [at 0xafff9]` — which
   discards the whole decode even though every command parsed correctly.
   Truncating the file to `0x8ED30` (just past `DONE`) fixes it:

   ```sh
   python3 -c "d=open('in.hex','rb').read(); open('out.bit','wb').write(d[:0x8ed30])"
   ecpunpack --idcode 0x41111043 out.bit out.config
   ```

3. The result is a `.config` text file: `.tile <NAME>:<TYPE>` sections
   containing `arc:` (routing), `word:` (multi-bit settings, **LSB first**)
   and `enum:` (named settings), plus one `.bram_init` section.

Decoded sizes: 7.6–8.6 MB of text per image, 4132 configured tiles in the
16.53 image.

For programmatic access, `pytrellis` works — but on this machine **only under
`/opt/homebrew/bin/python3.14`**, with
`sys.path.insert(0, '/opt/homebrew/opt/prjtrellis/lib/trellis')`. It gives the
full routing graph (`Chip.get_routing_graph(True, True)`, ~8 s to build for the
25F) which is needed to answer "is this pad actually connected to anything".

---

## 3. A trap: `word:` bit order is not uniform — HIGH

prjtrellis writes multi-bit settings as `word: NAME <bitstring>`, and the bit
order is set **per field** by the database, not globally. Two fields in these
images prove the two orders coexist:

* `EBRn.WID 110000000` reads as **3** LSB-first, which matches the
  `.bram_init 3` index and is unique in the design. MSB-first would give 384.
* The PLL's `MFG_GMC_TEST 1110` and `MFG_GMCREF_SEL 10` match Lattice's
  standard EHXPLLL manufacturing constants (14 and 2) only **MSB-first**.

**Check the field-to-frame-bit mapping in
`$(brew --prefix prjtrellis)/share/trellis/database/ECP5/tiledata/<TILE>/bits.db`
before reading any value.** Assuming one global order will silently corrupt
anything built on it.

---

## 4. Block RAM

### 4.1 The one initialised BRAM — HIGH

Every one of the five images contains **exactly one** `.bram_init` section,
and it is always **`.bram_init 3`**: 2048 nine-bit words written by the
`LSC_EBR_ADDRESS 0x1800` / `LSC_EBR_WRITE` pair at the end of the command
stream. Every other block RAM in the design starts uninitialised — so this is
the design's single ROM, and everything else is runtime storage.

The physical EBR carrying it is identified by `EBRn.WID = 3` (the only EBR in
each image with that value). It is configured `PDPW16KD` with
`DATA_WIDTH_R = 36` — i.e. **512 entries × 36 bits** — with registered
outputs. Its placement moves between builds (`MIB_R37C5:MIB_EBR1` in 16.53 and
13.39, `MIB_R25C5`/`MIB_R25C7` in the older ones).

### 4.2 Is it shared across firmware families? — HIGH, and a correction

An earlier note in this project said the block is identical across the
PWM / Normal / LS0allDA families. That is **almost** true:

| image | md5 of the 2048-word block |
|---|---|
| 6.69 LS0allDA, 9.53 PWM, 13.39 Normal, 16.53 PWM | `c826f7b5…` (identical) |
| **10.81 PWM** | `51d78de7…` (**different**) |

10.81's block differs only by a **five-entry-longer prologue at the very
start**; from entry ~7 onward it is identical, and the *set of addresses it
writes* is exactly the same 55 addresses as 16.53's. So it is a frozen shared
block that was touched once.

### 4.3 What the ROM contains — MEDIUM

Decoding the 512 × 36-bit entries: lane 3 is zero in all 512 entries and lane
2 only ever holds 0–7, so the real payload is **21 bits per entry**, and 351
of the 512 entries are used (contiguous, entries 0–350; the rest are zero).

The 21 bits split cleanly as **5-bit opcode (bits 20:16) + 16-bit immediate
(bits 15:0)**, and the result reads as a **register-write script**:

* Opcode `0x1B` (57×) carries what look like addresses in a coherent space —
  `0x8011`, `0x801E`, `0x804F`…`0x80DB`, `0x8165`, `0x81AB`, `0x8465`,
  `0x87F6/F8/FA/FC/FE`, `0xA000`–`0xA026`, `0xB818`–`0xB84C` — and alternates
  strictly with opcode `0x1C` (252×, the bulk of the entries) in the tail
  region, in step-1 and step-2 address ramps.
* Opcode `0x1F` (19×) carries a second address space (`0x0A43`…`0x0D41`) and
  is always followed by a low opcode (`0x00`, `0x04`, `0x10`–`0x17`) carrying
  the datum.
* Long runs of pure `0x1C` are consistent with burst writes to an
  auto-incrementing address set by a preceding `0x1B`.

**Ruled out** (all with evidence, see the dumps):

* **Not a gamma or brightness LUT** — HIGH. The longest strictly increasing
  run of the immediate field is 4. There is no 256- or 1024-point ramp
  anywhere in the block.
* **Not Lattice Mico8 / LM8 code** — MEDIUM-HIGH. LM8 is an 18-bit ISA; this
  is 21 bits with the top 5 forming an address/data tag, and there is no
  jump-target structure.
* **Not 8051 or PicoBlaze byte code** — MEDIUM. No strings ≥ 4 characters in
  either endianness or either bit packing.
* **The driver-chip ids are not in it** — HIGH. None of `0x014C`, `0x0187`,
  `0x0214`, `0x0215`, `0x00DE`, `0x00FD`, `0x013C` appears as an immediate or
  as a full 21-bit word.

**NOT RESOLVED:** what the address spaces `0x0Axx`–`0x0Dxx`, `0x80xx`–`0x87xx`,
`0xA0xx` and `0xB8xx` address, and therefore what the script configures. The
addresses are internal to the design; nothing in the bitstream names them.

### 4.4 All the other block RAMs — HIGH counts, MEDIUM interpretation

| image | EBRs instantiated (of 56) |
|---|---|
| 13.39 Normal | 49 |
| 9.53 PWM | 53 |
| 10.81 PWM | 53 |
| 16.53 PWM | **53** |
| 6.69 LS0allDA | 54 |

In 16.53: **10 EBRs are wide-write `PDPW16KD` with `DATA_WIDTH_R = 36`**
(512 × 36); the rest are ×9 with a few ×2 and ×4. 15 use
`WRITEMODE_B READBEFOREWRITE`; nearly all have registered outputs.

Caveat — HIGH: prjtrellis spreads one EBR's bits over 2–3 adjacent
`MIB_EBRn` tiles and `ecpunpack` emits a `MODE` enum *per tile*, so a single
EBR often reports `DP16KD` in one tile and `PDPW16KD` in another. Per-tile
`MODE` strings are individually untrustworthy; only the combination means
anything.

Arithmetic worth having on the bench:

* 53 × 18 Kbit = **954 Kbit ≈ 119 KB** on-chip, out of 1008 Kbit available.
* A 128 × 64 panel at 8 bits per colour is 128·64·24 = **192 Kbit ≈ 11 EBRs**
  — about a fifth of what is instantiated. Two or three full frames would fit.
* **There is no external DRAM.** No `DQSBUF`, `DDRDLL` or `DLLDEL`
  configuration exists anywhere in any of the five images (HIGH). All
  buffering is on-chip.

What the EBRs are *for* — a framebuffer vs line buffers vs FIFOs — is
**NOT RESOLVED**. The mix (ten wide 512×36 blocks plus ~43 narrow ×9 blocks)
is what you would expect from a pixel buffer plus many small FIFOs, but that
is a shape argument, not evidence.

---

## 5. Version comparison, at the decoded level

### 5.1 Resource use — HIGH

| metric | 6.69 LS0allDA | 9.53 PWM | 13.39 Normal | 10.81 PWM | 16.53 PWM |
|---|---|---|---|---|---|
| date | 2022-09-07 | 2022-10-31 | 2022-11-01 | 2023-09-07 | 2023-12-27 |
| LUT4s with non-zero INIT | 21 371 | 22 392 | 22 208 | **23 734** | 23 199 |
| LUT utilisation (of 24 288) | 88.0 % | 92.2 % | 91.4 % | **97.7 %** | 95.5 % |
| DFFs (distinct slice Q driving routing) | 12 112 | 13 023 | 11 279 | **14 062** | 13 074 |
| Carry-chain slices (CCU2) | 2 807 | 3 451 | 3 403 | 3 836 | 3 478 |
| Distributed-RAM slices (DPRAM/RAMW) | 178 / 89 | 40 / 20 | 118 / 59 | 40 / 20 | 36 / 18 |
| EBRs | 54 | 53 | 49 | 53 | 53 |
| DSP MULT18 / ALU54 | 24 / 12 | 26 / 13 | 26 / 13 | **28 / 14** | 24 / 12 |
| PIO sites with a `BASE_TYPE` | 377 | 377 | 377 | 377 | 377 |
| routing arcs | 233 152 | 238 773 | 229 023 | 259 917 | 248 398 |

The DFF count is a lower bound counted as distinct `(tile, Qn)` slice outputs
that source a routing arc — MEDIUM-HIGH.

### 5.2 What actually differs

1. **The board interface is frozen across all five images — HIGH.** Same 377
   configured PIO sites, same IO standards on 371–377 of them, one EHXPLLL at
   the same frequency plan, the same 22 DDR-registered IO sites, the same
   `USRMCLK` / `OSCG` / `GSR` setup. Practical consequence: **a pinout learned
   from any one image is valid for all of them.**

   Pairwise differing IO sites: 9.53 ↔ 10.81 ↔ 16.53 differ in **zero**
   sites. 13.39 differs from those three in 3 sites; 6.69 differs in 6.
   The one electrically interesting difference is `MIB_R50C4.PIOA` (the
   EFB0/GSR pin): `BIDIR_LVTTL33` with DRIVE 16 in 6.69, `BIDIR_LVCMOS25`
   with DRIVE 4 in every other image. 6.69 is the PCB 6.1 image, so this
   plausibly tracks the board revision — MEDIUM, the correlation is n = 1.

2. **No two images are re-places of one netlist — HIGH.** Comparing
   placement-independent LUT-function multisets gives Jaccard 0.64–0.75 for
   every pair, including the closest (9.53 vs 16.53, 0.748). Every version is
   a genuinely different design. (This measure proves dissimilarity well; a
   high score would not have proved identity.)

3. **Monotone growth — HIGH.** 6.69 (88 %) → 9.53/13.39 (~92 %) → 16.53
   (95.5 %) → 10.81 (97.7 %). The 25F is essentially full in the newer builds.
   Distributed RAM was designed out over time (178 → 36 DPRAM slices) while
   EBR use went up.

4. **"Normal" vs "PWM" is the biggest structural split — MEDIUM-HIGH.** It
   shows up as IO-cell register usage: `IOLOGIC*.MODE = IREG_OREG` appears
   **96 times in 13.39 and 6.69 but only 10 times in 9.53 / 10.81 / 16.53**.
   The older/Normal builds register ~96 more output pins inside the IO cell;
   the PWM builds moved that logic into the fabric.

5. **10.81 is an outlier, not a point on the 9.53 → 16.53 line — HIGH on the
   facts.** It is the largest design, the only one using all 28 multipliers
   and all 14 ALU54s, and the only one with a different BRAM ROM. Despite its
   version number it is dated 2023-09, *later* than 13.39 and *before* 16.53:
   **the version numbers are per product line, not one sequence.**

6. **The PLL differences are pure output-phase retiming — HIGH.** Across all
   five images the dividers, output enables, charge-pump current and loop
   filter are identical; only `CPHASE` / `FPHASE` of CLKOS, CLKOS2 and CLKOS3
   change. 9.53, 10.81 and 16.53 have byte-identical PLL configuration; 6.69
   differs only in CLKOS3 phase; 13.39 differs in CLKOS/CLKOS2/CLKOS3 phase.

   In LED-panel terms, phase is the launch-edge relationship between the data,
   shift-clock and latch outputs — exactly the knob you would expect to move
   between driver-chip families. That reading is MEDIUM; the "only phase
   changed" fact is HIGH.

7. **Structural enum fingerprint.** 953 non-PLC2 `(tiletype, key, value)`
   triples are common to all five (86–88 %). The triples unique to one image
   are dominated by: 13.39 — 36 IOLOGIC `CEMUX`/`OUTREG.REGSET` settings (the
   IO-register difference above); 9.53 — 27 `ALU54_7.*` DSP settings; 6.69 —
   the EFB0 pin drive and IOLOGIC GSR flags; 10.81 — an extra 36-bit
   `PDPW16KD` and two `MULT18_0.REG_PIPELINE_RST`; **16.53 — exactly one**
   (`MULT18_5.REG_INPUTA_CLK NONE`).

---

## 6. The driver-chip id — NOT RESOLVED

This was the question that mattered most, and it did not resolve. Read this
section for what was ruled out; do not read it as "the gateware ignores the
chip id".

### 6.1 How the id reaches the card — HIGH

From the vendor-SDK decode already in this repo
(`crates/e120-rcvbp/src/spec/basic_pack.rs`, vendor `ResetChipType`
@ `0x1e5130`), the 256-byte basic parameter pack carries the id as:

* **`+0x1B`** — the chip id if it fits in one byte (`< 0x100`), otherwise the
  literal escape `0xFE`.
* **`+0xE7..+0xE8`** — the full 16-bit id, **big-endian**, and zero when the
  id fitted in the byte slot.
* The pack's CRC-32 is computed with the chip-id bytes **zeroed**, so the id
  is deliberately excluded from the checksum.

Both ids of interest are above `0x100`, so the card sees
`+0x1B = 0xFE` and `+0xE7,+0xE8 = 01 4C` or `02 14`. Nothing else in the pack
changes when only the id changes — which is why the observed behaviour flip
(`0x14C` → per-pixel noise at ~2.8–4 A; `0x214` → dark at ~0.5 A) points at
the card, not at host-side table generation.

### 6.2 What was searched, and found absent

Method: every LUT4 INIT in all five images was extracted **and corrected for
constant-tied inputs**. This correction matters: ECP5 slices carry
`SLICEx.<P><k>MUX` enums that tie a LUT input to a constant with no routing
arc, and before modelling them **6264 of 23 199 LUTs (27 %) appeared to
depend on unrouted inputs** — i.e. the naive netlist was simply wrong. After
reduction, zero LUTs depend on an unrouted input, and the inter-tile routing
graph was reconstructed so that clusters were found by real net connectivity
rather than physical adjacency.

Against that netlist, all of the following came back **empty**:

| Searched for | Result |
|---|---|
| 16-bit compare-to-constant (4 one-hot LUT4s + AND) | 6 AND-of-one-hot clusters in 16.53, widest 11 bits, **none takes a coherent bus as input** — all mix registered flags with scattered combinational outputs. FSM condition trees, not data compares. |
| 8-bit compare-to-constant against any register byte (exhaustive symbolic sweep of every tile with ≥6 used flops) | 20 cones, **all 6-bit registers matched on only 4 bits** — plain 4-input ANDs. **Zero 8-bit matches.** |
| The `0xFE` escape-byte test specifically | Not present as an 8-bit equality test. Two cones have the right 7×1 + 1×0 literal shape, but their leaves are scattered control signals, not a byte register. |
| 4-to-16 decoder on a chip-id nibble | None. Largest group of one-hot LUTs sharing all four input nets is 4, and those are datapath mux-select decode. |
| CCU2 carry-chain compare-to-constant | 14 candidates, max 21 bits, all AND-reduces of scattered status signals. The 16 genuine wide CCU2 comparators in the design are all **variable-vs-variable** (PWM/greyscale threshold and counter compares). |
| Chip-id values in the microcode ROM | None of `0x014C`, `0x0187`, `0x0214`, `0x0215`, `0x00DE`, `0x00FD`, `0x013C`, `0x00FE` appears as an immediate. |
| Any constant present in 16.53 but not the older images | None. Cluster counts across images (6 / 0 / 5 / 6 / 2) are placement noise. |

One-hot LUT4s are not a comparator signature in this design at all — there
are ~2100 of them in every image, i.e. they are ordinary logic.

### 6.3 Why the negative is only MEDIUM-HIGH, not absolute

**The positive control failed.** This design demonstrably parses Ethernet,
yet the same search finds **no 8-bit constant comparator anywhere** — no SFD
`0xD5`, no ethertype `0x0107`, no `0x0aXX`. Constants we *know* must be
tested are as invisible as the chip id. So the correct reading is "this
design does not build constant comparisons out of LUT4s", not "the chip id is
never compared".

Surviving hypotheses, none of which could be distinguished:

* **(a)** The id is written into a BRAM/LUT-RAM register file by the packet
  parser and consumed by a sequencer that compares it against *table data* —
  a data-vs-data compare, which is exactly what the 117 CCU2 XNOR
  comparators in the design look like, and which would equally explain the
  missing Ethernet constants. This is the most likely one.
* **(b)** The id is compared bit- or nibble-serially against a streamed
  constant, which is indistinguishable from ordinary logic.
* **(c)** Only a 2–4 bit field of the id is used, below the detection floor.

### 6.4 The one concrete lead — MEDIUM

A **10-bit high-fanout mode-flag bundle** was found — flops
`R22C41_Q0,Q1,Q2,Q3,Q6,Q7`, `R26C42_Q5`, `R28C44_Q6,Q7`, `R21C45_Q4`, plus a
global qualifier `R14C31_Q0` that feeds 114 one-hot LUTs. 734 LUTs read this
bundle and 193 are pure functions of it; simulating all 1024 values gives
1022 distinct equivalence classes, so these are **ten independent, already
decoded mode flags** — plausibly the *outputs* of whatever the chip-id decode
is.

Their fan-in traces back to **`R27C44_Q0..Q3`**, a 4-bit field whose D-side
LUTs are trivial (M-input/`SD` bypass flops) and which has **no visible
combinational source** — exactly what a field loaded from a parameter store
looks like. That is where a follow-up should start.

### 6.5 What this does and does not tell the bench

* **It does not tell us which ids the gateware recognises.** That question is
  unanswered. Anyone who claims a list from this bitstream is guessing.
* **It does tell us the answer will not come cheaply from the bitstream.**
  Resolving it needs either full netlist recovery (LUT + FF + BRAM + routing
  → RTL) or an empirical sweep on the bench.
* The empirical sweep is already the right move and is already wired up
  (`scripts/chipsweep.sh`). **Prefer the bench answer.**

<!--SECTIONS-->

## Artefacts

Working directory (session scratchpad, not in the repo):

```
/private/tmp/claude-501/-Users-amd-e120/eebf5407-0aa9-43c6-b991-a4285ce428a5/scratchpad/trellis/
```

| File | What it is |
|---|---|
| `repro.sh` | regenerates every `.config` from the repo firmware |
| `t_*.config` | the five decoded bitstreams |
| `pinmap_16.53.txt` | every CABGA256 package pin → row/col/PIO/bank/special function, with its IO configuration lines |
| `used_pads.txt` | routing-graph pad-usage scan |
| `padscan*.py`, `diag.py` | the pytrellis scripts behind those |
