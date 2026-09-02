# SPI flash layout

How a vendor `.hex` maps into the card's flash, what else lives there, and the
`0x7F000` question. Artefacts:
`analysis/fpga/flash-layout.txt` (annotated address map),
`analysis/fpga/flash-address-map.txt`, `analysis/fpga/image-identity.tsv`,
`analysis/fpga/image-match-matrix.tsv`,
`analysis/fpga/failing-frames-primary-region.tsv`,
`analysis/fpga/failing-frames-primary-after-restore.tsv`,
`analysis/fpga/flash-map.py` (regenerates all of it, read-only).

## 1. Alignment: delta = 0 (HIGH)

A vendor `.hex` byte 0 lands on flash byte 0 of the bank base, ASCII header
and all. **The 128-byte size difference is not an offset**; it is trailing
padding at `.hex` `0x0B0000`–`0x0B007F` that the dumps simply stop short of.

Every bitstream command byte lines up exactly at delta 0 in all three dumps:
`BD B3` at `0x158`, `0xE2` at `0x162`, `0x22` at `0x16A`, `0x82` at `0x176`.
Deltas ±128, +342 and +214 were tested and all fall to chance.

## 2. Dump extents (HIGH)

| file | size | flash range | blocks |
|---|---|---|---|
| `card-dumps/primary-region.bin` | `0xC0000` | `0x000000`–`0x0BFFFF` | 0x00–0x0B |
| `card-dumps/primary-after-restore.bin` | `0xB0000` | `0x000000`–`0x0AFFFF` | 0x00–0x0A |
| `card-dumps/golden-bank.bin` | `0xB0000` | `0x200000`–`0x2AFFFF` | golden bank at block 0x20 |

Corroborated by `docs/archive/config-protocol.md` §25.2, which records the
banks as measured.

## 3. Primary bank, factory state (HIGH)

Per-64K-block match against 10.81 is exactly `1.000000` for blocks 0x00, 0x01,
0x02, 0x08, 0x09, 0x0A and chance-level for 0x03–0x07. A whole-file diff
against 10.81 yields exactly two differing segments: `0x030000`–`0x07FFFF` and
`0x0B0000`–`0x0B007F`.

```
0x000000-0x000155  Lattice ASCII header, "Date: Thu Sep 07 15:47:58 2023"
0x000156-0x02FFFF  bitstream commands + frames 0..2547   == 10.81 byte-for-byte
0x030000-0x033FFF  DATA: 4096 x 4-byte BE entries.  4091 = FFFFFF00,
                   4 = FFFFFF80, 1 = FFFFFF40.  Shape of a gamma/cal LUT.
                   CONTENT NOT IDENTIFIED.
0x034000-0x03FFFF  erased 0xFF
0x040000-0x04FFFF  DATA: 16384 x the constant word 99 99 99 08, nothing else.
                   CONTENT NOT IDENTIFIED.
0x050000-0x06FFFF  erased 0xFF (blocks 0x05, 0x06 wholly erased)
0x070000-0x07AFFF  the compiled boot-config image  (see section 6)
0x07A423-0x07EFFF  erased 0xFF
0x07F000-0x07F0FF  DATA, 256 bytes -- page 0xF0, the card-written record
0x07F100-0x07FFFF  erased 0xFF
0x080000-0x08E3FB  bitstream frame tail == 10.81
0x08E3FC-0x08ED23  USERCODE / EBR_ADDRESS 0x1800 / EBR_WRITE + 2304 B / DONE
0x08ED24-0x0AFFF7  erased 0xFF padding
0x0AFFF8-0x0AFFFF  end marker  00 00 00 01 C5 99 12 FD
0x0B0000-0x0B00FF  module mapping table page -- magic 11 22 33 44 55 66 47,
                   u32 fields, zlib stream at 0x0B002A -> 156 bytes
0x0B0100-0x0BFFFF  erased 0xFF
```

**Correction to an earlier note: the 8-byte end marker is per-image, not
universal.** 16.53's is `00 00 00 01 E0 89 5B A0`; 10.81's is
`00 00 00 01 C5 99 12 FD`. And 13.39 is a *different container*: its `0xFF`
run continues to `0xB007A` and its marker sits at `0xB007B`, matching
`third-party/README.md`'s note that the Normal family declares length
`0x0B0080` rather than `0x0B0000` (HIGH).

**Golden bank (HIGH).** A complete, hole-free bitstream, header date
`Sat Jul 09 14:10:43 2022`, all 7562 frame CRCs valid, the only `0xFF` run is
the tail. It matches **none** of the five images we have (see §5); it is a
build we do not possess.

