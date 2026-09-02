# The bitstream container

What is needed to parse, validate and rebuild the vendor firmware images in
`third-party/firmware/`. A value marked "inferred" rests on one stated
assumption; "not resolved" means not determined.

## 1. The part

All five `.hex` files are raw Lattice Diamond `.bit` bitstreams for a
Lattice ECP5 `LFE5U-25F-6CABGA256`, IDCODE `0x41111043`. The `.hex`
extension is a misnomer: the files are binary, not Intel HEX.

Two independent sources:

1. The ASCII header states `Part: LFE5U-25F-6CABGA256` and
   `Architecture: sa5p00` in plain text.
2. The command stream contains a `VERIFY_ID` command whose operand is
   `0x41111043`, the ECP5 25F IDCODE. `ecpunpack --idcode 0x41111043`
   accepts the file and decodes it against the LFE5U-25F database without a
   single unknown tile.

Every file is exactly 721 024 bytes (`0xB0080`).

## 2. ASCII header

342 bytes, starting `\xFF\x00`, then newline-separated `Key: value` lines,
terminated by `\x00\xFF`. Verbatim for 16.53:

```
Lattice Semiconductor Corporation Bitstream
Version:         Diamond (64-bit) 3.10.2.115
Bitstream Status: Final Version 10.25
Design name: lattice_lhf_lattice_lhf.ncd
Architecture: sa5p00
Part: LFE5U-25F-6CABGA256
Date: Wed Dec 27 13:00:41 2023
Rows: 7562
Cols: 592
Bits: 4476704
Readback:     Off
Security:     Off
Bitstream CRC: 0x3474
```

Header fields across the five images:

| image | Diamond version | Bitstream Status | Date |
|---|---|---|---|
| `E320_PCB6.1_LS0allDA_FPGA6.69` | 3.10.3.144 | Final Version 10.27 | Wed Sep 07 12:38:29 2022 |
| `E320_PCB6.0_PWM_FPGA9.53` | 3.10.3.144 | Final Version 10.27 | Mon Oct 31 15:33:32 2022 |
| `E320_PCB6.0_Normal_FPGA13.39` | 3.10.3.144 | Final Version 10.27 | Sat Nov 12 16:47:59 2022 |
| `E320_PCB6.0_PWM_FPGA10.81` | 3.10.3.144 | Final Version 10.27 | Thu Sep 07 15:47:58 2023 |
| `E320_PWM_FPGA16.53_..._SM16386S_SM16269SH` | 3.10.2.115 | Final Version 10.25 | Wed Dec 27 13:00:41 2023 |

Field meanings:

* `Rows / Cols / Bits` are not device geometry. `Rows` is the frame count
  (7562), `Cols` the frame width in bits (592 = 74 bytes), and
  `7562 x 592 = 4 476 704` = `Bits`.
* `Design name: lattice_lhf_lattice_lhf.ncd` is identical in all five images
  and carries no product information. It is not "E120" or "E320".
* `Bitstream CRC: 0x3474` is identical in all five images although the
  contents differ completely, so it is not a checksum of the bitstream. What
  it is: not resolved.
* The header date does not follow the version-number order. 10.81 is dated
  2023-09-07, after 13.39 (2022-11-12). See [version-diff.md](version-diff.md).
* 16.53 was built with an older Diamond than the other four.

<a id="3-command-stream-high"></a>
## 3. Command stream

Offsets are identical in all five images.

| Offset | Bytes | Command |
|---|---|---|
| `0x156` | `FF FF` | fill |
| `0x158` | `BD B3` | preamble |
| `0x15A` | `FF FF FF FF` | fill / dummy |
| `0x15E` | `3B 00 00 00` | `LSC_RESET_CRC`, zeroes the running CRC |
| `0x162` | `E2 00 00 00 41 11 10 43` | `VERIFY_ID`, operand = IDCODE `0x41111043` |
| `0x16A` | `22 00 00 00 40 00 00 20` | control register 0 write, value `0x40000020` |
| `0x172` | `46 00 00 00` | `LSC_INIT_ADDRESS` |
| `0x176` | `82 91 1D 8A` | `LSC_PROG_INCR_RTI`, flags `0x91`, frame count `0x1D8A` = 7562 |
| `0x17A` | 582 274 | frame data (7562 x 77) |
| `0x8E3FC` | `FF`... | pad |
| `0x8E408` | `C2 80 00 00 00 00 00 00` | `ISC_PROGRAM_USERCODE`, USERCODE `0x00000000` |
| `0x8E410` | `88 88` | pad/fill |
| `0x8E412` | `F6 00 00 00 00 00 18 00` | `LSC_EBR_ADDRESS`, address `0x1800` |
| `0x8E41A` | `B2 D0 01 00` | `LSC_EBR_WRITE` |
| `0x8E41E` | 2304 | EBR init payload: 2048 nine-bit words |
| `0x8ED1E` | `B8 28` | pad/fill |
| `0x8ED20` | `5E 00 00 00` | program `DONE` |
| `0x8ED24` | `FF`... | pad to `0xAFFFB` |
| `0xAFFFC` | `00 00 00 01 E0 89 5B A0` | 8-byte trailer |

