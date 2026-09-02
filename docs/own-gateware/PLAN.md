# Writing our own gateware for the E120

A feasibility study and implementation plan for replacing the Colorlight
gateware on the E120 receiving card with a design of our own, so that a P2.5
128×64 1/16-scan panel of SM16269S drivers can be driven with the chip protocol
and its timing under our control.

Confidence tags are used literally throughout and mean the same as everywhere
else in this repo: **HIGH** = read from the bytes or the datasheet and
cross-checked; **MEDIUM** = a strong pattern resting on one stated assumption;
**LOW** / **NOT RESOLVED** = a guess, flagged as one.

Skeleton and constraints: [`gateware/`](../../gateware/).

---

## 0. Verdict

**The logic is easy. The pins and the recovery are the whole project.**

Three things are true at once and they set the shape of everything below.

**The design fits with enormous margin.** Our v1 — one RGMII receive port, one
HUB75 connector, a 128×64 framebuffer, one S-PWM serialiser — is on the order
of 2 000 LUT4s and 12 block RAMs. The part has 24 288 LUT4s and 56 EBRs. The
vendor uses ~95 % of the LUTs and essentially all the block RAM, but it serves
twelve connectors, two PHYs, a config parser, a flash agent and the whole DSP
row. We are building about a twentieth of that. Throughput is not a concern
either: a full frame upload at a conservative 15.6 MHz DCLK takes 2.1 ms, so
476 uploads per second against a 60 Hz source. **HIGH.**

**Everything electrically necessary is already known.** The reference clock pin
(P6, 25 MHz), the PLL plan (×5 to 125 MHz), both RGMII port groups, the SPI
flash pins, the bank voltages (all 3.3 V), and the fact that there is no
external RAM and no MDIO — all decoded from the vendor bitstreams at HIGH
confidence, and the PLL divisors independently reproduced by running `ecppll -i
25 -o 125`, which emits exactly the vendor's `CLKI_DIV=1, CLKFB_DIV=5,
CLKOP_DIV=5`.

**But we do not know which FPGA pad goes to connector J1, and it is not
knowable from any bitstream.** Nothing in a bitstream ties a pad to a
connector. The 96 serial-RGB data pads are identified as a *set* and the
top-edge control pads as a *group*; which six of the 96 and which eight of the
~52 are J1's is unresolved and must be found on the bench. This is the largest
single body of work in the plan and §6 is mostly about doing it cheaply.

And underneath both: **the card currently has no proven recovery path.** Every
way of writing the primary flash bank requires the vendor gateware to be
running, so a bitstream that fails to bring up Ethernet cannot be replaced by
the image it replaced. §3 is therefore first, and its conclusion is that the
project should not proceed past building until a hardware-level recovery path
exists.

**Recommendation: proceed, in the order §3 → §2 → §6 → §7, and buy a SOIC-8
test clip before flashing anything.**

---

## 1. What we are replacing, and why it is worth doing

The panel does not render. The card's own internally generated test patterns do
not render either, which puts the fault at or below the card's raster stage.
The most developed hypothesis in `docs/fpga/chip-protocol-microcode.md` §6 is
that on a self-scanning S-PWM part there is a second alignment that has to hold
and that no amount of framebuffer correctness can fix — **the chip's own row
pointer against the card's row select** — and that everything observed is
consistent with the bytes being right and that alignment being wrong.

That alignment is not exposed as a configuration parameter. Colorlight ships a
different bitstream per driver-chip family precisely because the serial protocol
lives in gateware; there is no runtime setting for it. So with vendor gateware
the RCLK-per-row count, the LE tail lengths, the pre-activation policy and the
A–E phase are things we can only *ask for* through a parameter pack whose
interpretation we do not control.

**Own gateware turns every one of those into a knob.** That is the actual
argument for this project, and it is a good one: the remaining unknowns in §6
are almost all one-parameter sweeps, and a sweep you can run is worth more than
a datasheet you cannot get.

---

## 2. Toolchain

### What is on this Mac now (Apple silicon, arm64, macOS 26.6)

| tool | state |
|---|---|
| `ecppack`, `ecpmulti`, `ecpunpack`, `ecppll`, `ecpbram` | **installed** — Homebrew `prjtrellis` 1.4_9 |
| `yosys` | **missing** |
| `nextpnr-ecp5` | **missing**, and *not available in homebrew-core* — only `nextpnr-ice40` is |
| `openFPGALoader`, `ecpprog`, `openocd` | none installed |

`ecppack --help` confirms the options that matter: `--idcode`, `--compress`,
`--spimode`, `--freq`, `--bootaddr` ("set next BOOTADDR in bitstream and enable
multi-boot"), `--background`, `--svf`. And `ecpmulti` — the multiboot bitstream
assembler — is present with `--input`, `--address`, `--flashsize`. So the
multiboot tooling we would need for §3's staged approach is already here.

### Install commands

```sh
brew install yosys                 # homebrew-core has 0.68
```

`nextpnr-ecp5` needs one of two routes.

**Preferred — OSS CAD Suite.** One tarball, everything version-matched:

```sh
# from https://github.com/YosysHQ/oss-cad-suite-build/releases
curl -LO .../oss-cad-suite-darwin-arm64-<date>.tgz
tar xzf oss-cad-suite-darwin-arm64-<date>.tgz -C "$HOME"
xattr -dr com.apple.quarantine "$HOME/oss-cad-suite"   # macOS quarantine
source "$HOME/oss-cad-suite/environment"
```

> **Trap.** This ships its own prjtrellis database alongside the Homebrew one.
> Set `TRELLIS_DB` explicitly. A mismatched database does not error — it
> produces a subtly wrong bitstream. Given that this project's entire pinout
> knowledge came out of the Homebrew database, keep the two straight.

**Alternative — build from source** against the Homebrew prjtrellis, which is
worth doing if you want the analysis tooling and the build tooling to share one
database by construction:

```sh
brew install cmake boost eigen
git clone https://github.com/YosysHQ/nextpnr
cmake -S nextpnr -B nextpnr/build -DARCH=ecp5 \
      -DTRELLIS_INSTALL_PREFIX="$(brew --prefix prjtrellis)"
