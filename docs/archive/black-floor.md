# The fixed black floor: the void-line path, and what it rules in and out

> Archived. Superseded by [rendering.md](../rendering.md) ("The black floor"):
> the void-line column table decoded here is what `Block7Builder::void_line_columns`
> writes, and with it black measures 0.466 A (LEDs off). Kept for the decode
> and the verification of the other candidate tables.

2026-09-01. Static analysis only — nothing was executed, nothing touched the
card. Every hex address is a byte offset in the macOS build
`libCLTDevice.1.dylib` (`__TEXT` VAs = file offsets); the disassembly used is
`<scratch>/libCLTDevice.asm` (a scratch directory outside the tree). Image offsets are offsets into the 0x8000-byte
compiled parameter image (flash `0x70000`), per
[compiled-image-format.md](../compiled-image-format.md).

**The bench fact this file exists to explain.** An all-black frame leaves a
*fixed* per-slot, per-colour duty (~24 % of white): red lit on chain slots
0–127 and dark on 128–255, blue lit on all 256, green in between. It is
identical across power cycles and after other content (corr. 0.997–0.999),
scales only with the `0x0107` channel-gain bytes, is invariant to every driver
register and to the grey byte, and **changes shape with the load-length fields**
(CardScanLen / MaxPsc at basic-pack body `+0x0D`, `+0x39`, `+0xE3`, `+0xE5`).

**Headline.** The type-`0x1F` void-line table is now fully decoded, and the
bench result "both void-line regions = `0xFF` → panel completely dark" is
exactly what the decode predicts. But the decode also shows **our bytes are the
correct vendor output** — all-zero, byte-identical to the factory image, and
independently corroborated by the anti-void table the vendor derived *from* that
same void table. So candidate (a) is **not a misconfiguration**; what it gives
us instead is a proven, per-position output gate that can be used as a probe.
Candidates (b), (c) and (e) are closed as verified-correct below. The floor's
source is card-side, and §6 says how to localise it with two reflashes.

---

## 1. The type-`0x1F` void-line table — VERIFIED

### 1.1 The pack

`CRcvCommandManager::GetSendCMD_VoidLineInfo` @ **`0x199860`** allocates four
`SVoidLineInfoPack` of **`0x40C` bytes**, `bzero`s `0x40B` bytes from `+1` and
writes the type byte:

| pack offset | size | content | citation |
|---|---|---|---|
| `0x00` | 1 | type **`0x1F`** | `movb $0x1f, -0x1060(%rbp)` `0x1998d1` (and `…c54`, `…848`, `…43c`) |
| `0x01`–`0x02` | 2 | zero | `bzero` `0x1998cc` |
| `0x03` | 1 | pack index | `movb %r14b, -0x5(%rbx)` `0x1e5958`, rbx = pack+8 |
| `0x04`–`0x07` | 4 | zero | |
| `0x08` | `0x400` | payload slice | `memcpy` `0x1e5935` |

`CSendAndSaveRcvParam::GetVoidLineInfoPacks` @ **`0x1e58c0`**:

```
nPacks = 2 + 2 * IsSupportLargeLoad()          ; 0x1e58da .. 0x1e58e6
ChangeVoidLineDataFromNormalToCustom()         ; 0x1e58f0
src    = OBJ+0xD4C8                            ; a 0x1000-byte buffer
```

and the per-pack source offset comes from the jump table at `0x1e5998`
(entries `-0x23, -0x18, -0x78, -0x23` → `0x1e5975`, `0x1e5980`, `0x1e5920`,
`0x1e5975`):

| pack | source offset in the 0x1000 buffer | lands at image |
|---|---|---|
| 0 | `0x000` | `0x1000` |
| 1 | `0x800` | `0x1400` |
| 2 | `0x400` | `0x6800` (large load only) |
| 3 | `0xC00` | `0x6C00` (large load only) |

### 1.2 What the 0x1000-byte buffer is

`CHWParamRcvGeneral::ChangeVoidLineDataFromNormalToCustom` @ **`0x160160`**:

* returns immediately if `OBJ+0xD4D0 != 0` (a "custom void data already loaded"
  flag) — `0x16016e`;
* otherwise `bzero(OBJ+0xD4C8, 0x1000)` — `0x160188`;
* **returns with the buffer still all zero if the void-line count is 0**
  (`vt+0xD8` → `testb %al,%al; je` at `0x1601d2`);
