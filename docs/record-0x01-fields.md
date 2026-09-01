# Record 0x01 (0x0a01) — the complete field dictionary

Instruction-level decode of the 764-byte main receiver-parameter record, from
the macOS libCLTDevice disassembly (all addresses refer to it). Sources: the
record loader (`LoadBpBufFromBuffer` @ 0x1c5020, record-0x01 handler at
0x1c5f07, apply block 0x1c590c..~0x1c9c00), the serializer (`SaveBpToBuffer` @
0x1ca810, record-0x01 writer 0x1ca8c3-0x1cbbc7), the defaults constructor
(`SRcvParamBasic` @ 0x1cfcc0), and the pack builder (`GetBasicParam` @
0x1dfb50). Cross-validated statistically against 1219 unique vendor payloads
(`analysis/record01-fieldmine/`).

Structural fact: the loader memcpys the record including its 4-byte header
into `SRcvParamBasic`, so payload offset N maps mechanically to stack slot
-(0x32C-N)(%rbp) in both loader and serializer. The serializer covers payload
0x000-0x276; 0x277-0x2FB comes only from the constructor (verified
byte-for-byte against our installed config).

`OBJ+` = CHWParamRcvGeneral member; `vt+N` = vtable slot (no vtable data in
the dylib text, so unnamed slots are pinned but unlabeled); "pack" =
GetBasicParam output offset (pack = body offset + 4).

## Named fields

