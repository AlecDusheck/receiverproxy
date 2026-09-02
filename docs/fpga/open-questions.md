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

### 1.1b Does the card window `0x55` pixel packets against a cabinet position?

**Status:** NOT RESOLVED in the gateware, but it is now the leading
explanation and it has a cheap decisive test. See
[pixel-write-path.md §5](pixel-write-path.md).

The `row` and `x-offset` fields of a `0x55` packet are **absolute coordinates
in the whole virtual display** — both FPP and CLTNic broadcast one stream for
an entire wall and every receiver windows out its own rectangle. If this card's
stored window is not `(0,0) 128×64`, our rows miss.

**What would settle it:** a `0x07` discovery frame with the wire quiet, and a
dump of the `0x08` reply — `Data[21..24]` is the cabinet width and height *as
the card believes them*, `Data[38..41]` is the received-packet counter (send
exactly *K* pixel packets between two reads), `Data[2..3]` is the firmware
version. Read-only, one frame, no flash write.

**Loose end, stated honestly:** a fully-windowed-out stream should make an
all-black frame and an all-white frame look identical, and the bench says they
do not. Either the overlap is partial or a second mechanism is in play.

### 1.1c Where does a pixel byte physically go? — partly CLOSED

**CLOSED — HIGH:** every Ethernet frame enters through one of exactly two block
RAMs, `EBR@39,37` (left PHY RX clock) and `EBR@42,37` (right PHY RX clock),
1024 × 9 each, the design's only clock-domain crossings. At 1024 bytes they
cannot hold a maximum-size pixel packet, so the header is decoded and the
payload consumed **while the packet streams**.

**CLOSED — HIGH:** the only two candidate destination memories are a
**Bank A** of 8 EBRs (2048 × 9, shared `WEA = Q4@21,22`) and a **Bank B** of 12
EBRs (16 384 bits each, shared `WEA = Q4@44,26`). Both start uninitialised and
are written at run time; the same two-bank shape is in 10.81.

**REFUTED — HIGH:** the double-buffer-swap hypothesis. There is no third array
and the two banks are structurally different, so there is nowhere for an
un-swapped back buffer to live.