The only difference between images at this level: the control-register
operand at `0x16A` is `0x40000000` in the 6.69 (LS0allDA / PCB 6.1) image
and `0x40000020` in the other four. Bit 5 of ECP5 control register 0 is in
the SPI-mode / `MSPI` area of the register; its exact meaning here is not
resolved.

### Trailer

The trailer at the end of the file is what makes `ecpunpack` abort
([decode-method.md §2](decode-method.md#2-bitstream-to-text-config)). Whether
it is a Colorlight-added image descriptor rather than part of the Lattice
format is not resolved.

It is per-image, not universal. 16.53's is `00 00 00 01 E0 89 5B A0`;
10.81's is `00 00 00 01 C5 99 12 FD`. The Normal 13.39 image is a different
container: its `0xFF` run continues to `0xB007A` and its marker sits at
`0xB007B`, so it declares length `0x0B0080` rather than `0x0B0000`. The
first four bytes `00 00 00 01` are common; the last four differ per image.
Inferred: a checksum or build stamp.

## 4. Frame geometry and CRC

Each of the 7562 frames is 77 bytes:

```
74 bytes frame data | 2 bytes CRC, big-endian | 1 byte 0xFF inter-frame dummy
```

The CRC is CRC-16, polynomial `0x8005`, initial value 0, MSB-first, no
input or output reflection, no final XOR:

```python
def crc16(data, crc=0):
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x8005) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc
```

The CRC accumulates over the command stream since the last CRC reset, and
resets after each frame's CRC field:

* Frame 0 covers everything from `0x162` (immediately after
  `LSC_RESET_CRC`) through the frame's own 74 data bytes.
* Frames 1..7561 cover the preceding frame's `0xFF` dummy byte followed by
  the 74 data bytes.

With that model all 7562 frames validate in all five vendor images.

Frame 0 does not validate with an empty prefix: its stored CRC is `0x2C8E`
and `crc16(frame0_data)` is `0x2E47`. Scanning start offsets, only starts in
`0x15F..0x162` reproduce `0x2C8E`, and the bytes at `0x15F..0x161` are zero
and so do not affect the result. Independently, `crc16(frame0_data,
init=0xCEF5)` also gives `0x2C8E`, and `0xCEF5` is the running CRC after the
header commands.

The frame area ends at `0x17A + 7562 * 77 = 0x8E3FC`. `0x8ED24` is the end of
the EBR block, not of the frame area.

## 5. Writing a parser

```python
HDR   = 342
PRE   = 0x158     # BD B3
FSTART= 0x17A
NF    = 7562
FBYTES= 74

def frames(path):
    d = open(path, 'rb').read()
    assert d[PRE:PRE+2] == b'\xbd\xb3'
    p, prev = FSTART, d[0x162:FSTART]      # frame 0 prefix
    for i in range(NF):
        data = d[p:p+FBYTES]
        crc  = (d[p+FBYTES] << 8) | d[p+FBYTES+1]
        dummy= d[p+FBYTES+2]
        assert dummy == 0xFF
        assert crc16(prev + data) == crc, i
        prev = bytes([dummy])
        p += 77
    return p                                # 0x8E3FC
```

To rebuild an image after editing frame data: recompute each frame's CRC
with the running-prefix rule above, leave the header and command stream
untouched, and keep the padding and the 8-byte trailer. The ASCII header's
`Bitstream CRC` does not need updating (it is not a content checksum, §2).

<a id="6-the-word-bit-order-trap-high-that-it-exists"></a>
## 6. The `word:` bit-order trap

Not part of the container, but it affects every further decode. prjtrellis
writes multi-bit tile settings as `word: NAME <bitstring>`, and the bit
order is set per field by the database, not globally.

* The PLL's dividers and manufacturing constants are MSB-first: only that
  reading gives `MFG_GMC_TEST = 14`, `MFG_GMCREF_SEL = 2`, `ICP_CURRENT = 5`
  (Lattice's standard values), and only that reading yields a physically
  possible VCO frequency. See
  [resources.md §3](resources.md#3-clocking).
* `EBRn.WID` is not a usable LSB-first calibrator, although `110000000`
  reads as 3 and matches `.bram_init 3`: across 54 EBRs the field takes only
  three values (`100000000`, `110000000`, `100011111`), so it is not a clean
  index and the agreement may be coincidence.

Check the field-to-frame-bit mapping in
`$(brew --prefix prjtrellis)/share/trellis/database/ECP5/tiledata/<TILE>/bits.db`
before relying on a decoded value.

## 7. `BASE_TYPE` names are not IO standards

See [pinout.md](pinout.md#the-base_type-trap). prjtrellis's
`PIOx.BASE_TYPE` enum is degenerate: many IO standards share one bit pattern
and prjtrellis prints the alphabetically last name that matches. On this
part every bank is 3.3 V, so every `SSTL18` / `SSTL15` label in the decode
is an artefact.
