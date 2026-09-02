# SPI flash layout

The E120's configuration flash holds the FPGA bitstream, a golden copy of it,
the compiled boot-config image, the EEPROM mirror page and the module mapping
table. This page gives the address map, the mapping from a vendor `.hex` to
flash addresses, the identity of the installed image, and the state of the
`0x07F000` page.

Evidence: three flash dumps of the card (`card-dumps/`), the five vendor
`.hex` images in `third-party/firmware/`, and a frame-CRC checker validated
against all five images. Regenerable artefacts (not kept in the repository;
see [decode-method.md](decode-method.md)): `analysis/fpga/flash-layout.txt`,
`flash-address-map.txt`, `image-identity.tsv`, `image-match-matrix.tsv`,
`failing-frames-primary-region.tsv`,
`failing-frames-primary-after-restore.tsv`, `flash-map.py`.

## 1. Alignment of a vendor `.hex` in flash

A vendor `.hex` byte 0 lands on flash byte 0 of the bank base, ASCII header
included. The delta is 0.

The 128-byte size difference between a `.hex` and a dump is not an offset. It
is trailing padding at `.hex` `0x0B0000`-`0x0B007F` that the dumps stop
short of.

Every bitstream command byte lines up at delta 0 in all three dumps: `BD B3`
at `0x158`, `0xE2` at `0x162`, `0x22` at `0x16A`, `0x82` at `0x176`. Deltas
of -128, +128, +214 and +342 match at chance level.

## 2. Dump extents

| file | size | flash range | blocks |
|---|---|---|---|
| `card-dumps/primary-region.bin` | `0xC0000` | `0x000000`-`0x0BFFFF` | 0x00-0x0B |
| `card-dumps/primary-after-restore.bin` | `0xB0000` | `0x000000`-`0x0AFFFF` | 0x00-0x0A |
| `card-dumps/golden-bank.bin` | `0xB0000` | `0x200000`-`0x2AFFFF` | golden bank at block 0x20 |

`primary-region.bin` is the factory state. `primary-after-restore.bin` is the
primary bank after the factory image was written back with host page writes.
The bank extents agree with the card's flash-read replies.

## 3. Primary bank address map (factory state)

Per-64K-block match against 10.81 is exactly `1.000000` for blocks 0x00,
0x01, 0x02, 0x08, 0x09 and 0x0A, and chance-level for 0x03-0x07. A whole-file
diff against 10.81 yields exactly two differing segments:
`0x030000`-`0x07FFFF` and `0x0B0000`-`0x0B007F`.

```
0x000000-0x000155  Lattice ASCII header, "Date: Thu Sep 07 15:47:58 2023"
0x000156-0x02FFFF  bitstream commands + frames 0..2547   == 10.81 byte-for-byte
0x030000-0x033FFF  DATA: 4096 x 4-byte BE entries.  4091 = FFFFFF00,
                   4 = FFFFFF80, 1 = FFFFFF40.  Shape of a gamma/cal LUT.
                   Content not identified.
0x034000-0x03FFFF  erased 0xFF
0x040000-0x04FFFF  DATA: 16384 x the constant word 99 99 99 08, nothing else.
                   Content not identified.
0x050000-0x06FFFF  erased 0xFF (blocks 0x05, 0x06 wholly erased)
0x070000-0x07AFFF  the compiled boot-config image  (section 9)
0x07A423-0x07EFFF  erased 0xFF
0x07F000-0x07F0FF  DATA, 256 bytes: page 0xF0, the EEPROM mirror (section 6)
0x07F100-0x07FFFF  erased 0xFF
0x080000-0x08E3FB  bitstream frame tail == 10.81
0x08E3FC-0x08ED23  USERCODE / EBR_ADDRESS 0x1800 / EBR_WRITE + 2304 B / DONE
0x08ED24-0x0AFFF7  erased 0xFF padding
0x0AFFF8-0x0AFFFF  end marker  00 00 00 01 C5 99 12 FD
0x0B0000-0x0B00FF  module mapping table page: magic 11 22 33 44 55 66 47,
                   u32 fields, zlib stream at 0x0B002A -> 156 bytes
0x0B0100-0x0BFFFF  erased 0xFF
```

### End markers

The 8-byte end marker is per-image, not universal:

| image | marker | position |
|---|---|---|
| 10.81 | `00 00 00 01 C5 99 12 FD` | `0x0AFFF8` |
| 16.53 | `00 00 00 01 E0 89 5B A0` | `0x0AFFF8` |
| 13.39 | (Normal container) | `0x0B007B` |

13.39 is a different container: its `0xFF` run continues to `0x0B007A` and
its marker sits at `0x0B007B`. The Normal family declares length `0x0B0080`
where PWM and LS0allDA declare `0x0B0000` (`third-party/README.md`).

## 4. Golden bank

Block 0x20 holds a complete, hole-free bitstream: header date
`Sat Jul 09 14:10:43 2022`, all 7562 frame CRCs valid, the only `0xFF` run is
the tail. Its frame-data match against every one of the five vendor images
is in the 19-21 % chance band (section 8), so it is a build not present in
`third-party/firmware/`. Its 2304-byte EBR init block is byte-identical to
that of 13.39, 9.53, 6.69 and 16.53 and differs from 10.81's
([block-ram.md](block-ram.md)).

`rxp firmware write` never writes the golden bank. `rxp flash snapshot`
captures it.

## 5. Regions addressed by the vendor library but not read

The vendor host library's flash routines target `0x0A0000`, `0x1C0000`,
`0x1E0000`, `0x1F0000`, `0x390000`, `0x3A0000`, `0x3B0000`, `0x3C0000`,
`0xD60000`, `0xE70000`, `0xE80000` and `0xE90000` (inferred from host
disassembly, not read on hardware). An addrHi of `0xE9` implies an address
space of at least 16 MB.

## 6. Page 0xF0: the EEPROM mirror at `0x07F000`

### Position inside the bitstream

Bitstream frame data runs `0x00017A`-`0x08E3FB`. `0x07F000` sits about 62 %
of the way through it, inside frames 6750-6804.

In the vendor `.hex`, that sector is ordinary high-entropy frame data: of the
327 680 bytes in `0x30000`-`0x7FFFF`, only 4295 are `0xFF`, all 256 byte
values occur, and the frame CRCs there are valid. The vendor `.hex` at
`0x7F000` is therefore not padding. `third-party/README.md`'s statement that
"a `.hex` file's contents there are padding" is wrong as stated.

### Contents in the factory dump

`0x07F000`-`0x07F0FF` holds 256 bytes of card-written data and
`0x07F100`-`0x07FFFF` is erased:

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

`00 80 00 40` at `0x7F007` is 128 x 64, the screen size. The page is the
card's EEPROM mirror; the record map is in
[../eeprom-map.md](../eeprom-map.md), and
[../compiled-image-format.md](../compiled-image-format.md) records that page
0xF0 is EEPROM-backed and not part of the boot image. The repeated
`720E` / `0D91` / `0002` triples are part of that map.

### State after a host restore

`primary-after-restore.bin` differs from 10.81 in exactly one contiguous
span: `0x07F000`-`0x07FFFF`, all `0xFF`, one full 4 KB flash sector. The
restore's erase reached the sector; the host page writes into it did not
land. This matches the vendor library's treatment of the page: host writes to
`0x07F000` are redirected to the EEPROM and reach flash only through the
card's own firmware. The host writes the page through the linear frame type
`0x1900`, not through block/page writes.

A "read always returns `0xFF`" artefact is ruled out: the same read path
returned the data above at `0x7F000` in `primary-region.bin`.

Erasing block 0x07 clears the mirror and, with it, the EEPROM control area:
a 256-byte screen-size write over an erased mirror persists
`startX = startY = 0xFFFF` and the card then drops every pixel
([../receiver-identity.md](../receiver-identity.md)).

## 7. Failing frame CRCs

### `primary-after-restore.bin`: 55 failing frames

Indices 6750-6804, contiguous, occupying flash `0x07EFC0`-`0x08004A`. All
five vendor `.hex` files and `golden-bank.bin` have zero failures with the
same checker.

Cause: the frames overlapping `[0x7F000, 0x7FFFF]` are exactly 6750..6804
(count 55, span `0x7EFC0`-`0x8004A`). The first and last frames straddle the
sector boundary: frame 6750 loses its last 13 bytes, frame 6804 its first 2.
Misalignment is excluded (delta 0, and the other 7507 frames pass).

### `primary-region.bin`: 4113 failing frames

