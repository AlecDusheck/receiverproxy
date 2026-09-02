# Chip libraries for non-SH driver chips

SM16169SH (`0x014C`) is an addressed part: its configuration is a table of
numbered registers, serialised into record 0x84 and written by the card one
register write at a time. A large part of the vendor's chip list is not
addressed, including SM16169S (`0x00DE`), the closest relative of this
panel's silicon for which vendor data exists. This page describes the
unaddressed shape, what the vendor emits for it, and how
`crates/panelspec/src/chips.rs` carries it.

Vendor addresses are in `libCLTDevice.1.dylib` (iSet, macOS) unless another
build is named.

## 1. Two kinds of chip

| | addressed (SH style) | unaddressed (non-SH) |
|---|---|---|
| example | SM16169SH `0x014C` | SM16169S `0x00DE`, MBI5153 `0x002F` |
| `GetChipRegisterType` (`CChipTypeClassify`, `0xFA840`) | 6 | 3 (`0xDE`), 4 (`0x2F`) |
| `IsMultiRegisterChip` (`0xF9E00`) | true | false |
| record `0x0A84` in the `.rcvbp` | present, 33 quads | absent |
| where the chip settings live | `SChipCustomPlus`, 256 bytes, record 0x84 | `SChipCustom`, 16 bytes, record 0x01 `+0x06A..+0x079` |
| what `ResetChipCustom` writes | the 256-byte register table (chip `0x14C` case `0x15D0FC`, constants at `__TEXT,__const 0x4010B0`) | only `SChipCustom` (chip `0xDE` case `0x15A402`, tail `0x15DF74` = `SetChipCustom`) |

The record is absent, not empty: of the 5579 vendor `.rcvbp` files on disk,
2245 parse; 59 of those declare chip `0x00DE`, and none of the 59 contains a
`0x0A84` record (13 of the 16 files declaring `0x014C` do). The census is
three lines of `scripts/mapdump.records()`.

## 2. `SChipCustom` for `0x00DE`

`SChipCustom` is 16 bytes. It reaches the card unchanged as basic-pack body
`+0x70..+0x7F` (`crates/rcvbp/src/spec/basic_pack.rs`, `put(0x70, …)`),
so it is the whole driver-configuration payload for an unaddressed chip.

```
byte  0   0x80 = "use self GCK" | (serialClock >> 8) & 0x7F
byte  1   serialClock & 0xFF
byte  2   \                       bits 3:0 = scan - 1   (patched on every load)
byte  3   / colour-R config word  bits 7:4 = 0xE default
byte  4   \ colour-G config word  (same rule)
byte  5   /
byte  6   \ colour-B config word  (same rule)
byte  7   /
byte  8.. 9   0x0400  little-endian
byte 10..11   0x05C0  little-endian
byte 12..13   0x03C0  little-endian
byte 14..15   0x0000
```

Default for scan 16: `80 0f ef 39 ef 39 ef 39 00 04 c0 05 c0 03 00 00`.

The scan patch and the byte values are read from two independent vendor
builds (instruction-level citations in `config/chips/sm16169s-vendor.toml`).
The R/G/B pairing of bytes 2–7 is inferred from the three-fold repetition.

The scan patch is the analogue of `reg 0x02 = scan − 1` on the SH chips.
`ResetChipCustom` does it inline (dylib `0x15A444`–`0x15A462`:
`GetScanMode()`, then `((v-1) & 0x0F) | 0xE0` stored to bytes 2, 4 and 6);
LEDSetting 2.2.6 does it in a shared helper (`CLTInterface.dll 0x1802C7940`:
`cc[n] = (cc[n] & ~0x0F) | ((scan-1) & 0x0F)` for n = 2, 4, 6).

Byte 2 also drives the chip-control GCLK count.
`SetGclkNumsOfChipControlByChipCustom` (`0x151580`, case `0x151F21` for
`0x00DE`) reads `n = (chipCustom[2] >> 5) & 3` and writes
`SChipControl[10..13] = 00 81 00 81` when `n == 3`, `01 01 01 01` when
`n == 2`, and leaves the block alone otherwise. With the reset default
(`0xE?`) `n` is always 3, so the pair is `00 81 00 81` at every scan. This is
the coupling that [chip-control-block.md](chip-control-block.md) §2 describes
for `0x014C`, with a `SChipCustom` byte as the input instead of register 0x07.

## 3. `chips.rs`

