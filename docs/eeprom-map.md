# The receiver's on-board EEPROM: frame format and address map

The E120 keeps a small block of per-card settings in an I²C EEPROM, separate
from the SPI flash that holds the gateware and the compiled parameter image.
This is where the card's **identity inside a screen** lives, and it is the one
region our config-generation path never wrote.

Everything below is static analysis of
`iSet.app/Contents/Frameworks/lib/libCLTDevice.1.dylib` (Mach-O x86_64, C++
symbols intact). Nothing was executed.

## 1. The access frame, type 0x1900 (HIGH)

`BulidEepromFlashOperation(u32 *outLen, u8 **outBuf, u16 rcvIdx, u8 opcode,
u32 addr, u8 *data, u32 dataLen)` @ `0x30bdd0`. Transcribed literally from
`0x30be38`–`0x30bea2` (buffer base `%rbx`, offsets are *payload* offsets, i.e.
frame offset 12 + n):

```
movw $0x19, (%rbx)          payload[0..1] = 19 00      type word
movb $0x00, 0x2(%rbx)       payload[2]    = 0
movb %ah,   0x3(%rbx)       payload[3..4] = rcvIdx, BIG-endian
movb %al,   0x4(%rbx)
movb %r15b, 0x5(%rbx)       payload[5]    = opcode
movb %cl,   0x6(%rbx)       payload[6..9] = addr, BIG-endian u32
movb %al,   0x7(%rbx)
movb %dh,   0x8(%rbx)
movb %dl,   0x9(%rbx)
movb %cl,   0xa(%rbx)       payload[10..13] = dataLen, BIG-endian u32
movb %al,   0xb(%rbx)
movb %ah,   0xc(%rbx)
movb %al,   0xd(%rbx)
memcpy(buf + 0xe, data, dataLen)     payload[14..] = the data
```

| item | value | source |
|---|---|---|
| payload length | `max(0x80, dataLen + 0x12)` | `0x30be07`–`0x30be18` |
| frame length | `12 + payload length` | ″ |
| data attached only when | `opcode + 0x7b ∈ [0,5]` and `≠ 2`, i.e. opcode ∈ {0x85, 0x86, 0x88, 0x89, 0x8a} | `0x30be85`–`0x30be93` |
| no data for | 0x44 / 0x45 (read) and **0x87** (save/commit) | ″ |

This is byte-for-byte the frame `e120_proto::eeprom` builds (its `p[n]` =
`payload[n+2]`); the repo once treated it as a linear flash accessor.

### Opcodes seen at call sites

| opcode | meaning |
|---|---|
| `0x44` | read (`BulidEepromFlashOperation`) |
| `0x45` | read, second variant (used for addresses ≥ 0x118) |
| `0x85` | write |
| `0x86` | write, variant (`WriteEepromRcvCardLight`) |
| `0x87` | **save EEPROM to flash**: `CReceiverOP::SaveEepromFlash` @ `0x3b9f00`, `addr = 0`, `dataLen = 0` |
| `0x88` | write, second variant (addresses ≥ 0x118) |
| `0x89` / `0x8a` | write / save variants (`WriteDataToEepromFlashEx`, `SaveEepromFlashEx`) |

## 2. Why `0x0007F000` works

`docs/archive/config-protocol.md` §24 established that the type-0x1900 address
is masked into a small space, so the repo's `SCREEN_RECORD_ADDR = 0x0007F000`
lands at EEPROM byte **0**. The bench evidence agrees: `e120 card screen-size`
returns 256 bytes whose fields line up exactly with the address map below.

Independently, the card **mirrors the EEPROM to SPI flash at `0x07F000`**:
the day-one dump and a later snapshot of the primary region
were both taken with page-addressed (type 0x0600) reads, and both show the
EEPROM image there. So a page dump of block 0x07 is a free read-back of the
EEPROM, and a whole-block erase of block 0x07 *is visible* in that mirror.

## 3. Address map (HIGH)

Extracted mechanically from every call site of `BulidEepromFlashOperation`
(and its ROEFan/Power variants), reading the `%ecx` (opcode), `%r8d` (address)
and pushed length arguments. `addr`/`len` are decimal-exact from immediates.