Indices 2548-6804, spanning `0x2FFDE`-`0x8004A`. The frames overlapping the
span `0x30000`-`0x7FFFF` number 4257; every failing frame is in that set, and
the 144 that pass are all 78 bytes of `0x00` (CRC-16 with init 0 over zeros
is zero, so an all-zero frame validates trivially). 100 % of the frames
covering `0x30000`-`0x7FFFF` are corrupt as bitstream and 0 % outside it.

### Which bank the card boots: not resolved

The data outside `0x30000`-`0x7FFFF` is byte-identical to 10.81. When
`primary-region.bin` was taken, the primary bank held a bitstream with 4113
bad frames, and the card ran and reported 10.81. Two readings fit:

**(A) The card boots the golden bank.** ECP5 dual-boot with a golden image at
block 0x20 is the standard arrangement, and `golden-bank.bin` is a complete
valid bitstream. Against it: golden's EBR init block is not 10.81's, yet the
card reported 10.81; and neither bank contains a second `BD B3` preamble or
any jump command, so there is no in-bitstream multiboot redirect. Any fallback
would have to be device- or board-level.

**(B) `0x030000`-`0x07FFFF` is not the boot flash.** The card's flash-access
firmware redirects host reads and writes in that range to a separate
parameter store, as it does for `0x07F000`. Under this reading the boot flash
holds a contiguous intact 10.81, host writes to blocks 0x03-0x07 succeed into
the parameter store, and blocks 0x00-0x02 / 0x08-0x0A are write-protected.
This fits every observation, including `primary-after-restore.bin` reading
back 10.81's bytes in 0x03-0x07 after they were written there.

**Ruled out:** a loader that skips `0x030000`-`0x07FFFF`, as
`third-party/README.md` asserts. Skipping 320 KB out of a single continuous
`LSC_PROG_INCR_RTI` of 7562 frames is not expressible in this bitstream
format, the frames there are real CRC-valid frame data, and there is no jump
command. The README's "the bitstream is not contiguous / those contents are
padding" is wrong as stated; its practical rules about which regions the host
may write are correct.

### ECP5 behaviour on a frame CRC mismatch (inferred)

The device raises the CRC-error flag in the status register, aborts
configuration and leaves `DONE` deasserted; a bitstream/control-register
option disables CRC checking; ECP5 supports dual-boot fallback to a golden
pattern on a failed primary. Not verified from the ECP5 sysCONFIG usage guide:
(i) whether the control-register value used here (`0x40000020`, or
`0x40000000` in 6.69) disables CRC checking; (ii) whether ECP5 falls back to
golden without an explicit jump or `SPI_MODE` setting. Both bear on (A) vs (B).

## 8. Identity of the dumped firmware

Both primary dumps are `E320_PCB6.0_PWM_FPGA10.81_20230907.hex`, confirmed
three ways: the header date `Thu Sep 07 15:47:58 2023` matches only 10.81;
per-block match is exactly `1.000000` for every block outside the reserved
span; and 10.81's uniquely different EBR init block is present in both dumps.

Frame-data match percentages:

| dump | 13.39 | 10.81 | 9.53 | 6.69 | 16.53 |
|---|---|---|---|---|---|
| `primary-region` | 10.82 % | **45.12 %** | 10.93 % | 10.62 % | 10.56 % |
| `primary-after-restore` | 18.84 % | **99.31 %** | 18.57 % | 18.72 % | 18.02 % |
| `golden-bank` | 21.05 % | 19.07 % | 20.02 % | 20.72 % | 19.59 % |

`primary-region`'s 45 % is depressed by the 320 KB config overlay; restricted
to blocks 0x00-0x02 and 0x08-0x0A it is 100.0000 %.
`primary-after-restore`'s 0.69 % gap is the 4042 erased bytes at `0x7F000`.
The 18-21 % band is the chance floor for two unrelated 25F bitstreams.

The dumps were taken with 10.81 installed; the card shipped with 10.81. The
card runs 16.53, installed with `rxp provision --firmware`
([../rendering.md](../rendering.md)). `rxp discover` reports the running
version. Most of `docs/fpga/` analyses 16.53; check which image a claim
refers to before acting on it.

### Where the reported version comes from

Not an ASCII string: the only printable strings in any image are the Lattice
header. Not USERCODE: command `0xC2` at `0x08E408` encodes `0x00000000` in
all five vendor images and all three dumps. Not a fixed-offset literal: all
five images were searched for their own version encoded as `(maj,min)`,
`(min,maj)` and `maj*100+min`, big- and little-endian; the intersection of
hit offsets across the five is empty.