* otherwise walks the void groups and does `addb %bl, buf[i]` over a suffix of
  the range — an accumulating **offset** per index, not a flag.

Two branches, and they are what fix the buffer's layout:

| branch | selector | index limit | written at | uses |
|---|---|---|---|---|
| row / line | `vt+0xF8` == 0 | `0x7FF` (`0x1603e2`) | `buf[0x000..0x7FF]` | `GetHeightByNormalVoid` `0x160362`, accumulator `OBJ+0xE116` |
| column | `vt+0xF8` != 0 | `0xFFF` (`0x16029f`) | `buf[0x800..0xFFF]`, via `orl $0x800, %r15d` `0x160258` | `GetWidthByNormalVoid` `0x160226`, accumulator `OBJ+0xE118` |

So:

> **`buf[0x000..0x7FF]` = 2048 per-LINE offsets, `buf[0x800..0xFFF]` = 2048
> per-COLUMN offsets, one unsigned byte each.** Entry `a` is the number of void
> positions inserted before real position `a`; the physical position is
> `a + buf[a]`.

Without large-load support the card is sent only the **first 1024 entries of
each axis** (packs 0 and 1). Image `0x1000..0x13FF` = line offsets 0–1023,
image `0x1400..0x17FF` = column offsets 0–1023.

### 1.3 The remap semantics, proved by the anti-void derivation

`CHWParamRcvGeneral::GetAntiVoidLineParam(uchar* dst)` @ **`0x1604d0`** builds
the type-`0x32` table *from* the void table, in three passes over a `0x2000`
buffer (`dst[0x0000..0x0FFF]` = line axis, `dst[0x1000..0x1FFF]` = column axis,
2048 big-endian u16 each):

```
1. every entry's high byte |= 0xA0                       ; 0x160500-0x160518
   (bit7 = "void", bit5 = constant marker)
2. for a in 0..0x7FF:                                    ; 0x160530-0x160575
       p = a + voidbuf[a]        ; line axis
       if p <= 0x7FF: dst[2p]     &= 0x7F     ; clear "void" on the image of a
       q = a + voidbuf[0x800+a]  ; column axis
       if q <= 0x7FF: dst[0x1000+2q] &= 0x7F
3. running rank counters (init 0xFFFF), incremented for every NON-void entry;
   entry = ((rank>>8 | flags) & 0xBF) << 8 | (rank & 0xFF)   ; 0x160590-0x1605e6
```

This is decisive in three ways:

* it confirms `physical = real + offset` — step 2 marks exactly the *images* of
  the real positions as non-void;
* it explains the bench result: `voidbuf = 0xFF` ⇒ `p = a + 255`; for `a` in a
  256-slot line every image is still ≤ `0x7FF`, but every *real* position is
  displaced by 255, so nothing the card wants to emit lands where the panel is
  — **the panel goes dark, including the floor**;
* it means **a non-void entry is `0x00`, not a marker.** `0x2000` (bit 13) is
  the marker; it lives in the *anti-void* table, not this one.

`CDataRemappingManager::GetValueAfterVoidLine` @ `0x114fc0` and
`CalReadVoidLineTable` @ `0x114ec0` read the table back the same way: they test
`entry & 0x8000` (`cmpb $0x0,(%rbx,%rcx,2); jns`, `0x115019`) and return
`0xFFFF` for a position that is itself void.

### 1.4 The anti-void packs, for completeness

`GetAntiVoidLineInfoPacks` @ **`0x1e59b0`**: `nPacks = 4 + 4*IsSupportLargeLoad`,
same `0x40C` pack shape, source offsets from the bitmask dispatch at
`0x1e5a60`–`0x1e5a82` (`0xC3` → `i*0x400`; `0xC` → `0x800+i*0x400`; else
`(i-1)*0x400`):

| pack | source | image |
|---|---|---|
| 0,1 | `0x0000`, `0x0400` | `0x1800`, `0x1C00` — line axis, entries 0–1023 |
| 2,3 | `0x1000`, `0x1400` | `0x2000`, `0x2400` — column axis, entries 0–1023 |
| 4–7 | `0x0800`,`0x0C00`,`0x1800`,`0x1C00` | `0x7000`… — entries 1024–2047, large load only |

### 1.5 The exact bytes for our geometry (128x64, 1/16, one 256-slot lane)

`IsSupportLargeLoad` is false at a 256-clock load, and there are no void lines
or void points (basic-pack body `+0x1F` void-point count = 0), so:

| image | length | vendor value | ours |
|---|---|---|---|
| `0x1000`–`0x13FF` | 0x400 | **`00` x 1024** (line offsets, identity) | same |
| `0x1400`–`0x17FF` | 0x400 | **`00` x 1024** (column offsets, identity) | same |
| `0x6800`–`0x6FFF` | 0x800 | zeros (packs 2–3 not produced; the image writer leaves the region zeroed) | same |
| `0x1800`–`0x1BFF` | 0x400 | `20 00 20 01 … 23 FF` (line axis: `0x2000+n`) | same |
| `0x1C00`–`0x1FFF` | 0x400 | `20 00 20 01 … 23 FF` (column axis) | same |
| `0x7000`–`0x7FFF` | 0x1000 | zeros | same |

**Verified against the card**: `card-dumps/primary-region.bin` at `0x71000` and
`0x76800` are all zero (2048 bytes each), `0x71800` is `0x2000+n` twice, and
`0x77000` is all zero — and our generated `build/p25-128x64-sm16269s-block7.bin`
matches. The factory image's anti-void table being exactly `0x2000+n` with **no
`0x8000` bit anywhere** is independent proof that the void table it was derived
from was all zeros, i.e. that all-zero *is* the vendor's output for a module
with no void lines.

> **Conclusion for candidate (a): the void-line table is correct as written.
> `0x00` does not mean "void: output fixed data" — it means "no displacement".
> The `0xFF` experiment did not find a bug; it demonstrated that the card
> applies this remap to every position it drives, the floor included.**

---

## 2. Candidates closed by this analysis

### (b) data-swap / colour source / current exchange — CLOSED, our zeros are right

`CSendAndSaveRcvParam::GetCurrentExchangeParamPack` @ **`0x1f6890`** sets
`pack[5] = 1` and asks `CRCVCurrentDataManager::GetCurrentExchangeParam(pack+0xC,
0x100)` for the 256-byte body (image `0x0C00`). That function (body around
`0x1adb20`, the symbol is local) copies the 128 data-swap bytes from
`OBJ+0xD3CC..0xD44B` (`0x1adb9d`–`0x1adbf0`) and then, for every module
position, does

```
g = GetGroupIndexFromDataSwap(GetModuleIndexFromActualPos(...), swapCopy)   ; 0x1adc80
dst[g] = GetModuleIndex(...)                                               ; 0x1adcfc
```

— i.e. **a hub-data-group → module-index map**, one byte per group. With a
single module every group maps to module **0**, so the vendor's output is the
all-zero buffer it started from. This resolves the "may differ from vendor
output — NOT RESOLVED" note in
[compiled-image-format.md](../compiled-image-format.md) for image `0x0C00`:
**zeros are correct for one module.** (VERIFIED for the writer; the value
`GetModuleIndex` returns for module 0 is inferred to be 0 — MEDIUM.)

There is no per-lane padding data anywhere in the data-swap pack: it is the
64-byte lane map plus three `01 00` deseam fixed-point 1.0 pairs.

### (c) module-position table (type 0x17) — CLOSED

The vendor's all-zero output is the `> 64 tiles` bail-out only
(`GetDefaultModulePos` @ `0x1558b0`). Our screen is 8 x 4 = **32** tiles of the
16 x 16 grid unit, under the limit, so a real table with count `0x20` is what the
vendor would emit. Our table is that. Nothing here can make the card treat part
of a line as another module — the table carries `[outer, inner, x, y, w, h]`
rectangles of the *screen*, not of the shift chain.

### (e) "CardScanLen 256, extent 128" — CLOSED, that ratio is the vendor's own

Read from the two images (`card-dumps/primary-region.bin` `0x70000` vs
`build/…-block7.bin`), basic-pack body offsets:

| field | factory (256x384 wall) | ours (128x64) |
|---|---|---|
| `0x04` module `[H/2, W]` | `20 80` | `20 80` |
| `0x06` modules in line dir | `02` | `01` |
| `0x0B` OneScanLen | `0100` = 256 | `0100` = 256 |
| `0x0D` CardScanLen | `0200` = 512 | `0100` = 256 |
| `0x39` CardScanLen (per split) | `0200` | `0100` |
| `0x3B` extent in line dir | `0100` = 256 | `0080` = 128 |
| `0xE3`/`0xE5` MaxPsc | `0200` | `0100` |

