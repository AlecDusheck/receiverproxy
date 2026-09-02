# Record 0x01 (0x0a01): field dictionary

Record 0x01 is the 764-byte main receiver-parameter record of a `.rcvbp` file.
Its payload is 760 bytes after a 4-byte record header. The vendor library
libCLTDevice (macOS build; all addresses below are in it) loads it into the
`SRcvParamBasic` structure, serialises it back, and compiles it into the
256-byte basic-parameter pack that the card receives.

## Sources

| function | address | role |
|---|---|---|
| `LoadBpBufFromBuffer` | 0x1c5020 | record loader; record-0x01 handler at 0x1c5f07; apply block 0x1c590c..~0x1c9c00 |
| `SaveBpToBuffer` | 0x1ca810 | serializer; record-0x01 writer 0x1ca8c3-0x1cbbc7 |
| `SRcvParamBasic` constructor | 0x1cfcc0 | defaults |
| `GetBasicParam` | 0x1dfb50 | pack builder |
| `ExchangeThirdRegWhenLoadAndSaveFile` | 0x162270 | permutes bytes of the chip-custom blocks on load and save |
| `MakeCurrentOfDeadPixcelValid` | 0x167c50 | dead-pixel current data |
| `GetChipType` | 0x16daa0 | vtable slot vt+0x50 |

The field meanings are cross-checked statistically against 1219 unique vendor
record-0x01 payloads from the config corpus. The per-field survey itself is
not in the repository.

## Layout

* The loader copies the record including its 4-byte header into
  `SRcvParamBasic`. Payload offset N is stack slot `-(0x32C-N)(%rbp)` in both
  the loader and the serializer.
* The serializer writes payload 0x000-0x276. Payload 0x277-0x2FB comes only
  from the constructor and is byte-identical to the reference file's values.
* Notation: `OBJ+N` is a `CHWParamRcvGeneral` member; `vt+N` is a vtable slot
  (the dylib text carries no vtable data, so unnamed slots are pinned by
  offset only); "pack" is the `GetBasicParam` output offset, where
  pack offset = body offset + 4.
* Values quoted as "reference" are those of
  `third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`.

## Named fields

Meanings marked "inferred" rest on the accessor and the corpus survey, not on
a bench measurement.

