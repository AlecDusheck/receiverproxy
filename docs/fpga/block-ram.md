# Block RAM

Dumps: `analysis/fpga/bramdump_*_3.txt` (raw 2048 × 9-bit words) and
`analysis/fpga/rom_*_decoded.txt` (the 21-bit decode, one entry per line).
Per-instance settings: regenerate `ebr_detail.json` with
`analysis/fpga/scripts/parse4.py`.

## 1. Exactly one initialised BRAM — HIGH

Every one of the five vendor images contains **exactly one** `.bram_init`
section, and it is always **`.bram_init 3`**: 2048 nine-bit words, written by
the `LSC_EBR_ADDRESS 0x1800` / `LSC_EBR_WRITE` command pair at the end of the
command stream (see [bitstream-format.md](bitstream-format.md#3-command-stream--high)).

Every other block RAM in the design starts **uninitialised**. So this is the
design's single ROM; everything else is runtime storage.

The physical EBR carrying it is the only one in each image with `EBRn.WID = 3`.
It is configured `PDPW16KD`, `DATA_WIDTH_R = 36` — **512 entries × 36 bits** —
with `REGMODE_A/B OUTREG`, `GSR DISABLED`, `CSDECODE_A 011`, `WEAMUX INV`.

Its placement moves between builds (placement, not function):

| image | ROM EBR site |
|---|---|
| 16.53 | `MIB_R37C5:MIB_EBR1` EBR0 |
| 13.39 | `MIB_R37C5:MIB_EBR1` EBR0 |
| 10.81 | `MIB_R25C7:MIB_EBR3` EBR1 |
| 9.53 | `MIB_R25C5:MIB_EBR1` EBR0 |
| 6.69 | `MIB_R25C5:MIB_EBR1` EBR0 |

## 2. Shared across firmware families — HIGH, with a correction

An earlier note in this project said the block is identical across the
PWM / Normal / LS0allDA families. That is **almost** true:

| image | md5 of the 2048-word block |
|---|---|
| 6.69 LS0allDA, 9.53 PWM, 13.39 Normal, **16.53 PWM** | `c826f7b57ee48b22cf5f7f39986eb6ff` |
| **10.81 PWM** | `51d78de731f89299ead0f0ad78c13d6b` |

10.81 has 356 used entries against 351, `difflib` similarity 0.990, and the
**only** difference is a five-entry-longer prologue at the very start —
everything from entry ~7 onward is identical, and the *set of addresses
written* is exactly the same 55 addresses as 16.53's.

The block is unchanged across 15 months (2022-09 → 2023-12), across the
Normal/PWM split, and across the PCB 6.0 / 6.1 split. **Adding
SM16386S/SM16269SH support in 16.53 changed nothing in it.** — HIGH

## 3. Contents — MEDIUM

Decoding the 512 × 36-bit entries (four consecutive 9-bit words per entry:
lane0 = bits[8:0], lane1 = [17:9], lane2 = [26:18], lane3 = [35:27]):

* **lane3 is zero in all 512 entries**, and **lane2 only ever holds 0–7**. So
  the real payload is **21 significant bits per entry**; bits [35:21] are
  always zero. — HIGH
* **351 of 512 entries are used**, entries 0–350, **contiguous — no zero entry
  inside the used region**. Entries 351–511 are all zero. — HIGH
* Entropy 3.85 bits per 9-bit physical word over the whole 2048 (dominated by
  the zero tail); 6.52 bits/byte over the 702 bytes of the 16-bit payload
  field, with 173 distinct byte values.
* Bit-1 density: bits 0–17 ≈ 0.23–0.52 (data-like); bit 18 = 0.81,
  bit 19 = 0.95, bit 20 = 0.98, bit 21 = 0.00 — the top bits are a strongly
  skewed tag field, not data.

### It reads as a register-write script — MEDIUM

The 21 bits split cleanly as **5-bit opcode (bits 20:16) + 16-bit immediate
(bits 15:0)**. Opcode histogram for 16.53 (351 entries):

| opcode | count |
|---|---|
| `0x1C` | 252 |
| `0x1B` | 57 |
| `0x1F` | 19 |
| `0x10` | 6 |
| `0x11`, `0x15` | 3 each |
| `0x0D`, `0x0E`, `0x16` | 2 each |
| `0x00`, `0x04`, `0x12`, `0x14`, `0x17` | 1 each |

Evidence for the address/data reading:

* **Opcode `0x1B` immediates are a clean address set**:
  `0x8011`, `0x801E`, `0x802B`, `0x804F`, `0x8050`, `0x8053`, `0x805A`,
  `0x8061`, `0x807D`–`0x807F`, `0x809A`–`0x809D`, `0x80AA`–`0x80AC`,
  `0x80C7`–`0x80CA`, `0x80D7`, `0x80DB`, `0x8165`, `0x81AB`, `0x8465`,
  `0x87F6/F8/FA/FC/FE`, `0xA000/02/04/06/08/12/14/16/20/22/24/26`,
  `0xB818/1A/1C/1E`, `0xB82E`, `0xB832`, `0xB846/48/4A/4C`.
* They alternate strictly with `0x1C` words in the tail: entries 243–271 are
  `1B 87F6 / 1C … / 1B 87F8 / 1C … / 1B 87FA / …` — step-2 address ramps.
  Entries 290–334 step by 1. Entries 74–90 walk
  `0xA026, A024, A022, A020, A006, A004, A002, A000, A008`.
* **Opcode `0x1F` carries a second address space** (`0x0A43`, `0x0A44`,
  `0x0A46`, `0x0A4B`, `0x0A80`, `0x0A81`, `0x0A86`, `0x0A92`, `0x0B80`,
  `0x0B82`, `0x0BC0`, `0x0D08`, `0x0D41`) and is always followed by one of
  `0x00`, `0x04`, `0x10`–`0x17` carrying the datum — plausibly an addressed
  write where the low opcode bits select width or type.
* The long runs of pure `0x1C` (entries 25–73 and 95–242) are consistent with
  burst writes to an auto-incrementing address set by a preceding `0x1B`.

### Explicitly ruled out

| hypothesis | verdict | evidence |
|---|---|---|
| gamma or brightness LUT | **ruled out — HIGH** | longest strictly increasing run of the immediate field is 4. No 256- or 1024-point ramp anywhere; no 8-bit-indexed monotone table. |
| Lattice Mico8 / LM8 code | **ruled out — MEDIUM-HIGH** | LM8 is an 18-bit ISA; this is 21 bits with the top 5 forming an address/data tag. No jump-target structure: no immediate matches a plausible 0–350 code-address density. |
| 8051 or PicoBlaze byte code | **ruled out — MEDIUM** | no ASCII strings ≥ 4 chars in either endianness or either bit packing of the 702 payload bytes. |
| contains a driver-chip id | **ruled out — HIGH** | none of `0x014C`, `0x0187`, `0x0214`, `0x0215`, `0x00DE`, `0x00FD`, `0x013C`, `0x00FE` appears as an immediate or as a full 21-bit word. |
| scan table | **ruled out — MEDIUM** | it is identical across builds and across 15 months; a scan table would be panel-specific, and the card receives its scan table from the host anyway (see [parameter-path.md](parameter-path.md)). |

### NOT RESOLVED

What the address spaces `0x0Axx`–`0x0Dxx`, `0x80xx`–`0x87xx`, `0xA0xx` and
`0xB8xx` actually address, and therefore what the script configures. The
addresses are internal to the design; nothing in the bitstream names them.
The meaning of the 5-bit opcodes beyond "`0x1B`/`0x1F` = address, others =
data" is also unresolved.

*What would settle it:* recovering the netlist around the ROM's read port —
specifically what the 16-bit immediate fans out to and what decodes the
5-bit opcode. That is a netlist-recovery task, not a bitstream-reading one.

## 4. The other block RAMs

| image | EBR instances |
|---|---|
| 13.39 Normal | 49 |
| 9.53 PWM | 53 |
| 10.81 PWM | 53 |
| 16.53 PWM | **53** (a second pass counted 54 — see below) |
| 6.69 LS0allDA | 54 |

**Counting caveat — HIGH.** prjtrellis spreads one EBR's configuration bits
over 2–3 adjacent `MIB_EBRn` tiles, and `ecpunpack` emits a `MODE` enum *per
tile*. A single EBR frequently reports `DP16KD` in one tile and `PDPW16KD` in
another. **Per-tile `MODE` strings are individually untrustworthy** — only
their combination means anything, and "instance count" depends on the grouping
convention. Two independent passes gave 53 and 54 for 16.53.

For 16.53, grouped by (MODE tuple, widths):

| count | MODE values seen | widths |
|---|---|---|
| 10 | (PDPW16KD, DP16KD) | DPA = 9, DPB = 9, PDPW_R = 9 |
| **10** | (PDPW16KD, PDPW16KD[, PDPW16KD]) | **PDPW_R = 36** |
| 5 | (DP16KD, DP16KD) | DPA = 9, DPB = 9, PDPW_R = 9 |
| 5 | (DP16KD, PDPW16KD) | defaults (none emitted) |
| 5 | (DP16KD, PDPW16KD) | DPA = 2, DPB = 2/9, PDPW_R = 2/9 |
| 3 | (DP16KD, DP16KD) | defaults |
| 3 | (DP16KD, PDPW16KD) | DPA = 4, DPB = 4/9, PDPW_R = 4/9 |
| ~12 | mixed | DPA = 9/2/4 mixtures |

Robust summary: **10 EBRs are unambiguously wide-write `PDPW16KD` with
`DATA_WIDTH_R = 36`** (512 × 36); the rest are ×9 with a few ×2 and ×4. 15 use
`WRITEMODE_B READBEFOREWRITE`; nearly all have registered outputs
(`REGMODE_A/B OUTREG`) and `GSR DISABLED`.

## 5. Memory arithmetic for the bench

* 53 × 18 Kbit = **954 Kbit ≈ 119 KB** on-chip, out of 1 008 Kbit. The design
  uses essentially all of it. — HIGH
* A 128 × 64 panel at 8 bits per colour is 128·64·24 = **192 Kbit ≈ 11 EBRs**
  — about a fifth of what is instantiated. Two or three full frames would fit.
  At the panel's native **14-bit grey** it is 128·64·42 = **336 Kbit ≈ 19
  EBRs**, still comfortably inside. — HIGH (arithmetic)
* **There is no external DRAM.** No `DQSBUF`, `DDRDLL` or `DLLDEL`
  configuration exists anywhere in any of the five images, and the left/right
  edges have no bidirectional pins except one. All buffering is on-chip. — HIGH

> **Superseded in part.** The per-EBR pin, clock, write-gate and generator map
> is now in `analysis/fpga/ebr_map_16.53.txt` / `ebr_map_10.81.txt`, and the two
> Ethernet receive FIFOs, the two write banks and the output-stage buffer are
> identified in [pixel-write-path.md](pixel-write-path.md). The reason this was
> not possible earlier is a decode trap: **EBR bel pins are not set-arc sinks**
> — they hang off ordinary CIB J-pins by fixed connections.

**What the EBRs are *for* is NOT RESOLVED.** The mix (ten wide 512×36 blocks
plus ~43 narrow ×9 blocks) is what you would expect from a pixel buffer plus
many small FIFOs — Ethernet RX/TX, per-port scan FIFOs, a configuration store.
That is a shape argument, not evidence, and no EBR was traced to a logical
buffer.

*What would settle it:* tracing the write-address generators of the ten wide
blocks back to the Ethernet RX path, and their read-address generators forward
to the LED output stage.