The factory has extent 256 against CardScanLen 512 — **the same 2:1 ratio we
have**, and it is arithmetically forced: `OneScanLen = W·(H/2)/scan = W·2` for a
128x64 at 1/16, i.e. two row-groups share one 2·W-slot shift. The card is
*supposed* to be told "the line is 128 wide and the chain is 256 slots".
Nothing is wrong here.

### (d) scan table — not a *configuration* differentiator

Our scan table is byte-identical to the factory's, and
[output-stage.md §3](../fpga/output-stage.md) records that the generator produces
the same table for a 256- and a 512-clock load (the width enters only a frame-
time estimate). A table that does not change with the load cannot be the thing
whose *shape* changes when CardScanLen changes. The `(start,end)` pairs at
`+0x3C0` and the segment schedule remain the mechanism that converts whatever is
in the line buffer into duty — but they are not carrying a per-slot pattern.

### (f) default markers — searched, nothing found

The pack builders that touch per-position data are
`GetVoidLineInfoPacks` (`memcpy` of a `bzero`ed buffer),
`GetAntiVoidLineInfoPacks` (`0x2000|rank`, `0x8000` only for genuinely void
positions), `GetPixelSequencePacks` @ `0x1e5aa0` (`bzero` then the mapping) and
`GetVoidTablePack` @ `0x1e5710` (bails, zeros). The only non-zero constants any
of them writes into per-position data are the anti-void `0x20` marker and the
`0x8000` void flag. **No builder writes `0xFFFF`, `0x8000` or any other filler
into slots it considers unused.** — VERIFIED negative over these five builders.

---

## 3. What the `0xFF` result actually tells us — INFERRED but tight

1. The floor is emitted **through the card's normal per-position output path**;
   it is not leakage, not a stuck OE, and not the driver free-running. If it
   were any of those, displacing the position map could not extinguish it.
2. The floor is therefore **content in the line buffer** (`EBR@4,25`, 512 x 36,
   [pixel-write-path.md §3.1](../fpga/pixel-write-path.md)) for real slot
   positions, and pixel data **adds on top of it** rather than replacing it —
   sent patterns render correctly over it.
3. It is bit-exact across power cycles, so its source is deterministic: either a
   counter or a table the card loads identically from flash on every boot. Not
   uninitialised SRAM (that would not correlate 0.999 across a 45 s power-off),
   and not stale pixels (invariant to displayed content).
4. Its amplitude (~24 % of white) is within rounding of **one bit-plane at level
   12 of a 14-level schedule** (2^12/(2^14−1) = 25.0 %). Combined with (2), the
   economical reading is: *for one bit-plane slot per frame the card sources the
   RGB lanes from something other than the pixel buffer.*
5. Its boundary is at **slot 128 = the module width = the row-group boundary**
   (`slot = group·W + col`, `crates/e120-rcvbp/src/spec/mapping.rs`), not at 64.
   So the red lane differs between the two row-groups of one scan address —
   which is a *slot-index* structure, not a panel-column structure.
6. Its dependence on CardScanLen says the fill is addressed with the load length
   — consistent with (4)+(5) and with `EBR@4,25` being a per-scan-address line
   buffer of exactly `CardScanLen` slots.

None of (1)–(6) is a configuration byte we get wrong. Every configuration
candidate in the brief is now either verified-correct (§1.5, §2) or shown to be
width-independent (§2d). **The remaining hypothesis is card-side: one plane's
worth of the line buffer is filled from a fixed internal source.** That is not
decidable from the vendor library — the library has no model of the card's line
buffer at all — and [output-stage.md §7.3](../fpga/output-stage.md) already
records the unresolved 2:1 mux in the output stage (CCU2 counter vs block-RAM
data-out) as exactly this kind of "fixed source vs live data" selection.

---

## 4. Confidence register