| off | size | name / accessor | meaning | pack |
|---|---|---|---|---|
| 0x000 | 1 | GetMoudleWidth (OBJ+0x68) | module width px | 0x08/0x09; feeds OneScanLen, CardScanLen |
| 0x001 | 1 | GetMoudleHeight (OBJ+0x6a) | module height as stored (reference: 32) | same |
| 0x003 | 2 LE | GetVoidPointCount (OBJ+0x6e) | void-point count | 0x23-0x24 BE |
| 0x018 | 4 LE | flag word #1 | bit table below | many |
| 0x01C | 4 | f32 gamma (OBJ+0x98) | gamma (default 2.2; reference 2.8); triggers CalGamaTable | via gamma |
| 0x020 | 1 | GetScanMode (OBJ+0xC1) | scan denominator, literal (reference: 16) | 0x0B |
| 0x021 | 2 LE | SetSerialClockFrequency | serial clock frequency (reference: 8) | 0x0D-0x0E BE |
| 0x023 | 1 | GetGrayLevel | gray level (0x0E = 14-bit) | 0x0C |
| 0x024 | 2 LE | OBJ+0x82 | luminance / light-increase scalar (CalMaxLightInc) | 0x19 |
| 0x026 | 2 LE | vt+0x558/0x598 | 188 in the reference file; meaning inferred | 0x1B |
| 0x028 | 3 | OBJ+0x94..0x96 | 3-byte field (`ff ff ff`) | 0x05-0x07 |
| 0x02B | 1 | GetColorSwap (OBJ+0xD0) | colour-swap index | 0x14 |
| 0x02C | 3 | SetColorSource (OBJ+0xC4/C8/CC) | R/G/B exchange indices; (2,1,0) = none | 0x14 |
| 0x031 | 1 | SetGClock (OBJ+0xD3B8) | GCLK setting (default 0x14) | none |
| 0x032 | 1 | GetRedCurrentWithoutChip | red current gain 0-63 | 0x34 |
| 0x033 | 1 | GetGreenCurrentWithoutChip | green current gain | 0x35 |
| 0x034 | 1 | GetBlueCurrentWithoutChip | blue current gain | 0x36 |
| 0x035 | 1 | SetVirtualRedCurrent | virtual-red current gain | 0x37 |
| 0x036 | 1 | GetChipType low byte | chip id low (high byte at +0x204) | chip library; gray override when type == 0x5C |
| 0x037 | 1 | GetSerialType | serial type | none |
| 0x03C | 1 | GetLineDir (OBJ+0xD4C0) | line direction: 0/1 vertical, 2/3 horizontal | 0x08/0x09 order, 0x0A, 0x4A |
| 0x03D | 1 | OBJ+0xB5 low nibble (high nibble from OBJ+0xC2) | scan method | 0x26 |
| 0x03E | 1 | GetSplitSegment / GetSplitStyle (OBJ+0xB6) | split | 0x27 |
| 0x044 | 1 | OBJ+0xB9 | data-group / output code (low nibble) | 0x0A logic |
| 0x045 | 2 | SetGrayCompensation | gray compensation | 0x2C-0x2D |
| 0x049 | 2 LE | OBJ+0x7C | serial clock / 2 (derived from +0x021) | none |
| 0x04B | 2 LE | OBJ+0x7E | duplicate of +0x021 | none |
| 0x04E | 1 | GetOutPutModel ((v>>1)&7) | output model; bit0 = 32-group flag | 0x0A/0x2B |
| 0x050 | 1 | Get8nsOeEnableInfo (OBJ+0xBD) | OE timing info | 0x3B |
| 0x052 | 1 | OBJ+0xDC | special-module setting | 0x94 (with flag1 bit6) |
| 0x057 | 1 | OBJ+0x74 lo (hi at +0x24E) | GetModuleInputCount numerator (line dir 2/3) | 0x46, 0x4A |
| 0x058 | 1 | OBJ+0x76 lo (hi at +0x24F) | GetModuleInputCount numerator (line dir 0/1) | 0x47, 0x4A |
| 0x059 | 1 | HR_GetHighRefreshStyle | high-refresh style | none |
| 0x05A | 16 | OBJ+0xD3CC swap block 0 (ResetSwapData) | data/RGB swap table | none |
| 0x06A | 16 | SetChipCustom (vt+0x110, OBJ+0xD4D1..) | chip-custom block: +0x06A bit7 = use self GCK, bits 0-6 plus +0x06B = serial clock; +0x078/+0x079 permuted by `ExchangeThirdRegWhenLoadAndSaveFile` | 0x74-0x83 verbatim |
| 0x07A/0x08A/0x09A | 16 each | swap blocks 1-3 | | |
| 0x0AA | 4 | f32 OBJ+0xDEF8 (UpDateBaseRefresh) | base refresh rate (60.0) | none |
| 0x0AE | 4 | f32 HR_SetMinOE | minimum OE | +0x0CC when IsSpecialNotPWMType |
| 0x0B2 | 1 | HR_SetScanStyle | HR scan style | none |
| 0x0B3 | 1 | GetHubType (OBJ+0xD4C4) | hub type (reference: 0) | 0x4B |
| 0x0B4/B8/BC | 4 each | f32 OBJ+0x88/0x8C/0x90 | R/G/B current percent | 0xD8 via GetCurrentPercent |
| 0x0C0 | 2 LE | SetMaxWidth (OBJ+0xDF08) | MaxWidth (reference: 256) | 0x8C-0x8D BE |
| 0x0C2 | 2 LE | SetMaxHeight (OBJ+0xDF0A) | MaxHeight (reference: 384) | 0x8E-0x8F BE |
| 0x0C4 | 16 | vt+0x198 out-struct | dead-pixel current data (`MakeCurrentOfDeadPixcelValid`); inferred | none |
| 0x0E0 | 4 | SetChipCustomEX (vt+0x120) | chip-custom-EX (bytes permuted by ExchangeThirdReg) | 0xD4-0xD7 |
| 0x0E5 | 1 | OBJ+0xC2 | feeds +0x03D high nibble | 0x26 |
| 0x0E8 | 1 | GetLS9736ICNum / IsHasMBI5988 | | 0x91 |
| 0x0E9 | 1 | vt+0x48 lo (hi at +0x205) | secondary chip / decoder id | chip branches |
| 0x0EA | 4+2 | vt+0x178 | | 0xDC-0xE1 |
| 0x0FB | 1 | decode-chip enum 0-14 | which decoding chip is present | |
| 0x114..0x173 | 6 x 16 | swap blocks (OBJ+0xD44C..) | 96-entry row map (identity in 99.6 % of the corpus) | |
| 0x180/0x181 | 1 each | packed HDR-12bit / HLG-12bit gamma flags | | |
| 0x191 | 1 | SetIsSL9739EnabledEx (OBJ+0xBE) | | 0xDB |
| 0x199 | 1 | (OBJ+0xE141<<1) \| OBJ+0xE142 | bit1 = Set9929NewControl | |
| 0x19A..0x1D9 | 4 x 16 | swap blocks (OBJ+0xD40C..) | 64-entry lane map (ramp 64..127 or zero) | |
| 0x1DB | 4 LE | flag word #2 | bit table below | |
| 0x1E1/0x1E5/0x1E9 | 4 each | (OBJ+0xE124/28/2C & 0xFFFFFF) \| (OBJ+0xE130/34/38 << 24) | packed pairs | |
| 0x1EE/0x1F0 | 2 each | OBJ+0xE61E / OBJ+0xE620 | | 0xF9 / 0xFA |
| 0x1F7 | 1 | OBJ+0xE61C | | 0xFB |
| 0x202 | 1 | GetIsEnableToneMapping | tone-mapping enable | |
| 0x204 | 1 | GetChipType high byte | chip id high | chip library |
| 0x205 | 1 | vt+0x48 high byte | secondary chip id high | |
| 0x247 | 1 | GetChipOhmValB (vt+0x3F8) | inferred | 0xFC |
| 0x24B | 1 | OBJ+0xDF7B (remapped 0x1B -> 0xD8 when 8nsOe & 4) | decode chip type (GetNewDecodeChipType) | |
| 0x24C | 1 | GetDeadPixelsCurrentGain (vt+0x418) | inferred | 0xFD |
| 0x277-0x2FB | | constructor constants: `01 01`, zeros, `01 00 01 00`, `01 00`, then a 120-byte zero tail that is never read | none |