cmake --build nextpnr/build -j"$(sysctl -n hw.ncpu)"
```

Optional but recommended: `brew install verilator icarus-verilog` for
simulation. Simulating the RGMII → MAC → parser chain against a captured pcap
of our own CLI's output costs an afternoon and removes an entire class of
"is it the host or the card" question from the bench.

### The fit

| resource | LFE5U-25F has | vendor uses | our v1 estimate |
|---|---|---|---|
| LUT4 | 24 288 | ~20 170 (95 %) | ~2 000 (8 %) |
| EBR (18 Kbit) | 56 | 53–54 | 12 single-buffered, 24 double |
| framebuffer bits | 1 008 Kbit available | ~954 Kbit used | 197 Kbit (8 bpc) |
| PLL | 2 | 1 | 1 |
| DSP | 28 MULT / 14 ALU | all | 0 |

Storing the framebuffer at 8 bits per channel rather than 16 is deliberate and
matters: the wire carries 8 bits per channel and nothing more, so 16-bit
storage doubles the memory for zero information. The chip's 16-bit word is
produced on the way out. 8 bpc leaves room for double buffering (39 % of EBR);
16 bpc would not (78 %).

---

## 3. RECOVERY — and why it comes first

### 3.1 The situation, stated plainly

Today the primary bank (flash blocks 0x00–0x0A at `0x000000`) is written two
ways, and **both require the vendor gateware to be running**:

* the vendor gateware's SDRAM-staging agent (type `0x1A00` frames; blocks 0–2
  and 8);
* direct host writes (type `0x0600` / `0x2600`; blocks 3–7, 9–10).

So the failure mode is circular: if our bitstream does not bring up RGMII
receive and a flash write path, there is nothing left on the board that can
accept a replacement image.

### 3.2 Will the ECP5 fall back to the golden bank? Almost certainly not

There is a complete, valid, hole-free bitstream at flash `0x200000`
(`card-dumps/golden-bank.bin`, header date 2022-07-09, all 7562 frame CRCs
valid). It is tempting to treat that as a safety net. Three pieces of evidence
say it is not one:

1. **Neither bank contains a second `BD B3` preamble or any jump command**
   (`docs/fpga/flash-layout.md` §5). ECP5 multiboot is reached by an explicit
   jump embedded in the image the device loads first, not by the device
   searching flash. With no jump anywhere, nothing tells the device that
   `0x200000` exists. — HIGH that the jump is absent; the inference that this
   makes golden unreachable is MEDIUM-HIGH.
2. **The vendor's own upgrade descriptor for this card says "has a golden bank:
   false."** Whatever the E120 spec's marketing claim of "firmware program
   backup … no need to worry about loss of firmware program due to cable
   disconnection or power interruption during the upgrade" refers to, this
   card's descriptor says the feature is off.
3. **The golden image is not even the firmware the card reports.** Its 2304-byte
   EBR init block is byte-identical to 13.39/9.53/6.69/16.53 and *differs* from
   10.81, so it is very unlikely to be a 10.81 build — yet when the primary
   bank contained 4113 bad frames the card ran and reported 10.81. That is an
   argument that the card was *not* booting golden.

There is a live alternative reading, and it is not resolved: that
`0x030000`–`0x07FFFF` is not the boot flash at all and the card's flash-access
firmware redirects host reads and writes in that range to a separate parameter
store — which we *know* it does for `0x07F000`. Under that reading the primary
bank was never actually corrupt. **`docs/fpga/flash-layout.md` §5 leaves (A)
versus (B) open and this plan does not close it.**

Either way the conclusion for us is the same: **do not treat the golden bank as
a recovery path.** It has never been observed to load and there is no mechanism
in the flash by which it could.

Second point, and it is separate: even if golden *did* load, it is a
Normal-family-lineage build we do not otherwise possess. Whether it speaks the
Ethernet flash-agent protocol our CLI uses is unverified. It is the same design
lineage, so it probably does — but "probably" is not what you want from a
parachute.

### 3.3 Is there a JTAG header?

**Yes, very likely.** The E120 spec's board photo shows a **clearly visible
unpopulated 2×10 through-hole header footprint** immediately to the right of
the dual RJ45 stack, between the RJ45 and the left TSOP package: continuous
silkscreen outline, ~0.1" pitch, **square pad at pin 1**, a triangular pin-1
arrow, a key notch, and silkscreen `J25` above / `J24` below. Bare plated
holes, no connector fitted. It is absent from the mechanical drawing, which is
consistent with an unfitted footprint.

**But nothing labels it.** No `TCK` / `TMS` / `TDI` / `TDO` / `PROGRAMN` /
`INITN` / `DONE` / `CFG` silkscreen is legible anywhere in the document, and
the spec text never mentions JTAG. The photo is a single 652×379 JPEG placed at
134 DPI across a 145 mm board — about 4.5 pixels per millimetre — so 1 mm
reference designators are one to two pixels tall and unrecoverable at any
upscaling. The FPGA itself is covered by a paper "E120" sticker.

Also present and possibly relevant: a `T1` label beside a pair of large plated
through-hole pads below the left TSOP, and a 2×2 cluster of large
through-holes near the FPGA whose designators are not confidently legible.

**Confidence: MEDIUM-HIGH that J24/J25 is a programming header; NOT RESOLVED
what its pinout is.** Resolving it needs the physical board and a multimeter,
not another document — see experiment **E-JTAG** in §6.

### 3.4 The two accessible connectors that change the picture

Two things in the spec are worth more than they first appear.

**J19, the 5-pin external interface**, brings off the board:
`KEY-/GND`, `KEY+`, `POWER_LED-`, `LED+/3V3`, `DATA_LED-`.

That means the FPGA-driven signal LED (D2, the one that "flashes 1×/sec = normal
working, Ethernet connection normal") and the test button are both available on
a header we can probe and drive. **This gives us a free output oracle and a
free input oracle that do not involve the panel at all.** A bitstream that
blinks D2 proves it configured, that the PLL locked, and that our clock is the
frequency we think it is — measured with the webcam we already have, with the
panel unplugged and the 5 A supply barely loaded. That is milestone M1 and it
is worth a great deal.

Note this also **corrects** `docs/fpga/pinout.md` §5's "no dedicated status-LED
or button pins were identified": a `DATA_LED-` net driven at 1 Hz must come
from somewhere, and the three top-edge fabric inputs (`A10`, `A12`, `D12`) are
now much stronger candidates for `KEY+` than they were.

**J28, the physical test button**, drives four monochrome fields plus scan
patterns entirely inside the card. It bypasses the host, the Ethernet stack and
the `0x33` command path. It has still not been pressed on this bench and it is
the cheapest unrun experiment in the whole project — see §7, M0.

### 3.5 The recovery strategy, ranked

**R1 — SOIC-8 test clip and an external SPI programmer. Do this first.**
Cost about $15 (CH341A or a Pi/FT232H, plus a clip). It gives:

* a **full offline dump of the entire flash**, which we do not have — we hold
  only `0x000000`–`0x0BFFFF` and the golden bank, while the vendor library
  targets addresses up to `0xE90000`, implying a ≥16 MB device;
* an unconditional restore of the vendor image regardless of what the FPGA is
  doing;
* independence from JTAG, from Ethernet, and from anything we might break.

Procedure: power the board down, clip the flash, dump twice and compare, keep
the dump under `card-dumps/`. Hold `PROGRAMN` low or simply leave the board
unpowered so the FPGA does not contend for the bus (the running design drives
the flash — `CCLK.MODE USRMCLK`). **This single purchase converts the entire
project from irreversible to reversible and it should happen before any other
step.**

**R2 — JTAG on J24/J25, if E-JTAG identifies it.** ECP5 JTAG is available even
when configuration has failed and `DONE` is low, which is exactly the case that
matters. With four wires plus ground it gives: volatile SRAM configuration
(load a bitstream without touching flash *at all* — the ideal iteration loop),
and flash programming through an SRAM-resident proxy. `openFPGALoader` speaks
ECP5 and runs on macOS (`brew install openfpgaloader`, or build from source).

If R2 works, **the iteration loop becomes "load to SRAM over JTAG, power-cycle
to get the vendor image back"**, and the flash is never written during
development at all. That is by far the best outcome and E-JTAG is worth real
effort.

**R3 — our own gateware's first feature is an Ethernet flash programmer.**
Valuable, and it is milestone M3, but be honest about what it buys: it protects
against the *second* through *n*th flash, not the first. If the first image
fails, the flash agent inside it fails with it. Treat R3 as a convenience, not
as the safety net.

**R4 — multiboot staging.** Put our image at a high flash address (say
`0x400000`), leave the vendor primary untouched, and make address 0 a small
header that jumps to it. `ecppack --bootaddr` and `ecpmulti` both exist locally
and support this. Two problems:

* it still requires erasing and rewriting sector 0, which is the one
  irreversible step, and afterwards the vendor's own header and command stream
  are gone;
* whether the ECP5 falls *back* to another address on a CRC failure, as opposed
  to jumping *forward* on an explicit reconfiguration request, is exactly the
  question `docs/fpga/flash-layout.md` §5 flags as unverified. Do not build a
  safety argument on `--bootaddr` until that is settled against Lattice TN1260.

There is a genuinely useful sub-step, though, and it costs nothing:
**verify that we can write and read back an arbitrary non-primary flash region**
(e.g. `0x400000`) through the existing CLI, before caring whether anything can
boot from it. Reads at `0x200000` already work — `e120 snapshot` captures the
golden bank. If writes work too, our image can be staged and verified byte for
byte long before address 0 is touched.

**R5 — desolder the flash.** The last resort, listed only so the ranking is
complete. R1 makes it unnecessary.

### 3.6 The rule this section produces

> **Do not overwrite flash address 0 until either R1 or R2 is in place and has
> been demonstrated on this specific board.** Building, simulating, and
> planning are all free. The first write is not.

---

## 4. Pin map

Consolidated from `docs/fpga/pinout.md`, `analysis/fpga/PINTABLE_16.53.txt`
(197 package pins), `analysis/fpga/rgb96_pins.txt` and
`analysis/fpga/led_pin_classification_16.53.txt`. Machine-readable form:
[`gateware/e120.lpf`](../../gateware/e120.lpf).

The pinout is a property of the **board**, not of one firmware: running the
same decode over all five vendor images gives byte-identical direction flags on
**196 of 197 pins** (the exception is `R7`, the flash `D5/MISO2`, an input only
in Normal 13.39). That is what makes it safe to build on.

### 4.1 Known — HIGH

| function | pin(s) | note |
|---|---|---|
| **25 MHz reference** | `P6` | R47C0C bank 6, `LLC_GPLL0T_IN`. Traced `PLL0_LL REFCLK1 ← JREFCLK1_3 ← JPADDIC_PIO@(0,47)`. |
| **PLL** | `MIB_R50C2:PLL0_LL` | `CLKI_DIV=1, CLKFB_DIV=CLKOP_DIV=5` → CLKOP = 5×REF = 125 MHz, VCO 625 MHz. `ecppll -i 25 -o 125` emits the same divisors independently. |
| **PHY-A RXC** | `J1` | R23C0A, `PCLKT7_1`, dedicated clock input |
| **PHY-A RXD+RX_CTL** | `J2 K1 K2 J3 K3` | R23C0 B/C/D and R20C0 C/D. IDDR. Group HIGH, **order UNVERIFIED** |
| **PHY-A TXC** | `L1` | R26C0A, `PCLKT6_1`, ODDR fed 1/0 → a generated clock |
| **PHY-A TXD+TX_CTL** | `L2 M1 M2 P1 R1` | R26C0 B/C/D and R35C0 A/B. Group HIGH, **order UNVERIFIED** |
| **PHY-B RXC** | `M16` | R26C72C, `PCLKT3_0` |
| **PHY-B RXD+RX_CTL** | `L16 L15 M15 P16 R16` | order UNVERIFIED |
| **PHY-B TXC** | `J16` | R23C72A, `PCLKT2_1` |
| **PHY-B TXD+TX_CTL** | `J15 K16 K15 J14 K14` | order UNVERIFIED |
| **SPI flash CS** | `R8` | `SN/CSN` |
| **SPI flash MOSI** | `T8` | `D0/MOSI/IO0` |
| **SPI flash MISO** | `T7` | `D1/MISO/IO1`, input in every image |
| **SPI flash others** | `M7 N7 P7 R7 R6 T6 M9 P8 N8 M8` | D2/D3/D4/D5/D6/D7, WRITEN, CS1N, HOLDN, DOUT |
| **SPI flash CCLK** | *dedicated* | not a package IO; reached only via `USRMCLK`. `CCLK.MODE USRMCLK` in every image — the running design drives it. |
| **96 serial RGB data pads** | 48 left + 48 right | listed in full in `e120.lpf` §7 and `analysis/fpga/rgb96_pins.txt`. Identified as a set, **not** assigned to connectors. |
| **bank voltages** | all eight | 3.3 V, verified in all 35 `BANKREF` tiles of all five images. No 1.8/1.5 V anywhere. |
| **constant-strapped pads** | `A15 K12 M6` = 0, `E12 E13 M13` = 1 | DRIVE 16 / SLEWRATE FAST. HIGH that they are static outputs; **meaning NOT RESOLVED**. Reproduce the levels. |
| **top-edge fabric inputs** | `A10 A12 D12` | HIGH that they are inputs; function NOT RESOLVED. Prime candidates for the test button / `KEY+`. |
| **unused** | `A7`, `N16` | |

### 4.2 Known as a group, not decomposed

**Top-edge control pads.** They share one global synchronous blank
(`Q4@23,18`) and one 2:1 source select (`Q5@23,18`), and are **active low**:
`pad = 0` when blank, else `pad = NOT(select ? legA : legB)`. One leg is always
a CCU2 counter, the other always block-RAM data out. — HIGH.

Within them:

* Five pads have the **identical driver LUT** (`INIT 0011000000100010`):
  `A3 B4 B11 E5 E10`. Five identical pads muxing a counter against a table is
  the signature of the **A–E scan address lines**, with the table leg supplying
  the scan table's line order. — **MEDIUM**, and the best single lead we have.
* One contiguous run of **14 pads at DRIVE 8 / SLEWRATE FAST** spans
  `R0C27`…`R0C44`: `C7 B7 A8 E8 D8 C8 B8 B9 C9 D9 E9 A9 B10 C10`. These are the
  only top-edge pads with that drive/slew combination. — MEDIUM.

> **Caveat that must travel with that last row.** The "14 = 6 RGB + 5 address +
> CLK + LAT + OE" reading dates from before the 96 RGB pins were located on the
> left and right edges. Now that the RGB lines are known *not* to be on the top
> edge, 14 is a coincidence and must not be over-read. Treat the run as
> "14 fast control pads", which is still a useful narrowing.

### 4.3 Not known, and not derivable from any bitstream

* **Which pad carries which HUB75 signal** (A/B/C/D/E vs CLK vs LAT vs OE).
* **Which pads form which of the twelve connectors J1–J12.** Nothing in a
  bitstream ties a pad to a connector, so no amount of further decoding will
  produce this.
* **What the 34 bidirectional pins are.** Real (out-enable driven from fabric,
  hysteresis on), and 20 of them share one tri-state flip-flop
  (`Q2_SLICE@(25,2)`). Readback from the driver chain is plausible given the
  firmware is named for parts with status readback, but it is speculation.
* **Which pad is `DATA_LED-` and which is `KEY+`** on J19.

### 4.4 The HUB75E connector itself — HIGH, from the E120 spec p. 4

Standard HUB75E, 2×8, keyed:

| pin | signal | | pin | signal |
|---|---|---|---|---|
| 1 | RD1 | | 9 | A |
| 2 | GD1 | | 10 | B |
| 3 | BD1 | | 11 | C |
| 4 | GND | | 12 | D |
| 5 | RD2 | | 13 | CLK |
| 6 | GD2 | | 14 | LAT |
| 7 | BD2 | | 15 | OE |
| 8 | E | | 16 | GND |

On this panel **pin 15 (OE) carries the SM16269S's RCLK**, because the chip has
no OE pin at all — see §5.

---

## 5. Architecture for v1

```
  P6 25MHz ──► EHXPLLL ──► 125 MHz system clock
                             │
 J1 RXC ─┐                   │
 5 pads ─┴─► rgmii_rx ──► mac_rx ──► frame_parser ──► framebuffer
            IDDRX1F ×5     FCS,      0x55 / 0x0107     6 banks
            lane perm      MAC       / 0x0A            4096 × 8
            as a param     filter                          │
                                                           ▼
                                                     spwm_engine ──► HUB75 J1
                                                     config, upload,   6 RGB
                                                     VSYNC, RCLK      CLK LAT
                                                                      OE=RCLK
                                                                      A..E
