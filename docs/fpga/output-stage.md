# The LED output stage

What the card emits to the driver chips, and what it does not. Artefacts:
`analysis/fpga/sm16269-register-map.tsv` (the 33-register file, both vendor
tables side by side, per-field bit meanings, grey-depth derivation),
`analysis/fpga/output-stage-arithmetic.tsv` (OneScanLen / CardScanLen /
mapping / scan-table numbers for this exact panel, with verification results),
`analysis/fpga/minoe_corpus_survey.py` (the 370-file corpus survey).

## 0. The fact that frames everything else — HIGH

From `third-party/README.md`, corroborated by the firmware filenames and the
vendor download-page structure:

> **Colorlight ships a different FPGA bitstream per LED driver chip. There is
> no runtime setting for this — the driver-chip serial protocol is implemented
> in gateware.**

This is why the chip id in the parameter pack behaves as a **mode selector
into gateware that may or may not implement your chip**, rather than as a
parameter. It is also why "dead on Normal 13.39, responds on PWM builds" is a
protocol-level result and not a tuning problem.

## 1. The connector and what the chips need

### Established from the vendor documents — HIGH

* The panel connector is **HUB75E, 16-pin**, and the card has **twelve of
  them, J1–J12**. Our panel is on J1. Signal set (vendor spec p. 4):
  `RD1 GD1 BD1` / `RD2 GD2 BD2` (two RGB data groups), `A B C D E` (5 address
  lines), `CLK`, `LAT`, `OE`, `GND`.
* The E120 drives **24 groups of parallel RGB data or 32 groups of serial RGB
  data** (vendor spec p. 2) = 12 hubs × 2 groups.
* The SM16269 / SM16169SH register file is **33 registers, each carrying three
  values (R, G, B)** — the chip is configured **per colour channel**. Record
  0x84 is the flat `(reg, R, G, B)` quad stream in library order, zero-padded
  to 256 bytes.
* **Register `0x02` is patched at load time to `(scan − 1) & 0x3F` = 15.**
  The driver chip is *told the scan depth*. This is decisive: a plain shift
  register has no concept of scan depth. **This chip family runs its own line
  counter.**
* **Grey depth is derived from the registers, not declared:**

  ```
  g     = 128 << ((reg07 >> 3) & 3)
  m     = (reg03 < 0x40) ? 64 : 32
  total = m * g
  ```

  Ours: `reg07 = 0x44` → `g = 128`; `reg03 = 0x3F` → `m = 64`;
  `total = 8192` → **14-bit**. This matches basic-pack `+0x08 = 0x0E` and the
  Eager module datasheet's "Gray Grade: 14 bits" and "Refresh Frequency
  ≥ 3840 Hz".

### Register field meanings — MEDIUM (dialog labels, not a datasheet)

From LEDVISION's `ChipSetting.dll` dialogs IDD20173/IDD20174, recorded in
`config/chips/*.toml`:

| register | field |
|---|---|
| `0x03[7:6]` | low-grey high-refresh level; `< 0x40` selects the ×64 grey multiplier |
| `0x07[7:5]` | frequency-division factor − 1; `[4:3]` line grey; `[2:0]` blanking coarse/fine |
| `0x13[3]` | low-grey-high-refresh **enable**; `[4]` photo optimisation |
| `0x16[5:0]` | per-colour current gain 0–63 (the dialog's sliders) |
| `0x18[1:0]` | blanking ghost level |
| `0x19[2:0]` | blanking level |
| `0x1a[2:0]` | first-line dark compensation |
| `0x20[7:6]` | small period num |
| `0x0a[7]` | standby mode, inverted (0 = on) |
| `0xf0[7:4]` | black-screen standby (`0xA` = on) |

### What "S-PWM" means here — MEDIUM

The `m × g` factorisation **is** the S-PWM (scrambled-PWM) structure. A 14-bit
grey value is not emitted as one 8192-count burst; it is emitted as **64
interleaved sub-periods of 128 counts each** ("line grey" × "grey multiplier",
with `0x20[7:6]` "small period num" as a further subdivision control).

That is how a driver reaches ≥ 3840 Hz visual refresh from a ~60 Hz frame
rate, and it is why the module datasheet can claim 14-bit grey at 1/16 duty.
The arithmetic is HIGH (it is in `chips.rs`); the "64 interleaved sub-periods"
*interpretation* is MEDIUM.

### What could NOT be determined — NOT RESOLVED

> **Largely superseded — see
> [chip-protocol-microcode.md](chip-protocol-microcode.md).** There *is* now an
> SM16269 datasheet in `third-party/datasheets/`, and the 20-byte
> `SChipControl` block below is decoded: it is the driver-chip **serial-protocol
> descriptor** (pre-activation tail 14, register tail, second-command tail,
> data-latch tail 1, VSYNC tail 3, and two GCLK/RCLK-per-row counts).
> Specifically: the LAT/LE **tail length** is what selects register-write mode
> (14 pre-activation, then 5 per addressed write for our `0x14C` profile); a
> register write is **16 bits, MSB first**, on the RGB lanes; the chip has
> **no GCLK and no OE pin at all** — pin 21 is `RCLK`, which is both the grey
> clock and the row advance, so the HUB75 OE wire must carry a pulse train.

**There is no SM16269S / SM16169SH datasheet in this repo.**
`third-party/datasheets/` holds only the E120 spec and the Eager module spec,
neither of which documents the IC. So the following are **not** stated here,
deliberately — this project has lost hours to filling such gaps from general
knowledge of the MBI/ICN/SM family:

* how many LAT pulses (or what preamble) select register-write mode versus
  data mode;
* the bit order or bit width of a register write on the RGB lines;
* whether GCLK is a separate pin or `OE` free-running as the grey clock;
* whether `OE` is active-high or active-low on this part;
* the LAT-to-CLK phase relationship.

The one internal lead: the 20-byte `SChipControl` block, whose accessor is
named `SetGclkNumsOfChipControlByChipCustom` — i.e. it carries **GCLK
counts**. Its value here, identical in both chip libraries:

```
00 0e 01 05 06 01 03 00 00 00 00 97 00 97 00 08 02 00 0a 02
```

Byte 1 = `0x0E` = 14 and the pair `0x97` = 151 are the suggestive numbers.
Per-byte meaning: **NOT RESOLVED**.

## 2. Scan handling — HIGH

All numbers in `analysis/fpga/output-stage-arithmetic.tsv`.

The record stores **half** the module height (32, not 64) because a 128×64
HUB75 module is driven as two vertical halves on the two RGB groups
(`RD1/GD1/BD1` upper, `RD2/GD2/BD2` lower). Then:

```
OneScanLen  = W × (H/2) / scan  = 128 × 32 / 16 = 256
              clocks per scan address, per data group, per module
CardScanLen = OneScanLen × modulesInLineDir = 256
              (factory wall: 512, for 2 modules chained)
```

At 1/16 scan, 4 of the 64 rows are lit per address — 2 rows in each half. So
per address, per data group, the card shifts **2 rows × 128 columns = 256
pixels**, which is exactly OneScanLen. The 5 address lines A–E can express
1/32; only 4 are needed here.

> **`CardScanLen` was wrong (512) in the seller's installed config**, because
> it was compiled for a 2-modules-in-line-dir wall. On a single module the
> card would shift 512 clocks per address into a 256-pixel chain — a
> first-order raster corruption. Fixed in the generated config. — HIGH

## 3. The scan table — HIGH, and a genuinely new result

`GetScanTable` @ `0x1eabc0` → `CalScanTalbeDefault` @ `0x14d710`, transcribed
in `crates/e120-rcvbp/src/image/scan_table.rs`, computes:

1. `InitFieldTable16Segment` picks which grey level gets the full 16-slot
   segment, and takes the other 13 levels from a **hand-coded per-grey-depth
   block** (only 14-bit is transcribed; other depths need their own vendor
   blocks).
2. `FromSegmentToFrameTime` @ `0x1d0a70` assigns each enabled slot a bit time
   `2^level · minOE / segments`, **snapped to 8-unit quanta** with the
   vendor's round-half-away-from-zero.
3. `FieldTableToScanTable` @ `0x1d1c00` buckets slots by `slot % nSeg`,
   highest level first, and emits `(level, 24-bit BE value/8)` entries, with
   per-bucket `(start, end)` pairs at `+0x3C0`, the scan mode at `+0x39E`, the
   segment count at `+0x39F` and an identity line order at `+0x3A0`.

**Verified this session:** our generated scan table is byte-identical to the
factory image's (`card-dumps/primary-region.bin` at `0x76000` vs
`build/p25-128x64-sm16269s-block7.bin` at `0x6000`), and **every one of the
`0xE7` 24-bit value fields is zero in both.**