| claim | status |
|---|---|
| `SVoidLineInfoPack` = `0x40C` B, type `0x1F` at `+0`, index at `+3`, `0x400` payload at `+8` | **verified** (`0x1998d1`, `0x1e5958`, `0x1e5935`) |
| 2 packs without large load, 4 with; source offsets `0x000/0x800/0x400/0xC00` | **verified** (`0x1e58da`, jump table `0x1e5998`) |
| Buffer = 2048 line offsets at `0x000`, 2048 column offsets at `0x800` | **verified** (`0x160258` OR `0x800`; limits `0x7FF`/`0xFFF`) |
| Entries are byte offsets; `physical = real + offset`; `0x00` = no displacement | **verified** (`0x1602c8` accumulate; `0x160552`/`0x160563` image marking) |
| A no-void module gets an all-zero table | **verified** (early return at `0x1601d2` after the `bzero` at `0x160188`) |
| Anti-void entry = `0x2000` + rank, `+0x8000` if void; bit 6 forced 0 | **verified** (`0x160500`, `0x1605cd`, `0x1605e6`) |
| The factory card's void table was identity | **verified** — its anti-void image is `0x2000+n` with no `0x8000` anywhere |
| Our `0x1000`/`0x1400`/`0x6800`/`0x1800`/`0x7000` bytes equal the factory's | **verified** — byte census of `card-dumps/primary-region.bin` |
| Current-exchange body = group→module-index map; zeros correct for one module | **verified** for the writer (`0x1adc80`, `0x1adcfc`); **medium** that module 0 → 0 |
| extent 128 with CardScanLen 256 is the vendor relationship | **verified** — factory 256/512 is the same ratio |
| No pack builder emits a filler word for unused positions | **verified negative** over the five per-position builders |
| The floor is one bit-plane sourced from a fixed card-internal source | **inferred** (§3) |
| The floor's index space is the 256-slot chain rather than the 128-column module | **inferred** from the slot-128 boundary; §6 A settles it |

---

## 5. What did NOT get resolved

* Which internal source fills the plane. The library cannot say; the netlist
  question is the `Q5@23,18` 2:1 mux and `Q6@9,27` (the line buffer's only write
  gate) in [output-stage.md §7.2–7.4](../fpga/output-stage.md).
* `OBJ+0xD4D0`, the "custom void data" flag that makes
  `ChangeVoidLineDataFromNormalToCustom` return without rebuilding. Its record
  0x01 offset was not traced. Irrelevant while there are no void lines, but it
  is the switch that would let a hand-written void table survive a vendor
  rebuild.
* `vt+0xF8` (row-vs-column void axis selector), `vt+0xC8` (start void line),
  `vt+0xD8` (void line count), `vt+0xE8` (void line spacing) — named by their
  `Get/SetVoidLine*` accessors at `0x161d70`-ish but not mapped to record
  offsets. All are 0 for us.

---

## 6. The two experiments worth the reflash

Both use the void-line table as a **probe**, which §1.3 proves the card obeys.
Both are single-page flash writes into the boot image and are reverted by
restoring zeros.

### A. Which index space is the floor in? (do this first)

Write the **column** half only, image `0x1400`–`0x17FF`:

```
0x1400 + a = 0x00   for a = 0..127
0x1400 + a = 0xFF   for a = 128..255
0x1400 + a = 0x00   for a = 256..1023
```

Leave the line half (`0x1000`–`0x13FF`) all zero. For a vendor-consistent image
also rewrite the anti-void **column** block at image `0x1C00`–`0x1FFF` by the
§1.3 rule — non-void set = `{0..127} ∪ {256..1023}` (the displaced `383..510`
are already inside it), rank = running count of non-void positions:

```
p in 0..127     -> 0x2000 | p          (BE)
p in 128..255   -> 0xA000 | 127        (void)
p in 256..1023  -> 0x2000 | (p - 128)
```

**Read-out.** If the floor and the picture both vanish on chain slots 128–255,
the card's column axis *is* the chain slot and the floor lives in slot space —
then repeat with the halves swapped to confirm, and the floor's shape is a
function of slot index, which points the netlist work at the line-buffer
address generator. If instead a *different* 64- or 128-column band of the panel
goes dark, the card's column axis is the module column (0–127) and the
`0x0F`/`0x3B` extent is what indexes it. If the floor survives untouched while
the picture goes dark on those slots, the floor is injected **downstream of the
void remap** and every configuration hypothesis is dead — which is itself the
most valuable possible outcome.

### B. What is the line axis? (one page, no anti-void edit needed if you accept the inconsistency)

Write the **line** half, image `0x1000`–`0x13FF`:

```
0x1000 + a = 0x00   for a = 0
0x1000 + a = 0xFF   for a = 1..1023
```

If exactly one scan address stays lit, the line axis is the scan address
(0–15). If one panel row band stays lit, it is the panel row (0–63). Either
answer tells you how many of the 1024 entries the card actually consults, and
whether the floor is per-scan-address or per-row — which the camera can read off
one frame.

### Not worth doing

Sweeping the void table for a "correct" value: §1.5 shows the correct value is
the one we already ship, twice over (vendor code path and factory image).
Re-checking the module-position table, the current-exchange page, or the
extent/CardScanLen pair: closed in §2.
