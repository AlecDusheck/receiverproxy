# The driver-chip id

> **Read [chip-protocol-microcode.md](chip-protocol-microcode.md) alongside
> this.** It shows that the chip-specific serial protocol reaches the card as
> *parameter data* — the 20-byte `SChipControl` block at record 0x01 `+0x0C4`,
> which carries the LE/LAT command tail lengths and the GCLK/RCLK-per-row
> counts, and which the host tool selects **by chip id**. That is a complete
> mechanism-level explanation for the negative below: the search found no id
> comparator because the id-dependent behaviour is carried in as data. It does
> **not** dispose of §8's bench result, which reports the id alone changing the
> outcome — if that is literally true, both mechanisms are live and which one
> produced the `0x14C` vs `0x0214` difference is still open.

**Status: NOT RESOLVED.** The gateware's handling of the 16-bit driver-chip id
could not be located. This file records exactly what was searched, what came
back empty, why the negative is credible but not absolute, and the one
concrete lead worth following.

Read this as "here is what was ruled out", not as "the gateware ignores the
chip id" — the bench says it plainly does not ignore it.

Supporting dumps: `analysis/fpga/clusters_16.53.txt` (AND-of-one-hot
clusters), `analysis/fpga/chains_16.53.txt` (CCU2 carry-chain candidates),
`analysis/fpga/bytecmp_16.53.txt` (byte-register comparator sweep),
`analysis/fpga/lut_hist_16.53.txt`.

## 1. Why it matters

The card takes a 16-bit driver-chip id in its parameter pack, and the id alone
changes the card's behaviour completely:

| id sent | chip | observed |
|---|---|---|
| `0x014C` | SM16169SH/SL | per-pixel noise, ~2.8–4 A |
| `0x0214` | SM16269S (the silicon we actually have) | panel dark, ~0.5 A |

Nothing else in the pack changes when only the id changes (§2), so the
difference is in the card, not in host-side table generation.

## 2. How the id reaches the card — HIGH

From the vendor-SDK decode already in this repo
(`crates/e120-rcvbp/src/spec/basic_pack.rs`, vendor `ResetChipType`
@ `0x1e5130`), the 256-byte basic parameter pack carries the id as:

| pack offset | contents |
|---|---|
| `+0x1B` | the chip id **if it fits in one byte** (`< 0x100`), otherwise the literal escape `0xFE` |
| `+0xE7..+0xE8` | the full 16-bit id, **big-endian**, and **zero** when the id fitted in the byte slot |

The pack's CRC-32 trailer is computed with the chip-id bytes **zeroed**, so the
id is deliberately excluded from the checksum.

Both ids of interest are ≥ `0x100`, so the card sees
`+0x1B = 0xFE` and `+0xE7,+0xE8 = 01 4C` or `02 14`.

Related fields, from `docs/record-0x01-fields.md`:

| record 0x01 offset | field |
|---|---|
| `0x036` | `GetChipType` low byte |
| `0x204` | `GetChipType` high byte |
| `0x0E9` / `0x205` | secondary chip / decoder id, low / high |
| `0x0FB` | decode-chip enum, 0–14 |
| `0x24B` | decode chip type (`GetNewDecodeChipType`) |

## 3. The vendor chip table

From the extracted vendor tables (`scratchpad/chiptable/chiptable-merged.csv`;
LEDSetting 2.2.6, LEDVISION 9.6, iSet 7 macOS):

| id | name | UI group |
|---|---|---|
| `0x0DE` | SM16169S | 1 |
| `0x14C` | SM16169SH/SL | 1 |
| `0x170` | SM16169SW / SM16189 | 1 |
| `0x24D` | SM16169SK | 1 |
| **`0x214`** | **SM16269S** | **2** |
| `0x217` | SM16269SW | 2 |
| `0x13C` | SM16289 | 2 |
| `0x187` | SM16386S | 3 |
| `0x215` | SM16386SH | 3 |
| `0x076` | SM16388 | 3 |

Two things to note, both MEDIUM:

* The firmware is named for **SM16386S** (`0x187`, group 3) and
  **SM16269SH**. `SM16269SH` does not appear in any extracted table —
  the closest entries are SM16269S (`0x214`) and SM16269SW (`0x217`).
* `0x14C` is **group 1**, `0x214` is **group 2**. The "group" column is the
  vendor UI's dropdown category, so it correlates with chip family but is not
  proof of a protocol family. Do not build on it.

The repo currently sends `0x014C` deliberately: `config/chips/sm16269.toml`
records that the vendor SDK's own SM16269 handling runs through the `0x14C`
code path with sub-id `0x14D`, and `0x214` exists only in the newer
LEDSetting 2.2.6 table with no default parameter block available in the SDK
version analysed.

## 4. What was searched

Method — and the correction that made it valid:

Every LUT4 INIT in all five images was extracted **and corrected for
constant-tied inputs**. ECP5 slices carry `SLICEx.<P><k>MUX` enums that tie a
LUT input to a constant with **no routing arc**; 16.53 has 16 582 of them.
Before modelling this, **6264 of 23 199 LUTs (27 %) appeared to depend on
unrouted inputs** — the netlist was simply wrong. After reduction, zero LUTs
depend on an unrouted input, and routed-but-unused inputs are 0 for every
LOGIC-mode LUT. The inter-tile routing graph was then reconstructed
(canonicalised `N1_`/`S1_`/`E3_`… prefixes) so clusters were found by **real
net connectivity**, not physical adjacency.

Against that netlist:

| searched for | result |
|---|---|
| **16-bit compare-to-constant** (4 one-hot LUT4s + AND) | 6 AND-of-one-hot clusters in 16.53, widest 11 bits, **none takes a coherent bus as input**. Every one mixes registered flags with combinational outputs from scattered tiles — FSM condition trees, not data compares. |
| **8-bit compare-to-constant** against any register byte — exhaustive symbolic sweep of every PLC2 tile with ≥ 6 used flops, simulating all 2ⁿ values | 20 cones, **all 6-bit registers matched on only 4 bits** — plain 4-input ANDs. **Zero 8-bit matches.** |
| **the `0xFE` escape test specifically** | Not present as an 8-bit equality test. Two cones have the right 7×1 + 1×0 literal shape, but their leaves are scattered control signals, not a byte register. |
| **4-to-16 decoder** on a chip-id nibble | None. Groups of ≥ 3 one-hot LUTs sharing all four input *nets*: 7 in 16.53, max group size 4, all datapath mux-select decode. |
| **CCU2 carry-chain compare-to-constant** | 14 candidates, max 21 bits, all AND-reduces of scattered status signals. |
| **XNOR-reduce comparators** (`0x9009`, `0x6996`, `0x8421`, `0x8241`) | 117 `0x9009` and 106 `0x6996` instances, **all in CCU2 mode**. The 16 genuine wide comparators are all **variable-vs-variable** — PWM/greyscale thresholds and counter compares. |
| **generalised product terms** (multi-level AND trees, both polarities, De Morgan handled) | widest cone constrains 16 nets, only 9 of them flops. Product terms whose leaves are *all* flops with width ≥ 6: **9 in the whole device**, max width 9, and that one is a "counter == 0" test. |
| **chip-id values in the microcode ROM** | none of `0x014C`, `0x0187`, `0x0214`, `0x0215`, `0x00DE`, `0x00FD`, `0x013C`, `0x00FE` appears as an immediate or as a full 21-bit word. |
| **any constant in 16.53 but not the older images** | none. |

Cross-image counts (AND-of-one-hot clusters / CCU2 constant chains / byte
matches): 16.53 = 6/14/20, 13.39 = 0/15/19, 10.81 = 5/10/15, 9.53 = 6/12/16,
6.69 = 2/20/37. **The variation is placement noise, not a feature
difference.** — HIGH

One-hot LUT4s are **not** a comparator signature in this design: there are
~2 100 in every image (2 069–2 191), i.e. they are ordinary logic.

Example of what the clusters actually look like — `R6C30 C1`, INIT `0x8000`,
11 bits, sources `R11C30_Q6`, `R7C32_F3`, `R8C33_F6`, `R10C34_Q2`,
`R9C28_Q0`, `R16C36_Q6`… That is an FSM state/condition AND-tree.

### A note on LUT bit order

Whether the first character of a `word: …INIT` string is INIT[0] or INIT[15]
could **not** be pinned empirically, because reversing a 16-bit INIT is
exactly "complement all four LUT inputs" — a symmetry the consistency checks
are blind to. `bits.db` plus the prjtrellis writer convention say first
char = bit 0, and the enum literally named `1` means tie-to-logic-1; that pair
was used.

**All results above are invariant under this ambiguity**: complementing all
inputs maps subcubes to subcubes and one-hot INITs to one-hot INITs, so every
cluster, width and count is unchanged. Only the *polarity* of a decoded
constant would flip — and since no chip-id-shaped constant was found at all,
nothing hinges on it.

## 5. Why the negative is MEDIUM-HIGH, not absolute

**The positive control failed.** This design demonstrably parses Ethernet, yet
the same search finds **no 8-bit constant comparator anywhere** — no SFD
`0xD5`, no ethertype `0x0107`, no `0x0aXX`. Constants we *know* must be tested
are as invisible as the chip id.

So the correct reading is **"this design does not build constant comparisons
out of LUT4s"**, not "the chip id is never compared".

Surviving hypotheses, none of which could be distinguished:

* **(a) Register file + data-vs-data compare — most likely.** The id is
  written into a BRAM or LUT-RAM register file by the packet parser and
  consumed by a sequencer that compares it against *table data*. This is
  exactly what the 117 CCU2 XNOR comparators look like, and it equally
  explains the missing Ethernet constants.
* **(b) Bit- or nibble-serial comparison** against a streamed constant, which
  is indistinguishable from ordinary logic.
* **(c) Only a 2–4 bit field of the id is used**, below the detection floor.

## 6. The lead that looked concrete — REFUTED

A **10-bit high-fanout mode-flag bundle** was found:

```
R22C41_Q0, Q1, Q2, Q3, Q6, Q7
R26C42_Q5
R28C44_Q6, Q7
R21C45_Q4
+ a global qualifier R14C31_Q0 (feeds 114 one-hot LUTs)
```