Offsets not listed are constants (see the constructor), members whose source
is known but whose meaning is not resolved, or not resolved at all.

## Flag word #1 (payload +0x018, u32 LE)

| bit | source | pack |
|---|---|---|
| 0 | OBJ+0xBF | 0x17 |
| 1 | OBJ+0xC0 | 0x16 |
| 3-5 | OBJ+0xD3BD-BF | |
| 6 | OBJ+0xD8 SpModule enable | 0x94 |
| 7 | OBJ+0xD3C2 | |
| 10 | OBJ+0x78 | 0x48 |
| 11 | OBJ+0xD3C0 | 0x49 |
| 12 | OBJ+0xD6EA geometry source select; clear = module W/H/void from payload 0x000-0x007 (reference file: clear) | |
| 13 | gamma-10bit | |
| 14 | OBJ+0x86 | |
| 15 | OBJ+0xB4 | |
| 16 | OBJ+0xD6D8 | |
| 17 | OBJ+0xDEF0 | |
| 18 | vt+0x5A8() == 2 | |
| 19 | OBJ+0x87 == 0 | |
| 20 | vt+0xA8() == 0 | |
| 21 | GetIsSetCustomModulePosSusseed | |
| 22 | OBJ+0xDEF1 | |
| 23 | vt+0xB8() == 0 | |
| 24 | UseSeparateGamma8bit | |
| 26 | UseSeparateGamma10bit | |
| 28 | OBJ+0xE0C5 | |
| 29 | OBJ+0xE0AD | |
| 30 | OBJ+0xD4D0 (max-size gate) | |
| 31 | vt+0x5A8() == 4 | |

Bits 2, 8, 9, 25 and 27: not resolved.

## Flag word #2 (payload +0x1DB, u32 LE)

| bit | source |
|---|---|
| 0 | IsHasICND2019 |
| 1 | OBJ+0xE148 |
| 3-4 | gamma-calc method |
| 5 | OBJ+0xE120 |
| 6 | IsHasVOD5958 |
| 7 | UseSeparateGamma12bit |
| 8 | OBJ+0xE08E |
| 9 | OBJ+0xDF15 |
| 10 | vt+0x288() & 1 for chip in {0x74, 0x131, 0x15D} |
| 11 | vt+0x2A0 |
| 12 | OBJ+0xD6D9 |
| 13 | always set |
| 13 \| 14 | OBJ+0xE1A3 |

## Derived quantities (computed, not stored)

* `OneScanLen` @ 0x138e90 = `moduleW x moduleH_stored / scan`, clamped to
  at least 1. For 128 x 32 / 16: 256.
* `CardScanLen` @ 0x145420 = `OneScanLen` scaled by
  `GetMaxInLineDir() / module dimension`. Two modules in the line direction
  (the reference wall): 512; one module: 256.
* `GetModuleCountInLineDir` @ 0x14b1e0 = `ceil(GetMaxInLineDir() / moduleDim)`.
  `MaxInLineDir` comes from the `IRcvMaxSize` interface (the live layout),
  not from record 0x01, and is baked into the pack at compile time.
* `GetRgbSelValue` (OBJ+0xDF7C) is not stored in record 0x01; its setter is
  never called in the dylib.

## Pack offsets settled against the factory pack body

* vt+0x50 is `GetChipType` @ 0x16daa0, not `GetSplitSegment`. The gray
  override in the pack builder is a chip-type test.
* Pack 0x0B is the scan denominator (factory body[0x07] = 0x10 = 16), not
  `GetRgbSelValue`. The card has never been configured at 1/8 scan.
* Pack 0x0D-0x0E BE is the serial clock frequency (factory: 8), not the scan
  mode. Writing the scan denominator (16) there sets the serial clock to twice
  the factory value.
* Pack 0x8C-0x8F = MaxWidth / MaxHeight BE; pack 0x34-0x37 = the four current
  gains; pack 0x74-0x83 = the chip-custom block, record payload +0x06A..0x079
  verbatim.

## Limits

* Flag word #1 bits 2, 8, 9, 25, 27: not resolved.
* Payload +0x02F is not in the table: its meaning is not resolved. The value
  1 is the vendor `Reset()` default and the value in 961 of 1146 corpus
  files; with it cleared nothing displays on the reference module
  (see [rendering.md](rendering.md)).
* The meanings marked inferred (+0x026, +0x0C4, +0x247, +0x24C) have not been
  exercised on hardware.
