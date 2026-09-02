# The E120 FPGA gateware: index

Reverse engineering of the Colorlight E120 receiving card's gateware, from
the vendor firmware images in `third-party/firmware/`, with prjtrellis. No
Lattice Diamond and no Colorlight software is executed; vendor files are read
only.

[`docs/fpga-gateware.md`](../fpga-gateware.md) is the two-page overview. This
directory is the detail, one file per subject.

## Claims and their evidence

A statement in these pages is one of three things:

| form | meaning |
|---|---|
| plain statement | read directly out of the bytes or the prjtrellis database and cross-checked |
| "inferred" | a pattern resting on one stated assumption |
| "not resolved" | not determined; what has been ruled out is stated |

A "not resolved" entry overrides any confident statement about the same
subject elsewhere.

## The files

| file | covers |
|---|---|
| [bitstream-format.md](bitstream-format.md) | The `.bit` container: part identification, the ASCII header field by field, every command opcode and operand, frame geometry, the CRC-16 and the running-prefix rule; enough to write a parser or rebuild an image. The two decode traps (`word:` bit order, `BASE_TYPE` names). |
| [decode-method.md](decode-method.md) | Reproduction: tool versions, install, the truncation `ecpunpack` needs, the `.config` text format, the pytrellis API traps, the limits of backward tracing, and the scripts that produced these pages. |
| [flash-layout.md](flash-layout.md) | How a `.hex` maps into SPI flash (delta 0), the address map, the `0x7F000` EEPROM redirect, the 55 failing frame CRCs and their cause, which firmware the dumps hold (10.81), and where the compiled boot image lands. |
| [resources.md](resources.md) | LUT / FF / BRAM / DSP utilisation, the PLL and its bit order, the reference clock pin, every global clock net and DCC, clock domains, and how the LED outputs are registered. |
| [pinout.md](pinout.md) | The 197-pin table with direction from the routing graph (not from `BASE_TYPE`), the two RGMII gigabit ports pin by pin, the SPI flash bank, the ~147 LED-side pins including the 96 RGB data lines, and the board architecture they imply. |
| [block-ram.md](block-ram.md) | The one initialised BRAM: location, size, contents, the 21-bit opcode+immediate decode, what it is and is not, and every other block RAM's mode and width. |
| [pixel-write-path.md](pixel-write-path.md) | What a type-`0x55` pixel frame meets inside the card: the decode trap for EBR pins, the two Ethernet receive FIFOs (one per PHY, the design's only clock-domain crossings), the two banked destination memories, why the packet decode is not recoverable by any LUT-constant method, and the windowing the vendor's and FPP's senders imply. |
| [parameter-path.md](parameter-path.md) | All three transports (live Ethernet, flash writes, boot read), the 41-pack real-time push, the 256-byte basic pack, what is shipped precomputed and what is derived, and every constant searched for with FOUND / NOT FOUND / NEVER SEARCHED. |
| [output-stage.md](output-stage.md) | HUB75E connector and signal set, what the SM16269 family requires, the S-PWM structure, scan handling and the OneScanLen/CardScanLen arithmetic, the scan table (its all-zero bit times are normal), the pixel mapping, the output stage in the netlist (96 RGB pins, the global blank and 2:1 source select, counter vs BRAM), and the hypotheses that preceded the bench result. |
| [version-diff.md](version-diff.md) | 16.53 vs 10.81 vs 9.53 vs 13.39 vs 6.69 at the decoded level, what separates the PWM / Normal / LS families, and why the version numbers are not one sequence. |
| [chip-protocol-microcode.md](chip-protocol-microcode.md) | Where the driver-chip serial protocol lives. The microcode ROM is not it (bit-identical across the Normal/LS/PWM split, none of the protocol constants in it); 16.53 added no output-stage logic; the 20-byte `SChipControl` block is a per-chip serial-protocol descriptor (pre-activation / register / data-latch / VSYNC LE tail lengths and two GCLK/RCLK-per-row counts), cross-checked against the vendor's 29-file corpus and three open-source driver profiles. SM16269 datasheet facts: no OE, no GCLK; pin 21 is RCLK and is the grey clock. |
| [chip-id.md](chip-id.md) | The driver-chip id: how it reaches the card, the vendor chip table, what was searched and ruled out, why the negative is credible but not absolute, the refuted lead, and what to send. |
| [open-questions.md](open-questions.md) | Everything unresolved, tiered by impact, each with what would settle it, plus the searches that cannot succeed and must not be repeated. |

## Reading order

* Getting a picture on the panel: [`docs/rendering.md`](../rendering.md)
  (the settled recipe), then [output-stage.md](output-stage.md) §6,
  [open-questions.md](open-questions.md) Tier 1, [chip-id.md](chip-id.md) §8.
* Flashing or dumping the card: [flash-layout.md](flash-layout.md), and §5.1
  there for which image each dump holds.
* Continuing the reverse engineering:
  [decode-method.md](decode-method.md), [pinout.md](pinout.md),
  [open-questions.md](open-questions.md) Tier 2.

## Raw artefacts

Pages cite artefacts under `analysis/fpga/` (pin tables, EBR maps, LUT
histograms, decode scripts) and the bench card's flash dumps under
`card-dumps/`. The `analysis/fpga/` tree is not kept in the repository; the
findings are in the pages, and [decode-method.md](decode-method.md) says how
to regenerate the artefacts. `card-dumps/` holds `primary-region.bin`,
`primary-after-restore.bin` and `golden-bank.bin`.