`GetRCVTypeVersionDesp` formats `%d.%02d` from receiver-info reply bytes
`+0x10`/`+0x14`. The number is produced by the running gateware as a
register value, synthesised into fabric LUTs, and not recoverable by byte
search. The reported version is therefore the version of whichever bitstream
is actually configured.

<a id="6-the-compiled-boot-image-sits-at-absolute-flash-0x070000-high"></a>
## 9. Compiled boot image at `0x070000`

Every region in [../compiled-image-format.md](../compiled-image-format.md)
maps to `0x070000 + image_offset`, and every erased hole the format predicts
is present at the predicted address:

| image offset | absolute flash | content | in the factory dump |
|---|---|---|---|
| `0x0000` | `0x070000` | basic-parameter pack body | present, `a8 ff ff ff 20 80 02 10 ...` |
| `0x0100` | `0x070100` | void table | present (zeros) |
| `0x0500` | `0x070500` | data-swap pack | present |
| `0x0600` | `0x070600` | module-position pack | present |
| `0x0900` | `0x070900` | chip-register block | erased, as predicted |
| `0x0A00` | `0x070A00` | current segment | present (zeros) |
| `0x0C00` | `0x070C00` | current exchange | present |
| `0x0D00` | `0x070D00` | (unmapped) | erased, as predicted |
| `0x1000` | `0x071000` | void-line packs 0-1 | present (zeros) |
| `0x1800` | `0x071800` | anti-void-line packs 0-3 | present, ends `0x0727FE` |
| `0x2800` | `0x072800` | (unmapped) | erased to `0x072FFF`, as predicted |
| `0x3000` | `0x073000` | pixel-sequence packs x16 = the mapping | present |
| `0x6000` | `0x076000` | scan table | present |
| `0x6400` | `0x076400` | (unmapped, single scan table) | erased to `0x0767FF`, as predicted |
| `0x6800` | `0x076800` | void-line packs 2-3 | present (zeros) |
| `0x7000` | `0x077000` | anti-void packs 4-7 | present (zeros) |
| `0x8000` | `0x078000` | u32-LE length + `.rcvbp` | length `0x241F`, `.rcvbp` at `0x078004`, ends `0x07A422`, erased after |

`build/p25-128x64-sm16269s-block7.bin` lands at absolute flash `0x070000`
(`rxp flash restore-block`). It is exactly `0x10000` (one 64K block) and its
embedded `.rcvbp` header sits at file offset `0x8000`. Against the factory
dump's block 0x07 it is 82.03 % identical; the differences are geometry: its
basic pack has `20 80 01 10 ... 00 01` where the factory has
`20 80 02 10 ... 00 02`, and its embedded `.rcvbp` is `0x24E1` bytes against
the factory's `0x241F`. `build/p25-128x64-sm16269s-basic-pack.bin` is the
first `0x100` bytes of that block, flash `0x070000`-`0x0700FF`.

## 10. Block 0x0B: the module mapping table

`0x0B0000`-`0x0B00FF` holds a 256-byte module mapping table page: magic
`11 22 33 44 55 66 47`, then u32 fields `04 00 00 00 / 23 00 00 00 /
9C 00 00 00`, then a zlib stream at `0x0B002A` that inflates to 156 bytes.
`0x0B0100`-`0x0BFFFF` is erased. This is the "addrHi `0x0b` = module mapping
table" entry of the vendor library's flash address table, confirmed on
hardware.

## 11. Unresolved

* Which bank the card boots, (A) or (B) in section 7.
* `0x030000`-`0x033FFF`: 4096 x 4-byte BE entries, 4091 of them `FFFFFF00`,
  4 `FFFFFF80`, 1 `FFFFFF40`. The shape of a 4096-entry gamma or calibration
  LUT with almost no information in it. Not identified.
* `0x040000`-`0x04FFFF`: 64 KB of the constant word `99 99 99 08`. Not
  identified.
* Neither region corresponds to any region of the compiled image format, and
  blocks 0x03/0x04 are not assigned in the vendor library's flash address
  table. Block 0x03 is not wholly erased: only `0x034000`-`0x03FFFF` reads
  `0xFF`.
* The meaning of control-register bit 5 (`0x40000020` vs `0x40000000`).