`ChipLibrary` has `chip_custom: Option<[u8; 16]>`, `chip_custom_ex:
Option<[u8; 4]>`, `chip_custom_scan_patch: Option<{ bytes, mask, base }>`,
`emit_record_84: bool` (default `true`), `gray_bits: Option<u8>` and a
`record01_overrides` table.

* `record_84()` returns `None` when `emit_record_84` is false, and
  `spec::generate` omits the `0x0A84` record entirely, as the vendor does.
* `gray_bits()` returns the library's literal when the chip has no register
  table. `GetGrayLevelCalType` is 31 for `0x00DE` against 12 for `0x014C`, so
  the reg-0x07/0x03 derivation used for SH chips is the wrong formula here.
* Record 0x01 takes `chip_custom` at `+0x06A` with the scan patch applied and
  `chip_custom_ex` at `+0x0E0`, then the overrides.

`config/chips/sm16169s-vendor.toml` and `sm16269s-vendor-0x214.toml` load and
generate. Neither has been measured on the bench with this loader.

## 4. Per-id behaviour that changes the basic pack

Everything below is the vendor library's own behaviour, from
`CChipTypeClassify` (dylib `0xF8860`…) and `CSendAndSaveRcvParam::GetBasicParam`
`0x1DFB50`. `0x0214` and `0x0187` are unknown to every dispatch table in every
build, so they take each function's default arm, shown in the right-hand
column.

| property | address | `0x00DE` | `0x014C` | `0x0214` / `0x0187` |
|---|---|---|---|---|
| `IsPWMChip` = `IsUseSelfGCK` (`0x131430` is a pure wrapper) | `0xF88E0` | true (`cmpl $0xde` `0xF8C95`) | true (`cmpl $0x14c` `0xF8CD0`) | false (falls off the id list at `0xF8E70`) |
| `GetChipMaxScanMode` | `0xFE720` | 16 | 16 (sub-id 0) / 64 (sub-id `0x14D`, `0xFE7BC`) | 64 (out-of-table default `movl $0x40, %r14d` `0xFE747`) |
| `GetChipRegisterType` | `0xFA840` | 3 | 6 | 0 |
| `GetGrayLevelCalType` | `0xFAF50` | 31 | 12 | 31 |
| `IsMultiRegisterChip` | `0xF9E00` | false | true | false |
| `IsNeed16BitGrayWhenSend` | `0xFB710` | false | true (`cmpl $0x14c` `0xFB848`) | false |
| `IsHasGCLKRatioSetting` | `0xF8E80` | true | false | false |
| `IsHighRefreshValid` | `0xF9CC0` | true | false | false |
| `IsSM16259Series` | `0xFC850` | true | false | false |
| `IsSM16389Series` | `0xF8FF0` | false | true | false |
| `IsShowChipParamButton` | `0xFB9A0` | true | true | false |
| `IsSupportColorChange` | `0xF91F0` | true | true | false |
| `GetChipChannals` | `0xFE0D0` | 16 | 16 | 16 |
| `GetChipDataSendType` | `0xFE6F0` | 1 | 1 | 0 |
| name-table group | — | 1 (SM16169) | 1 | 2 (SM16269) / 3 (SM16380) |

Neither `0x00DE` nor `0x014C` appears in `SetChipControlParam`'s per-chip
fixup list (`0x1E3B00`; the ids it tests are `0x83 0x89 0x8B 0xA2..0xA5 0xA8
0xB1 0xB2 0xB6 0xBF 0xC0 0xC4 0xC5 0xC8 0xCA 0xCB 0xD2 0xDA 0xDF 0xE0 0xE3
0xF2 0xFD 0x124 0x126 0x127 0x12A 0x12C 0x130 0x132 0x13D 0x13F 0x156`), so
for all three ids the only pack-time change to `SChipControl` is the GCLK
recomputation.

Bytes of the 256-byte basic-pack body that differ between the three ids with
everything else equal:

| body offset | `0x00DE` | `0x014C` | `0x0214` | rule |
|---|---|---|---|---|
| `+0x08` (grey bits) | `0x0E` | `0x10` | `0x0E` | `pack[0x0C] = GetGrayLevel(); if (IsNeed16BitGrayWhenSend()) pack[0x0C] = 0x10;` at `0x1DFEEF`–`0x1DFF03`. The factory pack on this card (compiled by the card, not by the PC tool) has `0x0E` at this offset for chip `0x14C`, so the card's own compiler does not apply this rule; the generator must not be changed from this row. |
| `+0x10` | derived from min-OE (`0x1DFF94`–`0x1DFFD1`) | `OBJ+0x83` (`0x1DFFD6`) | `OBJ+0x83` | `IsHighRefreshValid()` gate at `0x1DFF81` |
| `+0x17` (pack `+0x1B`) | `0xDE` | `0xFE` | `0xFE` | ids < `0x100` use the byte slot; larger ones set the `0xFE` escape (`ResetChipType` `0x1E5130`) |
| `+0x70..+0x7F` | see §2 | zeros | zeros | `SChipCustom` |
| `+0x91..+0xA4` | `00 0e 02 04 08 01 03 00 00 00 00 81 00 81 00 10 00 00 00 00` | `00 0e 01 05 06 01 03 00 00 00 00 97 00 97 00 08 xx 00 0a 02` | all zero | `SChipControl` |
| `+0xE3..+0xE4` (pack `+0xE7`) | `00 00` | `01 4C` | `02 14` | escaped chip id, big-endian |
| `+0xFC..+0xFF` | — | — | — | CRC-32, computed with `+0x17` and `+0xE3..+0xE4` zeroed, so it moves with everything above |

## 5. Bench results by chip id

Measured on the bench panel (P2.5 128x64, SM16269S drivers, firmware 16.53):

* `0x14C` arms the drivers and, with the settings in
  [rendering.md](rendering.md), renders.
* `0x0214` never arms; the panel stays dark at 0.5 A. `IsPWMChip(0x214)` is
  false, so `ResetChipCustom` skips the prologue that sets bit 7 of
  `SChipCustom[0]` ("use self GCK") and the serial-clock bytes, the default
  arm adds nothing, and `ResetChipControl` zeroes its 20 bytes. A config
  declaring `0x0214` ships the driver an all-zero configuration. It is an
  absent chip library, not a tuning problem; no shipped Colorlight build has
  one.
* `0x0DE` never armed in the configuration measured, which was wrong for
  this chip: it carried the SH register table for `0x14C` and left
  `SChipCustom` at the `0x14C` value (`80 0F 00 …`). Byte 2 was therefore
  `0x00`, not `0xE?`, which (a) leaves the three colour config words zero and
  (b) makes `SetGclkNums` take the `n < 2` branch, so `SChipControl[10..13]`
  never gets `00 81 00 81`. `config/chips/sm16169s-vendor.toml` is the correct
  form of that configuration; it has not been measured.

## 6. Unresolved

* The meaning of the three per-colour config words in `SChipCustom[2..7]`
  beyond "bits 3:0 = scan − 1" and "bits 6:5 select the GCLK pair". The
  corpus shows the high nibble taking `0xC`, `0xD` and `0xE`, and the low
  byte `0x31`, `0x38`, `0x39`, `0x3B`, `0x3C` and `0xB9`, so at least five
  more bits are user-settable, presumably from the chip-parameter dialog
  (`IsShowChipParamButton(0xDE)` is true). The dialog resource is not
  decoded.
* The three 16-bit words at `SChipCustom[8..13]`. Corpus variants:
  `0400/05C0/03C0` (the default, 30 files), `0200/02C0/02C0`,
  `1000/11C0/11C0`, `0770/07F0/07F0`, `0600/05C0/03C0` and `0C90/0D90/0C90`
  (with a non-zero `[14..15]` of `6C 50` in that last group). Units unknown.
* `GetGrayLevel` for `GetGrayLevelCalType == 31`. The corpus value is 14 in
  56 of 59 files and 13 in 3; the formula is not traced.
* Whether the card's firmware compiler applies `IsNeed16BitGrayWhenSend` and
  `IsHighRefreshValid` the way `GetBasicParam` does. The one ground-truth
  pack on this card says no for the first; the second is not testable with
  this panel.
* `SChipGrobalConfig` (record `+0x0EA`): `00 40 00 40 00 40` in 44 of the 59
  SM16169S files and all zeros in the rest. Neither value comes from
  `ResetChipCustom`'s `0x00DE` case. Whole-struct only, as in
  [chip-control-block.md](chip-control-block.md) §8.
* SM16386S `0x0187` has no implementation in any of the three vendor builds
  on disk: it is a name-table entry in LEDSetting 2.2.6 (`CLTInterface.dll`
  file offset `0xD71554`, group 3) and nothing more. It is no comparison
  point; it is the control showing what an unimplemented id looks like.
