# Firmware archive

Colorlight ships a **different FPGA bitstream per LED driver chip**. There is no
runtime setting for this — the driver-chip protocol is implemented in gateware,
which is why no amount of configuration will make a card drive a panel its
firmware was not built for.

Everything here is archived because these files are hard to find and Colorlight
rotates their download URLs.

## Naming

`E320_PCB<rev>_<VARIANT>_FPGA<version>_<date>[_<chips>].hex`

`E320` is a **platform name, not a product name**. Colorlight builds one
gateware line for the whole "Classic Receiving Card" family — E80, E120,
5A-75B, 5A-75E and E320 — and names every build after the E320. Their E120
download page links these same files, with anchor text reading
"E120/E80 V9.53（PWM）". Proven here: this card's factory flash is byte-identical
to `E320_PCB6.0_PWM_FPGA10.81_20230907.hex`.

Variants seen across the range: `Normal` (plain shift-register drivers), `PWM`
(driver ICs with internal PWM engines), `LS0allDA` (the LS chip family), and
per-chip `Custom Series` builds.

## images/

| File | Notes |
|---|---|
| `E320_PCB6.0_PWM_FPGA10.81_20230907.hex` | **This card's factory image.** Filed by Colorlight under `Custom Series/DP3153/Legacy Versions/` — PWM-family gateware built for the **DP3153** driver chip. |
| `E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex` | Build for **SM16269SH / SM16386S**. The closest published match to this project's SM16269S panel. Validated: correct IDCODE, all 7562 frames CRC-valid. |
| `E320_PCB6.0_PWM_FPGA9.53_20221031.hex` | Generic 2022 PWM build, linked from the E120/E80 download page. |
| `E320_PCB6.0_Normal_FPGA13.39_20221101.hex` | Normal-family build. Note it uses a **different image format** — declared length `0x0B0080` with its end marker at `0xB007B`, where PWM/LS0allDA declare `0x0B0000` with the marker at `0xAFFFB`. |
| `E320_PCB6.1_LS0allDA_FPGA6.69_20220907.hex` | LS chip family. |

Source: Colorlight's own download pages and their per-driver-IC library at
`ledincloud.com/resouce/Update Program/Colorlight/receiving card/`.

## card-dumps/

Read out of this card's SPI flash over Ethernet. Keep these — they are the only
copy of what this specific card shipped with.

| File | Notes |
|---|---|
| `primary-region.bin` | Blocks 0x00–0x0B as first found. Matches the factory image at 0x00000–0x30000 and 0x80000–0xB0000, but config had been written at 0x070000 **over the bitstream**, so the primary was never a loadable image on this card. |
| `golden-bank.bin` | Blocks 0x20–0x2A, the golden/backup bitstream, build dated Jul 2022. Untouched. |
| `primary-after-restore.bin` | The primary after writing the factory image back. 4042 bytes short of the file, all in `0x07f000–0x07ffff`, a page the card refuses to write. |

## Flash layout

```
0x000000-0x02FFFF  primary image, first 3 blocks -- WRITE PROTECTED, will not erase
0x030000-0x0AFFFF  primary image, remainder      -- writable
0x070000-0x07AFFF  where the card stores its .rcvbp configuration
0x07F000-0x07FFFF  a page the card will not write by any route we found
0x200000-0x2AFFFF  golden/backup image
```

## Writing firmware

Blocks are erased and written with the ordinary flash frames, but **only after
unlocking the program region**, which is the step that is easy to miss:

1. `set_program_writable(true)` — type `0x2300`, `0xFF` at payload+3.
   The builder negates the flag, so enable is `0xFF`, not `0x01`.
2. Erase each block — type `0x0600`, opcode `0x23`.
3. Write each 256-byte page — type `0x0600`, opcode `0x85`.
4. `set_program_writable(false)` to relock.

Without step 1, erases and writes are silently ignored and report no error.
Blocks 0x00–0x02 refuse to erase even unlocked.