```

### 5.1 Keep the vendor wire format

The `0x55` row frame is byte-exact against Colorlight's own sender DLL, with
three independent confirmations (the compile-time frame template, the per-field
patch sites, two separate length computations). Keeping it means `e120-cli` —
`image`, `play`, `fill`, `brightness` — works against our gateware unchanged.

That is not merely convenient. **It removes the host from the variable set.**
During bring-up, a known-good transmitter that has already been validated
byte-for-byte against the vendor is worth more than any format improvement we
could design. Extensions go in new type bytes, never by changing `0x55`.

One thing worth fixing while we are here: the row field is `base(screen) + y`
where `base = (screen−1) << 12` for screen ≤ 9. v1 takes the low 12 bits and
ignores the screen selector.

### 5.2 The framebuffer

Six banks of 4096 × 8, one per HUB75 lane: `R1 G1 B1` for the upper half
(y = 0..31) and `R2 G2 B2` for the lower (y = 32..63), addressed by
`(y mod 32) × 128 + x`.

The split is not arbitrary — a 128×64 HUB75 module is driven as two vertical
halves on the two RGB groups, which is why the card's own mapping record stores
only *half* the module height. It makes the write path trivial (one pixel
touches three banks, never two of the same colour) and the read path trivial
(read all six at one address, get a complete HUB75 column).

8 bits per channel, expanded to the chip's 16 on the way out. See §0.

### 5.3 The S-PWM engine, and the five habits it breaks

The SM16269S is not a shift register with a latch. Per the datasheet block
diagram: `SDI → 16-bit shift register → SRAM (8 Kbit) → sixteen SM-PWM
processors`, with `RCLK → 16-bit counter → PWM controller`. Five consequences,
each of which breaks a HUB75 reflex:

1. **There is no OE pin.** The 24-pin part is `GND, SDI, DCLK, LE, OUT0..15,
   RCLK, SDO, REXT, VDD`. The connector's OE wire carries RCLK, so it must be a
   **continuous pulse train**, never a blanking level. — HIGH on the pin list,
   MEDIUM-HIGH on the OE-carries-RCLK wiring.
2. **RCLK is the grey clock and the row advance at once, and it must free-run
   during upload.** The open-source reference runs it on a separately pinned
   thread specifically so a frame upload cannot stall it, and the sibling
   SM16380 corpus records a *verified hardware failure* when the grey clock was
   held low during upload: the panel kept showing power-up SRAM noise. In our
   design RCLK is a completely independent process, by construction.
3. **The row is implicit in upload order, not addressed.** A–E go to the
   module's own row decoder; the chip advances its *own* row pointer from RCLK.
   Those two counters must stay in phase, and nothing specifies the phase.
4. **Commands are selected by LE pulse count, not by an opcode.** The datasheet
   states this in as many words — "LE: data latch control; issues control
   commands in conjunction with DCLK" — and then omits the table entirely.
5. **R, G, B are six parallel lanes.** A register write puts the *same* register
   address with *three different per-colour values* on the R, G and B lanes
   simultaneously. That is how a per-colour register file is reached over a
   HUB75 bus.

### 5.4 The command tails, and why we believe them

From `SChipControl`, record 0x01 `+0x0C4`, for chip id `0x014C`, as shipped in
the seller's own `.rcvbp`:

```
00 0e 01 05 06 01 03 00 00 00 00 97 00 97 00 08 02 00 0a 02
   14  1  5  6  1  3                    151   151
   │   │  │  │  │  └─ VSYNC tail                        HIGH
   │   │  │  │  └──── data-latch tail                   HIGH
   │   │  │  └─────── second command tail — UNKNOWN, do not emit
   │   │  └────────── config-register write tail        HIGH
   │   └───────────── protocol variant selector         MEDIUM
   └───────────────── pre-activation / unlock tail      HIGH
