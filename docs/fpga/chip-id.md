# The driver-chip id

The card takes a 16-bit driver-chip id in its 256-byte basic parameter pack.
The id selects the driver protocol. On the host side it indexes the vendor
tool's per-chip jump tables, which emit the 20-byte `SChipControl` descriptor,
`SChipCustom`, the record 0x84 register table and the serial clock
([chip-protocol-microcode.md](chip-protocol-microcode.md),
[../chip-control-block.md](../chip-control-block.md)). On the card side the
id changes whether the SM16269S outputs arm at all (measured, firmware 16.53).
No comparator for the id is visible in the 16.53 netlist; the search and its
limits are below.

Artefacts (not kept in the repository; regenerate per
[decode-method.md](decode-method.md)): `analysis/fpga/clusters_16.53.txt`
(AND-of-one-hot clusters), `analysis/fpga/chains_16.53.txt` (CCU2 carry-chain
candidates), `analysis/fpga/bytecmp_16.53.txt` (byte-register comparator
sweep), `analysis/fpga/lut_hist_16.53.txt`,
`analysis/fpga/negative_results_and_method.txt`.

## 1. The id in the parameter pack

Vendor `ResetChipType` @ `0x1e5130`, reproduced in
`crates/rcvbp/src/spec/basic_pack.rs`:

| pack offset | contents |
|---|---|
| `+0x1B` | the chip id when it fits in one byte (`< 0x100`); otherwise the escape `0xFE` |
| `+0xE7..+0xE8` | the full 16-bit id, big-endian; zero when the id fitted in the byte slot |

The pack's CRC-32 trailer is computed with both chip-id fields zeroed. The id
is excluded from the checksum, so changing the id needs no checksum
recomputation.

Both ids of interest are `>= 0x100`, so the card sees `+0x1B = 0xFE` and
`+0xE7,+0xE8 = 01 4C` or `02 14`.

Related record 0x01 fields ([../record-0x01-fields.md](../record-0x01-fields.md)):

| record 0x01 offset | field |
|---|---|
| `0x036` | `GetChipType` low byte |
| `0x204` | `GetChipType` high byte |
| `0x0E9` / `0x205` | secondary chip / decoder id (`GetChipTypeEx`), low / high |
| `0x0FB` | decode-chip enum, 0-14 |
| `0x24B` | decode chip type (`GetNewDecodeChipType`) |

## 2. The vendor chip table

From the chip-name tables of LEDSetting 2.2.6, LEDVISION 9.6 and iSet 7
(macOS). "Group" is the vendor UI's dropdown category; it correlates with chip
family and is not a protocol family.

| id | name | UI group |
|---|---|---|
| `0x0DE` | SM16169S | 1 |
| `0x14C` | SM16169SH/SL | 1 |
| `0x170` | SM16169SW / SM16189 | 1 |
| `0x24D` | SM16169SK | 1 |
| `0x214` | SM16269S | 2 |
| `0x217` | SM16269SW | 2 |
| `0x13C` | SM16289 | 2 |
| `0x187` | SM16386S | 3 |
| `0x215` | SM16386SH | 3 |
| `0x076` | SM16388 | 3 |

* Firmware 16.53 is filed as `SM16386S_SM16269SH`. `SM16269SH` appears in no
  chip-name table; the nearest entries are SM16269S (`0x214`) and SM16269SW
  (`0x217`).
* `0x0214` exists only in the LEDSetting 2.2.6 table (`CLTInterface.dll`
  `0xD70EA0`). libCLTDevice and LEDVISION 9.6 stop at `0x15D`. In every
  vendor build, every chip jump table sends `0x0214` to its default arm: no
  registers, an all-zero `SChipControl`, `IsPWMChip` false.
* `0x014D` is SM16380SH, not an SM16269 sub-variant. `config/chips/sm16269.toml`
  carries `sub_id = 0x014D` and describes it as the SM16269 sub-variant; that
  description is wrong. The panel spec `config/panels/p25-128x64-sm16269s.toml`
  uses `config/chips/sm16269s-factory.toml`, family `0x014C`, sub-id `0`.

## 3. Measured behaviour by id

Reference module: P2.5 128x64, SM16269S drivers, firmware 16.53, method in
[../bench.md](../bench.md).

| id sent | vendor name | measured |
|---|---|---|
| `0x014C` | SM16169SH/SL | renders with the settings in [../rendering.md](../rendering.md); under a wrong configuration it still drives the outputs, with per-pixel structure |
| `0x0214` | SM16269S | panel dark. The pack a `0x0214` declaration produces carries an all-zero `SChipControl` and no register table |
| `0x00DE` | SM16169S | never armed in the one configuration it was tried in (that configuration kept the `0x14C` register table and `SChipCustom`; the corrected form, `config/chips/sm16169s-vendor.toml`, is not measured) |
| `0x002F` | MBI5153 (sub `0x008A`, SM16159) | never arms |