| addr | len | record | function |
|---|---|---|---|
| `0x00` | 2 | debug bytes | `Read/WriteEepromDebugByte` |
| **`0x02`** | **42** | **control area** (see `docs/receiver-identity.md`) | `WriteEepromCtrlAreaOffset` @ `0x3b2fc0`, `GetSaveCMD_CtrlAreaParam` @ `0x1a4fc0` |
| `0x2c` | 18 | colour-gamut coefficients | `WriteEepromColorGamutCoef` |
| `0x3e` | 1 | gamut-adjust enable | `Read/WriteEepromGamutAdjEnable` |
| `0x40` | 1 | calibration status | `Read/WriteEepromCalibrationStatus` |
| `0x41` | 1 | "no input" show info | `Read/WriteEepromNoInputShowInfo` |
| `0x42` | 1 | turn-on screen show | `ReadEepromTurnOnScreenShow` @ `0x3c0b30` |
| `0x43` | 3 | white-balance adjust | `Read/WriteEepromWhiteBalanceAdj` |
| `0x4b` | 1 | calibration-coefficient source | `Read/WriteEepromCaliCoefFrom` |
| `0x4c` | 1 | seam enable | `Read/WriteEepromSeamEnable` |
| `0x4d`–`0x55` | 9 | **NOT RESOLVED**; no `BulidEepromFlashOperation` call site covers it. Factory content `28 00 00 00 01 80 01 00 00`, which contains the reference file's wall dimensions 384 (`0x0180`) and 256 (`0x0100`) | — |
| `0x56` | 3 | void-line info | `ReadEepromVoidLineInfo` |
| `0x59` | 1 | receiver-card light | `WriteEepromRcvCardLight` (opcode 0x86) |
| `0x5a` | 20 | receiver card name (ASCII) | `Read/WriteEepromRcvCardName` |
| `0x6e` | 1 | 14-way open flag | `Read/WriteEeprom14WayOpenFlag` |
| `0x6f` | 1 | gamma-calibration status | `Read/WriteEepromGammaCaliStatus` |
| `0x70` | 1 | ROE current/bright flag | `WriteROEEepromCurrentBrightFlag` |
| `0x72` | 1 | virtual-pixel param | `Read/WriteEepromVirtualPixelParam` |
| `0x76` | 1 | full-screen seam-factor enable | `Read/WriteEepromFullScreenSeamFactorEnable` |
| `0x77` | 1 | four-deseam | `Read/WriteEepromFourDeseam` |
| `0x7b` | 1 | plus-module 7-way adjust enable | `Read/WriteEepromPlusModule7WayAdjEnable` |
| `0x7c` | 1 | double-cali chroma enable | `Read/WriteEepromDoubleCaliChromaEnable` |
| `0x7d` | 1 | plus low-bright cali enable | `Read/WriteEepromPlusLowBrightCaliEnable` |
| `0x7e` | 1 | double-cali enable | `Read/WriteEepromDoubleCaliEnable` |
| `0x7f` | 2 | double-cali threshold | `Read/WriteEepromDoubleCaliThreshold` |
| **`0x92`** | **32** | **control-area blob, high half, companion to `0x02`** | `WriteEepromExtendDataOffset` @ `0x3c1820` |
| `0xb2` | 1 | parameter switch | `Read/WriteEepromParamSwitch` |
| `0xb3` | 1 | plus-chip low-bright cali enable | `Read/WriteEepromPlusChipLowBrightCaliEnable` |
| `0xb4` | 3 | plus-chip low-bright uniformity | `Read/WriteEepromPlusChipLowBrightCaliUniformity` |
| `0xc1` | 12 | GX custom FCCL | `Read/WriteEepromGXCustomFCCL` |
| `0xc8` | 1 | plus temperature-control enable | `Read/WriteEepromPlusTemperatureControlEnable` |
| `0xce` | 16 | double-cali threshold (long form) | `Read/WriteEepromDoubleCaliThreshold` |
| `0xe1` | 1 | plus-module current-adjust enable | `Read/WriteEepromPlusModuleCurrentAdjEnable` |
| `0xf4` | 2 | preset temperature info / ROE fan | `Read/WriteEepromPresetTempInfo`, `WriteEepromROEFanParam` |
| `0xf6` | 1 | power-off bright coefficient | `Read/WriteEepromPowerOffBrightCoef` |
| `0xf7` | 2 | EMC info | `Read/WriteEepromEMCInfo` |
| `0xf9` | 1 | module power switch | `Read/WriteEepromModulePowerSwitch` |
| `0xfa` | 1 | current/bright flag | `Read/WriteEepromCurrentBrightFlag` |
| `0xfd` | 1 | screen-shake param | `Read/WriteEepromScreenShakeParam` |
| `0x118` | 15–16 | multi-seam param (opcodes 0x45 / 0x88) | `Read/WriteEepromMultiSeamParam` |
| `0x127` | 1 | thermal-cali param (opcodes 0x45 / 0x88) | `Read/WriteEepromThermalCaliParam` |

The EEPROM is therefore **at least 0x128 bytes**, not 256. Addresses ≥ 0x118
use a different opcode pair (0x45 / 0x88), which fits a second device or a
paged access above 0x100.

> `e120 card screen-size --set` writes 256 bytes starting at EEPROM 0, i.e.
> every record in the table above. Written back after a block-0x07 erase, it
> persisted `0xFF` into `NoInputShowInfo` (0x41), `TurnOnScreenShow` (0x42),
> `SeamEnable` (0x4c) and the control-area offsets
> ([receiver-identity.md](receiver-identity.md) §4). It refuses a record
> that reads as erased. `e120 provision` and `scripts/eeprom-restore.py`
> write each record at its own address and length; the card silently ignores
> a write that spans record boundaries, and drops back-to-back writes.