**Regions not read but targeted by the vendor library** (from
`config-protocol.md` §22.4, host disassembly, unverified on hardware;
MEDIUM): `0x0A0000`, `0x1C0000`, `0x1E0000`, `0x1F0000`, `0x390000`,
`0x3A0000`, `0x3B0000`, `0x3C0000`, `0xD60000`, `0xE70000`, `0xE80000`,
`0xE90000`. An addrHi of `0xE9` implies a **≥ 16 MB address space**.

## 4. The 0x7F000 question

### 0x7F000 is INSIDE the bitstream image region (HIGH)

Frame data runs `0x00017A`–`0x08E3FB`. `0x7F000` sits about 62 % of the way
through it, in the middle of **frames 6750–6804**.

In the vendor `.hex` that sector is ordinary, high-entropy frame data: only
4295 of the 327 680 bytes in `0x30000`–`0x7FFFF` are `0xFF`, all 256 byte
values are present, and its frame CRCs are valid there like everywhere else.

**So the vendor `.hex` at `0x7F000` is not padding.** This contradicts
`third-party/README.md`'s claim that "a `.hex` file's contents there are
padding". That claim is wrong as stated (HIGH), and this is a correction to
existing repo documentation.

### What lives there normally: HIGH for existence, MEDIUM for content

In the factory dump, `0x07F000`–`0x07F0FF` holds 256 bytes of card-written
parameter data and `0x07F100`–`0x07FFFF` is erased:

```
0007f000: 0000 0000 0000 0080 0040 0000 0000 0000
0007f040: 0000 00ff ffff ffff ffff ff00 0228 0000
0007f050: 0001 8001 0000 0000 0000 ffff ffff ffff
0007f070: 0000 0000 ff00 0000 0000 0000 0000 0072
0007f080: 0eff ffff ...
0007f0c0: ffff ffff ffff ffff ffff ffff ffff 720e
0007f0d0: 720e 0d91 0d91 0d91 0002 0002 0002 0100
0007f0f0: 0080 8080 3200 1f00 0000 0000 0007 ff00
```