**CLOSED — HIGH:** the memory feeding the HUB75 pads is `EBR@4,25`
(`= MIB_R25C4/C5 EBR0`, the uninitialised `WID = 1` block of
[output-stage.md §7.4](output-stage.md#74-the-control-group-source-ram-starts-empty--high)):
`PDPW16KD`, 512 × 36, `WEAMUX = INV` so `WEA` is **tied high**, which makes
`CSA0 ← Q6@9,27` the **entire** write enable of the output-stage buffer. 512
entries is a scan/line buffer, not a frame buffer — so there is at least one
more stage between a bank and the pads.

**Still NOT RESOLVED:** which bank the raster reads, which the Ethernet writes,
and what gates either bank's write.

**Do not repeat:** any LUT-constant search for the `0x55` type byte, the
`08 88` marker, or a row-field comparator. The positive control (the Ethernet
SFD and the EtherType) already failed — this design does not build constant
comparisons out of LUT4s. Nor a shallow EBR-to-EBR dataflow search: depth 3
finds zero edges chip-wide (`negative_results_and_method.txt` N6–N9).

**Best surviving gateware lead:** forward netlist recovery of what consumes
`DOA*`/`DOB*` of `EBR@39,37` and `EBR@42,37`. That is a *localised* task in
`x 38..46, y 30..45`, the one region of the die whose function is now certain.

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

**What would settle it:** the empirical sweep (`scripts/bench.py run --boot
--spec …` per candidate id). Cheap, because the id is excluded from the pack
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

### 2.1 Which top-edge pad carries which HUB75 control signal?

**Partly CLOSED.** The 96 RGB data pins **are** identified — the `IREG_OREG`
signature in the Normal/LS builds maps exactly onto the 96 left/right-edge
pads, with zero discrepancy
([output-stage.md §7.1](output-stage.md#71-the-96-rgb-data-pins-are-identified--high),
`analysis/fpga/rgb96_pins.txt`). The control group is identified **as a
group** — the top-edge pads, sharing a global synchronous blank
(`Q4@23,18`) and a 2:1 source select (`Q5@23,18`), active-low.

**Still NOT RESOLVED:** which of those pads is A, B, C, D, E, CLK, LAT or OE,
and which pins belong to which of the twelve HUB75E connectors.

**What would settle it:** continuity-buzzing the PCB from the J1 connector to
the BGA, or a clear photo of the connector traced out. Note that a scan
address line should be driven by a small counter and CLK by a toggling flop —
`analysis/fpga/pad_driver_logic_16.53.tsv` has the per-pad driver logic to
start from.

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

**Ruled out:** LUT-RAM in 16.53 — only 18 blocks of 16×4, against 59 and 89 in
13.39 and 6.69. Too small and too fragmented to hold the tables.

**The `R27C44_Q0..Q3` lead is REFUTED — do not follow it.** It is an ordinary
8-bit CCU2 accumulator; it looked sourceless only because CCU2 carry travels
on fixed, non-configurable wires. 1012 of 6956 CCU2 LUTs in 16.53 have zero
routed inputs, so "no combinational source" is the normal appearance of every
increment stage on the die. See
[chip-id.md §6](chip-id.md#6-the-lead-that-looked-concrete--refuted).

**Best surviving lead:** the block RAM feeding the top-edge control pads is
`MIB_R25C4/C5` EBR0 = **`EBR@4,25`**, `PDPW16KD`, 512 × 36, **`WID = 1` — not
initialised at config time**, so it comes up empty and is written at run time.
That is a run-time-written table feeding the output stage directly.

**Now searchable:** `analysis/fpga/ebr_map_16.53.txt` records the driven pins,
clock, write gate and generator locations of **all 53** instantiated block
RAMs, so "which EBR holds the parameter pack" is now a question you can pose
against a table rather than against the whole die. A 256-byte pack wants a
small, singly-written, CLKOP-clocked block whose address generator is *not*
part of either large bank — several candidates in the map fit.
See [pixel-write-path.md §1](pixel-write-path.md) for why this was not
possible before (EBR pins are not set-arc sinks).

**Why it matters:** finding the store turns "which chip ids does the gateware
recognise" into "which stored byte feeds the mode selector", which is
tractable. It is also a candidate explanation for the panel scanning garbage
(§1.1) — an uninitialised RAM being scanned.

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

### 4.4 What is the fabric-generated global net `BDCC0` used for?

**Partly CLOSED.** It is **not a clock**: the design is single-clock (98.9 % of
flops on PLL CLKOP) and `G_HPBX0900` appears as `.CE` on output-stage flops, so
this net is a **clock enable**. Its specific role — presumably the LED shift or
pixel-rate gate — is NOT RESOLVED.

### 4.7 What does the 2:1 output mux select between?

One leg is a CCU2 counter (`x = 24..26, y = 7..11`), the other is block RAM
data out. **Whether that is "internal test pattern vs live pixel data" or a
within-frame command/data time-multiplex (SM16xxx configuration words vs pixel
data) is NOT RESOLVED** — both readings fit.

This is worth resolving: if it is test-pattern-vs-data, the select bit
`Q5@23,18` is the card's test-mode control, and the test-pattern question in
§1.2 becomes answerable from the netlist.

### 4.8 Which top-edge pads are gated by the global blank?

`Q4@23,18` blanks the top-edge control group but **not** the 96 RGB pads. The
exact membership of the blanked set (20–23 pads) is in
`analysis/fpga/pad_driver_logic_16.53.tsv`; 4 of 21 classifiable pads did not
fit the normalised truth table and were not explained.

### 4.5 What are the six constant-strapped output pins?

`A15`, `M6`, `K12` (constant 0), `E12`, `E13` (constant 1) and one more, all
at `DRIVE 16 / SLEWRATE FAST`. HIGH that they are static level outputs;
their function is NOT RESOLVED.

### 4.6 Exact EBR instance count

Two independent passes gave 53 and 54 for 16.53. The difference is a
tile-grouping convention, not a real disagreement about utilisation. Minor,
but do not quote a precise figure without re-deriving it.