**734 LUTs read this bundle** and 193 are pure functions of it; simulating all
1024 values gives 1022 distinct equivalence classes, so these are ten
*independent* already-decoded flags. That part still stands — MEDIUM.

Its fan-in appeared to terminate at **`R27C44_Q0..Q3`**, described as "a 4-bit
field with no combinational source" — exactly what a parameter-store-loaded
mode selector would look like.

> **That lead is REFUTED — HIGH. Do not follow it.**
>
> `R27C44` is an ordinary **8-bit CCU2 accumulator**: all four slices are
> `MODE = CCU2`, giving `F0..F7` and `Q0..Q7`, all sharing one clock enable
> `.CE = F7@42,31`. Each bit's routed operand comes from a different register
> (`Q0@46,27`, `Q1@46,27`, `Q6@45,27`…) — it adds a value to itself.
>
> **Why it looked sourceless:** in CCU2 mode the carry travels on the
> *dedicated* carry chain (`FCI ← HFIE0000@(x,y)`, `FCO → HFIE0000@(x+1,y)`),
> which are **fixed, non-configurable connections, not set arcs**. Any tracer
> that follows only configured arcs sees a CCU2 LUT missing an input.
>
> Measured: of the **6956 CCU2-mode LUTs in 16.53, 1012 have zero routed
> inputs and 2295 have exactly one.** So "no combinational source" is the
> normal appearance of *every increment stage on the die*, not a signature of
> anything.

This is a good example of the failure mode this document set exists to
prevent: a striking structural observation that turns out to be an artefact of
the tool. Details in `analysis/fpga/negative_results_and_method.txt`.

`R28C39_Q6/Q7` — which decode a one-hot FSM state at `R30C35`/`R28C35`
(confirmed one-hot because the AND cone evaluates to constant-0 over free
inputs) — was not re-examined and is still MEDIUM.

## 7. What would resolve it

Ranked by cost:

1. **Bench sweep — do this first.** Sweep the whole vendor id table and
   measure the card's response (current draw + camera): one
   `scripts/bench.py run --boot --spec …` per candidate id, with `bench.py
   locate` fixing the panel crop so a bumped camera fails loudly. This answers the question that
   actually matters ("which id should we send") without resolving the
   gateware at all.
2. **Find the parameter store.** The output-stage trace narrowed where to
   look: the block RAM feeding the top-edge control pads is
   `MIB_R25C4/C5` EBR0, `PDPW16KD`, **`WID = 1` — not initialised at config
   time**, so it starts empty and is written at run time. That is a
   run-time-written table feeding the output stage directly. A 256-byte pack
   store was **not** located, and LUT-RAM is ruled out as too small in 16.53
   (18 blocks of 16×4, against 59 and 89 in 13.39 and 6.69). See
   [output-stage.md](output-stage.md#7-the-output-stage-in-the-netlist).
3. **Full netlist recovery** — LUT + FF + BRAM + routing → RTL, then simulate.
   Expensive, and the only route to a definitive "these ids and no others".
   Note the tracing-reliability caveat in
   [decode-method.md](decode-method.md#6-how-far-backward-tracing-can-be-trusted--high):
   deep backward walks are only ~93 % reliable and far worse at the die edge.

## 8. What this means for the bench

**The bench answers what the netlist could not.** — HIGH

1. **The gateware demonstrably does branch on the chip id.** `0x014C` produces
   per-pixel noise at 2.8–4 A; `0x0214` leaves the panel dark at 0.5 A. That
   is direct behavioural proof that hypothesis (a) in §5 is right and that the
   LUT-level negative was a **search limitation, not a fact about the
   design**.
2. **`0x014C` is very likely the id to send, and `0x0214` is not.** —
   MEDIUM-HIGH. The evidence:
   * The vendor's own chip table names `0x14C` "SM16169SH/SL".
   * `config/chips/sm16269.toml` records that the vendor SDK's SM16269
     handling runs through the `0x14C` code path with sub-id `0x14D`.
   * The 16.53 firmware is filed as `SM16386S_SM16269SH`.
   * The panel **responds** at `0x14C` and is **dark** at `0x214`.
   * Per-pixel noise is not a failure signature — it means the serial chain
     loads, the latch fires, the PWM engines run and the current sinks work.
     A dark panel at 0.5 A means the drivers were never armed.

   Reading: the silicon marked "SM16269S" is served by the `0x14C` family
   entry, and `0x0214` is a different part this build does not implement,
   falling through to a no-drive default.

   **Practical consequence: stop sending `0x0214`.**
   `config/panels/p25-128x64-sm16269s.toml` already points at
   `config/chips/sm16269.toml` (family `0x14C`, sub `0x14D`), which is
   correct.
3. **We still do not know the full set of ids the gateware recognises.**
   Anyone quoting a list derived from this bitstream is guessing. A bench sweep
   (`scripts/bench.py run --boot` per id) is the way to find out.
4. The id is **excluded from the pack CRC-32**, so sweeping it needs no
   checksum recomputation — one more reason the sweep is the cheap path.