With `minOE = 1e-4`, every snapped bit time rounds to zero. So **the scan
table as shipped is a level *sequence*, not a timing table** — 152 entries
carrying a level and no duration, plus segment bucketing and line order.

That looked like a candidate fault, so it was tested against the corpus
(`analysis/fpga/minoe_corpus_survey.py`, 370 vendor `.rcvbp` files, all
parsed): **`minOE = 1e-4` appears in 93 files overall and in 24 of the 29 that
use the modern 764-byte record 0x01.** An all-zero bit-time field is the
vendor's normal output for modern configs.

**This largely rules minOE out**, and it is consistent with the PWM reading:
for a chip that generates its own grey, the card only has to *sequence
levels* — the durations live in the chip's own PWM engine. HIGH on the
measurement, MEDIUM on the interpretation.

*How the gateware would consume it* — MEDIUM, the most economical reading and
not evidence: per scan address, walk the bucket for that segment, and for each
`(level, value)` entry drive the corresponding grey plane / OE window. With
zero values the "window" degenerates and the chip's internal S-PWM supplies
the modulation.

## 4. Pixel mapping and module positions — HIGH

### Record 0x03 is the whole mapping

4096 entries = `W × (H/2)`, one per pixel of *one* half. For pixel `i` in
raster order over the 128×32 half (`crates/e120-rcvbp/src/spec/mapping.rs`):

```
row   = i / 128
col   = i % 128
line  = row % scan                          -> 0..15,  the SCAN ADDRESS
group = (storedH/scan - 1) - row/scan       -> reversed_groups = true (vendor default)
slot  = group * W + col                     -> 0..255, the POSITION IN THAT
                                               ADDRESS'S 256-CLOCK SHIFT-OUT
```

Each entry is `(line u8, slot u16 LE)`; in the boot image the u16 is flipped
to BE and the 4096 entries are sliced into 16 packs of 256.

So the table is literally *"framebuffer pixel → (which of the 16 scan
addresses, which of the 256 serial slots)"*. It reproduces the 34-config
vendor consensus for 128×64 @ 1/16 byte-exactly, and 1039 of 1517 corpus
tables from geometry alone.

`reversed_groups` (234 of 241 two-group vendor configs) means the **last**
row-group is shifted out first — the standard consequence of a chain that
fills from the far end.

The **second half of the module is not covered by record 0x03**; it is driven
from the same 4096-entry map on the second RGB group — MEDIUM (the arithmetic
requires it; not independently confirmed).

### Module positions

The screen (MaxW × MaxH) tiled by the 16×16 grid unit, count at `+0x005`,
10-byte entries from `+0x016`: `[outer idx, inner idx, x BE, y BE, w BE,
h BE]`. Four direction variants; line_dir 0/1 *compact* dropped tiles, 2/3
leave *positional* holes.

The vendor emits **all zeros when the tile count exceeds 64** — which is why
the seller's 256×384 wall (384 tiles) had an all-zero table, and why our
single module gets a real one (count `0x20`, 32 entries). The index byte order
remains MEDIUM.

### Data-swap / lane map — MEDIUM

Record 0x01 carries four 16-byte swap blocks plus a 64-byte lane ramp; our
generator writes plain identity ramps (`0x00..0x0F`, `0x10..0x3F`,
`0x40..0x7F`). The seller's file regenerates byte-exactly from that, so
identity is at least the vendor tool's output for this module type.

