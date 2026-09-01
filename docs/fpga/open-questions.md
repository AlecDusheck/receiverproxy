# Open questions

Everything in `docs/fpga/` that is **NOT RESOLVED**, with what would settle
each. Ordered by how much it would change what we do next.

Negative results are recorded here too — knowing that something was searched
for and not found is worth as much as finding it, and re-running a dead search
is the most expensive mistake available.

---

## Tier 1 — blocks getting a picture on the panel

### 1.1 Why does the panel not render content?

**Status:** NOT RESOLVED. Ranked hypotheses with per-hypothesis experiments
are in [output-stage.md](output-stage.md#ranked-hypotheses-for-the-panel-not-rendering).

**What would settle it, in order:**

1. Flash `build/p25-128x64-sm16269s-block7.bin`, fix the screen-size record,
   `reload-params --full`, `send-params` — the current generated config has
   **never actually been on the card**; every scrambled-content result
   predates the corrected serial clock, scan-line length, module positions and
   double latch.
2. Kill every background `e120 … fill --hold` streamer, confirm the wire is
   quiet, then **press the physical test button** on the card. That bypasses
   the host, the Ethernet stack and the `0x33` command path entirely. If the
   button lights the panel, the output stage is proven good and everything
   left is the data path.
3. Send **exactly one lit pixel** at (0,0). A scrambled raster and a missing
   raster are indistinguishable under a uniform fill and completely different
   under one pixel.

### 1.2 What does the card's test-pattern selector byte mean?

**Status:** NOT RESOLVED. The frame is right (`33 00`, `0x09` at payload+5,
selector at payload+6, 279 bytes — matches `SetRcvCardTestMode` @ `0x3d54e0`
exactly). The **enum is not recoverable**: it lives in the UI layer, and
`ScrnTest.dll` yields only `NORMAL`/`RED`-family strings with no numeric
mapping.

This is the single most important ambiguity in the whole picture, because
"the card's own test pattern does not render either" is the fact that rules
out our pixel data — and it may instead be an artefact of a concurrent
streamer.

**What would settle it:** the sweep in 1.1 step 2, with the ammeter logging
and nothing else on the wire.

### 1.3 Which driver-chip ids does the gateware recognise?

**Status:** NOT RESOLVED as a set. The bench strongly indicates **`0x014C` is
correct and `0x0214` is not** (MEDIUM-HIGH — see
[chip-id.md](chip-id.md#8-what-this-means-for-the-bench)), but the full set is
unknown.

**Ruled out — do not repeat these searches:** 16-bit and 8-bit
compare-to-constant in LUT4s, the `0xFE` escape test, 4-to-16 decoders on a
chip-id nibble, CCU2 carry-chain compare-to-constant, chip-id values in the
microcode ROM, and any constant present in 16.53 but not the older images.
**The positive control failed too** — the Ethernet SFD `0xD5` and the
ethertype are equally invisible — so *this design does not build constant
comparisons out of LUT4s at all*. Searching harder cannot work.

**What would settle it:** the empirical sweep (`scripts/chipsweep.sh` with
`scripts/panel-score.py`). Cheap, because the id is excluded from the pack
CRC-32.

### 1.4 Is the data-swap / lane map identity correct for 1/16 on this module?

**Status:** NOT RESOLVED. Our generator writes identity ramps and the
seller's file regenerates byte-exactly from that, but `docs/rcvbp-format.md`
records that swap block 0 was **wholly reordered** between 32S and 64S
variants of the same module, so it is scan-dependent.

**What would settle it:** a vendor `.rcvbp` for a 1/16 128×64 module of this
family with a non-identity block 0 — or, failing that, bisecting the block on
the bench after hypotheses 1–5 in [output-stage.md](output-stage.md) are
eliminated.

### 1.5 Is the serial clock 8 or 15?

**Status:** NOT RESOLVED. `config/panels/*.toml` carries 8 (inherited from the
seller's wall config); `config/chips/sm16269.toml` gives the vendor default
for this chip as 15. The pack carries the value three times (`+0x09`, `+0x2C`,
`+0x2E`) and it also feeds the scan-table line time.

**What would settle it:** a one-line change and a photograph. RAM-only, no
flash write.

---

## Tier 2 — would materially advance the gateware understanding

### 2.1 Which pins carry which HUB75 signal?

**Status:** NOT RESOLVED. ~147 LED-side pins are classified by direction and
electrical class ([pinout.md](pinout.md)), but nothing in the bitstream ties a
pad to a connector, and 147 does not factor cleanly into 12 HUB75E ports under
any obvious sharing scheme.

**Best lead:** `IOLOGIC*.MODE = IREG_OREG` appears **96 times** in the Normal
13.39 and LS0allDA 6.69 builds and only 10 times in the PWM builds.
**96 = 32 serial RGB groups × 3 colour lines**, exactly the E120 spec's "32
groups of serial RGB data". If that holds, the 96 IOLOGIC sites in 13.39 are
the RGB data pins, and the same pads can then be found in 16.53.

**What else would settle it:** continuity-buzzing the PCB from the hub
connector to the BGA, or a clear photo of the connector traced out.

### 2.2 What are the 34 bidirectional pins?

**Status:** NOT RESOLVED. They are real (out-enable driven from fabric,
`HYSTERESIS ON` input buffers), and 20 of them share a single OE flip-flop
`Q2_SLICE@(25,2)`. Readback from the LED driver chain is *plausible* given
SM16386S/SM16269SH have status/error readback, but that is speculation.

**What would settle it:** a scope on the hub connector during a chip-register
write, watching for the card driving then releasing a line.

### 2.3 What does the microcode ROM configure?

**Status:** NOT RESOLVED. 351 used entries of 512, 5-bit opcode + 16-bit
immediate, addressing spaces `0x0Axx`–`0x0Dxx`, `0x80xx`–`0x87xx`, `0xA0xx`,
`0xB8xx`. Byte-identical across four of the five builds.

**Ruled out:** gamma/brightness LUT (HIGH), Lattice Mico8 (MEDIUM-HIGH),
8051/PicoBlaze (MEDIUM), any chip id (HIGH), scan table (MEDIUM).

**What would settle it:** netlist recovery around the ROM's read port —
specifically what the 16-bit immediate fans out to and what decodes the 5-bit
opcode.

### 2.4 Where is the parameter store?

**Status:** NOT RESOLVED. The 256-byte pack arrives over Ethernet and lands
somewhere — BRAM, LUT-RAM or a flop file — and it has not been located.

**Best lead:** `R27C44_Q0..Q3`, a 4-bit field with no combinational source
feeding a 10-bit already-decoded mode bundle read by 734 LUTs (1022 distinct
equivalence classes over 1024 input values, so ten *independent* flags). That
is what a parameter-store-loaded mode selector looks like.

**Why it matters:** finding it turns "which chip ids does the gateware
recognise" into "which stored byte feeds the mode selector", which is
tractable.

### 2.5 Where is gamma applied?

**Status:** NOT RESOLVED. Record 0x01 carries a γ float at `+0x01C` (2.8
here); the corpus's gamma/calibration records are all zero in an uncalibrated
profile; and there is a separate `0x85`-opcode "write gamma table" path.

**Ruled out — HIGH:** it is not a boot-time ROM. The BRAM sweep found no 256-
or 1024-point ramp anywhere in the one initialised block RAM.

**Candidate:** the 24 MULT18 / 12 ALU54 DSP blocks, which are all populated in
MAC configuration but whose operands are unknown.

### 2.6 What do the DSP blocks compute?

**Status:** NOT RESOLVED. The whole DSP row is populated as MULT18X18D
feeding ALU54B with input and pipeline registers enabled. Per-channel gamma or
brightness scaling is the obvious role in an LED controller; there is **no
evidence** for it and it is not claimed.

**What would settle it:** tracing the multiplier operand nets back to a
buffer read port and a coefficient source.

---

## Tier 3 — flash and boot

### 3.1 Does the card boot the primary bank or the golden bank?

**Status:** NOT RESOLVED, and it matters for every flashing operation.

Two readings survive the evidence
([flash-layout.md](flash-layout.md#can-the-card-boot-this--the-important-partly-unresolved-bit)):

* **(A)** The card boots the golden bank at block 0x20. *Against:* golden's
  EBR init block is not 10.81's, yet the card reports 10.81; and there is no
  jump command or second preamble anywhere.
* **(B)** `0x030000`–`0x07FFFF` is not the boot flash at all — the card's
  firmware redirects host access in that range to a separate parameter store,
  as it demonstrably does for `0x07F000`.

**Ruled out — HIGH:** the third reading, that the loader *skips*
`0x030000`–`0x07FFFF`. Skipping 320 KB out of a single continuous
`LSC_PROG_INCR_RTI` of 7562 frames is not expressible in this format, the
frames there are real CRC-valid frame data, and there is no jump command.
`third-party/README.md`'s "the bitstream is not contiguous / those contents
are padding" is **wrong as stated**, though its practical rules about which
regions the host may write are correct.

**What would settle it:** the ECP5 sysCONFIG usage guide (does control
register `0x40000020` disable CRC checking? does ECP5 fall back to golden
automatically without an explicit jump?), plus a read of the flash above
`0x2B0000`.

### 3.2 Where does the reported firmware version come from?

**Status:** NOT RESOLVED, but sharply narrowed. It is **not** an ASCII string,
**not** USERCODE (`0x00000000` in all five images and all three dumps), and
**not** a fixed-offset literal — all five images were searched for their own
version in six encodings and the intersection of hit offsets is empty.

`GetRCVTypeVersionDesp` formats `%d.%02d` from receiver-info reply bytes, so
the number is produced by the **running gateware** as a register value —
synthesised into fabric LUTs, scrambled by placement, not recoverable by byte
search.

**Consequence, and it is useful:** the version the card reports is the version
of whichever bitstream is *actually configured*. That is a live probe for 3.1.

### 3.3 What are the two unidentified flash regions?

**Status:** NOT RESOLVED.

* `0x030000`–`0x033FFF` — 4096 × 4-byte BE entries, 4091 of them `FFFFFF00`.
  The shape of a 4096-entry gamma or calibration LUT with almost no
  information in it.
* `0x040000`–`0x04FFFF` — 64 KB of the constant word `99 99 99 08`.

Neither corresponds to any region in `docs/compiled-image-format.md`, and
blocks 0x03/0x04 are unassigned in the protocol doc's §22.4.

### 3.4 What is the 8-byte end marker, and control-register bit 5?

**Status:** NOT RESOLVED. The marker at `0xAFFF8` is per-image
(`…E0 89 5B A0` for 16.53, `…C5 99 12 FD` for 10.81) and 13.39 uses a
different container length entirely. Control register 0 is `0x40000020` in
four images and `0x40000000` in 6.69.

### 3.5 What is the ASCII header's `Bitstream CRC: 0x3474`?

**Status:** NOT RESOLVED. It is identical in all five images despite
completely different contents, so it is not a content checksum.

---

## Tier 4 — nice to know

### 4.1 What is the LS0allDA firmware family?

Only the name and the resource profile are known. NOT RESOLVED.

### 4.2 Why is 10.81's ROM prologue five entries longer?

NOT RESOLVED. It is the only ROM difference among the five builds.

### 4.3 Is `CLKOS2` used at all?

The PLL enables it, but it is **not routed to any DCC**. NOT RESOLVED.

### 4.4 What is the fabric-generated global clock `BDCC0`?

A flip-flop (`Q1_SLICE@(25,48)`) re-buffered onto the global network with a
fan-out of ~660, also feeding both edge-clock trees and `DCS0`/`DCS1`.
Almost certainly the LED shift clock or a divided pixel clock — **NOT
RESOLVED**.

### 4.5 What are the six constant-strapped output pins?

`A15`, `M6`, `K12` (constant 0), `E12`, `E13` (constant 1) and one more, all
at `DRIVE 16 / SLEWRATE FAST`. HIGH that they are static level outputs;
their function is NOT RESOLVED.

### 4.6 Exact EBR instance count

Two independent passes gave 53 and 54 for 16.53. The difference is a
tile-grouping convention, not a real disagreement about utilisation. Minor,
but do not quote a precise figure without re-deriving it.