```

Corroborated four ways: the SM16380 open-source command enum is literally
`VSYNC=3, CFG1=4, CFG2=8, PREACTIVE=14` and its corpus entry carries 14/4/8/…/3;
the DP3265S profile is 13 addressed registers all with tail 5 and its corpus
entry carries 5/5 with exactly 13 registers; the block is **all-zero for exactly
the non-S-PWM chips** and non-zero for every S-PWM chip; and the GCLK column
across the corpus is a clean `(1024 >> n) + small` ladder.

**And an independent confirmation found while writing this plan.** The
open-source `angyalr` driver's shipping SM16380SH register sequence is
*byte-identical* to this project's vendor-extracted SM16269 register table in
`config/chips/sm16269.toml`, including the per-colour values —
`0x0c` → `08/18/30`, `0x14` → `14/22/32`, `0x19` → `04/03/03`,
`0x1a` → `03/01/01`, `0x1c` → `12/8f/8f`, `0x16` → `30/30/30`, plus
`0x0a 0x0b 0x0e 0x18 0x1b 0x1f 0x20 0x22`. Only `0x02`, `0x03` and `0x07`
differ, and those are the scan and timing fields that are panel-specific by
construction. **Two independently derived sources — a decompiled vendor DLL and
a Raspberry Pi bit-banger — converge on the same register file.** That is the
single highest-confidence fact available about this chip, and it settles that
the SM16269S uses the addressed `(addr << 8) | value` scheme with a 5-clock
tail.

> **A correction worth recording so nobody repeats it.** The `angyalr` tree
> contains an `SPWM_SM16269S_SETTINGS` block with `GAIN = 0x003f`,
> `CFG1 = 0x2408`, `CFG2 = 0x3ce0` and LE tails `{0,3}` / `{0,5}` / `{0,7}`.
> **It is dead code** — the `"sm16269s"` profile entry actually registers
> `spwm_create_sm16380sh_config`, and the SM16269S-specific structures are
> defined and never referenced. Those CFG words are abandoned, unvalidated
> bring-up guesses. Do not implement them.

### 5.5 The sequences

**Command primitive.** LE high across exactly N DCLK rising edges, RGB lanes
low, then LE low, then a spacer.

**Config write** — 16 bits, MSB first, `(addr << 8) | value`, per chip along
the chain, with the 5-clock tail asserted on the last chip. Three different
per-colour values on the R, G and B lanes.

**Grey upload** — output-major, chip-minor:

```
for channel in 0..15:              # chip OUTPUT index, outer
  for chip in 0..CHAIN-1:          # chip index, inner
    for bit in 15 downto 0:        # MSB first
      six lanes := that pixel's grey bit;  DCLK↑ DCLK↓
  LE := 1; DCLK↑ DCLK↓; LE := 0    # 1-clock data latch, last chip only