`00 80 00 40` at `0x7F007` is **128 × 64**, the screen size. The repeated
`720E` / `0D91` / `0002` triples and the mixed `0x00`/`0xFF` field structure
fit the "screen-size / geometry record" description in
`docs/compiled-image-format.md` ("Page 0xF0 is EEPROM-backed … and not part of
the image"). The individual fields were not decoded.

### Why our dump reads erased (HIGH)

`primary-after-restore.bin` differs from 10.81 in **exactly one contiguous
span**: `0x07F000`–`0x07FFFF`, all `0xFF`. That is one full 4 KB flash sector.

The coherent reading: our restore **erased** the sector (a block/sector erase
reached it) but the **page writes into it were refused or redirected**, which
is exactly the behaviour `config-protocol.md` §22.4 describes ("redirected to
a small EEPROM; no flash write reaches it"; "the host never writes `0x07F000`
directly; the card's firmware does").

A "read always returns `0xFF`" artefact is **ruled out**, because the same
read path returned real data at `0x7F000` in `primary-region.bin`.

### Where the reported version string comes from: NOT RESOLVED, but narrowed

* Not an ASCII string: the only printable strings in any image are the
  Lattice header.
* Not USERCODE: command `0xC2` at `0x08E408` encodes `0x00000000` in all five
  vendor images **and** all three dumps.
* Not a fixed-offset literal: all five images were searched for their own
  version encoded as `(maj,min)`, `(min,maj)` and `maj·100+min`, big- and
  little-endian; the intersection of hit offsets across the five is **empty**.

`GetRCVTypeVersionDesp` formats `%d.%02d` from receiver-info reply bytes
`+0x10`/`+0x14`, so the number is produced by the **running gateware** as a
register value, synthesised into fabric LUTs, scrambled by placement, and not
recoverable by byte search. The version the card reports is therefore the
version of whichever bitstream is **actually configured**.

## 5. The failing frame CRCs

### `primary-after-restore.bin`: exactly 55 failing frames (HIGH)

Indices **6750–6804**, contiguous, occupying flash `0x07EFC0`–`0x08004A`.

All five vendor `.hex` files and `golden-bank.bin` have **zero** failures with
the same checker, so the CRC model is sound.

**Cause: coincident with the carved-out sector (HIGH).** The set of frames
that *overlap* `[0x7F000, 0x7FFFF]` is precisely 6750..6804: count 55, span
`0x7EFC0`–`0x8004A`. Identical set. The first and last frames straddle the
sector boundary (frame 6750 loses its last 13 bytes; frame 6804 its first 2).

Not misalignment (delta 0 is proven and the other 7507 frames pass). Not a
host/card write collision beyond this one sector.

### `primary-region.bin`: 4113 failing frames (HIGH)

Indices 2548–6804, spanning `0x2FFDE`–`0x8004A`. The frames overlapping the
reserved span `0x30000`–`0x7FFFF` number 4257; **every failing frame is in
that set**, and the 144 that "pass" are all 78 bytes of `0x00` (CRC-16 with
init 0 over zeros is zero, so an all-zero frame trivially validates).

So **100 % of the frames covering `0x30000`–`0x7FFFF` are corrupt as
bitstream, and 0 % outside it.** Clustered in exactly one region.

### Can the card boot this? The important, partly unresolved bit

The data *outside* the reserved span is byte-identical to 10.81, so the
primary bank is unambiguously a 10.81 install (§5.1). But when
`primary-region.bin` was taken the primary bank was a bitstream with 4113 bad
frames, and the card ran and reported 10.81. Two readings survive:

**(A) The card boots the golden bank.** ECP5 dual-boot with a golden image at
block 0x20 is the standard arrangement, and `golden-bank.bin` is a complete
valid bitstream.
*Against it:* golden's 2304-byte EBR init block is byte-identical to
13.39/9.53/6.69/16.53 and **differs from 10.81** (10.81 has the known
five-entry-longer prologue; see [block-ram.md](block-ram.md)), so golden is
very unlikely to be a 10.81 build, yet the card reported 10.81. Also neither
bank contains a second `BD B3` preamble or any jump command, so there is no
in-bitstream multiboot redirect; any fallback would have to be device- or
board-level.

**(B) `0x030000`–`0x07FFFF` is not the boot flash.** The card's flash-access
firmware redirects host reads/writes in that range to a separate parameter
store, as it is known to do for `0x07F000`. Under this reading the
real boot flash holds a contiguous intact 10.81, the card boots and reports
10.81, host writes to blocks 0x03–0x07 "succeed" into the parameter store, and
blocks 0x00–0x02 / 0x08–0x0A are write-protected. Every observation fits,
including `primary-after-restore.bin` reading back 10.81's bytes in 0x03–0x07
after we wrote them there.

**Choosing between (A) and (B): NOT RESOLVED.**

**A third reading is ruled out (HIGH).** `third-party/README.md`
asserts that the loader *skips* `0x030000`–`0x07FFFF`. It cannot: skipping
320 KB out of a single continuous `LSC_PROG_INCR_RTI` of 7562 frames is not
expressible in this bitstream format, the frames there are real CRC-valid
frame data, and there is no jump command anywhere. **The README's "the
bitstream is not contiguous / those contents are padding" is wrong as stated,
even though its practical rules about which regions the host may write are
correct.**

### ECP5 behaviour on a frame CRC mismatch: MEDIUM, flagged as unverified

Understanding: the device raises the CRC-error flag in the status register,
aborts configuration and leaves `DONE` deasserted; there is a
bitstream/control-register option to disable CRC checking; and ECP5 supports
dual-boot fallback to a golden pattern on a failed primary.

**Not confident about** (i) whether the control-register value used here
(`0x40000020`, or `0x40000000` in 6.69) disables CRC checking, or (ii) whether
ECP5 falls back to golden automatically without an explicit jump or
`SPI_MODE` setting. Both need the ECP5 sysCONFIG usage guide to settle, and
both bear directly on the (A)-vs-(B) question.

### 5.1 Which firmware is installed (HIGH)

**Both primary dumps are `E320_PCB6.0_PWM_FPGA10.81_20230907.hex`**, confirmed
three independent ways: the header date `Thu Sep 07 15:47:58 2023` matches
only 10.81; per-block match is exactly `1.000000` for every block outside the
reserved span; and 10.81's uniquely different EBR init block is present in
both dumps.

Frame-data match percentages:

| dump | 13.39 | 10.81 | 9.53 | 6.69 | 16.53 |
|---|---|---|---|---|---|
| `primary-region` | 10.82 % | **45.12 %** | 10.93 % | 10.62 % | 10.56 % |
| `primary-after-restore` | 18.84 % | **99.31 %** | 18.57 % | 18.72 % | 18.02 % |
| `golden-bank` | 21.05 % | 19.07 % | 20.02 % | 20.72 % | 19.59 % |

`primary-region`'s 45 % is depressed purely by the 320 KB config overlay;
restricted to blocks 0x00–0x02 and 0x08–0x0A it is 100.0000 %.
`primary-after-restore`'s 99.31 % gap is the 4042 erased bytes at `0x7F000`.
The ~18–21 % band is the chance floor for two unrelated 25F bitstreams, so
**`golden-bank.bin` matches none of the five and is a build we do not have.**

> **This matters for everything else in `docs/fpga/`.** Most of the analysis
> targets 16.53 because that is the firmware the project intends to run, but
> **the card as dumped is running 10.81**. Check which image a claim refers to
> before acting on it.

## 6. The compiled boot image sits at absolute flash `0x070000` (HIGH)

Every region in `docs/compiled-image-format.md` maps to
`0x070000 + image_offset`, and **every erased hole the doc predicts is present
at exactly the predicted address**:

| image offset | absolute flash | content | verified |
|---|---|---|---|
| `0x0000` | `0x070000` | basic-parameter pack body | present, `a8 ff ff ff 20 80 02 10 …` |
| `0x0100` | `0x070100` | void table | present (zeros) |
| `0x0500` | `0x070500` | data-swap pack | present |
| `0x0600` | `0x070600` | module-position pack | present |
| `0x0900` | `0x070900` | chip-register block | **erased** ✓ predicted |
| `0x0A00` | `0x070A00` | current segment | present (zeros) |
| `0x0C00` | `0x070C00` | current exchange | present |
| `0x0D00` | `0x070D00` | (unmapped) | **erased** ✓ predicted |
| `0x1000` | `0x071000` | void-line packs 0–1 | present (zeros) |
| `0x1800` | `0x071800` | anti-void-line packs 0–3 | present, ends `0x0727FE` |
| `0x2800` | `0x072800` | (unmapped) | **erased to `0x072FFF`** ✓ predicted |
| `0x3000` | `0x073000` | pixel-sequence packs ×16 = the mapping | present |
| `0x6000` | `0x076000` | scan table | present |
| `0x6400` | `0x076400` | (unmapped, single scan table) | **erased to `0x0767FF`** ✓ predicted |
| `0x6800` | `0x076800` | void-line packs 2–3 | present (zeros) |
| `0x7000` | `0x077000` | anti-void packs 4–7 | present (zeros) |
| `0x8000` | `0x078000` | u32-LE length + `.rcvbp` | length `0x241F`, `.rcvbp` at `0x078004`, ends `0x07A422`, erased after ✓ |

**`build/p25-128x64-sm16269s-block7.bin` lands at absolute flash
`0x070000`** (HIGH). It is exactly `0x10000` (one 64K block) and its embedded
`.rcvbp` header sits at file offset `0x8000`, matching the documented image
offset. Against the factory dump's block 0x07 it is 82.03 % identical; the
differences are the expected geometry changes (its basic pack has
`20 80 01 10 … 00 01` where the factory has `20 80 02 10 … 00 02`, and its
embedded `.rcvbp` is `0x24E1` vs the factory's `0x241F`).
`build/p25-128x64-sm16269s-basic-pack.bin` is the first `0x100` bytes of that
block, i.e. flash `0x070000`–`0x0700FF`.

## 7. Block 0x0B: the module mapping table (HIGH)

`primary-region.bin`'s extra `0x10000` is block 0x0B.
`0x0B0000`–`0x0B00FF` holds a 256-byte **module mapping table** page: magic
`11 22 33 44 55 66 47`, then u32 fields `04 00 00 00 / 23 00 00 00 /
9C 00 00 00`, then a **zlib stream at `0x0B002A` that inflates to 156 bytes**.
`0x0B0100`–`0x0BFFFF` is erased.

This is exactly the "addrHi `0x0b` = module mapping table" entry in §22.4 of
the protocol doc, and the **first physical confirmation of that table.**

## 8. Unidentified data: NOT RESOLVED

* `0x030000`–`0x033FFF`: 4096 × 4-byte BE entries, 4091 of them `FFFFFF00`,
  4 `FFFFFF80`, 1 `FFFFFF40`. The shape of a 4096-entry gamma or calibration
  LUT, but with almost no information in it.
* `0x040000`–`0x04FFFF`: 64 KB of the constant word `99 99 99 08`.

Neither corresponds to any region in `compiled-image-format.md`, and blocks
0x03/0x04 are not assigned in §22.4 of the protocol doc.
`config-protocol.md` states block 0x03 "reads 0xFF per your scan", true of
`0x034000`–`0x03FFFF` but **not** of `0x030000`–`0x033FFF`, so that earlier
scan was partial.