| off | size | name / accessor | meaning | pack | conf |
|---|---|---|---|---|---|
| 0x000 | 1 | GetMoudleWidth (OBJ+0x68) | module width px | 0x08/0x09; feeds OneScanLen, CardScanLen | high |
| 0x001 | 1 | GetMoudleHeight (OBJ+0x6a) | module height as stored (32 here) | same | high |
| 0x003 | 2 LE | GetVoidPointCount (OBJ+0x6e) | void-point count | 0x23-0x24 BE | high |
| 0x018 | 4 LE | flag word #1 | see bit table below | many | high |
| 0x01C | 4 | f32 gamma (OBJ+0x98) | gamma (default 2.2; ours 2.8); triggers CalGamaTable | via gamma | high |
| 0x020 | 1 | GetScanMode (OBJ+0xC1) | scan denominator, literal (16 here) | **0x0B** | high |
| 0x021 | 2 LE | SetSerialClockFrequency | serial clock frequency (8 here) | **0x0D-0x0E BE** | high |
| 0x023 | 1 | GetGrayLevel | gray level (0x0E = 14-bit) | 0x0C | high |
| 0x024 | 2 LE | OBJ+0x82 | luminance/light-increase scalar (CalMaxLightInc) | 0x19 | high |
| 0x026 | 2 LE | vt+0x558/0x598 | 188 in our file | 0x1B | medium |
| 0x028 | 3 | OBJ+0x94..0x96 | 3-byte field (ff ff ff) | 0x05-0x07 | high |
| 0x02B | 1 | GetColorSwap (OBJ+0xD0) | colour-swap index | 0x14 | high |
| 0x02C | 3 | SetColorSource (OBJ+0xC4/C8/CC) | R/G/B exchange indices; (2,1,0)=none | 0x14 | high |
| 0x031 | 1 | SetGClock (OBJ+0xD3B8) | GCLK setting (default 0x14) | — | high |
| 0x032 | 1 | GetRedCurrentWithoutChip | red current gain 0-63 | 0x34 | high |
| 0x033 | 1 | GetGreenCurrentWithoutChip | green current gain | 0x35 | high |
| 0x034 | 1 | GetBlueCurrentWithoutChip | blue current gain | 0x36 | high |
| 0x035 | 1 | SetVirtualRedCurrent | virtual-red current gain | 0x37 | high |
| 0x036 | 1 | GetChipType low byte | chip id low (pairs with +0x204) | chip lib; gray override if type==0x5C | high |
| 0x037 | 1 | GetSerialType | serial type | — | high |
| 0x03C | 1 | GetLineDir (OBJ+0xD4C0) | line direction: 0/1 vertical, 2/3 horizontal | 0x08/0x09 order, 0x0A, 0x4A | high |
| 0x03D | 1 | OBJ+0xB5 low nibble (+OBJ+0xC2 high) | scan method | 0x26 | high |
| 0x03E | 1 | GetSplitSegment/GetSplitStyle (OBJ+0xB6) | split | 0x27 | high |
| 0x044 | 1 | OBJ+0xB9 | data-group / output code (low nibble) | 0x0A logic | high |
| 0x045 | 2 | SetGrayCompensation | gray compensation | 0x2C-0x2D | high |
| 0x049 | 2 LE | OBJ+0x7C | serial clock / 2 (derived from +0x021) | — | high |
| 0x04B | 2 LE | OBJ+0x7E | duplicate of +0x021 | — | high |
| 0x04E | 1 | GetOutPutModel ((v>>1)&7) | output model; bit0 = 32-group flag | 0x0A/0x2B | high |
| 0x050 | 1 | Get8nsOeEnableInfo (OBJ+0xBD) | OE timing info | 0x3B | high |
| 0x052 | 1 | OBJ+0xDC | special-module setting | 0x94 (with flag1 bit6) | high |
| 0x057 | 1 | OBJ+0x74 lo (hi at +0x24E) | GetModuleInputCount numerator (line dir 2/3) | 0x46, 0x4A | high |
| 0x058 | 1 | OBJ+0x76 lo (hi at +0x24F) | GetModuleInputCount numerator (line dir 0/1) | 0x47, 0x4A | high |
| 0x059 | 1 | HR_GetHighRefreshStyle | high-refresh style | — | high |
| 0x05A | 16 | OBJ+0xD3CC swap block 0 (ResetSwapData) | data/RGB swap table | — | high |
| 0x06A | 16 | SetChipCustom (vt+0x110, OBJ+0xD4D1..) | **chip-custom block**: +0x06A bit7 = use self GCK, bits0-6 + 0x06B = serial clock; 0x078/79 permuted by ExchangeThirdRegWhenLoadAndSaveFile @ 0x162270 | **0x74-0x83 verbatim** (resolves §21.2's gap) | high |
| 0x07A/0x08A/0x09A | 16 ea | swap blocks 1-3 | | | high |
| 0x0AA | 4 | f32 OBJ+0xDEF8 (UpDateBaseRefresh) | base refresh rate (60.0) | — | high |
| 0x0AE | 4 | f32 HR_SetMinOE | minimum OE | +0x0CC when IsSpecialNotPWMType | high |
| 0x0B2 | 1 | HR_SetScanStyle | HR scan style | — | high |
| 0x0B3 | 1 | GetHubType (OBJ+0xD4C4) | hub type (0 here) | 0x4B | high |
| 0x0B4/B8/BC | 4 ea | f32 OBJ+0x88/0x8C/0x90 | R/G/B current percent | 0xD8 via GetCurrentPercent | high |
| 0x0C0 | 2 LE | SetMaxWidth (OBJ+0xDF08) | MaxWidth (256 here) | 0x8C-0x8D BE | high |
| 0x0C2 | 2 LE | SetMaxHeight (OBJ+0xDF0A) | MaxHeight (384 here) | 0x8E-0x8F BE | high |
| 0x0C4 | 16 | vt+0x198 out-struct | dead-pixel current data (MakeCurrentOfDeadPixcelValid @ 0x167c50) | — | medium |
| 0x0E0 | 4 | SetChipCustomEX (vt+0x120) | chip-custom-EX (bytes permuted by ExchangeThirdReg) | 0xD4-0xD7 | high |
| 0x0E5 | 1 | OBJ+0xC2 | feeds +0x03D high nibble | 0x26 | high |
| 0x0E8 | 1 | GetLS9736ICNum/IsHasMBI5988 | | 0x91 | high |
| 0x0E9 | 1 | vt+0x48 lo (hi at +0x205) | secondary chip/decoder id | chip branches | high |
| 0x0EA | 4+2 | vt+0x178 | | 0xDC-0xE1 | high |
| 0x0FB | 1 | decode-chip enum 0-14 | which decoding chip present | | high |
| 0x114..0x173 | 6x16 | swap blocks (OBJ+0xD44C..) | 96-entry row map (identity in 99.6% of corpus) | | high |
| 0x180/0x181 | 1 ea | packed HDR-12bit / HLG-12bit gamma flags | | | high |
| 0x191 | 1 | SetIsSL9739EnabledEx (OBJ+0xBE) | | 0xDB | high |
| 0x199 | 1 | (OBJ+0xE141<<1) \| OBJ+0xE142 | bit1 = Set9929NewControl | | high |
| 0x19A..0x1D9 | 4x16 | swap blocks (OBJ+0xD40C..) | 64-entry table (ramp 64..127 or zero) | | high |
| 0x1DB | 4 LE | flag word #2 | see below | | high |
| 0x1E1/0x1E5/0x1E9 | 4 ea | (OBJ+0xE124/28/2C & 0xFFFFFF) \| (OBJ+0xE130/34/38 << 24) | packed pairs | | high |
| 0x1EE/0x1F0 | 2 ea | OBJ+0xE61E / OBJ+0xE620 | | 0xF9 / 0xFA | high |
| 0x1F7 | 1 | OBJ+0xE61C | | 0xFB | high |
| 0x202 | 1 | GetIsEnableToneMapping | tone-mapping enable | | high |
| 0x204 | 1 | GetChipType high byte | chip id high | chip lib | high |
| 0x205 | 1 | vt+0x48 high byte | secondary chip id high | | high |
| 0x24B | 1 | OBJ+0xDF7B (remap 0x1B->0xD8 if 8nsOe&4) | decode chip type (GetNewDecodeChipType) | | high |
| 0x247 | 1 | GetChipOhmValB (vt+0x3F8) | | 0xFC | medium |
| 0x24C | 1 | GetDeadPixelsCurrentGain (vt+0x418) | | 0xFD | medium |
| 0x277-0x2FB | — | constructor constants: `01 01`, zeros, `01 00 01 00`, `01 00`, then a 120-byte zero tail never read | — | high |

Offsets not listed above are either constants (see the constructor), provenance-
known-but-meaning-unresolved members, or explicitly NOT RESOLVED — the full
per-byte account incl. all vt-slot pins is in the session transcript and
`analysis/record01-fieldmine/fielddict.csv`.

## Flag word #1 (payload +0x018, u32 LE)

bit0 OBJ+0xBF (pack 0x17) · bit1 OBJ+0xC0 (pack 0x16) · bits3-5 OBJ+0xD3BD-BF ·
bit6 OBJ+0xD8 SpModule enable (pack 0x94) · bit7 OBJ+0xD3C2 · bit10 OBJ+0x78
(pack 0x48) · bit11 OBJ+0xD3C0 (pack 0x49) · **bit12 OBJ+0xD6EA: geometry
source select — clear = module W/H/void from payload 0x000-0x007 (our file:
clear)** · bit13 gamma-10bit · bit14 OBJ+0x86 · bit15 OBJ+0xB4 · bit16
OBJ+0xD6D8 · bit17 OBJ+0xDEF0 · bit18 vt+0x5A8()==2 · bit19 OBJ+0x87==0 ·
bit20 vt+0xA8()==0 · bit21 GetIsSetCustomModulePosSusseed · bit22 OBJ+0xDEF1 ·
bit23 vt+0xB8()==0 · bit24 UseSeparateGamma8bit · bit26 UseSeparateGamma10bit ·
bit28 OBJ+0xE0C5 · bit29 OBJ+0xE0AD · bit30 OBJ+0xD4D0 (max-size gate) · bit31
vt+0x5A8()==4. Bits 2, 8, 9, 25, 27: NOT RESOLVED.

## Flag word #2 (payload +0x1DB, u32 LE)

bit0 IsHasICND2019 · bit1 OBJ+0xE148 · bits3/4 gamma-calc method · bit5
OBJ+0xE120 · bit6 IsHasVOD5958 · bit7 UseSeparateGamma12bit · bit8 OBJ+0xE08E ·
bit9 OBJ+0xDF15 · bit10 vt+0x288()&1 for chip ∈ {0x74,0x131,0x15D} · bit11
vt+0x2A0 · bit12 OBJ+0xD6D9 · bit13 always set · bits13|14 OBJ+0xE1A3.

## Derived quantities (computed, not stored)

* **OneScanLen** @ 0x138e90 = `moduleW x moduleH_stored / scan`, clamped >= 1.
  Ours: 128 x 32 / 16 = **256**.
* **CardScanLen** @ 0x145420 = OneScanLen scaled by `GetMaxInLineDir() /
  module dimension`. Factory (2 modules in line dir): 512; one module: 256.
* **GetModuleCountInLineDir** @ 0x14b1e0 = `ceil(GetMaxInLineDir()/moduleDim)`
  where MaxInLineDir comes from the IRcvMaxSize interface (the CARD/live
  layout), not record 0x01 — baked into the pack at compile time.
* GetRgbSelValue (OBJ+0xDF7C) is not stored in record 0x01; its setter is
  never called in the dylib.

## Corrections to earlier docs (verified against the factory pack body)

1. vt+0x50 is **GetChipType** @ 0x16daa0, not GetSplitSegment — §21.2's gray
   override is a chip-type test.
2. **pack 0x0B is the scan denominator** (factory body[0x07] = 0x10 = 16),
   not GetRgbSelValue. The card never was at 1/8 scan.
3. **pack 0x0D-0x0E BE is the serial clock frequency** (factory = 8), not the
   scan mode. §21.2 conflated these; basic-pack-single-module.bin (v1)
   accidentally doubled the serial clock — v2 fixes this.
4. New §7.3 fills: pack 0x8C-0x8F = MaxWidth/MaxHeight BE; pack 0x34-0x37 =
   the four current gains; pack 0x74-0x83 = the chip-custom block (record
   payload +0x06A..0x079 verbatim).