VSYNC (tail 3)
```

Reversing that nesting produces "scrambled 16-pixel rectangles" — a signature
the reference bring-up notes call out by name, and therefore something to
recognise in a photograph rather than debug from first principles.

**Arithmetic cross-check, and it lands exactly.** `OneScanLen = W × (H/2) /
scan = 128 × 32 / 16 = 256` slots per scan address. 256 slots = 16 chips-worth
× 16 outputs on a 128-wide half at 1/16 scan. So the card's "slot" index *is*
the chip-output index and one 256-slot pass is one full 16-bit-word sweep of the
chain. Our engine and the vendor's tables describe the same thing.

**Power-on order, and it matters on a 5.1 A supply:**

1. RCLK free-running and A–E scanning, before anything else
2. VSYNC
3. write register `0x16` (gain) **low**
4. upload a **black** frame
5. write the remaining 32 registers
6. only then raise the gain

An armed panel showing unmodulated content already draws ~4.5 A here and rails
the limit at full brightness.

### 5.6 Brightness

Brightness reaches the panel as register `0x16 [5:0]`, the current gain — not as
an OE duty cycle, because there is no OE. The field is **not linear**. The
datasheet gives `IOUT = 19400/Rext × G` with a stated range of 12.5 %–193 %,
and the gain word reconstructs as

```
G = 2^(2·G5 + G4) / 8  ×  (1 + (8·G3 + 4·G2 + 2·G1 + G0)/16)
```

which checks at both stated endpoints (`000000` → 1/8, `111111` → 31/16) and at
the vendor default `0x30` → exactly 1.000. Three checks pass, so **HIGH — but
verify it against the datasheet figure by eye before relying on it**, because
it was reconstructed from text extraction that garbled the original equation.

A 256-entry ROM mapping percent to gain word is the right implementation. A
linear map — which the open-source driver uses — gives a badly skewed curve.

### 5.7 Timing budget

| symbol | meaning | value |
|---|---|---|
| `tSU0` | SDI → DCLK↑ setup | 5 ns min |
| `tH0` | DCLK↑ → SDI hold | 5 ns min |
| `tSU1` / `tSU2` | LE↑ / LE↓ → DCLK↑ setup | 10 ns min |
| `tH1` | DCLK↑ → LE↓ hold | 10 ns min |
| `f_DCLK` | data clock | **25 MHz** (dynamic characteristics), 30 MHz (absolute maximum) |
| `V_IH` / `V_IL` | SDI thresholds | ≥0.5·VDD / ≤0.3·VDD |
| LE pull-down | internal | 155 kΩ typ |

Design for 25 MHz, not 30. From a 125 MHz system clock, ÷6 = 20.8 MHz and
÷8 = 15.6 MHz; start at ÷8. Driving LE and the RGB lanes on the DCLK **falling**
edge meets every setup and hold figure above with a wide margin.

**No RCLK maximum, no minimum blanking and no LE pulse-width minimum are
specified anywhere in the datasheet.**

### 5.8 What v1 deliberately does not do

No transmit path beyond the M2 calibration echo and an optional discovery
reply. No PHY-B / daisy chain. No 10/100 support — and note we *cannot ask* the
PHY what it negotiated, because there is no MDIO anywhere on this board, so if
the panel is silent the host link speed is the first thing to check by hand. No
gamma, no calibration LUT, no double buffering (all deferred to M6+). No
`.rcvbp` parsing: the register file is a compile-time constant until something
renders.

---

## 6. Unknowns, and the experiment for each

Every experiment below uses only what is on the bench: the KA3005P supply read
through `scripts/psu.sh`, a webcam through `scripts/panelcap.py` (90-frame
average — the panel multiplexes 1/16, so a single exposure is scan phase, not
content), and `scripts/compare.py` for interleaved current comparisons. No
scope, no logic analyser.

Two rig rules that override everything: **read the PSU and power-cycle it,
never change its settings**, and **keep brightness ≤ 40 until content is right**
— an armed panel at full brightness rails the 5.1 A limit and browns out.

### 6.1 The blocking unknowns — pins

**E-JTAG — is J24/J25 a JTAG header?** *Cost: an hour and a multimeter.*
With the board unpowered, buzz the 20 holes against: the SPI flash pins (known
package pins, and the flash is a standard SOIC-8 that a clip can reach), ground
(the RJ45 shells and the screw terminal), and 3.3 V. A JTAG header will show
one pin at ground, one at 3.3 V, and four pins that are isolated from
everything else on the board. Then compare the survivors against the ECP5
CABGA256 JTAG ball assignment. Success unlocks the entire safe iteration loop
in §3.5 R2 and is the highest-value hour in this plan.

**E-LED — which pad drives `DATA_LED-` on J19?** *Cost: one bitstream per
candidate group, but they can be batched.*
Build the M1 blink target driving **many** candidate pads at *different*
frequencies simultaneously — 1 Hz, 2 Hz, 4 Hz, 8 Hz — and film D2 (or a LED on
J19) with `panelcap.py`. One capture identifies the pad by its blink rate, and a
handful of builds covers every candidate. Panel unplugged throughout. Yields the
output oracle everything else leans on.

**E-KEY — which of `A10` / `A12` / `D12` is the button?** *Cost: free once
E-LED works.* Mirror each input onto a distinct LED blink rate; press J28.

**E-ADDR — which five pads are A–E on J1?** *Start from the five
identical-driver pads `A3 B4 B11 E5 E10` (MEDIUM).*
Drive a static row address that walks 0,1,2,4,8 while everything else emits a
plausible-but-wrong waveform, and photograph which physical rows illuminate. The
five lines are *binary weighted*, so a photograph reads the bit order directly —
this does not need a permutation sweep at all, only five captures.

**E-CLK — which pads are CLK, LAT and OE?** *After E-ADDR.*
OE/RCLK is the one that must pulse for anything at all to light, so it is
identified first and by elimination: hold each candidate as a static level in
turn while the rest run; the pad whose stalling makes the panel go dark is
RCLK. LAT and CLK are then distinguished by whether stalling one freezes the
image (LAT) or scrambles it (CLK).

**E-DATA — which six of the 96 pads are J1's RGB lanes?** *The big one, but it
collapses.* 96 pads in prjtrellis pad order R2→R47 is the most likely candidate
ordering for "group 0, group 1, …", and J1 should be groups 0 and 1 — i.e. the
first six left-edge pads `B1 D3 C1 C2 E3 F3`, or the first six right-edge pads.
Sweep **windows**, not orderings: there are ~16 candidate 6-pad windows per
edge, and a wrong window shows *nothing* while the right one shows *something*.
Ordering within the window then reads straight off the photograph as a colour
permutation — one more capture, no sweep. Roughly 32 builds worst case, each a
few minutes.

> **Do not populate `gateware/pins-hub75.lpf` until E-ADDR, E-CLK and E-DATA
> have all resolved.** `make hub75` refuses to build without it, deliberately.
> A guessed pinmap that half-works is the most expensive possible outcome: it
> produces plausible garbage, and plausible garbage is what has cost this
> project the most time already.

### 6.2 RGMII lane order

**E-RGMII.** Group membership of `{J2,K1,K2,J3,K3}` and `{L2,M1,M2,P1,R1}` is
HIGH; which pin is which is not. Two facts make it cheap:

* RX_CTL is the only one of the five asserted for exactly the duration of a
  frame — the gateware can identify it by itself;
* with RX_CTL known, only 4! = 24 data orderings remain, and each is checked
  against the known preamble/SFD (`55 55 … D5`) and the known destination MAC
  `11:22:33:44:55:66` that every host frame carries.

Better still, the **raw nibble echo** (milestone M2) resolves RX and TX at once:
transmit the five captured RX pad values on the five TX pads with no
de-permutation at either end, send frames whose nibbles walk a single bit, and
read which lane it returns on. The composed permutation drops out directly. No
scope, no panel, and the host does the arithmetic.

### 6.3 SM16269S protocol unknowns

The datasheet has 17 pages and **confirms the omission**: pins, absolute maxima,
DC and dynamic characteristics, the current formula and the packages — **no
register map, no command table, no LE-pulse-count table, no RCLK timing, no
scan encoding.**

| # | unknown | best guess and source | conf. | experiment |
|---|---|---|---|---|
| U1 | **RCLK pulses per row.** Nothing specifies it. | `SChipControl[10..13]` = 151, which the vendor's own `GetScanCycleLevel` formula reproduces exactly for our `reg07 = 0x04` / sub-id `0x0000`. Compare the sibling SM16380's `(1024 >> fmpwm) + 3 = 131`. | **LOW** | **E-RCLK**: sweep `RCLK_PER_ROW` over a decade around 151 (say 64…512, coarse then bisect), capture each with `panelcap.py`, score by correlation against the sent image. Wrong values desynchronise the chip's row pointer from the external decoder, whose signature is *every physical row showing the same content*. |
| U2 | **A–E phase against the chip's internal row rollover.** | nothing anywhere | **NONE** | **E-PHASE**: sweep `ROW_PHASE` across one full row period at the best U1 value. Do U1 and U2 as a coarse 2-D grid before bisecting either. |
| U3 | **Is pre-activation (tail 14) sent before *every* config write, or once?** | The vendor block carries the 14 and the sibling protocol doc says "each is preceded by 14" — but the only reference that **demonstrably drove a panel** sends `LAT3(+6), LAT14(+8)` once and then all register blocks with no pre-activation between them. | **MEDIUM** | **E-PREACT**: two bitstreams. Try once-first, because that is the one with hardware behind it. |
| U4 | **Does the 5-clock config tail overlap the last five payload bits, or follow them?** These are different waveforms. | The working reference overlaps (LE rises when 5 clocks remain); the abandoned SM16269S-specific code used a post-data tail. | **MEDIUM-HIGH** for overlap | **E-TAIL**: two bitstreams. Symptom of getting it wrong is that register writes silently do nothing — check by sweeping gain and watching supply current, which is the one register with an unambiguous physical readout. |
| U5 | **`SChipControl[4]` = 6, the "second command" tail.** SM16269 has 6 where DP3265 has 5. | nothing names it | **NONE** | Do not emit it. Revisit only if U3/U4 both fail. |
| U6 | **DCLK duty and minimum high/low widths.** Only edge-relative setup/hold are given. | 50 % at ≤25 MHz | MEDIUM | start at ÷8 = 15.6 MHz; if it works, walk up and find the edge. |
| U7 | **RCLK maximum frequency.** Silent. | ≤25 MHz, like DCLK; the sibling spec says 25 MHz at 3.3 V | LOW-MED | falls out of E-RCLK |
| U8 | **Minimum blanking between the last data latch and VSYNC.** | at least one DCLK period with LE low | MEDIUM | cheap to be generous |
| U9 | **Register `0x02` field width** — `[5:0]` per the vendor map, `[7:0]` per the reference. Value is `scan − 1` = 15 either way. | | HIGH that the field exists | no experiment needed at 1/16 |
| U10 | **Semantics of `0x05 06 08 09 0d 0e 10 11 12 17 1b 1c 1d 1e 1f 22`.** | ship the vendor defaults verbatim | HIGH on the values, **zero on meaning** | none. Do not sweep 16 unknown registers. |
| U11 | **The gain formula's exact rendering.** | reconstructed and triple-checked (§5.6) | HIGH | read the datasheet figure by eye once |
| U12 | **Wire colour order (BGR vs RGB).** | BGR, inferred from the caller's 32-bit BGRA surface, not fixed inside CLTNic | MEDIUM | one photograph of a red-only fill settles it |

### 6.4 The single measurement that would close U1, U3 and U4 at once

A logic-analyser capture of the **running vendor card** driving this panel,
checking three things: is there a 14-clock burst before each register write or
only one at frame start; does LE overlap the last five payload bits or follow
them; and how many RCLK edges occur per A–E transition.

That is a ~$15 8-channel USB analyser and an afternoon, and it converts the
three highest-risk sweeps in this plan into three read-offs. **Given that the
bench already lacks a scope and this has cost the project real time, buying one
alongside the SOIC-8 clip is the highest-leverage $30 available.** It is
recorded here as a recommendation, not a dependency — the sweeps in §6.3 work
without it, just slower.

---

## 7. Milestones and acceptance tests

Each milestone is the smallest artefact that answers one question, and each has
a test that can fail. The ordering is a risk ordering, not a feature ordering.

### M0 — before any gateware: close the cheap open questions

*No FPGA work at all.*

1. **Press the physical test button (J28).** Kill every background streamer
   first (`pkill -f e120`), confirm the wire is quiet, then press it. It bypasses
   the host, the Ethernet stack and the `0x33` command path entirely.
   **Acceptance:** `panelcap.py capture testbutton` shows structure. If the
   button lights the panel, the vendor output stage is proven good and the whole
   diagnosis changes — and it means our gateware has a working reference to
   match rather than a mystery to solve.
2. **Acquire a SOIC-8 clip and an SPI programmer.** Take a full offline dump.
   **Acceptance:** two independent dumps of the whole device compare equal, and
   the first `0xC0000` matches `card-dumps/primary-region.bin`.
3. **E-JTAG.** Buzz J24/J25.
   **Acceptance:** a documented pin assignment, or a documented negative.
4. **Verify a non-primary flash write.** Write a known pattern at `0x400000`
   through the CLI and read it back.
   **Acceptance:** byte-exact readback, and the primary region unchanged
   (`e120 dump-flash` + `scripts/flash-review.py`).

> Do not skip 2. Everything after this point assumes it.

### M1 — proof of life: blink

`make TARGET=blink`. PLL, reset, and a ~1 Hz square wave on candidate LED pads
at distinct frequencies (E-LED). **Panel unplugged.**

**Acceptance:** the D2 signal LED blinks at a rate we chose, filmed with
`panelcap.py`. That single observation proves the bitstream configured, the
25 MHz reference is where we think it is, the PLL locked, and the system clock
is the frequency we think it is. Nothing else in the plan can be trusted until
this passes.

**Fallback if E-LED finds nothing:** put the blink on *all* 14 fast top-edge
pads at once with the panel connected and brightness irrelevant — any of them
toggling should produce *some* visible or current-measurable effect. Use
`compare.py` with the toggle on and off as two conditions.

### M2 — RGMII receive and lane calibration: echo

`make TARGET=echo`. `rgmii_rx` + `mac_rx` + the raw nibble echo.

**Acceptance, in order:**
1. Host sends a frame; the card sends *anything* back. (Link and RX clock live.)
2. The single-bit-walk calibration recovers a consistent, self-inverse
   permutation across 20 repetitions.
3. With that permutation applied, `mac_rx` reports `out_good` on ≥99.9 % of
   1 000 sent frames and `out_bad` on a frame with a deliberately corrupted
   FCS. The pass/fail is reported by blinking the LED at one of two rates —
   still no panel needed.

### M3 — the flash agent

`make TARGET=flash`. SPI master over `USRMCLK` + `R8`/`T8`/`T7`, driven by new
Ethernet frame types of our own (not the vendor's — we are not bound to them
here, and a simple read/erase/program/verify command set is easier to get right).

**Acceptance:** with this bitstream **loaded to SRAM over JTAG** (or with the
vendor image still in flash and this one staged elsewhere), read back
`0x000000`–`0x0BFFFF` over Ethernet and compare byte-for-byte against
`card-dumps/primary-region.bin`. Then write and verify a scratch region. Only
after this passes is our gateware allowed to *be* a recovery path.

### M4 — the pixel path, still with no panel

`framebuffer` + `frame_parser`. No HUB75 output; instead a readback command that
dumps framebuffer contents back over Ethernet.

**Acceptance:** send an image with `e120 image`, read the framebuffer back, and
compare against the source in Python. This is the one milestone that can be
verified *exactly*, with no camera and no interpretation, and it retires the
entire host-to-buffer path as a suspect. Do not skip it because it feels like a
detour — "which bytes reach the scan buffer" is the exact question this project
has been unable to answer for weeks.

Also simulate this in Verilator against a pcap of real CLI output.

### M5 — first light

`make hub75`, after E-ADDR / E-CLK / E-DATA have produced `pins-hub75.lpf`.
`spwm_engine` with the register file as a compile-time constant, gain forced
low, and the U1/U2 sweep parameters exposed.

**Acceptance, staged, and each stage is a real gate:**
1. **Any light at all**, at gain ≤ 8, with supply current under 1 A.
2. **A single lit pixel at (0,0)** appears at one place. One pixel isolates
   addressing from ordering completely — a scrambled raster and a missing
   raster look identical under a fill and totally different under one pixel.
3. **A single row**, then **a single column**, land where they should.
4. **A full-white fill** gives a current step that `compare.py` resolves against
   black by ≫0.033 A (the measured spread on this rig), interleaved and
   repeated.
5. **A photograph of a known image** correlates >0.9 with the source under
   `panelcap.py compare`.

Between stages 1 and 2, run E-RCLK × E-PHASE as a coarse 2-D grid, scoring each
cell with `panelcap.py` correlation. That grid is the most likely place for
first light to actually happen.

### M6 — make it good

Double buffering on the `0x0107` latch. Gamma or a calibration LUT on the 8→16
expansion. The gain ROM for a correct brightness curve. Brightness and
per-channel gain from the `0x0A` and `0x0107` frames. `.rcvbp` / parameter-pack
parsing from flash so the panel comes up from cold. PHY-B pass-through. 10/100
fallback. A discovery reply so `e120 discover` sees our card.

---

## 8. Risks, ranked

**R-1. The first flash is irreversible, and everything else is downstream of
that.** Until a SOIC-8 clip or a confirmed JTAG header exists, a bitstream that
fails to bring up RGMII cannot be replaced by the image it replaced: every
existing flash-write path needs the vendor gateware running, the golden bank has
no jump pointing at it, and the vendor's own descriptor says the card has none.
*Mitigation:* M0 step 2, before anything else. ~$15. This is the recommendation
that matters most in this document.

**R-2. The J1 pinout is not derivable and the sweep could be long.** §6.1 argues
it collapses to a few dozen builds because wrong windows show nothing and wrong
orderings show colour permutations — but that argument rests on the assumption
that prjtrellis pad order matches vendor group order, which is MEDIUM at best.
If it does not hold, the search is much larger.
*Mitigation:* do E-ADDR first — five binary-weighted lines read straight off a
photograph and, once A–E are known, they confirm or refute the pad-order
assumption for everything else.

**R-3. U1 (RCLK per row) and U2 (A–E phase) are a 2-D search with no
specification behind either axis.** These are the two parameters most likely to
be the actual reason the panel has never rendered, and they are the two we know
least about.
*Mitigation:* the 2-D grid in M5, and the logic-analyser capture in §6.4, which
would turn both into read-offs.

**R-4. The link speed is unknowable from the card.** No MDIO exists anywhere on
this board, so if the PHY has negotiated 10/100 our gigabit-only RX sees
nothing and cannot say why. *Mitigation:* check the host link speed by hand
before debugging M2; treat "RX completely silent" as a link-speed question
first.

**R-5. Toolchain drift.** Two prjtrellis databases on one machine, and the
entire pinout knowledge base derived from one of them.
*Mitigation:* pin `TRELLIS_DB` explicitly; record the versions in the build log.

**R-6. Power.** The panel rails a 5.1 A supply at full brightness, and a
gateware bug can plausibly turn every driver output on at once.
*Mitigation:* gain forced low in every build until M5 stage 4; `psu.sh`'s
automatic shut-off armed on every power-on; never flash while `ka3005p status`
shows `CH1: Cc`.

---

## 9. Corrections this plan makes to existing repo documents

* **`docs/fpga/pinout.md` §5** — "No dedicated status-LED or button pins were
  identified." The E120 spec's J19 external interface carries `POWER_LED-`,
  `LED+/3V3`, `DATA_LED-`, `KEY+` and `KEY-/GND`, and D2 is described as
  FPGA-driven (it flashes at rates that encode link state). Such pins therefore
  exist; they are simply not yet located in the bitstream. The three top-edge
  fabric inputs `A10`, `A12`, `D12` are now strong `KEY+` candidates.
* **`docs/fpga/pinout.md` §4** — the "14 pins at DRIVE 8 = one HUB75E port's
  signal count" observation predates the location of the 96 RGB pads on the
  left and right edges. With the RGB lines known not to be on the top edge, the
  14 is a coincidence and should not be read as one port's worth of HUB75.
* **Open-source SM16269S profile** — the `SPWM_SM16269S_SETTINGS` block
  (`GAIN = 0x003f`, `CFG1 = 0x2408`, `CFG2 = 0x3ce0`, tails 3/5/7) is **dead
  code**; the `"sm16269s"` profile actually registers the SM16380SH config
  path. Those constants are unvalidated bring-up guesses and must not be
  implemented. The tails to use are 14 / 5 / 1 / 3 from `SChipControl`.
* **New positive result** — the shipping open-source SM16380SH register
  sequence is byte-identical to this repo's vendor-extracted SM16269 table for
  every register except the three panel-specific ones. Two independently
  derived sources agree, which is stronger evidence for the register file than
  either alone.