Under `0x014C`, chip-control tails `2/4/8` and `3/5/7` never arm; only the SH
pattern `1,5,6` renders ([../rendering.md](../rendering.md)).

Per-pixel structure at `0x014C` is not a failure signature: the serial chain
loads, the latch fires, the PWM engines run and the current sinks work. A
dark panel means the drivers were never armed.

Two mechanisms are consistent with the table and are not separated by it: the
card branches on the id itself, or the card acts on the id-selected descriptor
bytes (`SChipControl`, record 0x84) and never reads the id. The `0x0214`
measurement changed both the id and the descriptor. Which mechanism produces
the `0x014C` versus `0x0214` difference is not resolved.

<a id="4-what-was-searched"></a>
## 4. Netlist search for an id comparator

### Netlist preparation

Every LUT4 INIT in all five images is extracted and corrected for
constant-tied inputs. ECP5 slices carry `SLICEx.<P><k>MUX` enums that tie a
LUT input to a constant with no routing arc; 16.53 has 16 582 of them.
Uncorrected, 6264 of 23 199 LUTs (27 %) appear to depend on unrouted inputs.
Corrected, zero LUTs depend on an unrouted input, and routed-but-unused inputs
are 0 for every LOGIC-mode LUT. The inter-tile routing graph is reconstructed
with canonicalised `N1_`/`S1_`/`E3_` prefixes, so clusters are found by net
connectivity, not by physical adjacency.

### Results, 16.53

| searched for | result |
|---|---|
| 16-bit compare-to-constant (4 one-hot LUT4s + AND) | 6 AND-of-one-hot clusters, widest 11 bits, none takes a coherent bus as input. Every one mixes registered flags with combinational outputs from scattered tiles: FSM condition trees, not data compares |
| 8-bit compare-to-constant against any register byte: exhaustive symbolic sweep of every PLC2 tile with >= 6 used flops, all 2^n values simulated | 20 cones, all 6-bit registers matched on only 4 bits, plain 4-input ANDs. Zero 8-bit matches |
| the `0xFE` escape test | not present as an 8-bit equality test. Two cones have the 7x1 + 1x0 literal shape, but their leaves are scattered control signals, not a byte register |
| 4-to-16 decoder on a chip-id nibble | none. Groups of >= 3 one-hot LUTs sharing all four input nets: 7, max group size 4, all datapath mux-select decode |
| CCU2 carry-chain compare-to-constant | 14 candidates, max 21 bits, all AND-reduces of scattered status signals |
| XNOR-reduce comparators (`0x9009`, `0x6996`, `0x8421`, `0x8241`) | 117 `0x9009` and 106 `0x6996` instances, all in CCU2 mode. The 16 genuine wide comparators are all variable-vs-variable: PWM/greyscale thresholds and counter compares |
| generalised product terms (multi-level AND trees, both polarities, De Morgan handled) | widest cone constrains 16 nets, 9 of them flops. Product terms whose leaves are all flops with width >= 6: 9 in the device, max width 9, and that one is a "counter == 0" test |
| chip-id values in the microcode ROM | none of `0x014C`, `0x0187`, `0x0214`, `0x0215`, `0x00DE`, `0x00FD`, `0x013C`, `0x00FE` appears as an immediate or as a full 21-bit word |
| any constant in 16.53 but not in the older images | none |

Cross-image counts (AND-of-one-hot clusters / CCU2 constant chains / byte
matches): 16.53 = 6/14/20, 13.39 = 0/15/19, 10.81 = 5/10/15, 9.53 = 6/12/16,
6.69 = 2/20/37. The variation is placement noise, not a feature difference.

One-hot LUT4s are not a comparator signature in this design: every image has
about 2 100 of them (2 069-2 191); they are ordinary logic.

A representative cluster: `R6C30 C1`, INIT `0x8000`, 11 bits, sources
`R11C30_Q6`, `R7C32_F3`, `R8C33_F6`, `R10C34_Q2`, `R9C28_Q0`, `R16C36_Q6`. An
FSM state/condition AND-tree.

### LUT bit order