But `docs/rcvbp-format.md` notes that block 0 (`+0x05A..0x069`) was **wholly
reordered** between a 32S and a 64S variant of the same module — so it is
scan-dependent. **Whether identity is correct for 1/16 on this module is NOT
RESOLVED.**

## 5. What the bitstream says about the physical output

* **Only 12 of the ~125 LED-side outputs have any IOLOGIC** in the PWM builds
  — they are driven from fabric LUT/FF outputs through ordinary routing, not
  through the IO registers. The pipelining is in the fabric, on the CLKOP
  domain plus a fabric-generated global clock (`BDCC0`, fan-out ~660). —
  MEDIUM-HIGH. See [resources.md](resources.md#4-output-registration--medium-high).
* **The logic is massively replicated**: 20 170 used LUT4s but only 2 129
  distinct effective INIT values, with the top handful appearing 600–1300
  times each. That is one datapath slice instantiated hundreds of times —
  consistent with a per-channel serialiser/PWM engine, though replication
  alone does not prove what is replicated. — HIGH / MEDIUM.
* **A strong cross-build lead — MEDIUM.** `IOLOGIC*.MODE = IREG_OREG` appears
  **96 times** in the Normal 13.39 and LS0allDA 6.69 builds but only **10**
  times in all three PWM builds. **96 = 32 serial RGB groups × 3 colour
  lines**, which is exactly the E120 spec's "32 groups of serial RGB data".

  Reading: Normal builds register all 96 serial data lines *inside the IO
  cell*, because a Normal build generates the greyscale itself and must clock
  data out at bit-plane rates; PWM builds moved that into the fabric because
  the per-line data rate is far lower when the chip does the PWM. If it holds,
  **the 96 IOLOGIC sites in 13.39 are the RGB data pins** — the fastest route
  to a classified HUB75 pin list.

## 7. The output stage in the netlist

Traced from the pads backward. Artefacts:
`analysis/fpga/output_stage_16.53.txt`, `analysis/fpga/rgb96_pins.txt`,
`analysis/fpga/led_pin_classification_16.53.txt`,
`analysis/fpga/pad_driver_logic_16.53.tsv`,
`analysis/fpga/build_comparison.txt`,
`analysis/fpga/negative_results_and_method.txt`.

### 7.1 The 96 RGB data pins are identified — HIGH

The `IREG_OREG` lead in §5 holds. The 96 IOLOGIC sites in 13.39 and 6.69 are
**byte-identical between those two builds**, and mapping them to package pins
gives **exactly** the 96 left- and right-edge pads that the pin census
classified as plain fabric-driven outputs in 16.53 (47 LEFT OUT + 48 RIGHT OUT
+ 1 LEFT BIDIR). **Zero discrepancy in either direction.**

96 = **32 serial RGB groups × 3 colour lines**, matching the E120 spec exactly.
The list is `analysis/fpga/rgb96_pins.txt`.

> **Retraction — HIGH.** [pinout.md](pinout.md#phy-management--retracted) suggested `T4`
> was MDIO. `T4` is one of the 96 RGB pins. **There is no MDC/MDIO group
> anywhere in this design.**

### 7.2 Two master control bits — HIGH

Counting how often each signal feeds a pad-driver LUT across all 197 pads, two
flip-flops in **one slice** dominate — `Q5@23,18` (23 pads) and `Q4@23,18`
(20 pads), with duplicates `Q5@39,18` / `Q4@39,18` for the lower half.
Everything else feeds 1–3 pads.

Normalising the pad LUTs against them gives, for 17 of 21 classifiable pads:

```
pad = 0                                 when Q4@23,18 = 0
pad = NOT( Q5@23,18 ? dA : dB )         otherwise
```

So:

* **`Q4@23,18` is a global synchronous BLANK.**
* **`Q5@23,18` is a 2:1 source select.**
* **The outputs are active-low.**

The 96 RGB pads are **not** gated by this — the blanked group is the
**top-edge control pads**.

### 7.3 What the 2:1 mux selects between — HIGH that it is counter vs BRAM

* One leg is always a **CCU2 counter** at `x = 24..26, y = 7..11`, sharing
  `.CE = F3@26,8`.
* The other leg always terminates on **block RAM data out** — verified:
  `JQ5@5,25 ← JDOB13_EBR`, `JQ2@4,25 ← JDOB2_EBR`, with probes
  `A3 → JDOB2`, `A11 → JDOB13`, `A2 → JDOB4`, `E6 → JDOA5`.

**Whether that is "test pattern vs live data" or a within-frame command/data
time-multiplex (i.e. SM16xxx configuration words vs pixel data) is NOT
RESOLVED.** Both readings fit the structure.

### 7.4 The control-group source RAM starts empty — HIGH

Every build contains exactly one `.bram_init` block, and it targets the EBR
with `WID = 3`. The block RAM feeding the top-edge control group is
`MIB_R25C4/C5` EBR0, `PDPW16KD`, **`WID = 1` — not initialised**.

So that RAM comes up **empty at configuration time and must be written at run
time.** That is a concrete mechanism by which a card with no valid parameters
loaded would scan a buffer of nothing.

No 256-byte parameter-pack store was located. LUT-RAM is ruled out as the
store in 16.53: only 18 blocks of 16×4, against 59 and 89 in 13.39 and 6.69 —
too small and too fragmented.

### 7.5 The build difference, confirmed — HIGH

The one large output-stage difference between the dead-on-panel family and the
working family is exactly the `IREG_OREG` count. In 13.39 and 6.69 **all 96
RGB pads go through the IO output register**, clocked by PLL CLKOP, with LSR
tied on all 96 and CE tied on 72. In the PWM builds only 10 do.

Details in `analysis/fpga/build_comparison.txt`.

### 7.6 Clocking of the output stage — HIGH, and it corrects an earlier reading

* **No pad anywhere is driven from a global clock net.** HUB75 DCLK is
  **fabric-generated data**, not a routed clock.
* **The design is single-clock.** 98.9 % of flip-flops are on PLL CLKOP in
  16.53 (12 589 of 12 725); the same holds in 10.81 and 13.39.
* **There is no slow LED clock domain.** The fabric-divided clock distributed
  on a global net is used as a **clock enable**, not a clock — `G_HPBX0900`
  appears as `.CE` on output-stage flops. This supersedes the earlier reading
  in [resources.md](resources.md#3-clocking--resolved-end-to-end) of `BDCC0` as a second clock
  domain.
* `G_LDCC2CLKI ← G_JOSC` in all five builds — the internal `OSCG` oscillator
  is on a global net.

### NOT RESOLVED about the physical output

* **Which top-edge pad carries which HUB75 control signal** (A–E vs CLK vs LAT
  vs OE). The **RGB data group is now identified** (§7.1) and the control
  group is identified *as a group* (§7.2), but it has not been decomposed.
* **Whether the scan counter's terminal count is 16.** Not determined.
* **Whether the FPGA drives HUB75 directly or through buffers.** All banks are
  3.3 V so every FPGA IO is 3.3 V, but the board has driver ICs visible in the
  vendor photo and the bitstream says nothing about what is on the far side of
  a pad.
* **The actual on-wire waveform — never measured.** This bench has a PSU and a
  webcam, no scope or logic analyser. Every timing statement in this file is
  derived from tables, not observed.
* **OE polarity, and whether OE free-runs as the grey clock.** Searched in the
  vendor SDK twice: `IsChipHasOE`, `Is8nsOeEnable`, `Get8nsOeEnableInfo`,
  `GetMinOE`, `HR_SetMinOE` all exist but bottom out in a chip-library
  sub-object that could not be resolved through vtable ambiguity. Nothing in
  `libCLTDevice` selects OE behaviour or a GCLK mode — **consistent with it
  being a gateware property, hence unfixable by configuration.**

## 6. Reconciling the bench facts

### Fact by fact

**"The card's own test-pattern generator does not render either — current flat
across all selectors."**

The frame itself is correct: `test_mode()`
(`crates/e120-proto/src/discovery.rs`) matches
`CReceiverOP::SetRcvCardTestMode` @ `0x3d54e0` exactly — type `33 00`, `0x09`
at payload+5, selector at payload+6, 279-byte frame. — HIGH

**But the selector enum is not recoverable.** It lives in the UI layer;
`ScrnTest.dll` yields only `NORMAL`/`RED`-family strings with no numeric
mapping. So "flat across all selectors" may mean the pattern generator is
broken, **or** that no valid selector was ever hit, **or** that a background
`fill --hold` streamer was concurrently overwriting the framebuffer during the
sweep. — **NOT RESOLVED, and this is the single most important ambiguity in
the whole picture.**

What it *does* rule out unambiguously: **our frame data is not the sole
cause.** If a card-internal source cannot light the panel either, the fault is
at or below the card's raster/output stage, not in the host's pixel bytes. —
HIGH. Note the tension with the fact that content *does* change the panel: the
two cannot both be simply true, which is why the concurrency explanation
deserves priority.

**"Our pixel frames are byte-exact FPP and verifiably on the wire."** Removes
the host encoder from the suspect list entirely. — HIGH

**"Brightness works — current scales with the parameter."** The strongest
positive signal on the bench. It proves the card is receiving and parsing our
frames, the scan engine is running, OE/current modulation is under control,
and the driver chips are powered, armed and sinking current. It localises the
fault into the **pixel data path** — what gets written into the raster buffer
and in what order it is shifted out. — HIGH

**"Completely dead on Normal 13.39, responds on PWM builds."** A clean,
decisive result: **SM16269S is a PWM-class (S-PWM, self-scanning,
register-configured) driver, and only PWM-family gateware speaks its
protocol.** A Normal build emits a plain shift-register waveform with
card-generated bit-plane OE modulation; an S-PWM chip that never receives its
register writes stays in its power-on state and never lights. Combined with §0,
this confirms **16.53 is the right build and there is no point chasing other
families.** — HIGH

**"Chip id `0x014C` → per-pixel noise at 2.8–4 A; `0x0214` → dark at 0.5 A."**
Two conclusions:

1. **The gateware does branch on the chip id**, notwithstanding that no
   comparator against it can be found in the LUT netlist. Direct behavioural
   proof that the register-file hypothesis is right and the LUT-level negative
   was a search limitation. — HIGH
2. **`0x014C` is very likely the correct id for this panel and `0x0214` is
   not.** The vendor table names `0x14C` "SM16169SH"; the 16.53 firmware is
   filed as `SM16386S_SM16269SH`; the panel *responds* to `0x14C` and is dark
   at `0x214`. The silicon marking "SM16269S" appears to be served by the
   `0x14C` family entry (sub-variant `0x14D`), while `0x0214` is a different
   part this build does not implement, falling through to a no-drive default.
   — **MEDIUM-HIGH.**

**"Per-pixel noise at 2.8–4 A"** is good news badly disguised. Per-pixel
variation means individual pixels are individually addressable at varying
levels — the serial chain loads, the latch fires, the PWM engines run, the
current sinks work. **A panel showing per-pixel noise while the host sends a
uniform white fill is a panel displaying buffer contents that are not our
content.** — HIGH

### Ranked hypotheses for the panel not rendering

**#1 — Our content is not landing in the buffer the scan engine reads
(geometry/window mismatch).** Most likely.
*For:* brightness works but content does not; per-pixel noise under a uniform
fill is the signature of uninitialised RAM being scanned; the installed config
was compiled for a 256×384 wall while the EEPROM screen-size record at flash
`0x7F000` says 128×64 (verified: `… 00 80 00 40 …`) — the card's two notions
of its own size disagreed; `0x55` row frames carry an absolute row index and a
pixel offset, so a wrong window puts them outside the displayed region.
*Against:* the card-internal test pattern reportedly also fails, which a
window mismatch would not explain — unless the concurrency artefact is the
real story.
*Experiment:* flash `build/p25-128x64-sm16269s-block7.bin`, set screen size
128×64, `reload-params --full`, `send-params`, then send **a single lit pixel**
at (0,0) and photograph. One pixel isolates addressing from ordering
completely — a scrambled raster and a missing raster look identical under a
fill and totally different under one pixel. Then a single row, then a single
column.

**#2 — The results predate the corrected config.** Nearly as likely, and cheap
to eliminate.
*For:* `HANDOFF.md` open item 1 is literally "flash and test the generated
config", and records that the scrambled-content tests "predate the corrected
serial clock, scan-line length, module positions and double latch now in the
generator". CardScanLen alone was 512 instead of 256 — a guaranteed raster
corruption.
*Against:* nothing.
*Experiment:* the same flash-and-retest. **Do this before anything else.**

**#3 — The test-pattern "failure" is an artefact of concurrent streaming.**
Likely, and it is the fact that makes #1/#2 hard to hold.
*For:* `scripts/bench.py boot` and `chipsweep.sh` both leave
`e120 … fill --hold` running in the background; a card-internal pattern
written into the framebuffer would be overwritten immediately, and the
`0x01 0x07` latch frames come from our streamer.
*Against:* if the card free-runs its scan, a test pattern ought to survive at
least momentarily.
*Experiment:* `pkill -f e120`, confirm no frames on the wire, wait, then sweep
`payload[6] = 0x00..0x0F` one second apart with the ammeter logging. **And
press the physical test button** (E120 spec item 5 — four monochrome fields
plus scan patterns), which bypasses the host, the Ethernet stack and the
`0x33` command path entirely. If the button lights the panel, the output stage
is proven good and everything left is the data path.

**#4 — Serial clock mismatch (8 vs the chip default 15).** Plausible, cheap.
*For:* `config/panels/*.toml` carries `serial_clock = 8`, inherited from the
seller's wall config; `config/chips/sm16269.toml` gives the vendor default for
this chip as **15**. The pack carries it three times (`+0x09`, `+0x2C`,
`+0x2E`) and it also feeds the scan-table line time. The chip-custom block at
record `+0x06A` separately carries the chip's *reset* clock, so a mismatch
between the two is expressible. A shift clock outside the chip's window
produces exactly "latches, but wrong bits".
*Against:* the seller's wall presumably worked at 8 with these modules at some
point (unverified).
*Experiment:* delete the `serial_clock = 8` line so the chip-library default
15 is used, regenerate, `send-params`, photograph. One line, RAM-only.

**#5 — The chip register table is wrong for this silicon.** Plausible.
*For:* the two library tables differ materially — `reg 0x07` = `0x44`
(frequency division 3) vs `0x04` (division 1), `reg 0x0a` = `0x02` vs `0x00`,
`reg 0xf0` = `0x03` vs `0x00`, and `0x0b`/`0x0c`/`0x11`/`0x1b`/`0x1c`/`0x1f`
all differ. The seller shipped the `0x14C` table with the sub-variant id
*unset* although the silicon is SM16269S. The chip pack is RAM-only, so this
sweeps freely.
*Against:* both tables produce the same derived 14-bit grey, and the panel
already responds to the `0x14C` id.
*Experiment:* run both `config/chips/sm16269.toml` and
`config/chips/sm16169sh.toml` against the same otherwise-corrected spec,
chip-pack only, photograph each. Then bisect the differing registers, starting
with `0x07`.

**#6 — Data-swap / lane-map identity is wrong for 1/16 on this module.**
Lower. Only worth doing after #1–#5; large search space, no consensus data.

**#7 — minOE / all-zero scan-table bit times.** Investigated and largely
ruled out — the factory image on this card has byte-identical bytes, and 24 of
the 29 modern-format vendor configs use the same `minOE = 1e-4`. It is the
vendor norm, not a defect.

**#8 — Card-side hardware fault in the HUB75 output stage.** Lowest, and
largely excluded: the panel now draws 2.8–4 A and shows per-pixel structure
under the `0x14C` id, so the output stage demonstrably drives the panel. The
old §17/§19 analysis in `docs/archive/config-protocol.md`, which concluded the
panel was unpowered or the ribbon wrong, is **superseded** and should be read
as historical.

### The one-line summary

Brightness working while content does not, plus per-pixel noise under a
uniform fill, plus a panel that answers `0x014C` and not `0x0214`, says:
**the driver protocol is right, the drivers are armed, and the raster is being
scanned — what is wrong is which bytes reach the scan buffer.** Flash the
current generated config, fix the screen-size record, kill every background
streamer, then light exactly one pixel.
