# Decode method

How the pages in `docs/fpga/` are reproduced from the vendor firmware images
with open-source tooling. No Lattice Diamond, no Colorlight software and
nothing from the vendor is executed; vendor files are only read.

## 1. Tooling

| Tool | Version | Install |
|---|---|---|
| [prjtrellis](https://github.com/YosysHQ/prjtrellis) | 1.4 (Homebrew `prjtrellis 1.4_9`) | `brew install prjtrellis` |
| `ecpunpack` | ships with prjtrellis | |
| `pytrellis` | ships with prjtrellis | see §4 |
| Python | 3.14 for `pytrellis`, any 3.x otherwise | |

Homebrew paths:

```
binaries   /opt/homebrew/bin/ecpunpack, ecppack, ecpbram, ecppll, ecpmulti
database   /opt/homebrew/opt/prjtrellis/share/trellis/database
pytrellis  /opt/homebrew/opt/prjtrellis/lib/trellis/pytrellis.so
```

The formula pulls `boost`, `boost-python3` and `python@3.14`. Building from
source (cmake + boost-python + C++17) also works, takes much longer and
produces a ~1 GB database; the bottle is sufficient.

## 2. Bitstream to text config

The `.hex` files are `.bit` files. No header stripping is needed;
`ecpunpack` parses the ASCII header itself.

The one required step is cutting the trailing padding. `ecpunpack` parses
every command, walks past `DONE` into the `0xFF` fill, reaches the 8-byte
trailer at `0xAFFFC`, hits a `0x00` byte and aborts:

```
bitstream: program DONE
Failed to process input bitstream: Bitstream Parse Error: unsupported command 0x00 [at 0xafff9]
```

The abort discards the whole decode although every command parsed. Truncating
the file just past `DONE` (offset `0x8ED30`) avoids it:

```sh
python3 -c "d=open('in.hex','rb').read(); open('out.bit','wb').write(d[:0x8ed30])"
ecpunpack --idcode 0x41111043 out.bit out.config
```

Output on success:

```
bitstream size: 5768192 bits
bitstream: reset crc
bitstream: Overriding device ID from 0x41111043 to 0x41111043
bitstream: device ID: 0x41111043
bitstream: set control reg 0 to 0x40000020
bitstream: init address
bitstream: settings: 91 1d 8a
bitstream: reading 7562 config frames (with 1 dummy bytes)
bitstream: set USERCODE to 0x00000000
bitstream: program DONE
```

`--idcode` is optional (the stream declares it) and guards against a
wrong-part decode.

### All five images

`analysis/fpga/scripts/repro.sh` (see §7) does the truncation and unpack for
all five images:

```sh
sh analysis/fpga/scripts/repro.sh /tmp/rxp-trellis
```

Resulting `.config` sizes, a sanity check:

| image | bytes |
|---|---|
| 13.39 Normal | 7 555 556 |
| 6.69 LS0allDA | 7 627 032 |
| 9.53 PWM | 7 905 321 |
| 16.53 PWM | 8 152 406 |
| 10.81 PWM | 8 560 772 |

## 3. The `.config` text format

```
.device LFE5U-25F
.comment <the ASCII header, line by line>

.tile <TILENAME>:<TILETYPE>
arc:  <sink> <source>          # a routing connection that is switched ON
word: <NAME> <bitstring>       # a multi-bit setting  (bit order varies per field, see §5)
enum: <NAME> <VALUE>           # a named setting
unknown: F<frame>B<bit>        # a set bit prjtrellis has no name for

.bram_init <n>
<2048 nine-bit hex words, 8 per line>
```

Tile names encode position: `MIB_R37C5:MIB_EBR1` is row 37, column 5.
16.53 has 4132 configured tiles and 248 398 routing arcs.

## 4. Programmatic access with pytrellis

`pytrellis` imports only under `/opt/homebrew/bin/python3.14`:

```python
import sys
sys.path.insert(0, '/opt/homebrew/opt/prjtrellis/lib/trellis')
import pytrellis
pytrellis.load_database('/opt/homebrew/opt/prjtrellis/share/trellis/database')

cc = pytrellis.ChipConfig.from_string(open('t_16.53.config').read())
ch = cc.to_chip()
rg = ch.get_routing_graph(True, True)     # ~8 s for the 25F, both args required
```

### API traps

1. Globalise arc endpoints with `rg.globalise_net`, not `rg.id_at_loc`.
   prjtrellis writes wire names with relative prefixes (`N1_`, `S3_`,
   `W2_`, ...). `id_at_loc` does not resolve them and 62 % of the 248 398
   arcs fail to match. The correct call:

   ```python
   rid = rg.globalise_net(row, col, name)   # (y, x) order
   # returns rid.loc.x == col, rid.loc.y == row
   ```

   With it, 247 740 of 248 398 arcs resolve to distinct sinks, and exactly
   one sink out of 247 740 has more than one driver. That count is the
   sanity check that the decode is right.

2. `RoutingBel.pins` is a list of name idents, not wires. To find the wires
   attached to a bel, iterate `tile.wires` and read each wire's
   `belsUphill` / `belsDownhill`, which are `(RoutingId, ident)` tuples, not
   objects with `.bel` / `.pin` attributes.

3. `rg.tiles` is a `RoutingTileMap`, not a dict, so it has no `.get()`.
   Build `{(tl.loc.x, tl.loc.y): tl for tl in rg.tiles.values()}` once.

4. LUT inputs can be tied to constants with no routing arc. ECP5 slices
   carry `SLICEx.<P><k>MUX` enums; value `1` ties that LUT input to logic 1.
   16.53 has 16 582 of these. Without modelling them, 6264 of 23 199 LUTs
   (27 %) appear to depend on unrouted inputs. After reducing each INIT by
   its tied inputs, zero LUTs depend on an unrouted input.

5. `word:` bit order varies per field, and `BASE_TYPE` names are degenerate.
   See [bitstream-format.md §6](bitstream-format.md#6-the-word-bit-order-trap)
   and [pinout.md](pinout.md#the-base_type-trap). Check
   `database/ECP5/tiledata/<TILE>/bits.db` for every field.

## 5. LUT INIT bit order

For `word: SLICEx.Kn.INIT`:

```
INIT[k] = string[k]        where k = A + 2B + 4C + 8D
```

Proof: LUTs with exactly one routed input use only the strings `0101...`,
`0011...`, `00001111...` and `00000000 11111111` for a routed A, B, C and D
respectively. Under this indexing every one of them is a buffer; under the
reverse indexing every route-through LUT in the design would be an inverter.

This does not contradict the MSB-first reading of the PLL `DIV`/`CPHASE`
words; those are separate database fields with their own bit order. See
[bitstream-format.md §6](bitstream-format.md#6-the-word-bit-order-trap).

<a id="6-how-far-backward-tracing-can-be-trusted-high"></a>
## 6. Limits of backward tracing

Two hard limits.

### CCU2 carry inputs are invisible to an arc-only tracer

In `MODE = CCU2` the carry travels on dedicated, non-configurable
connections (`FCI <- HFIE0000@(x,y)`, `FCO -> HFIE0000@(x+1,y)`) which are
not set arcs. A tracer following only configured arcs sees a CCU2 LUT with a
missing input.

In 16.53: of 6956 CCU2-mode LUTs, 1012 have zero routed inputs and 2295 have
exactly one.

"This register has no combinational source" is therefore the normal
appearance of every increment stage on the die, not a signature of anything.
`R27C44_Q0..Q3`, once read as a parameter-loaded 4-bit mode field, is an
ordinary 8-bit accumulator; see
[chip-id.md §6](chip-id.md#6-the-lead-that-looked-concrete-refuted).

### Deep backward walks dead-end at the die edge

2495 of 38 285 net roots dead-end on a plain span wire, and 1848 of those
are at `x <= 1`, `x >= 71`, `y <= 1` or `y >= 49`. prjtrellis clamps
out-of-range span-wire locations to the die boundary, which destroys the
information needed to continue.

An "undo the clamp" fallback resolves a unique candidate for only 891 of
2477 dead-ends; it is unsafe and is not used.

Practical impact: only 45 of the 96 RGB pads resolve to a driver cell.
Backward tracing is roughly 93 % reliable in the interior and much worse at
the edge, which is where the IO is.

Prefer forward reasoning, or one-hop IOLOGIC configuration, over deep
backward walks. The 96 RGB pins were identified by a one-hop IOLOGIC
signature, not by tracing.

## 7. Scripts

The scripts that produced these pages lived in `analysis/fpga/scripts/`,
which is not kept in the repository. They expect the `.config` files in the
current directory (or a path edited at the top of each script).

| script | what it does |
|---|---|
| `repro.sh` | firmware `.hex` -> truncated `.bit` -> `.config`, all five images |
| `netbuild.py` | arcs -> absolute-node graph using `globalise_net` |
| `final.py`, `props.py`, `pintable.py` | per-pin routing evidence -> the pin table |
| `clocks.py` | PLL, DCC/DCS global clock muxes, edge clocks, clock domains |
| `extract.py` | LUT INIT dump with constant-tie reduction |
| `netlist.py` | global wire canonicalisation |
| `analyze.py`, `hunt.py`, `terms2.py`, `cones.py` | one-hot / product-term comparator search |
| `chains.py` | CCU2 carry-chain comparator candidates |
| `bus.py` | register-bus discovery |
| `parse4.py` | per-EBR-instance settings parser |
| `padscan3.py`, `diag.py` | pad-usage scans built on `id_at_loc`; superseded, they exhibit the failure in §4 trap 1 |

## 8. Large intermediates

`arcs.pkl` (~5 MB) and `full_16.53.pkl` are scratch files and are not
committed. `netbuild.py` / `full.py` regenerate them after `repro.sh`, about
a minute per image.
