# The chip-protocol fields of record 0x01

The bytes of record 0x01 that describe the driver chip's serial protocol to
the card: the 20-byte `SChipControl` block, the `SChipCustom`,
`SChipCustomEX` and `SChipGrobalConfig` structures, the chip id, and the
record bytes that look like GCLK settings but never reach the card.

Sources. Every hex address below is a file offset in the macOS build
`libCLTDevice.1.dylib`; `full.asm` is its `objdump -d` with demangled names.
Vendor field defaults are cross-checked against 1146 vendor `.rcvbp` files
that parse (`vendor/led-config-files` plus LEDVISION 9.6 `config_files/`) and
the chip-name tables in `CLTInterface.dll` from LEDSet 2.26. Several bytes are
copied whole by the vendor library and never read field by field; their
meaning is listed under [Unresolved](#8-unresolved). Where
`docs/fpga/chip-protocol-microcode.md` assigns a meaning to such a byte from
corpus patterns and open-source driver profiles, that meaning is given here as
"inferred".

## 0. The chip-parameter block in `CHWParamRcvGeneral`

All five chip structures are contiguous members of the object, each with a
whole-struct getter/setter pair.

| member | object offset | size | accessor (address) | record 0x01 offset | basic-pack body offset |
|---|---|---|---|---|---|
| `SChipCustom` | `+0xD4D1` | 16 | `GetChipCustom` `0x16dc50` / `SetChipCustom` `0x16dc70` | `+0x06A..0x079` | `+0x70..0x7F` |
| `SChipCustomEX` | `+0xD4E1` | 4 | `GetChipCustomEX` `0x16dc80` / `Set…` `0x16dc90` | `+0x0E0..0x0E3` | `+0xD0..0xD3` |
| `SChipCustomPlus` | `+0xD4E5` | 256 | `GetChipCustomPlus` `0x16dca0` (a 256-byte `memcpy`) | record 0x84 (the driver-register table) | none |
| `SChipControl` | `+0xD5E5` | 20 | `GetChipControl` `0x16e0f0` / `SetChipControl` `0x16e110` | `+0x0C4..0x0D7` | `+0x91..0xA4` |
| `SChipGrobalConfig` | `+0xD5F9` | 6 | `GetChipGrobalConfig` `0x16e070` / `Set…` `0x16e090` | `+0x0EA..0x0EF` | `+0xD8..0xDD` |
| `SChipCustom5th` | `+0xD5FF` | 6 | `GetChipCustom5th` `0x16e0b0` / `Set…` `0x16e0d0` | not in record 0x01 | none |

`GetChipControl` is `movups 0xd5e5(%rsi),%xmm0` + `movl 0xd5f5(%rsi),%ecx`:
16 + 4 = 20 bytes.

`SChipControl` is vtable slot `vt+0x198` (get) / `vt+0x1A0` (set). The
`vt+0x198` out-struct at record `+0x0C4` is the chip-control block, not
"dead-pixel current data" as `docs/record-0x01-fields.md` lists it.

### How it reaches the card

`CSendAndSaveRcvParam::GetBasicParam` `0x1dfb50` calls
`CSendAndSaveRcvParam::SetChipControlParam(SBasicParamPack*)` `0x1e3b00`
(call site `0x1e2a0b`). That function:

1. applies per-chip-id fixups to the object's `SChipControl`
   (`0x1e3b28`–`0x1e48c8`);
2. copies the finished 20 bytes into the pack at pack `+0x95` / body `+0x91`
   (`movups %xmm0, 0x95(%r13)` `0x1e48f0`, `movl %eax, 0xa5(%r13)` `0x1e48e2`);
3. calls `CHWParamRcvGeneral::SetGclkNumsOfChipControlByChipCustom` `0x151580`
   (`0x1e4969`) on the pack's copy, passing `&pack[0x95]`, `pack[0x74..0x83]`
   (`SChipCustom`), `pack[0xDC..0xE1]` (`SChipGrobalConfig`), a 256-byte
   `SChipCustomPlus` fetched via `vt+0x130`, and the chip id.

Bytes 10–13 of the block in the boot image are therefore recomputed at
pack-build time and need not match what the `.rcvbp` stores. The generator in
this repository (`crates/rcvbp/src/spec/basic_pack.rs`, `put(0x91, …)`)
copies the record bytes verbatim. That is correct only while the record's own
bytes 10–13 equal what the vendor would compute; for the reference
configuration they do (§2).

## 1. Per-byte table for `SChipControl`

Sources: `CHWParamRcvGeneral::ResetChipControl()` `0x1454d0` (jump table at
`0x146030` indexed by `chipType − 0x10`, covering ids `0x10..0x15D`) and
`SetGclkNumsOfChipControlByChipCustom` `0x151580` (jump table at `0x1530FC`
indexed by `chipType − 0x12`).

* chip `0x14C`: `ResetChipControl` case at `0x145517`, shared with ids
  `0x47 0xBB 0xC1 0xC2 0xD6 0x110 0x11A 0x125 0x12A 0x12C 0x135 0x13C 0x14A`.
* chip `0x2F`: case at `0x145645`, shared with `0xC4 0xCA 0xE3`.
* ids not in the table get the whole 20 bytes zeroed (`0x145fdf`).

| byte | record off | pack body | 0x14C | 0x2F | where it is written | meaning |
|---|---|---|---|---|---|---|
| 0 | `+0x0C4` | `+0x91` | `00` | `00` | `movl $0x05010E00, 0xd5e5` `0x145583` / `movl $0x04030E00` `0x145645` | not resolved; 0 in every chip's block |
| 1 | `+0x0C5` | `+0x92` | `0e` | `0e` | same dword | `0x0E` in every non-zero branch of `ResetChipControl`: a family constant, not a parameter. Inferred: pre-activation LE tail length |
| 2 | `+0x0C6` | `+0x93` | `01` | `03` | same dword | Varies 1–7 by chip; for ids `0xCD/0x111` it is `2·vt+0x610() + 0x1B` (`0x145996`), for `0xBF/0xCB` it is `3 + 3·(chip==0xBF)` (`0x145961`), `7` for `0x74` (`0x14571f`); a small count or index, not a flag. Inferred: protocol-variant selector |
| 3 | `+0x0C7` | `+0x94` | `05` | `04` | same dword | not resolved. Inferred: register (CFG1) LE tail length |
| 4 | `+0x0C8` | `+0x95` | `06` | `08` | `movw $0x0106, 0xd5e9` `0x14558d` / `movw $0x0108` `0x14564f` | not resolved. Inferred: second-command (CFG2) LE tail length |
| 5 | `+0x0C9` | `+0x96` | `01` | `01` | same word | `01` in almost every branch. Inferred: data-latch LE tail length |
| 6 | `+0x0CA` | `+0x97` | `03` | `03` | `movb $0x3, 0xd5eb` `0x145596`; for 0x2F only when sub-id `== 0x8A` (`0x145688`) | Values 2, 3, 5 across the chip table. Inferred: VSYNC LE tail length |
| 7 | `+0x0CB` | `+0x98` | `00` | `00` | zeroed unless chip is one of `0xC1`, `0xC2`, `0x135` (`0x145550`); some chips get it from `SetGclkNums…` (e.g. `0x151616`: `= chipCustom[8]`); `SetChipControlParam` sets it to `vt+0x98() & 0x0F` when `IsSpecialNotPWMType()` (`0x1e404e`) | a nibble-width quantity tied to the output model on non-PWM chips (inferred) |
| 8 | `+0x0CC` | `+0x99` | `00` | `00` | same word as byte 7; on the `IsSpecialNotPWMType` path `= round(OBJ+0xDEFC × k)` (`0x1e4059`–`0x1e408b`), derived from the minimum-OE float | min-OE-derived count on non-PWM chips (inferred) |
| 9 | `+0x0CD` | `+0x9A` | `00` | `00` | `movb $0x0, 0xd5ee` | not resolved |
| 10–13 | `+0x0CE..0x0D1` | `+0x9B..0x9E` | `00 97 00 97` | `00 81 00 81` (variants `01 01 01 01`, `02 01 02 01`) | `SetGclkNumsOfChipControlByChipCustom`, §2 | a 16-bit GCLK / "scan cycle level" count, big-endian, repeated: `(hi, lo, hi, lo)` |
| 14–15 | `+0x0D2..0x0D3` | `+0x9F..0xA0` | `00 08` | `00 10` | `movw $0x0800, 0xd5f3` `0x14557a` / `movw $0x1000` `0x145668` | not resolved. Reads as big-endian `0x0800` / `0x1000`; other chips get `0x0000`, `0x0400`, `0x0500`, `0x1000` |
| 16 | `+0x0D4` | `+0xA1` | `02` | `00` | not written by `ResetChipControl` for either chip (only `chip == 0x11A` gets `1`, `0x145573`); some chips get `vt+0xA8()` in `SetChipControlParam` (`0x1e3e84`) | not resolved. For chip 0x14C the reset path never assigns it; the `02` in the reference file was written by an earlier LEDVISION save and its origin is outside the reset path |
| 17 | `+0x0D5` | `+0xA2` | `00` | `00` | `movb $0x0, 0xd5f6` at the top of `ResetChipControl` (`0x1454eb`), unconditionally zero for every chip; only `0x36/0x40` later set `0x20` (`0x1e3ed4`) | always 0 for this chip |
| 18–19 | `+0x0D6..0x0D7` | `+0xA3..0xA4` | `0a 02` | `00 00` | `movw $0x020A, 0xd5f7` `0x1455a7`; the 0x2F group never writes it | not resolved |

Both observed 20-byte values are fully accounted for except byte 16: every
other byte is a literal from the chip's `ResetChipControl` case, a zero it
writes, or (10–13) the computed GCLK count.

## 2. Bytes 10–13: the GCLK count

`SetGclkNumsOfChipControlByChipCustom(SChipControl& sc, SChipCustom, SChipGrobalConfig, SChipCustomPlus, int chipType)`
`0x151580`. Register layout: `rdi=this`, `rsi=&sc`, `rdx:rcx = chipCustom[0..15]`,
`r8 = grobalConfig`, stack (`rbp+0x10`) `= chipCustomPlus[0..255]`, `r9d = chipType`.
Jump table `0x1530FC`, index `chipType − 0x12`.

### chip 0x14C, case `0x151ADB`

```
level = SSM16169SHChipCustomPlus::GetScanCycleLevel()          ; 0xf0840
if (GetChipTypeEx() == 0x14D)                                  ; 0x151af7
    level = SSM16269ChipCustomPlus::GetScanCycleLevel()        ; 0xf0670
sc[10] = level >> 8;  sc[11] = level & 0xFF;
sc[12] = level >> 8;  sc[13] = level & 0xFF;                   ; 0x151b0d..0x151b19
```

Both `GetScanCycleLevel` implementations read one byte: `SChipCustomPlus[0x15]`
(`movzbl 0x15(%rdi)`, `0xf084e` / `0xf067e`). `SChipCustomPlus` is record 0x84,
so byte `0x15` = quad 5, R slot = register `0x07`, red value (register order
`0x02,0x03,0x04,0x05,0x06,0x07,…`). Both routines are auto-vectorised into two
identical lanes plus a scalar recomputation and return `max()` of the three, so
they reduce to a single scalar formula. With `b = reg07_R`:

`SSM16169SHChipCustomPlus::GetScanCycleLevel` (used when sub-id is not 0x14D):

```
A = (b & 0xC0) ? 2 : (b >> 5) + 1          ; 0xf093a-0xf094b
u = (b >> 2) & 1 ; v = b & 3 ; n = (b >> 3) & 3
level = ceil( trunc(128 · 2^n) / A  +  (v + 10u + 12)  +  1 )
```

(`128.0` at `0x3fc770`/`0x3fc7f0`, `10` at `0x3fe310`, `12` at `0x3fe330`,
`ldexp(1.0, n)` via the `_ldexp` stub `0x3fb6e6`, `roundps $0xa` = ceil.)

`SSM16269ChipCustomPlus::GetScanCycleLevel` (used when sub-id == 0x14D):

```
A = (b >> 5) + 1 ; u = (b >> 2) & 1 ; v = b & 3 ; n = (b >> 3) & 3
C     = ceil( (v + 6 + trunc(23·A / (2 − u))) / A )
level = ceil( trunc(64 · 2^n) / A  +  C  +  1 )
```

(`23.0` at `0x3fe2d0`/`0x3fe0c4`, `6` at `0x3fe2f0`, `64.0` at `0x3fc7b0`/`0x3fc7f8`.)

Check against the reference file
`third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`: its record
0x84 has `reg 0x07 = 04 04 04`, its sub-id (`+0x0E9`/`+0x205`) is `0x0000`,
so the SM16169SH branch applies: `A=1, u=1, v=0, n=0` gives
`1·128/1 + (0+10+12) + 1 = 151 = 0x97`, which is bytes 11 and 13 of
`00 0e 01 05 06 01 03 00 00 00 00 97 00 97 00 08 02 00 0a 02`. This pins the
decode.

Consequences:

* The generated config (`build/p25-128x64-sm16269s.rcvbp`) has the same
  `reg 0x07 = 0x04` and the same sub-id `0x0000`, so `0x97` is correct and
  self-consistent for it.
* With `sub_id = 0x14D` (as `config/chips/sm16269-defaults.toml` has) the vendor
  computes `sm16269(0x04) = 94 = 0x5E`, not `0x97`; with the vendor
  `reg 0x07 = 0x44` as well it becomes `48 = 0x30`. Changing `reg 0x07` or
  the sub-id without recomputing bytes 10–13 desynchronises the card's GCLK
  count from the chip's own frequency-division setting. The chip library
  stores `chip_control` as a literal, so `config/chips/sm16269-defaults.toml` pairs
  `sub_id = 0x14D` + `reg 0x07 = 0x44` with a `chip_control` of `0x97`, a
  combination the vendor never emits (it would emit `0x30`).

### chip 0x2F, case `0x151BCF`

Acts only when `GetChipTypeEx() == 0x102`; for the corpus sub-id `0x8A` it
falls through to `0x1526AB` and the bytes stay as `ResetChipControl` left them.
The three corpus variants:

* `01 01 01 01`: `ResetChipControl` `0x14568f` (`movl $0x01010101`), taken when
  sub-id `== 0x8A`.
* `00 81 00 81` and `02 01 02 01`: the shape produced by `0x1515E9`
  (`cl = ((cc[2]&1)·2) ^ 2`, `sil = (cc[2]&1) ? 0x81 : 1`, written to
  `sc[10..13]`). Same pair-of-pairs layout; sub-id dependent.

## 3. `+0x030` and `+0x031` (`SetGClock`): host-side only

Loader mapping (record-0x01 handler inside
`CRcvParamFileManager::LoadBpBufFromBuffer` `0x1c5020`; payload byte `N` lives at
stack slot `−(0x32C − N)(%rbp)`):

| record | stack slot | destination | address |
|---|---|---|---|
| `+0x02F` | `-0x2FD` | `OBJ+0xD4` (as a dword) | `0x1c722c` |
| `+0x030` | `-0x2FC` | `OBJ+0xD3C1` | `0x1c7239` |
| `+0x031` | `-0x2FB` | `vt+0xA0` = `SetGClock(u8)` `0x16db70` → `OBJ+0xD3B8` | `0x1c7245` |
| `+0x043` | `-0x2E9` | `OBJ+0xB8` | `0x1c713c` |
| `+0x044` | `-0x2E8` | `OBJ+0xB9` | `0x1c76be` |

A whole-image scan for readers of these members (walking `full.asm` and
attributing every `0x…(%r*)` reference to its enclosing function) gives:

* `OBJ+0xD3B8` ("GCLK setting", record `+0x031`, reference file `0x14`) is
  referenced by `Reset()` (which sets the default, `movb $0x14, 0xd3b8` at
  `0x12fb30`, so `0x14` is the vendor default), `SetGClock`, `operator=`,
  `SaveBpToBuffer`, and one consumer:
  `CHWParamRcvGeneral::GetShowGrayValue()` `0x165210` (read at `0x16598d`,
  feeding a grey-depth computation together with `GetChipCustom`). It is not
  referenced by `GetBasicParam`, `SetChipControlParam`, or any pack builder.
  Record `+0x031` is a host-side value used to display or compute a grey
  level. It is not in the basic pack and cannot affect what the card emits.
  It is not a GCLK divider, a source-pin selector, or an enable.

* `OBJ+0xD3C1` (record `+0x030`, vendor default `0x32` = 50, `movb $0x32,
  0xd3c1` at `0x12fb51`; reference file `0x32`) is referenced only by
  `Reset()`, `operator=`, the loader and the serialiser. Nothing reads it. It
  is stored and round-tripped and otherwise dead.

The GCLK-related surface for this chip:

| function | address | value for 0x14C | value for 0x2F |
|---|---|---|---|
| `CHWParamRcvGeneral::IsUseSelfGCK()` | `0x131430` | wrapper that returns `CChipTypeClassify::IsPWMChip()` → true | true |
| `CChipTypeClassify::IsPWMChip()` | `0xf88e0` | true (`cmpl $0x14c` at `0xf8cd0`) | true (`cmpw $0x2f`, `0xf8945`) |
| `CChipTypeClassify::IsHasGCLKRatioSetting()` | `0xf8e80` | false; 0x14C is excluded by the `id−0x110` / `btq $0x1000100000000401` idiom at `0xf8ef0`/`0xf8f09` | true |
| `CChipParamCalculator::GetGclkCount()` | `0xecf40` | 0; index `0x13A` falls past the `cmpl $0x138` bound at `0xecfb1`, so the default arm returns 0 | computed |
| `CHWParamRcvGeneral::IsGclkAndDclkLinkage()` | `0x167c20` | false (only chip `0x105`, and then only if `chipCustom[9] & 8`) | false |
| `CHWParamRcvGeneral::IsSpecialNotPWMType()` | `0x161860` | false | false |

There is no `OeAsGclk`, `LatWidth`, `VsyncMode`, `SelfScan`, `GclkMode` or
`IsHasGclk` symbol anywhere in the library; the only `Vsync*` symbols are
sender-side (`CProcessorVOP::SetVSync*`). LAT/LE pulse widths, VSYNC issuance
and the data-latch command encoding are not parameters in the `.rcvbp`. The
chip's serial protocol lives in the FPGA bitstream, selected by chip id
(`docs/fpga/output-stage.md` §0).

`IsUseSelfGCK()` is how the "use self GCK" bit gets set:
`ResetChipCustom()` `0x156ea0` does, before any per-chip branch,
`if (IsPWMChip()) { chipCustom[0] |= 0x80; v = vt+0x590(); chipCustom[1] = v & 0xFF;
chipCustom[0] = (chipCustom[0] & 0x80) | ((v >> 8) & 0x7F); }` (`0x156f02`–`0x156f97`).
Record `+0x06A..0x06B = 80 0F` in the reference file: bit 7 set, serial
clock 15. Correct for chip 0x14C.

## 4. The "chip-custom-EX" bytes

### `+0x073..0x078`: bytes 9–14 of `SChipCustom`, not a separate field

`SChipCustom` is `+0x06A..0x079`, so `+0x073..0x078` are its bytes 9–14. The
library never reads them individually except `chipCustom[9] & 8` in
`IsGclkAndDclkLinkage` (chip `0x105` only). They are passed whole to
`SetGclkNumsOfChipControlByChipCustom` and copied to pack body `+0x79..0x7E`.

The corpus value `03 d0 07 f4 03 79` is a chip-0x2F artefact.
`ResetChipCustom`'s case for chip `0x2F` + sub `0x8A` (`0x157A99`) writes
`chipCustom[8..15] = c4 03 44 03 c4 03 79 00` (`movabsq $0x7903c4034403c4`,
`0x157ad8`), three little-endian 16-bit values (`0x03C4`, `0x0344`,
`0x03C4`) plus `0x79`; the corpus files carry an edited variant
(`… 07d0 … 03f4 … 79`). It also writes `chipCustom[3]=[5]=[7]=0xF9` and
`chipCustom[2]=[4]=[6] = ((clamp(vt+0x590(),1,0x20) − 1) & 0x1F) | 0x60`.

For chip `0x14C` `ResetChipCustom` takes case `0x15D0FC`, which only fills the
256-byte register table (`SChipCustomPlus`, constants at `0x4010B0…` and the
sub-id-`0x14D` table at `0x401130`) and does not touch `SChipCustom` past the
common prologue. The prologue zeroes bytes 2–15 (`0x156fca`/`0x156fd5`).
All-zero at `+0x06C..0x079` is the vendor default for chip 0x14C; the corpus
values belong to chip 0x2F and are not transferable.

### `+0x0E0..0x0E3`: `SChipCustomEX`, pack body `+0xD0`

Four bytes, whole-struct accessors only. `ResetChipCustom` sets it to 0 for
every chip in its prologue (`movl $0x0, -0x174(%rbp)` then `vt+0x128` =
`SetChipCustomEX`, `0x156fff`–`0x157016`); the chip-0x14C case does not
override it. Zero is the vendor default for chip 0x14C.

The corpus `79 00 79 00` is chip-0x2F specific: the same `0x79` = 121 that its
`SChipCustom` carries, stored as two little-endian `0x0079`. Same
"value repeated twice" layout as `SChipControl[10..13]`, so it is probably a
second count of the same kind for that chip family; not resolved in detail.

Bytes here are permuted by `ExchangeThirdRegWhenLoadAndSaveFile` `0x162270` on
load/save for some chips, and `GetBasicParam` overwrites the low 6 bits of
pack `+0xD4` (body `+0xD0`) with `vt+0x580()` (`0x1e1fb0`–`0x1e1fbd`).

## 5. `+0x02F` and `+0x043`

| record | object | basic pack | vendor `Reset()` default | reference file | corpus (1146 files) | readers |
|---|---|---|---|---|---|---|
| `+0x02F` | `OBJ+0xD4` (dword) | pack `+0x1D`, body `+0x19` (`0x1e0303`) | 1 (`movl $0x1, 0xd4(%rdi)`, `0x12facd`) | 0 | `1`: 961, `0`: 177, `2`: 4, `0xFF`: 4 | only `GetBasicParam`, `GetRcvParamBufForEeprom`, the loader and the serialiser |
| `+0x043` | `OBJ+0xB8` | pack `+0x2A`, body `+0x26` (`0x1e037c`) | none (`Reset()` sets `OBJ+0xB9 = 0` but never `0xB8`) | `0x60` | `0`: 1126, `0x60`: 8, `2`: 5, `0x1C`: 4, `0x40`: 3 | same set |

Both are plain stored bytes with no named accessor anywhere in the library;
their meaning is not resolved. What is known:

* Both reach the card (they are in the basic pack, hence in the boot image);
  they are not inert like `+0x030`/`+0x031`.
* `+0x02F` selects whether the panel displays at all. Measured: with the
  reference file's `0` nothing displays; with `1` (the vendor default) the
  panel renders ([rendering.md](rendering.md)). The reference spec sets it
  to `1` via `[record01_overrides]`.
* `OBJ+0xB8` sits inside the scan/output member cluster (`0xB5` scan method,
  `0xB6` split, `0xB7` (loader forces 0), `0xB8` unknown, `0xB9`
  data-group/output code, `0xBB`, `0xBC`, `0xBD` 8 ns OE), and in the pack it
  is written as the pair `body[0x26] = OBJ+0xB8`,
  `body[0x27] = OBJ+0xB9 | (outputModel≥2)`. Its `0x60` occurs in 8 of 1146
  vendor files, on chips `0xA2`, `0xE5`, `0xFD`, `0x9D`, at scans
  8/10/16/28/45: no correlation with scan, pitch or geometry. Measured: the
  panel renders with the reference file's `0x60` in place; the byte has not
  been swept.

## 6. How the chip id changes the card-side protocol

`GetChipType()` `0x16daa0` = `OBJ+0xD3C4` (record `+0x036` low, `+0x204` high);
`GetChipTypeEx()` `0x16da90` = `OBJ+0xDF20` (record `+0x0E9` low, `+0x205` high).
`CChipTypeClassify` `0xf8870` caches nothing; it holds only a
`CHWParamRcvGeneral*` and re-calls the vtable each time.
`GetChipTypeExPosition` `0xfcdb0` folds `0x14D → 0x14C` and `0x8A → 0x2F`.

| property | address | 0x14C | 0x2F |
|---|---|---|---|
| `IsPWMChip` | `0xf88e0` | true | true |
| `IsUseSelfGCK` | `0x131430` | true | true |
| `IsSpecialNotPWMType` | `0xfe080` | false | false |
| `IsSoftSyncPWMChip` | `0xf9c00` | false | false |
| `IsHasGCLKRatioSetting` | `0xf8e80` | false | true |
| `IsHighRefreshValid` | `0xf9d00` | false (same exclusion mask) | true |
| `IsDoubleBufScan` | `0xfa470` | false | false |
| `IsMultiRegisterChip` | `0xf9ea0` | true | false |
| `IsSM16389Series` | `0xf9000` | true (`0x14C` is in the `pcmpeqw` table at `0x3FEDA0` = `{0x14C,0x13C,0x11A,0xBB}`) | false |
| `IsSM16159Series` | `0xfda50` | false | true |
| `IsNeed16BitGrayWhenSend` | `0xfb840` | true | false |
| `IsNeedExChangeReg2` / `Reg3` | `0xfdef0` / `0xfe040` | false | true |
| `IsFixationSerialFrequency` | `0xf9110` | false | false |
| `IsUseDoubleEdgeDCLK` | `0xfeac0` | false (only `0x15D`) | false |
| `GetChipDataSendType` | `0xfe6f0` | 1 | 1 |
| `GetChipSerialType` | `0xfc940` | 0 | 0 |
| `GetChipRegisterType` | `0xfa860` | 6 | 4 |
| `GetChipMaxScanMode` | `0xfe740` | 64 | 32 |
| `GetChipChannals` | `0xfe100` | 16 | 16 |
| `GetGrayLevelCalType` | `0xfb000` | 12 | 31 |
| `GetChipBaseType` | `0xfdb70` | 6 | 1 |
| `GetChipCloseTimeUnit` | `0xfa7b0` | 100 | 100 |

Fields of record 0x01 that are chip-id dependent, and where the dependence is
implemented: `SChipControl` (`ResetChipControl` `0x1454d0`, per-chip jump table),
`SChipCustom` (`ResetChipCustom` `0x156ea0`, per-chip jump table at `0x15E76C`),
`SChipCustomPlus` = record 0x84 (same function), `SChipCustomEX`
(zeroed by default, per-chip overrides), the serial clock
(`ResetIS`, via `IsPWMChip` and `GetChipMaxScanMode`), the grey level
(`GetGrayLevel`, from `SChipCustom[2]`/`[3]`/`[9]` and the register table), and
the pack-time fixups in `SetChipControlParam` `0x1e3b00`, whose per-chip lists
do not contain `0x14C`. For chip 0x14C the only pack-time change to the block
is the GCLK count of §2.

On the card, the id selects the driver protocol. Measured: chip id `0x014C`
with sub-id 0 arms the SM16269S outputs and the panel renders; `0x0214` and
`0x00DE` never arm ([rendering.md](rendering.md)).

## 7. Chip names

Recovered from the 13 name tables in the dylib, all referenced from
`CHWParamRcvGeneral::InitChipNameMap()` `0x12F100`. Record layout, stride
`0x104` on macOS: `{u16 chipType; wchar_t name[64] (UTF-32LE); u16 groupIdx}`;
the 4-byte `wchar_t` is why `strings` finds nothing. On the Windows DLLs the
same tables use stride `0x84` with 2-byte `wchar_t`.

| id | vendor name | evidence |
|---|---|---|
| `0x002F` | MBI5153 | `_MBIChipNameTable` record at dylib `0x4571D8`; `CLTInterface.dll` (LEDSet 2.26) `0xD77EAC` |
| `0x008A` | SM16159 | `_SMChipNameTable` `0x45E8BC`; `CLTInterface.dll` `0xD704D4` |
| `0x014C` | SM16169SH (newer builds: `SM16169SH/SL`) | `_SMChipNameTable` `0x45EAC4`; `CLTInterface.dll` `0xD705DC` |
| `0x014D` | SM16380SH (newer: `SM16380SH/SA`) | `_SMChipNameTable` `0x45FB04`; `CLTInterface.dll` `0xD713C8`; LEDVISION 9.6 `CLTDevice.dll` `0x438394` |
| `0x0214` | SM16269S | `CLTInterface.dll` `0xD70EA0`, only in the LEDSet 2.26 build |

Neighbours: `0x00DE` SM16169S, `0x0170` SM16169SW/SM16189, `0x024D` SM16169SK,
`0x0217` SM16269SW, `0x0215` SM16386SH.

Two facts that follow:

1. `0x014D` is SM16380SH, not an SM16269 sub-variant.
   `config/chips/sm16269-defaults.toml`'s header comment and `sub_id = 0x014D` describe
   it as the SM16269 sub-variant; that is wrong. The C++ class
   `SSM16269ChipCustomPlus` is bound to id `0x14D`
   (`CChipParamCalculator::CalRefreshFreqSM16169SH` `0xE28A0` branches on
   `GetChipTypeEx() == 0x14D` and calls it, `0xE299F`), an internal misnomer in
   the vendor's own source. The class name is not evidence.
2. The real SM16269S is `0x0214`, and `libCLTDevice`/LEDVISION 9.6 do not
   know it; the dylib's tables stop at `0x15D`. `0x014C` (SM16169SH) is the
   closest id those builds can express, and it is what the reference file and
   the generated config declare. `0x0214` is a dead id in the vendor's own
   code: every chip jump table sends it to the default arm, so it gets no
   registers, no chip control and no PWM classification.

The `0x002F` corpus is not "SM16169 modules": `0x2F` is MBI5153 with sub-id
SM16159, a different chip family. Its `SChipControl`, `SChipCustom` and
`SChipCustomEX` values are not transferable to `0x14C`, and
`IsHasGCLKRatioSetting` is true for it and false for `0x14C`.

## 8. Unresolved

* `SChipControl` bytes 0–6, 9, 14–15, 16, 18–19: source pinned to the exact
  instruction, meaning not resolved in the library. The struct is only ever
  copied whole; no field accessor, no name, no arithmetic on individual bytes
  for this chip. Inferred meanings for bytes 1–6 are in
  `docs/fpga/chip-protocol-microcode.md` §2.
* `SChipControl` byte 16 = `0x02` in the reference file: `ResetChipControl`
  never writes it for chip `0x14C`, so its origin is outside the reset path.
  Not required for rendering; not swept.
* Record `+0x02F` and `+0x043`: meaning not resolved (§5). `+0x02F = 1` is
  required for rendering; measured.
* `SChipCustomEX` for chip families other than `0x2F`.
* `SChipGrobalConfig` (record `+0x0EA`, reference file `00 40 00 40 00 40`):
  whole-struct accessors only, no per-field reader anywhere.
* LAT/LE pulse widths, VSYNC issuance, register-write-vs-data mode selection,
  RGB bit order, OE polarity: not in the configuration at all. They are in
  the FPGA bitstream, selected by chip id.

## 9. Values that are correct for the reference configuration

* `SChipControl` bytes 10–13 = `00 97 00 97` (`reg 0x07 = 0x04`, sub-id `0`);
  the vendor computes the same 151.
* `SChipCustom` = `80 0F 00 …`: bit 7 is the "use self GCK" flag and
  `IsPWMChip(0x14C)` is true, serial clock 15; bytes 2–15 zero is the vendor
  default for this chip.
* `SChipCustomEX` = zeros is the vendor default.
* Record `+0x030 = 0x32` and `+0x031 = 0x14` are the vendor defaults and
  never reach the card; `+0x031` ("GCLK setting") only feeds
  `GetShowGrayValue()`. Sweeping it can change nothing on the panel.
* The corpus values for `+0x073..0x078` and `+0x0E0..0x0E3` belong to MBI5153 /
  SM16159 and are not ported.