`INIT[k] = string[k]` with `k = A + 2B + 4C + 8D`
([decode-method.md §5](decode-method.md#5-lut-init-bit-order)). The results
above are invariant under the reverse indexing: reversing a 16-bit INIT is
"complement all four LUT inputs", which maps subcubes to subcubes and one-hot
INITs to one-hot INITs, so every cluster, width and count is unchanged. Only
the polarity of a decoded constant would flip, and there is no chip-id-shaped
constant to flip.

## 5. Limits of the negative

The positive control fails. The design parses Ethernet, yet the same search
finds no 8-bit constant comparator anywhere: no SFD `0xD5`, no EtherType
`0x0107`, no `0x0aXX`. Constants that must be tested are as invisible as the
chip id.

The supported statement is "this design does not build constant comparisons
out of LUT4s", not "the chip id is never compared".

Readings consistent with the netlist, not separated by it:

* Register file plus data-vs-data compare. The id is written into a BRAM or
  LUT-RAM register file by the packet parser and consumed by a sequencer that
  compares it against table data. The 117 CCU2 XNOR comparators have this
  shape, and the same reading accounts for the missing Ethernet constants.
* Bit- or nibble-serial comparison against a streamed constant, which is
  indistinguishable from ordinary logic.
* Only a 2-4 bit field of the id is used, below the detection floor.

The measured result in §3 (`0x014C` arms, `0x0214` does not) shows the card's
behaviour does depend on the id-selected content, so the LUT-level negative is
a search limitation, not a property of the design.

<a id="6-the-lead-that-looked-concrete-refuted"></a>
## 6. `R27C44_Q0..Q3` is an accumulator, not a mode field

A 10-bit high-fanout mode-flag bundle exists:

```
R22C41_Q0, Q1, Q2, Q3, Q6, Q7
R26C42_Q5
R28C44_Q6, Q7
R21C45_Q4
+ a global qualifier R14C31_Q0 (feeds 114 one-hot LUTs)
```

734 LUTs read this bundle and 193 are pure functions of it; simulating all
1024 values gives 1022 distinct equivalence classes, so these are ten
independent, already-decoded flags.

The bundle's fan-in terminates at `R27C44_Q0..Q3`. `R27C44_Q0..Q3` is not a
parameter-loaded 4-bit mode selector; it is an ordinary 8-bit CCU2
accumulator. All four slices are `MODE = CCU2`, giving `F0..F7` and `Q0..Q7`,
all sharing one clock enable `.CE = F7@42,31`. Each bit's routed operand comes
from a different register (`Q0@46,27`, `Q1@46,27`, `Q6@45,27`); it adds a
value to itself.

It appears sourceless to an arc-only tracer because in CCU2 mode the carry
travels on the dedicated carry chain (`FCI <- HFIE0000@(x,y)`,
`FCO -> HFIE0000@(x+1,y)`), fixed connections that are not set arcs. Of the
6956 CCU2-mode LUTs in 16.53, 1012 have zero routed inputs and 2295 have
exactly one. "No combinational source" is the normal appearance of every
increment stage on the die
([decode-method.md §6](decode-method.md#6-how-far-backward-tracing-can-be-trusted-high)).

`R28C39_Q6/Q7` decode a one-hot FSM state at `R30C35`/`R28C35` (the AND cone
evaluates to constant 0 over free inputs); inferred, not re-examined after the
CCU2 correction.

## 7. The parameter store

Not located. What is known:

* The block RAM feeding the top-edge control pads is `MIB_R25C4/C5` EBR0 =
  `EBR@4,25`, `PDPW16KD`, `WID = 1`, not initialised at configuration time,
  so it starts empty and is written at run time
  ([output-stage.md §7.4](output-stage.md#74-the-control-group-source-ram-starts-empty-high),
  [pixel-write-path.md](pixel-write-path.md)).
* A 256-byte pack store was not found.
* LUT-RAM is ruled out as the store in 16.53: 18 blocks of 16x4, against 59
  in 13.39 and 89 in 6.69.

## 8. Which id to send

`0x014C`, with sub-id `0` (`config/chips/sm16269s-factory.toml`, selected by
`config/panels/p25-128x64-sm16269s.toml`). Evidence:

* the vendor chip table names `0x14C` "SM16169SH/SL", and it is the closest
  id libCLTDevice and LEDVISION 9.6 can express for this part;
* firmware 16.53 is filed as `SM16386S_SM16269SH`;
* measured: the panel renders at `0x14C` and is dark at `0x214`.

`0x0214` is a dead id in the vendor code and ships the driver an all-zero
configuration. Do not send it.

## 9. Unresolved

* The full set of ids firmware 16.53 recognises. No list can be derived from
  the bitstream. What would settle it: a sweep of the vendor id table on the
  bench, one `scripts/bench.py run --boot --spec <spec>` per candidate id with
  the panel crop fixed by `scripts/bench.py locate`, reading current and the
  camera. The id is outside the pack CRC-32, so the sweep needs no checksum
  work.
* Whether the card branches on the id or on the id-selected descriptor bytes
  (§3). What would settle it: a `0x0214` pack that differs from the `0x014C`
  pack only at `+0xE7..+0xE8`, diffed byte by byte before sending.
* Where the parameter pack is stored on the die (§7). What would settle it: a
  small, singly-written, CLKOP-clocked EBR in `analysis/fpga/ebr_map_16.53.txt`
  whose address generator belongs to neither large bank, traced to the packet
  parser.
* A definitive comparator. What would settle it: full netlist recovery (LUT +
  FF + BRAM + routing to RTL) and simulation. Deep backward walks are about
  93 % reliable in the interior and worse at the die edge
  ([decode-method.md §6](decode-method.md#6-how-far-backward-tracing-can-be-trusted-high)).
