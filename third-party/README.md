# Third-party material

Vendor firmware builds (`firmware/`), `.rcvbp` configs (`configs/`), and
datasheets (`datasheets/`). `configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`
is the reference file the tests compare against: its records match the card's
day-one flash (`crates/e120-rcvbp/tests/factory.rs`); where it came from is
not recorded, and its name says a 256x384 wall, which is not this bench.
`configs/donor-*.rcvbp` is our own construction, not vendor material. Everything
else here is vendor material, and none of it is ever executed.

## Firmware archive

Colorlight ships a **different FPGA bitstream per LED driver chip**. There is no
runtime setting for this: the driver-chip protocol is in gateware, so no
configuration will make a card drive a panel its firmware was not built for.

The files are archived here because they are hard to find and Colorlight
rotates its download URLs.

## Naming

`E320_PCB<rev>_<VARIANT>_FPGA<version>_<date>[_<chips>].hex`

`E320` is a **platform name, not a product name**. Colorlight builds one
gateware line for the whole "Classic Receiving Card" family (E80, E120,
5A-75B, 5A-75E and E320) and names every build after the E320. Its E120
download page links these same files, with anchor text reading
"E120/E80 V9.53（PWM）". This card's factory flash is byte-identical
to `E320_PCB6.0_PWM_FPGA10.81_20230907.hex`.

Variants seen across the range: `Normal` (plain shift-register drivers), `PWM`
(driver ICs with internal PWM engines), `LS0allDA` (the LS chip family), and
per-chip `Custom Series` builds.

## firmware/

| File | Notes |
|---|---|
| `E320_PCB6.0_PWM_FPGA10.81_20230907.hex` | **This card's factory image.** Filed by Colorlight under `Custom Series/DP3153/Legacy Versions/`: PWM-family gateware built for the **DP3153** driver chip. |
| `E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex` | Build for **SM16269SH / SM16386S**, the closest published match to this project's SM16269S panel. Checked: correct IDCODE, all 7562 frames CRC-valid. |
| `E320_PCB6.0_PWM_FPGA9.53_20221031.hex` | Generic 2022 PWM build, linked from the E120/E80 download page. |
| `E320_PCB6.0_Normal_FPGA13.39_20221101.hex` | Normal-family build with a **different image format**: declared length `0x0B0080` with its end marker at `0xB007B`, where PWM/LS0allDA declare `0x0B0000` with the marker at `0xAFFFB`. |
| `E320_PCB6.1_LS0allDA_FPGA6.69_20220907.hex` | LS chip family. |

Source: Colorlight's own download pages and their per-driver-IC library at
`ledincloud.com/resouce/Update Program/Colorlight/receiving card/`.

## Flash layout

```
0x000000-0x02FFFF  bitstream, part 1 -- host CANNOT write; only the card can
0x030000-0x07FFFF  RESERVED          -- not part of the loadable image
0x070000-0x07AFFF    the card's .rcvbp configuration lives here
0x07F000-0x07FFFF    redirected to a small EEPROM; no flash write reaches it
0x080000-0x0AFFFF  bitstream, part 2 -- host CANNOT write; only the card can
0x200000-0x2AFFFF  golden/backup image
```

**The bitstream is not contiguous.** It occupies 0x000000-0x02FFFF and
0x080000-0x0AFFFF only; the 320KB between them is reserved for configuration
and is never loaded. The card's own upgrade path programs exactly those two
regions and skips the middle, and a `.hex` file's contents there are padding.

The card shipped running firmware 10.81 while its flash had *holes* at
0x034000-0x040000 and 0x050000-0x070000 and its configuration written over
0x070000: a "corrupt" image by any contiguous reading, and it configured the
FPGA fine. If an upgrade leaves 0x030000-0x07FFFF looking stale, that is
correct behaviour, not a failed write.

The two write paths cover different regions:

| region | host direct write | card SDRAM upgrade |
|---|---|---|
| 0x000000-0x02FFFF | refused | **works** |
| 0x030000-0x07FFFF | works | skipped (by design) |
| 0x080000-0x0AFFFF | refused | **works** |

**Only the SDRAM path can install firmware.** The direct path reaches only
the reserved region, so flashing firmware with it produces a mixed bank.

## Writing firmware

Blocks are erased and written with the ordinary flash frames, but **only after
unlocking the program region**:

1. `set_program_writable(true)`: type `0x2300`, `0xFF` at payload+3.
   The builder negates the flag, so enable is `0xFF`, not `0x01`.
2. Erase each block: type `0x0600`, opcode `0x23`.
3. Write each 256-byte page: type `0x0600`, opcode `0x85`.
4. `set_program_writable(false)` to relock.

Without step 1, erases and writes are silently ignored and report no error.
Blocks 0x00–0x02 refuse to erase even unlocked.
