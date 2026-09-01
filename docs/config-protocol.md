# Colorlight receiver-card configuration protocol

Reverse-engineered statically from `libCLTDevice.1.dylib` (iSet 7.0 macOS,
x86_64 Mach-O, full C++ symbols). Addresses below are file/vaddr offsets in
that dylib. Every claim is tagged with the function + instruction it came from.
Items I could not fully pin down are marked **UNCERTAIN**.

---

## 0. TL;DR for the Rust implementation

* **Framing is trivial and fully confirmed.** Every control frame is raw
  Ethernet: `dst = 11:22:33:44:55:66`, `src = 22:22:33:44:55:66`, followed by a
  payload whose **first two bytes are the "type" field** (the EtherType slot,
  frame offset 12–13). This is exactly the framing our `protocol.rs` already
  uses.
* **The config upload is NOT a verbatim blob replay.** The `.rcvbp` file's
  decompressed 89 070-byte blob is parsed into a `CHWParamReceiver`/
  `CHWParamRcvGeneral` object and then **re-serialized into many small typed
  packs** (`Get*ParamPack` functions). You cannot just wrap the blob in one
  Ethernet frame.
* **"Send parameters" (to RAM / real-time)** = build a `vector<CSendControl*>`
  of typed packs via `GetParamPacksBasic[Ex]`, then transmit them in order with
  short `usleep` gaps. Primary pack type is **0x05** (basic parameter, 260-byte
  payload → 272-byte frame). Additional packs: **0x10**, **0x18** (and larger,
  chunked gamma-table packs).
* **"Save to flash"** = everything "send" does, **plus** EEPROM/flash write
  packs (`SaveBasicParam`, `DoWriteGammaTable*`) so the config persists across
  power cycles.
* **Transport on macOS** = a raw socket (`socket`/`bind`/`sendto`/`recvfrom`,
  PF_NDRV-style), not libpcap, in this dylib. Our BPF sender is an equivalent
  substitute and works for both TX and RX.

**Confidence:** framing = *certain*; pack ordering/type bytes = *high*; exact
byte layout of the 0x05 basic-param body = *partial* (structure known, not every
field decoded); flash-save sequence = *medium*.

---

## 1. Ethernet framing (CONFIRMED)

`CSendControl::CSendControl(unsigned char* data, unsigned int len)` @ `0x2572a0`
(and identical twin @ `0x257330`) builds the on-wire buffer:

```
allocate len + 12                      ; lea r12, [r15+0xc]; operator new[]
bzero(buf, len+12)
memcpy(buf + 0x0c, data, len)          ; payload copied to offset 12
*(u64*)(buf + 0)  = 0x2222665544332211 ; little-endian →  bytes 11 22 33 44 55 66 22 22
*(u32*)(buf + 8)  = 0x66554433         ; little-endian →  bytes 33 44 55 66
```

Resulting frame:

```
offset 0   : 11 22 33 44 55 66     destination MAC (the receiver card)
offset 6   : 22 22 33 44 55 66     source MAC (the sender/PC)
offset 12  : <payload[0]>          == "type" high byte  (EtherType slot)
offset 13  : <payload[1]>          == "type" low byte
offset 14+ : <payload[2..]>        == command body
```

So a "pack" as produced by the `Build*`/`Get*ParamPack` helpers is the **Ethernet
payload including the 2-byte type at its start**; `CSendControl` only prepends
the 12 MAC bytes. `GetBufAddress()` @ `0x257450` / `GetBufferLen()` @ `0x257460`
return this full frame for transmission.

### Header shape of the `Build*` family (CONFIRMED via `BuildDetectRcvCard` @ `0x30a370`)

```
payload[0] = 0x07        ; type high  (mov word [rbx], 7)
payload[1] = 0x00        ; type low
payload[2] = 0x00
payload[3] = index MSB   ; mov [rbx+3], ah   (receiver index, u16)
payload[4] = index LSB   ; mov [rbx+4], al
payload[5..] = 0x00      ; zero padded; total payload 0x110 = 272 bytes for detect
```

This is the **discovery/detect** frame: type `0x0007`, 272-byte payload, receiver
index at bytes 3–4. Matches the community "0x07 discover" packet and our
`protocol::discovery()`.

---

## 2. "Send parameters" frame sequence

Entry chain:

```
CReceiverOP::SaveBasicParamFromFile(path,...)          @ 0x3a9e10   (loads .rcvbp → object)
CReceiverOP::SendOrSaveBasicParam / DoSendOrSaveBasicParam @ 0x3b5280 / 0x3b5400
  → CBasicParamSendAndWriter::SendOrSave(...)           @ 0x31e230
      → CBasicParamSendAndWriter::DoSendSave()          @ 0x31e520
```

`DoSendSave` @ `0x31e520` calls, in order (confirmed from its call list):

1. `CHWParamRcvGeneral::Multi_ModuleDataConversion()` @ `0x16ad60` — expand the
   parsed cabinet/module layout into the internal data model.
2. `PrepareShapeModuleParam()` @ `0x32c1a0`.
3. **`GetParamPacksBasic(CHWParamRcvGeneral*, vector<CSendControl*>&, uint, uchar, bool)`** @ `0x31f1e0`
   — builds the ordered pack list (see §2.1).
4. `PrepareFlashData()` @ `0x32c2c0` (only relevant to the flash-save path).
5. `CalculateTime()` @ `0x32c720`.
6. **`SendRealTimePacks()`** @ `0x32cf40` — hands the pack vector to the device
   IO for transmission, with `usleep` pacing between groups (multiple `usleep`
   calls interleaved with `NotifyProgress`).
7. On the save path: `SaveBasicParam()` @ `0x32d130` and the
   `DoWriteGammaTable*` family (§3).

### 2.1 Packs emitted by `GetParamPacksBasic` (@ `0x31f1e0`) and the `Ex` variant `GetParamPacksBasicEx` (@ `0x3261f0`)

Each pack is: bzero a stack buffer, set a **type byte at payload[0]**, set a few
sub-fields, fill the body from a `CSendAndSaveRcvParam::Get*ParamPack` call, then
`CSendControl(buf, len)` and push into the vector. Confirmed type bytes / sizes:

| Order | payload[0] (type) | payload size | frame size | Body source (`CSendAndSaveRcvParam::`) | Notes |
|------|------|------|------|------|------|
| chip-custom | 0x83 **UNCERTAIN** | large (~0x10ab8 buf) | — | `GetChipCustomPlusParamPack` @ 0x1ea2b0 | only for multi-register chips (`IsMultiRegisterChip`) |
| data-swap | (set with 0x05 group) | 0x104=260 | 272 | `GetDataSwapEx2ParamPack` @ 0x1ec700 | |
| **basic param** | **0x05** | **0x104 = 260** | **272** | `GetBasicParam` @ 0x1dfb50 | main scan/chip/timing pack; see §2.2 |
| void table | 0x05 group | 0x104 | 272 | `GetVoidTablePack` @ 0x1e5710 | |
| pixel sequence | — | var | — | `GetPixelSequencePacks` @ 0x1e5aa0 | multiple packs, count out-param |
| void line info | — | var | — | `GetVoidLineInfoPacks` @ 0x1e58c0 | multiple |
| anti void line | — | var | — | `GetAntiVoidLineInfoPacks` @ 0x1e59b0 | multiple |
| gamma-calib gray | — | 0x403 body | — | `GetGammaCalibrationGrayPack` @ 0x1f4850 | |
| gamma-calib delta | — | var | — | `GetGammaCalibrationDeltaPack` @ 0x1f4950 / `...NewDelta` @ 0x1f4a20 | |
| gamma tables | — | large, chunked | — | `Get8/10/12BitGamaTablePack`, HDR/HLG/XYZ variants | see §3 |
| config pack A | **0x10** | (seen ×2) | — | — | matches FPP "0x10 len 1026" |
| config pack B | **0x18** | (seen ×2) | — | — | matches FPP "0x18" config type |

Type bytes were read directly from the `mov byte [rbp - off], imm` that precedes
each `CSendControl` call in the disassembly of `0x31f1e0` / `0x3261f0`:
`0x05`, `0x10`, `0x18`, `0x83` observed; sub-field bytes `0x01`, `0x80` observed.

### 2.2 The 0x05 basic-parameter pack (partial layout)

From `GetParamPacksBasic` @ `0x31f5f7…0x31f647` (single-index path):

```
payload[0x00] = 0x05        ; type            (mov byte [rbp-0x240], 5)
payload[0x0a] = 0x01        ;                 (mov byte [rbp-0x236], 1)
payload[0x1e] = 0x80        ;                 (mov byte [rbp-0x222], 0x80)
payload[0x02..0x103] = body ; filled by GetBasicParam(&pack) @ 0x1dfb50
size = 0x104 (260) → 272-byte frame
```

From `GetParamPacksBasicEx` @ `0x326346` / `0x326502` (multi-receiver path) the
same pack additionally carries a **1-based receiver index at payload[3]**:

```
payload[0x00] = 0x05
payload[0x03] = 0x01, 0x02, …   ; receiver index, increments per card
```

This matches FPP's observation that config packets carry the receiver number at
`data[3]`. For our single E120 the index is 1.

`GetBasicParam` @ `0x1dfb50` writes the body fields (scan mode, chip control,
GCLK ratio, small-card flags, phase, anti-route, etc.) into the struct — e.g.
`[+0x04]=0xA8`, packed nibbles via `shl dl,2` / `shl cl,5` into `[+0x22]`, and
many zeroed reserved bytes. **UNCERTAIN:** the full field-by-field meaning of the
259-byte body is not decoded here; it derives from the parsed `.rcvbp` object and
the selected chip's `pm_*.dat` profile. Reproducing it exactly in Rust means
replicating `GetBasicParam` field-by-field.

---

## 3. "Save to flash" (persistent) additions

On top of §2, the save path runs (from `DoSendSave` / `SendOrSave` call lists):

* `PrepareFlashData()` @ `0x32c2c0`.
* `SaveBasicParam()` @ `0x32d130`.
* Gamma-table writers, each `Prepare* → Calculate*Time → DoWrite*`:
  * `DoWriteGammaTable()` @ `0x32db60`
  * `DoWriteHDRGammaTable()` @ `0x32df40`
  * `DoWriteHLG12BitGammaTable()` @ `0x32e320`
  with `usleep` + `NotifyProgress` between each.
* Gamma buffers for SPI flash come from
  `CHWParamRcvGeneral::GetRcvGammaTableBufForSPIFlash` @ `0x154c30`,
  `...GetRcvGammaCaliBufForSPIFlash` @ `0x1607b0`,
  `...GetRcvHDRGammaTableBufForSPIFlash` @ `0x15f990`,
  `...GetRcvHLG12BitGammaTableBufForSPIFlash` @ `0x15f9f0`.

The low-level EEPROM/flash writes use the receiver-flash pack builders:

* `BuildRcvCardFlashOperation` @ `0x30b790`,
  `BuildRcvCardFlashOperationEx` @ `0x30b8e0`,
  `BuildModuleEepromFlashOperation` @ `0x30b690`,
  `BuildRcvStorageErase` @ `0x30bad0`,
  `BuildRcvStorageHalfPageWrite` @ `0x30bb70`,
  `BuildSDRAMOperation` @ `0x30cd70`.
  Signatures end in `(…, unsigned char* data, unsigned int len)` — the payload is
  chunked and the chunk length is a `u32` arg. **UNCERTAIN:** exact chunk size and
  flash-address stride were not measured; capture is the fastest way to nail them.
* Higher-level EEPROM param packs: `CReceiverOP::WriteDataToEepromFlash` @ `0x3b9d60`,
  `...Ex` @ `0x3b9e30`, `SaveEepromFlash` @ `0x3b9f00`,
  `CSendAndSaveRcvParam::GetEepromPacks` @ `0x1e7df0`,
  `...GetEepromExPacks` @ `0x1e8990`,
  `...GetRcvParamBufForEeprom` @ `0x1ecba0`.

FPP's captured "save config" types line up: **0x11** (save config, `data[3]`=rx
index), plus `0x1F`, `0x26`, `0x31`, `0x32`, `0x76` seen during a LEDVISION save.
These are the flash/erase/table packs above. (Type bytes for the flash builders
not individually confirmed here — mark **UNCERTAIN**, resolve by capture.)

---

## 4. Transport & responses

* **Send layer:** `CDeviceSetIO::SendData(...)` @ `0x24ff30` / `0x24ff20`
  → `CDeviceChainIO` subclass. This dylib imports `socket`, `bind`, `sendto`,
  `recvfrom`, `recv`, `setsockopt` — a **raw Ethernet socket** (macOS PF_NDRV,
  bound to the chosen `en*` interface with a `sockaddr_ndrv`). No libpcap import
  here (the Windows build used WinPcap; the class name `CDeviceChainIOPcap`
  survives but the macOS build uses raw sockets). **Our BPF-based sender is a
  valid equivalent.**
* **Receive/ack:** `CDeviceChainIOPcap::WriteRead` @ `0x22a7c0` and
  `AnalysisReadDataNetCard` parse returned frames via `CSendCmdReceiveData`
  (`GetOpType` @ …, `GetFrameCountLength`, `AddReceiveData`, `GetReceiveFrameCount`,
  `SetRetHRESULT`). The model is **request → collect N reply frames → set an
  HRESULT**. Success = expected reply frame count reached and HRESULT OK.
* **Discovery reply:** type `0x08` frame from the card (our capture: payload[0]=
  `0x64` for E120, `[1..2]` firmware `10.81`, `[20..23]` detected size 128×64).
  Used to confirm a card is present before/after config.
* **Pacing:** `usleep` calls sit between pack groups in `SendRealTimePacks` and
  between each flash/gamma write — a few hundred µs to low-ms. Exact values
  **UNCERTAIN** (register-loaded); use ≥500 µs between frames to be safe.

---

## 5. How the `.rcvbp` maps onto the wire

* File = 32-byte header + zlib stream at offset `0x20`. For
  `P2.5-32S-128X64-SM16269S-256X384I.rcvbp` it inflates to **89 070 bytes**.
* That blob is the serialized `CHWParamReceiver` (cabinet + module + chip +
  gamma + calibration). It is **loaded into an object**, not transmitted.
  (`SaveBasicParamFromFile` @ `0x3a9e10` reads the file; `CBasicParamSendAndWriter::
  DoLoadParam` @ `0x337d90` loads param into the sender.)
* The object is then **re-serialized** by the `Get*ParamPack` functions into the
  typed packs of §2–§3. Therefore the decompressed blob's bytes do **not** map
  1:1 to any single frame; individual fields are scattered into 0x05/0x10/0x18/
  gamma/eeprom packs.

**Implication for us:** a faithful Rust re-implementation of the *full* config
upload requires porting `GetBasicParam` + the pack builders (large but bounded —
all sym!). The **fast path** to first light is smaller (see §6).

---

## 6. Recommended implementation path (for the parent)

1. **Fastest test:** the card already answers discovery and self-reports 128×64,
   which means it *has* some stored params. The dark panel is more likely a
   scan/chip mismatch than "no data path". Try our existing `0x55` pixel + `0x01`
   sync + `0x0A` brightness stream first with a **continuous refresh** and a
   confirmed panel power supply before writing any config. (Already attempted;
   still dark → config is genuinely needed, or panel power/ribbon issue.)
2. **Minimum viable config:** implement the **type 0x05 basic-parameter pack**
   (272-byte frame, `payload[0]=0x05`, `payload[3]=1`) by porting `GetBasicParam`
   @ `0x1dfb50` field-mapping from the parsed `.rcvbp`. Send it (real-time, no
   flash) then the pixel stream. This is the smallest thing that can light a
   correctly-scanned panel.
3. **Robust path:** add a `pcap-summary` / `replay` mode (already scaffolded in
   `main.rs`) so that if a capture of iSet/LEDVISION configuring *this* card is
   ever obtained, we can replay the exact `PC→card` frames byte-for-byte and/or
   diff them against our generated packs. This sidesteps the UNCERTAIN fields.
4. **Persistence:** only after real-time send lights the panel, port the flash
   writers (§3) to make it survive power cycles.

---

## Appendix A — functions analyzed

| Address | Symbol | Role |
|------|------|------|
| 0x3a9e10 | `CReceiverOP::SaveBasicParamFromFile` | load `.rcvbp` from disk, kick off send/save |
| 0x3b5280 | `CReceiverOP::SendOrSaveBasicParam` | public send/save entry |
| 0x3b5400 | `CReceiverOP::DoSendOrSaveBasicParam` | constructs `CBasicParamSendAndWriter`, calls SendOrSave |
| 0x31e230 | `CBasicParamSendAndWriter::SendOrSave` | top of send/save logic |
| 0x31e520 | `CBasicParamSendAndWriter::DoSendSave` | ordered pipeline: convert → build packs → send → (save) |
| 0x31e990 | `...::DoSendSaveEx` | multi-receiver variant |
| 0x31f1e0 | `...::GetParamPacksBasic` | builds ordered `vector<CSendControl*>` of typed packs |
| 0x3261f0 | `...::GetParamPacksBasicEx` | same, with per-receiver index at payload[3] |
| 0x32cf40 | `...::SendRealTimePacks` | transmit pack vector, usleep-paced |
| 0x32d130 | `...::SaveBasicParam` | persist basic param to flash |
| 0x32db60 / 0x32df40 / 0x32e320 | `...::DoWrite[HDR/HLG12Bit]GammaTable` | flash gamma-table writers |
| 0x337d90 | `...::DoLoadParam` | load parsed param into sender |
| 0x2572a0 / 0x257330 | `CSendControl::CSendControl(uchar*,uint)` | **prepend MACs**, wrap payload → full frame |
| 0x257450 / 0x257460 | `CSendControl::GetBufAddress/GetBufferLen` | expose frame for TX |
| 0x30a370 | `BuildDetectRcvCard` | discovery frame (type 0x0007, idx@3-4, 272B) |
| 0x1dfb50 | `CSendAndSaveRcvParam::GetBasicParam` | fill 0x05 basic-param body |
| 0x1ec700 | `...::GetDataSwapEx2ParamPack` | data-swap pack body |
| 0x1ea2b0 | `...::GetChipCustomPlusParamPack` | multi-register chip custom pack |
| 0x1e5710 | `...::GetVoidTablePack` | void-point table |
| 0x1e5aa0 | `...::GetPixelSequencePacks` | pixel-sequence packs (multi) |
| 0x1e58c0 / 0x1e59b0 | `...::Get[Anti]VoidLineInfoPacks` | void-line info packs |
| 0x1e7df0 / 0x1e8990 | `...::GetEeprom[Ex]Packs` | EEPROM param packs |
| 0x1ecba0 | `...::GetRcvParamBufForEeprom` | EEPROM param buffer |
| 0x30b790… | `BuildRcvCardFlashOperation` (+Ex/Module/Storage/SDRAM) | low-level flash/erase/write frames, `(data*,len)` chunked |
| 0x24ff30 | `CDeviceSetIO::SendData` | dispatch pack list to device chain IO |
| 0x22a7c0 | `CDeviceChainIOPcap::WriteRead` | send + collect reply frames, set HRESULT |

## Appendix B — imports proving the transport
`socket`, `bind`, `sendto`, `recv`, `recvfrom`, `setsockopt` (raw Ethernet,
PF_NDRV-style). No `pcap_*` imports in the macOS dylib.

---

## 7. The 0x05 body, field by field

Source: `CSendAndSaveRcvParam::GetBasicParam(SBasicParamPack*)` @ `0x1dfb50`
(13 702 bytes, fully disassembled). Register conventions inside the function:

* `rsi` (later reloaded from `var_450h` into `rbx`/`rcx`/`rdx`/`rax`) = **pointer
  to the pack = payload byte 0**. So `mov byte [rbx + 0x46], al` writes
  **payload[0x46]**, i.e. frame offset `14 + 0x46`.
* `r14` = `this` (`CSendAndSaveRcvParam`); `qword [r14]` = **P**, the
  `CHWParamRcvGeneral` object parsed from the `.rcvbp`. `OBJ+0xNN` below means
  `*(P + 0xNN)`.

### 7.1 Endianness (CONFIRMED)

Every 16-bit field is stored **big-endian**: the code loads the value, executes
`rol ax, 8`, then `mov word [pack+off], ax`. Example at `0x1dff1a`:

```
call CHWParamRcvGeneral::GetOneScanLen()
rol  ax, 8
mov  word [rbx + 0xf], ax          ; payload[0x0f..0x10] = scan length, BE
```

Single bytes obviously have no order. `dword` stores at 0x088 and 0x100 were not
seen to be byte-swapped (**UNCERTAIN**).

### 7.2 Fixed header bytes (CONFIRMED, function prologue `0x1dfb75`–`0x1dfc09`)

```
payload[0x00] = 0x05                 ; type, written by the CALLER (GetParamPacksBasic)
payload[0x03] = receiver index       ; written by CALLER (Ex path), 1-based
payload[0x04] = 0xA8                 ; CONSTANT   (mov byte [rsi+4], 0xa8)
payload[0x05] = OBJ+0x94             ; mov al,[rdi+0x94] ; mov [rsi+5],al
payload[0x06] = OBJ+0x95
payload[0x07] = OBJ+0x96
payload[0x08] = module W/H pair lo   ; from qword[P+0x68] (W=+0x68, H=+0x6a) via GetLineDir() swap
payload[0x09] = module W/H pair hi   ; the two are swapped when GetLineDir() >= 2
payload[0x0a] = 0x01                 ; written by CALLER
payload[0x1e] = 0x80                 ; written by CALLER
```

The `[+8]/[+9]` pair: `r12 = qword[P+0x68]` (low 16 = module width `GetMoudleWidth`,
next 16 = module height `GetMoudleHeight`), `r15d = dword[P+0x70]`; `GetLineDir()`
decides the order (`cmp eax,2` + `cmovb`), so for line-dir < 2 the bytes are
(height-derived, width-derived) and swapped otherwise. **Confidence: high on the
mechanism, medium on which byte is width vs height** — verify against a capture
or just try both.

### 7.3 Full offset table

Legend: `P->X()` = value returned by that `CHWParamRcvGeneral` accessor;
`OBJ+0xNN` = raw byte copied from the parsed object at that member offset
(meaning may be unknown, but you can copy it); `CONST` = literal; `[ops]` =
bit-manipulation applied before the store (packed bitfield). Multiple entries
separated by `|` are alternate code paths for the same byte (different chip /
scan configurations) — the first is the main path.

| payload off | size | endian | source(s) |
|---|---|---|---|
| 0x00a | byte | - | P->GetModuleCountInLineDir() | OBJ+0x46 |
| 0x00b | byte | - | P->GetRgbSelValue() | OBJ+0x4c |
| 0x00c | byte | - | P->GetGrayLevel() | CONST 0x10 | CONST 8 |
| 0x00d | word | BE | P->GetScanMode() |
| 0x00f | word | BE | P->GetOneScanLen() | OBJ+0x4a |
| 0x011 | word | BE | P->GetCardScanLen() | OBJ+0x42 |
| 0x013 | byte | - | ?[or] |
| 0x014 | byte | - | P->GetColorSwap()[add/or] |
| 0x015 | byte | - | CONST 0x99 | CONST 0x77 | CONST 0 |
| 0x016 | byte | - | OBJ+0xc0 |
| 0x017 | byte | - | OBJ+0xbf |
| 0x018 | byte | - | ? |
| 0x019 | byte | - | OBJ+0x82 |
| 0x022 | byte | - | ?[and/or/shl/or] |
| 0x023 | word | BE | P->GetVoidPointCount() | OBJ+0x48 |
| 0x026 | byte | - | OBJ+0xb5[and] |
| 0x027 | byte | - | OBJ+0xb6[and] |
| 0x028 | byte | - | OBJ+0xb7 |
| 0x029 | byte | - | ? | OBJ+0x47 |
| 0x02c | word | - | OBJ+0xc369 |
| 0x02e | byte | - | ? | ? |
| 0x030 | byte | - | ?[shr] |
| 0x031 | byte | - | ?[shr] |
| 0x032 | byte | - | ?[shr] |
| 0x033 | byte | - | ? |
| 0x038 | byte | - | OBJ+0xba |
| 0x03a | byte | - | OBJ+0xbc |
| 0x03b | byte | - | P->Get8nsOeEnableInfo() |
| 0x03c | byte | - | ? |
| 0x03d | word | BE | P->GetCardScanLen() | OBJ+0x44 |
| 0x03f | byte | - | ? |
| 0x040 | byte | - | ? |
| 0x041 | byte | - | ?[add] |
| 0x046 | byte | - | OBJ+0x74 |
| 0x047 | byte | - | OBJ+0x76 |
| 0x048 | byte | - | OBJ+0x78 |
| 0x049 | byte | - | OBJ+0xd3c0 |
| 0x04a | byte | - | P->GetModuleInputCount() |
| 0x04b | byte | - | P->GetHubType() |
| 0x04c | byte | - | ? |
| 0x04d | byte | - | ? |
| 0x04e | byte | - | ? |
| 0x04f | byte | - | ? |
| 0x050 | byte | - | ?[add] |
| 0x051 | byte | - | ? |
| 0x074 | byte | - | ?[and/or] | ?[shr/and] | ?[and/and/or] |
| 0x075 | byte | - | P->GetScanMode() | ? |
| 0x076 | byte | - | ?[and/or] | ?[shl/and/or] | ?[and/or/shl/or] |
| 0x077 | byte | - | ?[and/or] | ?[and/shl/or] | ?[or/and/or] |
| 0x078 | byte | - | ?[and/or] | ?[or/and/or] | ? |
| 0x079 | byte | - | ?[and/or] | ?[shr/or] | ?[or/and] |
| 0x07a | byte | - | ?[and/or] | ?[or/and/or] | ? |
| 0x07b | byte | - | ?[or/and/or] | ?[or/shr/or] | ?[and/or] |
| 0x07c | byte | - | ?[and/or] | ?[and/shl/and/or] | ?[shl/add/and/or] |
| 0x07e | byte | - | ?[or/and/or] | ?[and/or] |
| 0x07f | byte | - | ?[shr/and/and/or] |
| 0x080 | byte | - | ?[and/or] | ?[shl/and/or] |
| 0x082 | byte | - | ?[and/and/or] |
| 0x088 | dword | - | ? |
| 0x08c | byte | - | ? |
| 0x08d | byte | - | ? |
| 0x08e | byte | - | ? |
| 0x08f | byte | - | ? |
| 0x090 | byte | - | CONST 1 |
| 0x091 | byte | - | OBJ+0xdf16[and] |
| 0x092 | word | - | CONST 0 | ? |
| 0x094 | byte | - | P->GetSpMoudleSetting() |
| 0x09f | byte | - | ?[or] | ?[or/shr] |
| 0x0a0 | byte | - | ? | ?[shr] |
| 0x0a1 | byte | - | ? | ?[shr] |
| 0x0a2 | byte | - | ? | ?[shr] |
| 0x0a9 | byte | - | P->GetColorSwap() |
| 0x0ac | byte | - | ?[add] |
| 0x0ad | byte | - | ?[add] |
| 0x0ae | byte | - | ?[shr] |
| 0x0af | byte | - | ?[shr] |
| 0x0d4 | byte | - | ?[and/and/or] |
| 0x0d6 | byte | - | ?[and/and/or] |
| 0x0d8 | byte | - | P->GetCurrentPercent()[or] |
| 0x0d9 | byte | - | P->UpdateXMSeriesChipDataByFrameRate() |
| 0x0db | byte | - | OBJ+0xbe |
| 0x0dc | byte | - | ?[and/and/or/or] |
| 0x0dd | byte | - | ?[and/shl/or] | ?[and/shl/add/or] |
| 0x0df | byte | - | ?[or/and/or] |
| 0x0e1 | byte | - | ?[and/or] |
| 0x0e3 | byte | - | P->GetSumChipCurrent() |
| 0x0e7 | word | BE | ? | ? |
| 0x0e8 | byte | - | ? |
| 0x0e9 | word | BE | ? |
| 0x0ed | byte | - | P->GetRealGrayLevel() |
| 0x0ee | byte | - | ? |
| 0x0ef | byte | - | ? |
| 0x0f0 | byte | - | ? |
| 0x0f1 | byte | - | ? |
| 0x0f2 | byte | - | ? |
| 0x0f3 | byte | - | ? |
| 0x0f8 | byte | - | ?[shr/and/or/or] |
| 0x0f9 | byte | - | OBJ+0xe61e |
| 0x0fa | byte | - | OBJ+0xe620 |
| 0x0fb | byte | - | OBJ+0xe61c[or/or] |
| 0x0fc | byte | - | P->GetChipOhmValB() |
| 0x0fd | byte | - | P->GetDeadPixelsCurrentGain() |
| 0x0fe | byte | - | CONST 0[or] |
| 0x100 | dword | - | ?[add] |

### 7.4 Named fields (highest confidence)

| payload | field |
|---|---|
| 0x04 | constant 0xA8 (pack sub-type / magic) |
| 0x05–0x07 | copied from OBJ+0x94..0x96 |
| 0x08–0x09 | module width / height (order set by `GetLineDir()`) |
| 0x0a | `GetModuleCountInLineDir()` |
| 0x0b | `GetRgbSelValue()` |
| 0x0c | `GetGrayLevel()`; forced to 0x10 if `IsNeed16BitGrayWhenSend()`, to 8 if `GetSplitSegment()==0x5c` |
| 0x0d–0x0e | **`GetScanMode()`**, BE |
| 0x0f–0x10 | **`GetOneScanLen()`**, BE |
| 0x11–0x12 | **`GetCardScanLen()`**, BE |
| 0x14 | `GetColorSwap()` (+add/or) |
| 0x15 | constant 0x99 / 0x77 / 0x00 depending on branch |
| 0x22 | packed bitfield: `shl dl,2` and `shl cl,5` OR-ed together (`0x1dfc8b`–`0x1dfc98`) |
| 0x23–0x24 | `GetVoidPointCount()`, BE |
| 0x26–0x28 | OBJ+0xb5 (masked 0x0f) / OBJ+0xb6 / OBJ+0xb7 |
| 0x38, 0x3a | OBJ+0xba, OBJ+0xbc |
| 0x3b | `Get8nsOeEnableInfo()` |
| 0x3d–0x3e | `GetCardScanLen()` / OBJ+0x44, BE |
| 0x46–0x48 | OBJ+0x74, OBJ+0x76, OBJ+0x78 |
| 0x4a | `GetModuleInputCount()` |
| 0x4b | `GetHubType()` |
| 0x74–0x82 | densely packed bitfields (masks 0x80/0xe0/0xc3/0xfc/0x3f seen) — chip timing/OE/GCLK group |
| 0x90 | constant 0x01 |
| 0x91 | OBJ+0xdf16 (masked) |
| 0x94 | `GetSpMoudleSetting()` (masked 0x3f) |
| 0xa9 | `GetColorSwap()` |
| 0xd8 | `GetCurrentPercent()` derived |
| 0xd9 | `UpdateXMSeriesChipDataByFrameRate()` derived |
| 0xe3 | `GetSumChipCurrent()` |
| 0xed | `GetRealGrayLevel()` |
| 0xfc / 0xfd | `GetChipOhmValB()` / `GetDeadPixelsCurrentGain()` |

Bytes not listed in the table are left **zero** by the initial `bzero` in the
caller (`bzero(pack+1, 0x103)` at `0x31f5e6`) — so a faithful implementation
zero-fills 260 bytes and only writes the listed offsets.

**Per-field confidence:** offsets + sizes + endianness = **high** (each is a
literal instruction). Accessor attribution = **high** where a `call
CHWParamRcvGeneral::X` immediately precedes the store, **medium** where the value
passed through several registers or a branch. The exact bitfield packing for
0x74–0x82 is **low/medium** — the masks are recorded but the sub-field semantics
were not fully decoded.

---

## 8. rcvbp → object mapping

### 8.1 File layout (CONFIRMED byte-for-byte)

`CHWParamReceiver::LoadFromFile` @ `0x1716a0` reads the whole file, then
`CHWParamReceiver::LoadFromBuffer(u8* buf, u32 len, u32 kind)` @ `0x170e50`
validates a **16-byte signature**. For `.rcvbp` the compare chain at `0x171142`
(and `0x17123a`) is:

```
dword [buf+0x00] == 0xBE192020        ; cmp edi, 0xbe192020
word  [buf+0x04] == 0x2374            ; cmp ecx, 0x2374
word  [buf+0x06] == 0x4543            ; cmp ecx, 0x4543
byte  [buf+0x08] == 0xB1
byte  [buf+0x09] == 0xC7
byte  [buf+0x0a] == 0x93
byte  [buf+0x0b] == 0x03
byte  [buf+0x0c] == 0x9B
byte  [buf+0x0d] == 0x83
byte  [buf+0x0e] == 0xAE
byte  [buf+0x0f] == 0xAB
```

Our file's first 16 bytes are `20 20 19 be 74 23 43 45 b1 c7 93 03 9b 83 ae ab`
— **every byte matches**. (Two other signatures exist in the same function:
`0x43A62D8B` and `0x213F3ACB`, for the other Colorlight param file kinds.)

Then `CRcvParamFileManager::LoadBpFromBuffer(u8*, u32, bool)` @ `0x1c48d0`:

```
LoadBpHeadFromBuffer(...)              @ 0x1c49f0   -> outLen, isCompressed
out = new u8[outLen]; bzero(out,outLen)
if (isCompressed) {
    memcpy(out, buf, 0x20)                       ; header copied verbatim
    destLen = dword [buf + 0x18]                 ; = 0x15BEE = 89070  (our file!)
    uncompress(out + 0x20, &destLen, buf + 0x20, srcLen)   ; sym._uncompress (zlib)
    data = out + 0x20;  len = destLen
} else {
    memcpy(out, buf, len); data = out + 0x14; len -= 0x14
}
LoadBpBufFromBuffer(data, len, flag)   @ 0x1c5020
```

So the header is:

| offset | size | meaning |
|---|---|---|
| 0x00–0x0f | 16 | signature (above) |
| 0x10 | u32 | kind/version — our file `04 00 00 00` = 4 |
| 0x14 | u32 | compressed size — our file `b1 24 00 00` = 0x24B1 = 9393 |
| 0x18 | u32 | **decompressed size** — our file `ee 5b 01 00` = 0x15BEE = 89070 (used as zlib `destLen`) |
| 0x1c | u32 | 0 |
| 0x20.. | | zlib stream |

This exactly reproduces what we did by hand, so
`scratchpad/rcvbp_raw.bin` **is** the buffer handed to `LoadBpBufFromBuffer`.

### 8.2 The blob is NOT a flat struct image — it is a TLV record stream (CONFIRMED)

`LoadBpBufFromBuffer` @ `0x1c5020` (19 822 bytes) sets up many local record
buffers each preceded by a 4-byte descriptor, e.g.

```
mov dword [rbp-0x1c48], 0x080A0904   ; bytes 04 09 0a 08 , then bzero 0x900
mov dword [rbp-0x4050], 0x0A0A2404   ; bytes 04 24 0a 0a , then bzero 0x2400
mov dword [rbp-0xd058], 0x960A9004   ; bytes 04 90 0a 96 , then bzero 0x9000
```

i.e. records of the form **`u16 length (little-endian, INCLUDING the 4-byte
header) | u8 marker | u8 record-id`**, payload follows.

Walking our blob with that rule consumes it **exactly**, ending on the final byte
(0x15BEE) with no slack — proof the format is right:

| blob offset | length | marker | id | notes |
|---|---|---|---|---|
| 0x000000 | 768 | 0x0a | **0x01** | **basic parameters** (see 8.3) |
| 0x000300 | 4100 | 0x0a | 0x8d | table |
| 0x001304 | 6148 | 0x0a | 0x91 | table |
| 0x002b08 | 18437 | 0x0a | 0xd8 | large table (calibration?) |
| 0x00730d | 6148 | 0x0a | 0x95 | table |
| 0x008b11 | 36869 | 0x0a | 0xda | largest (gamma/cali) |
| 0x011b16 | 2595 | 0x0a | 0x8e | table |
| 0x012539 | 12294 | 0x0a | 0x03 | routing table (`00 10 00 40 00 00 41 …`) |
| 0x01553f | 36 | 0x09 | 0x07 | |
| 0x015563 | 14 | 0x0a | 0x83 | `… ff ff ff 00` |
| 0x015571 | 14 | 0x0a | 0x89 | `… 80 80 80 00` |
| 0x01557f | 9 | 0x0a | 0x86 | |
| 0x015588 | 260 | 0x0a | 0x8a | |
| 0x01568c | 260 | 0x0a | 0x84 | |
| 0x015790 | 274 | 0x0a | 0xcd | |
| 0x0158a2 | 584 | 0x00 | 0x8f | marker 0x00 |
| 0x015aea | 260 | 0x0a | 0xca | ends exactly at 0x15bee |

### 8.3 Record 0x01 = the basic parameters

Payload (764 bytes) begins:

```
+0x000: 80 20 01 00 00 00 00 00 02 00 00 00 00 00 00 00
+0x010: 00 00 00 00 00 00 00 00 00 00 10 00 33 33 33 40
+0x020: 10 08 00 0e 01 00 bc 00 ff ff ff 03 02 01 00 00
+0x030: 32 14 2b 2b 2b 2b 4c 00 00 00 00 00 00 f2 00 00
```

* `+0x000 = 0x80 = 128` → **panel/module width**, matches our 128-wide panel.
* `+0x001 = 0x20 = 32` → **1/32 scan**, matches the file name `P2.5-32S-…`.
* `+0x01c = 33 33 33 40` = float `2.8f` (little-endian IEEE-754) — a scalar param.

**Confidence:** the record framing is *certain*; the identification of
`+0x000`/`+0x001` as width/scan is *high* (two independent known truths agree);
the rest of the record's field layout is **not yet decoded**.

### 8.4 What this means for the Rust port

There is **no constant delta** that maps blob bytes to `CHWParamRcvGeneral`
members — `LoadBpBufFromBuffer` dispatches each record into a different
sub-structure, and `GetBasicParam` then reads *derived* accessors (many compute
values rather than returning a stored byte). Two viable strategies:

1. **Port the subset** — implement only record 0x01's field extraction plus the
   ~60 `GetBasicParam` stores that come from `OBJ+0xNN` and simple accessors.
   Unknown-meaning bytes can simply be copied. This is the recommended path.
2. **Capture-and-diff** — obtain one capture of iSet/LEDVISION configuring this
   card, then use `e120 pcap-summary --dump` to read the real 272-byte type-0x05
   frame and diff it against a generated one. This resolves every UNCERTAIN
   bitfield at once and is far cheaper than decoding all 13 702 bytes of
   `GetBasicParam`.

---

## Appendix C — functions analyzed in phase 2

| Address | Symbol | Role |
|---|---|---|
| 0x1dfb50 | `CSendAndSaveRcvParam::GetBasicParam` | fills the 260-byte 0x05 body (§7) |
| 0x1716a0 | `CHWParamReceiver::LoadFromFile` | reads param file from disk |
| 0x170e50 | `CHWParamReceiver::LoadFromBuffer` | validates the 16-byte signature, dispatches by kind |
| 0x1c48d0 | `CRcvParamFileManager::LoadBpFromBuffer` | header parse + zlib `uncompress` from +0x20 |
| 0x1c49f0 | `CRcvParamFileManager::LoadBpHeadFromBuffer` | reads out-length / compressed flag |
| 0x1c5020 | `CRcvParamFileManager::LoadBpBufFromBuffer` | TLV record dispatcher (§8.2) |
| 0x44e350 | `vtable for CHWParamRcvGeneral` | resolved vt+0x580=`GetRgbSelValue`, vt+0x590=`GetScanMode`, vt+0x050=`GetSplitSegment`, vt+0x278=`GetCurrentPercent`, vt+0x288=`GetSumChipCurrent`, vt+0x2c0=`UpdateXMSeriesChipDataByFrameRate`, vt+0x3f8=`GetChipOhmValB`, vt+0x418=`GetDeadPixelsCurrentGain` |
| 0x13a310 / 0x138da0 | `GetLineDir` / `GetMaxScan` | member reads at P+0xd4c0 etc. |

### Appendix D — `CHWParamRcvGeneral` member offsets recovered from accessors

| member | accessor |
|---|---|
| +0x30 | `IsModified` |
| +0x38 | `GetRcvMaxWidth` / `GetRcvMaxHeight` / `IsSupportLargeLoad` |
| +0x68 | `GetMoudleWidth` / `GetOneScanLen` / `GetPixelRoutingStepCount` |
| +0x6a | `GetMoudleHeight` |
| +0x6e | `GetVoidPointCount` |
| +0x74 | `GetDefaultModulePos` |
| +0x7a | `GetAXS6018Param` |
| +0xb6 | `GetSplitSegment` / `GetSplitStyle` |
| +0xb9 | `GetOutputCount` / `GetRealOutPutCount` |
| +0xbb | `GetOutPutModel` |
| +0xbd | `Get8nsOeEnableInfo` / `IsHasSM5266` |
| +0xc4 | `IsHasColorExchange` |
| +0xd0 | `GetColorSwap` |
| +0xd3c4 | `GetDefaultGray` / `GetSupporttedGray` |
| +0xd3c6 | `GetSerialType` |
| +0xd4c0 | `GetLineDir` / `GetModuleInputCount` / `GetModuleCountInLineDir` |
| +0xd4c4 | `GetHubType` |
| +0xdf08 / +0xdf0a | `GetMaxWidth` / `GetMaxHeight` |
| +0xdf16 | `GetLS9736ICNum` / `IsHasMBI5988` |
| +0xe050 | `GetGamaTable` |
| +0xe11a | `IsEnableGammaCalibration` |


---

## 9. record → object mapping

Everything below is from `CRcvParamFileManager::LoadBpBufFromBuffer` @ `0x1c5020`
(19 822 bytes). **No capture was used; this is pure static analysis.**

### 9.1 The record dispatcher (CONFIRMED)

The record walk and dispatch at `0x1c5b8e`–`0x1c5bcf`:

```
ecx = dword [r12]          ; the 4-byte record header
ebx = cx                   ; record length  (u16 LE, includes header)  <- confirms the TLV rule
eax = ecx >> 0x18          ; record ID = header byte 3
al  = id + 0x7f
if (al > 0x8f) -> default (skip record)
jump  [0x1c9eb8 + al*4]    ; 144-entry rel32 jump table
```

So **`index = (record_id + 0x7F) & 0xFF`**, valid for index ≤ 0x8F. Resolved
handlers for the records present in our file:

| record id | index | handler | destination local | cap |
|---|---|---|---|---|
| **0x01** | 0x80 | `0x1c5f07` | `rbp-0x330` (`SRcvParamBasic`) | 0x300 |
| 0x03 | 0x82 | `0x1c654c` | | |
| 0x07 | 0x86 | `0x1c63f2` | | |
| 0x83 | 0x02 | `0x1c606d` | | |
| **0x84** | 0x03 | `0x1c60f8` | `rbp-0x11df8` | 0x104 |
| 0x86 | 0x05 | `0x1c64b9` | | |
| 0x89 | 0x08 | `0x1c6088` | | |
| 0x8a | 0x09 | `0x1c5f5e` | | |
| 0x8d | 0x0c | `0x1c62ef` | | |
| 0x8e | 0x0d | `0x1c652f` | | |
| 0x8f | 0x0e | `0x1c64d4` | | |
| 0x91 | 0x10 | `0x1c5d77` | | |
| 0x95 | 0x14 | `0x1c5f41` | (cap 0x1804) | |
| 0xca | 0x49 | `0x1c65be` | | |
| 0xcd | 0x4c | `0x1c65a1` | | |
| 0xd8 | 0x57 | `0x1c5bd1` | (skipped — just advances) | |
| 0xda | 0x59 | `0x1c6512` | | |

Every handler has the identical shape (record 0x01's, verbatim):

```
cmp  ebx, 0x300              ; cap = destination struct size
mov  eax, 0x300
cmovb eax, ecx               ; n = min(record_len, cap)
movzx edx, ax
lea  rdi, [rbp - 0x330]      ; destination local struct
mov  rsi, r12                ; source = record START (header included!)
call memcpy
or   [presence_mask], 1      ; mark record seen
```

**Consequence:** the record is copied **including its 4-byte header**, so

> **struct byte `k` = record byte `k`, and record payload byte `N` = struct byte `N+4`.**

### 9.2 Record 0x01 → `CHWParamRcvGeneral` members (CONFIRMED)

`SRcvParamBasic` (the `rbp-0x330` local, constructed at `0x1c50ad`) is then applied
to the object in a long inline block. It is **NOT a single flat copy** — it is a
mix of direct copies, 16-byte `movups` blocks, bit-unpacking, and setter calls.

The two largest direct copies (`0x1c5916`, `0x1c5a04`):

```
mov   rcx, qword  [rbp-0x32c]      ; payload+0x00 .. +0x07
mov   qword [OBJ + 0x60], rcx      ; -> OBJ+0x60  (8 bytes verbatim)

movups xmm0, xmmword [rbp-0x324]   ; payload+0x08 .. +0x17
movups xmmword [OBJ + 0x50], xmm0  ; -> OBJ+0x50  (16 bytes verbatim)
```

**The coordinator's near-flat hypothesis is explicitly REFUTED for OBJ+0x68.**
`OBJ+0x68` (`GetMoudleWidth`) and `OBJ+0x70` are written at `0x1c59d9`/`0x1c59dd`
from a *different* stack buffer (`rbp-0x37c24` / `rbp-0x37c1c`) — i.e. from a
**different record**, not record 0x01. That is why record-0x01 payload+0x08
(`0x02`) is not 128. So there is **no single delta**; the module geometry the
0x05 pack reads at `OBJ+0x68/0x6a` comes from another record (most likely the
module/cabinet record — id 0x8d/0x91/0x95, **UNCERTAIN which**).

There is also a conditional byte permutation of `OBJ+0x60` (`0x1c5935`–`0x1c5985`,
gated on flag bit `0x1000`: `shr 0x28` / `shl 0x18` / masks `0xff0000`,
`0xff000000`), and a mask `OBJ+0x68 &= 0x0000FFFFFFFFFFFF` when flag bit
`0x2000` is set. **Flag source UNCERTAIN.**

Verified direct record-0x01 → object copies:

| object member | ← record 0x01 payload | size |
|---|---|---|
| OBJ+0x000a | payload+0x26e | 1B |
| OBJ+0x000b | payload+0x26f | 1B |
| OBJ+0x0050 | payload+0x008 | 16B |
| OBJ+0x0060 | payload+0x000 | 8B |
| OBJ+0x0074 | payload+0x24e | 2B |
| OBJ+0x0076 | payload+0x24f | 2B |
| OBJ+0x007c | payload+0x049 | 2B |
| OBJ+0x007e | payload+0x04b | 2B |
| OBJ+0x0082 | payload+0x024 | 2B |
| OBJ+0x0094 | payload+0x028 | 1B |
| OBJ+0x0095 | payload+0x029 | 1B |
| OBJ+0x0096 | payload+0x02a | 1B |
| OBJ+0x00ac | payload+0x0e4 | 4B |
| OBJ+0x00b0 | payload+0x176 | 4B |
| OBJ+0x00b5 | payload+0x03d | 1B |
| OBJ+0x00b6 | payload+0x03e | 1B |
| OBJ+0x00b7 | payload+0x03e | 1B |
| OBJ+0x00b8 | payload+0x043 | 1B |
| OBJ+0x00b9 | payload+0x044 | 1B |
| OBJ+0x00bb | payload+0x04e | 1B |
| OBJ+0x00bc | payload+0x04f | 1B |
| OBJ+0x00bd | payload+0x050 | 1B |
| OBJ+0x00be | payload+0x191 | 1B |
| OBJ+0x00bf | payload+0x018 | 1B |
| OBJ+0x00c2 | payload+0x0e5 | 1B |
| OBJ+0x00d4 | payload+0x02f | 4B |
| OBJ+0x00dc | payload+0x052 | 4B |
| OBJ+0xc0e0 | payload+0x018 | 1B |
| OBJ+0xc369 | payload+0x045 | 2B |
| OBJ+0xd3c1 | payload+0x030 | 1B |
| OBJ+0xd3c6 | payload+0x037 | 1B |
| OBJ+0xd3c8 | payload+0x038 | 2B |
| OBJ+0xd3ca | payload+0x03a | 2B |
| OBJ+0xd3cc | payload+0x05a | 16B |
| OBJ+0xd3dc | payload+0x07a | 16B |
| OBJ+0xd3ec | payload+0x09a | 16B |
| OBJ+0xd3fc | payload+0x09a | 16B |
| OBJ+0xd41c | payload+0x1ca | 16B |
| OBJ+0xd42c | payload+0x1ca | 16B |
| OBJ+0xd43c | payload+0x1ca | 16B |
| OBJ+0xd44c | payload+0x164 | 16B |
| OBJ+0xd45c | payload+0x144 | 16B |
| OBJ+0xd46c | payload+0x144 | 16B |
| OBJ+0xd47c | payload+0x144 | 16B |
| OBJ+0xd48c | payload+0x154 | 16B |
| OBJ+0xd49c | payload+0x164 | 16B |
| OBJ+0xd4be | payload+0x041 | 2B |
| OBJ+0xd668 | payload+0x236 | 16B |
| OBJ+0xd678 | payload+0x236 | 16B |
| OBJ+0xd688 | payload+0x236 | 16B |
| OBJ+0xdf04 | payload+0x059 | 4B |
| OBJ+0xdf13 | payload+0x1db | 1B |
| OBJ+0xdf16 | payload+0x0e8 | 1B |
| OBJ+0xdf6d | payload+0x175 | 8B |
| OBJ+0xdf73 | payload+0x175 | 8B |
| OBJ+0xdf7b | payload+0x24b | 1B |
| OBJ+0xdf8c | payload+0x0c2 | 1B |
| OBJ+0xdfc4 | payload+0x112 | 4B |
| OBJ+0xe070 | payload+0x17a | 1B |
| OBJ+0xe088 | payload+0x1fc | 4B |
| OBJ+0xe08c | payload+0x200 | 2B |
| OBJ+0xe08e | payload+0x1dc | 1B |
| OBJ+0xe0a8 | payload+0x194 | 4B |
| OBJ+0xe0ac | payload+0x198 | 1B |
| OBJ+0xe0c4 | payload+0x190 | 1B |
| OBJ+0xe0c8 | payload+0x202 | 1B |
| OBJ+0xe13c | payload+0x1ed | 1B |
| OBJ+0xe13d | payload+0x192 | 1B |
| OBJ+0xe142 | payload+0x199 | 1B |
| OBJ+0xe61b | payload+0x1f6 | 1B |
| OBJ+0xe61c | payload+0x1f7 | 1B |
| OBJ+0xe61e | payload+0x1ee | 2B |
| OBJ+0xe620 | payload+0x1f0 | 2B |
| OBJ+0xe6a0 | payload+0x249 | 4B |
| OBJ+0xe6e8 | payload+0x25a | 1B |
| OBJ+0xe6e9 | payload+0x17a | 1B |
| OBJ+0xe6eb | payload+0x269 | 1B |

Bit-unpacking (`0x1c5a0b`–`0x1c5a2b`): the **dword at payload+0x018** is split
into individual boolean members — `OBJ+0xbf = bit0`, `OBJ+0xc0 = bit1`,
`OBJ+0xc2xx = bit3`, … (`and cl,1` / `shr cl,N`). `OBJ+0xc0e0` also comes from
this dword.

Setter-mediated fields (record payload → `CHWParamRcvGeneral::Set*`), from the
same block:

| record 0x01 payload | setter |
|---|---|
| +0x020 | `SetScanMode(u8)` |
| +0x021 | `SetSerialClockFrequency(u16)` |
| +0x023 | `IsGrayLevelSupportted` check |
| +0x036 | `CreateConfigSC6618Lib` / `CreateConfigSC6618MultiLib` / `CreateAXS6018Lib` (chip library selection) |
| +0x03a, +0x03c, +0x041 | `SetLineDir` |
| +0x03e, +0x043 | `SetScanMode` |
| +0x058 | `SetHubType` |
| +0x059, +0x0c0 | `SetMaxWidth` |
| +0x0c2 | `SetMaxHeight` |
| +0x018 | `CalGamaTable()` |

### 9.3 The loop is now closed

Chaining §9.2 with the §7 table gives file bytes → 0x05 pack bytes with no gap:

| record 0x01 payload | → object | → 0x05 pack payload |
|---|---|---|
| +0x028..+0x02a | OBJ+0x94..0x96 | **[0x05], [0x06], [0x07]** |
| +0x018 bit0 | OBJ+0xbf | **[0x17]** |
| +0x018 bit1 | OBJ+0xc0 | **[0x16]** |
| +0x03d | OBJ+0xb5 | **[0x26]** (masked 0x0f) |
| +0x03e | OBJ+0xb6 | **[0x27]** |
| +0x04f | OBJ+0xbc | **[0x3a]** |
| +0x050 | OBJ+0xbd | `Get8nsOeEnableInfo` → **[0x3b]** |
| +0x24e | OBJ+0x74 | **[0x46]** |
| +0x24f | OBJ+0x76 | **[0x47]** |
| +0x045 | OBJ+0xc369 | **[0x2c]** |
| +0x030 | OBJ+0xd3c1 | near **[0x49]/[0x4a]** (OBJ+0xd3c0) |

**Confidence:** dispatcher and memcpy semantics = *certain*; the direct-copy table
= *high* (each row is one instruction pair); setter attribution = *medium* (call
follows the load within a few instructions, but branches exist).

---

## 10. Chip-register pack

### 10.1 Correction to §2.1 / §7 — `payload[3]` is a pack sub-index, not the receiver number

At `0x31f2ce`–`0x31f31d` the chip pack is built as:

```
mov byte [rbp-0xc390], 5        ; payload[0] = 0x05   <- SAME type as the basic pack
mov word [rbp-0xc38f], 0        ; payload[1..2] = 0
mov byte [rbp-0xc38d], 1        ; payload[3] = 1
call GetChipCustomPlusParamPack
CSendControl(buf, 0x104)        ; 260-byte payload -> 272-byte frame
```

and in `GetParamPacksBasicEx` successive packs set `payload[3] = 1`, then `2`.
So within type 0x05, **`payload[3]` distinguishes which parameter pack this is**;
`payload[4] = 0xA8` (written only by `GetBasicParam`) further marks the basic
pack. My earlier reading of `payload[3]` as the receiver index in §2.1/§7.2 was
**wrong** — please use this section instead.

### 10.2 Layout of the chip-register pack (CONFIRMED, `GetChipCustomPlusParamPack` @ `0x1ea2b0`)

```
call  [vtable + 0x130]                 ; = CHWParamRcvGeneral::GetChipCustomEX()
                                       ;   fills a 0x120-byte local
memcpy(pack + 4, local, 0xB4)          ; payload[0x04 .. 0xB7] = 180 bytes chip config

rax = OBJ[0xd6d0]                      ; chip-register data object
movups pack+0xB8  <- rax[0x00..0x0F]
movups pack+0xC8  <- rax[0x10..0x1F]
movups pack+0xD8  <- rax[0x20..0x2F]
movups pack+0xE8  <- rax[0x30..0x3F]
movups pack+0xF4  <- rax[0x3C..0x4B]
call ExchangeChipRegisterWhenColorChanged(pack)   ; permutes registers per colour swap
```

| pack payload | content |
|---|---|
| 0x00 | 0x05 (type) |
| 0x01–0x02 | 0 |
| 0x03 | 1 (pack sub-index) |
| 0x04–0xB7 | 180 B from `GetChipCustomEX()` |
| 0xB8–0xF3 | 60 B chip registers from `OBJ+0xd6d0` |
| 0xF4–0x103 | 16 B (overlapping tail, `rax+0x3c`) |

### 10.3 Where the SM16269S register values live in the file

Record **id 0x84** (260 B, blob `0x01568c`) is a table of **4-byte entries
`(register_index, R, G, B)`**, 64 entries:

```
02 0f 0f 0f | 03 3f 3f 3f | 04 00 00 00 | 05 00 00 00
06 00 00 00 | 07 04 04 04 | 08 00 00 00 | 09 00 00 00
0a 00 00 00 | 0b 2c 2c 2c | 0c 00 01 03 | 0d 00 00 00
0e 05 05 05 | 0f 00 00 00 | 10 00 00 00 | 11 04 1e 50
```

The first byte of each quad increments monotonically (0x02, 0x03, 0x04 …), and
the following three are per-colour values — consistent with driver-chip register
index + RGB values. Its handler `0x1c60f8` memcpys it to `rbp-0x11df8` (cap
0x104). **The identification as SM16269S registers is inference from the shape
and from the filename, not from a decoded chip table — mark MEDIUM confidence.**
The exact route from that local into `OBJ+0xd6d0` was **not** traced.

---

## 11. Verdict: minimum frames for first light

**Mandatory, in order, per refreshed frame:**

1. **Type 0x05, `payload[3]=1`** — chip-register / chip-custom pack (§10). Without
   it the SM16269S drivers are never initialised and the panel stays dark
   regardless of scan config.
2. **Type 0x05, `payload[4]=0xA8`** — basic parameter pack (§7): scan mode, scan
   lengths, gray level, hub type, line dir.
3. Type **0x0A** brightness, then **0x55** row packets, then **0x01** sync — the
   part we already implement and which is already verified on the wire.

Packs 1 and 2 are sent once (real-time, no flash write needed to test); 3 repeats
per frame. Flash/EEPROM writes (§3) are **not** needed for first light — only for
persistence across power cycles.

**What remains genuinely unknowable from static analysis alone:**

* The ~40 bitfields at 0x74–0x82 and 0x9f–0x102 of the basic pack: the masks are
  recovered but their inputs pass through `GetChipCustomEX()` and chip-library
  constructors (`CreateConfigSC6618Lib`, `CreateAXS6018Lib`) that build tables
  from the `ChipData/LS/pm_*.dat` files shipped with LEDVISION. Those .dat files
  are *data we have on disk* — decoding them is a separate, tractable task, and
  is the honest next step instead of a capture.
* Which record supplies `OBJ+0x68/0x70` (module geometry) — needs the handlers for
  ids 0x8d/0x91/0x95 to be traced.
* The flag bits (0x1000 / 0x2000) that select the `OBJ+0x60` byte permutation.

**Practical recommendation:** because `GetChipCustomEX()` and the 180-byte block
dominate pack 1, and because our card already self-reports the correct 128×64
geometry, the highest-value next step is decoding `ChipData/LS/pm_*.dat` for the
SM16269/16269S profile — that is what feeds the still-unknown bitfields, and it
is fully static.

### Appendix E — phase-3 functions

| Address | Symbol | Role |
|---|---|---|
| 0x1c5020 | `CRcvParamFileManager::LoadBpBufFromBuffer` | TLV walk + 144-way jump table @ `0x1c9eb8` |
| 0x1c5f07 | case handler, record 0x01 | memcpy → `SRcvParamBasic` (`rbp-0x330`) |
| 0x1c60f8 | case handler, record 0x84 | memcpy → `rbp-0x11df8` (chip registers) |
| 0x1cfcc0 | `SRcvParamBasic::SRcvParamBasic` | ctor of the 0x300-byte basic-param struct |
| 0x1ea2b0 | `CSendAndSaveRcvParam::GetChipCustomPlusParamPack` | builds the 260-byte chip pack (§10.2) |
| 0x16dc80 | `CHWParamRcvGeneral::GetChipCustomEX` | vt+0x130; supplies the 180-byte chip config |
| 0x1ea370 | `CSendAndSaveRcvParam::ExchangeChipRegisterWhenColorChanged` | colour-swap permutation of chip registers |
| 0x162270 | `CHWParamRcvGeneral::ExchangeThirdRegWhenLoadAndSaveFile` | consumes `SRcvParamBasic` |
| 0x167c50 | `CHWParamRcvGeneral::MakeCurrentOfDeadPixcelValid` | consumes `SRcvParamBasic` |


---

## 12. Readback of receiver parameters (read-only)

> Numbering note: the coordinator asked for "## 11"; §11 was already taken by the
> first-light verdict, so this is §12.

Everything here is static analysis of `libCLTDevice.1.dylib`. **No vendor binary
was executed and no frame was transmitted.**

### 12.1 Call chain (CONFIRMED)

```
CReceiverOP::ReadbackRcvBasicParam(u32 devId, int port, int rcvIdx, SRcvParamInfo&)   @ 0x3c2230
  └─ virtual [vtable+0x620]  ==  CReceiverOP::ReadFlashToBuffer(...)                  @ 0x3c68e0
       └─ BuildRcvCardFlashOperation(&outLen,&outBuf, rcvIdx, 0x44, hi, lo, 1, NULL, 0) @ 0x30b790
       └─ virtual [deviceIO + 0x48]   (send + collect reply)
```

The argument list at `0x3c2272`–`0x3c228b` is literal:

```
esi = devId          (caller-supplied)
edx = port           (caller-supplied)
ecx = rcvIdx         (caller-supplied)
r8d = 7              <- address HIGH byte
r9d = 0x80           <- address LOW byte
push 0x400           <- length: 1024 bytes
push [r14]           <- destination buffer
push 0x1f4           <- timeout 500 ms
```

and the second call at `0x3c22ee` is identical except `r9d = 0x84` and the
destination advanced by `0x400` — used only when more than 0x3FD bytes are
requested.

### 12.2 Address encoding (CONFIRMED)

Inside `ReadFlashToBuffer` at `0x3c6962`:

```
r15d = (arg4 << 8) | arg5      ; = (7 << 8) | 0x80 = 0x0780
...per 1024-byte chunk:
r8d = r15w >> 8                ; address high byte  -> payload[7]
r9d = r15w & 0xff              ; address low  byte  -> payload[8]
...
add r15d, 4                    ; advance 4 units per 1024 bytes
add rbx, 0x400
```

`+4 units == +1024 bytes`, so **the address unit is a 256-byte page** and the
basic-parameter region begins at **page 0x0780** (byte address 0x78000).
`usleep(1000)` runs before each chunk.

### 12.3 Frame layout produced by `BuildRcvCardFlashOperation` (CONFIRMED)

Body at `0x30b81d`–`0x30b858`, with the type byte computed at `0x30b8b3`:

```
n = max(0x80, datalen + 0xa)          ; datalen = 0  ->  n = 0x80 = 128
buf = new[n]; bzero(buf, n)

; ---- type byte selection ----
; opcodes {0x30,0x31,0x32,0x40,0x41,0x42,0x50,0x52} take the direct path: type = 0x26
; every other opcode falls to 0x30b8b3:
;     cl = ((addrHi < 3) && (opcode == 0x44)) << 5 | 6
;     type = (addrHi < 8) ? cl : 0x26
; our case: opcode 0x44, addrHi 7  ->  (7<3)=false  ->  cl = 0x06 ; (7<8)=true -> type = 0x06

payload[0] = type                     ; 0x06
payload[1] = 0
payload[2] = 0
payload[3] = rcvIdx >> 8              ; big-endian u16, same slot as discovery
payload[4] = rcvIdx & 0xff
payload[5] = opcode                   ; 0x44
payload[6] = flag                     ; 1
payload[7] = addrHi                   ; 0x07
payload[8] = addrLo                   ; 0x80
payload[9] = 0
; memcpy(payload+0xa, dataptr, datalen) — SKIPPED when dataptr == NULL
outLen = 0x80
```

### 12.4 The concrete request frame

Reading the first 1024 bytes of the basic-parameter region. Total on the wire =
12 MAC bytes + 128 payload bytes = **140 bytes**.

```
 offset  bytes
 0x00    11 22 33 44 55 66      destination MAC (card)
 0x06    22 22 33 44 55 66      source MAC (sender)
 0x0c    06 00                  type 0x0600            <- payload[0..1]
 0x0e    00                     payload[2]
 0x0f    00                     payload[3] = rcvIdx MSB   ** see note **
 0x10    01                     payload[4] = rcvIdx LSB   ** see note **
 0x11    44                     payload[5] = opcode 0x44 (read)
 0x12    01                     payload[6] = flag 1
 0x13    07                     payload[7] = address high (page 0x07xx)
 0x14    80                     payload[8] = address low  (page 0x0780)
 0x15    00                     payload[9]
 0x16..  00 x 118               payload[0x0a..0x7f], all zero
```

Second chunk (bytes 0x400–0x7FF of the region): identical, except
`payload[8] = 0x84`.

**Receiver index — UNKNOWN whether 0- or 1-based.** `ReadbackRcvBasicParam` takes
it from its caller (UI-driven), so no constant is available statically. The frame
above uses `0x0001`; if it draws no reply, try `0x0000`. This is the only field I
would expect to need trial and error, and getting it wrong yields no reply
rather than any write.

### 12.5 Response (PARTIAL — flagged)

From `0x3c69de`–`0x3c6a83`:

```
recvBuf   = rbp-0x1030,  size 0x1000 (4096)
timeout   = 500 ms
edx       = 0xff09        ; passed to the send/collect call — reply selector/filter (meaning UNCERTAIN)
on success:
    memcpy(dst + chunkOffset, rbp-0x1021, min(remaining, 0x400))
```

`rbp-0x1021` is `recvBuf + 0x0F`, so **the returned data begins 15 bytes into the
reassembled receive buffer**, and 1024 bytes are taken per request.

**UNCERTAIN, and important:** whether those 15 bytes are an Ethernet header
remnant or a protocol header of the reassembled payload could not be settled —
the reassembly happens behind the virtual call `[deviceIO + 0x48]`, which I did
not trace. The reply's type byte is likewise unconfirmed; `0xff09` is the only
related constant. In practice you can resolve this by dumping whatever arrives
with `e120 listen` after sending the request — that is itself read-only.

### 12.6 Does the readback body match the 0x05 send-pack body?

**I could not confirm this, and I want to be explicit because the coordinator
identified this correspondence as what makes the approach work.**

Evidence against assuming they are identical:

* The readback is a **raw flash region read** (page 0x0780, 1024-byte chunks),
  not a rendered pack. Its natural layout is the card's flash format.
* The sibling entry point `CReceiverOP::ReadbackRcvBasicParam(..., SRcvFileBasicParam*)`
  @ `0x3b54e0` fills a **`SRcvFileBasicParam`** — a *file*-shaped struct — and
  takes a different path (allocating 0x27000). That naming suggests flash layout
  tracks the **file** representation (`SRcvParamBasic`, §9) rather than the 0x05
  wire pack.
* Nothing in `ReadbackRcvBasicParam` references `GetBasicParam` or the 0xA8 marker.

Where the 260-byte body sits inside the 1024-byte block is therefore **not
established**. The read is still worth doing — it is safe and it yields real data
to align against §7 and §9 — but treat "readback body == 0x05 body" as a
hypothesis to test, not a fact to build on.

### 12.7 Safety verdict: READ-ONLY — no erase, no write

Confirmed by direct comparison of call sites of `BuildRcvCardFlashOperation`
(93 sites enumerated; representative ones disassembled):

| call site | opcode (`ecx`) | pushes (flag, dataptr, datalen) | nature |
|---|---|---|---|
| `0x3c69cc` (`ReadFlashToBuffer`) | **0x44** | `1, NULL, 0` | **read — no payload** |
| `0x3c67a1` | 0xed | `0, NULL, 0` | read-like |
| `0x32dd6b` (gamma writer) | **0x85** | `0, buffer, 0x100` | **write — 256-byte payload** |
| `0x3b9206` (EEPROM writer) | **0x66** | `0, buffer, 1` | **write — payload** |

Three independent reasons the §12.4 frame cannot write:

1. **`dataptr = NULL`, `datalen = 0`.** The builder's `memcpy(payload+0xa, ...)`
   is guarded by `test rsi,rsi; je` at `0x30b882` — with NULL it is skipped. The
   frame carries **zero data bytes**; payload[0x0a..0x7f] are the `bzero` output.
   A write command with nothing to write cannot deliver content.
2. **Opcode 0x44 is used only by `ReadFlashToBuffer`**, whose entire post-send
   logic is `memcpy(dst, recvBuf+0x0F, 0x400)` — consuming returned data. Write
   paths use distinct opcodes (0x85, 0x66) *and* supply real buffers.
3. **Erase has a separate builder** — `BuildRcvStorageErase` @ `0x30bad0` — which
   is not on this path at all.

**The arguments that must not be altered**, in order of risk:

* **`payload[5]` (opcode) — the critical byte.** Keep it `0x44`. Changing it to
  `0x85` or `0x66` selects a write opcode. Note also that `0x30/0x31/0x32/0x40/
  0x41/0x42/0x50/0x52` change the type byte to `0x26` and take the other branch —
  do not substitute opcodes experimentally.
* **`payload[6]` (flag = 1).** Its semantics are **not** decoded. The read path
  passes 1 and both observed write paths pass 0, so 1 is the value that co-occurs
  with reads — but do not assume "1 == read"; leave it at 1.
* **payload[0x0a] onward must stay all zero.** These are where a write's data
  would land.
* `payload[7]/[8]` (address) select *which* region is read; a wrong value reads
  the wrong place, which is harmless.

My verdict: sending the §12.4 frame is **safe** — it is a zero-payload read
request. The residual uncertainty is `payload[6]`'s meaning; if you want zero
residual risk, that is the one byte worth further static work before transmitting.

---

# Verified on hardware: reading the card's stored configuration

Confirmed against a real E120 (firmware 10.81) on 2026-08-31.

Sending the read frame (type `0x0600`, opcode `0x44`, page `0x0780`) with
**receiver index 0** — index 1 gets no answer — makes the card reply with
frames of type `0x0901`, 1070 bytes each:

```
[14 bytes Ethernet][1 byte status = 0x01][1024 bytes flash data][zero padding]
```

Successive 1024-byte chunks come from pages advancing by 4. Concatenated, the
flash region holds:

```
[u32 little-endian total length, counting itself][a complete .rcvbp file]
```

For this card the length was 9112, giving a 9108-byte file: a 32-byte header
plus a 9076-byte zlib stream that inflated to 33 504 bytes and parsed as 14
records. **The card stores its configuration in the same `.rcvbp` container
format as the files shipped with panels**, which means configuring it may not
require re-serializing into typed packs at all.

## Root cause of the dark panel

Diffing the card's stored configuration against the panel's own `.rcvbp`:

* The card's copy is **missing record `0x84` entirely** — the driver-chip
  register table.
* In record `0x01`, the whole region `+0x269`–`+0x282` is zero on the card but
  populated in the panel's file.

The panel uses an SM16269S, a PWM driver IC that emits nothing until its
registers are programmed. The card is configured for a panel with plain
shift-register drivers, which need no such initialisation. Geometry matches
(128x64, 1/32 scan), which is why the card self-reports the correct size while
the panel stays dark and draws no current.

---

## 13. Writing configuration

Static analysis only. Nothing was executed; no write frame has been sent.

### 13.1 Correction to §12.3 (matters for the safety argument)

The data-attachment guard in `BuildRcvCardFlashOperation` is **opcode-driven**,
not merely pointer-driven. At `0x30b85c`–`0x30b87c`:

```
cl = (op != 0x85) && (op != 0x32) && (op != 0x77) && (op != 0x42) && (op != 0x52)
al = (op != 0x66)
test al, cl ; jne skip_memcpy          ; skip when op is NOT one of the data ops
if (dataptr != NULL) memcpy(payload + 0xa, dataptr, datalen)
```

So a payload is attached **only** for opcodes `{0x85, 0x77, 0x66, 0x52, 0x42, 0x32}`.
Opcode `0x44` (read) is structurally incapable of carrying data — the read frame
we already sent is even safer than §12 claimed.

### 13.2 Address encoding: `addrHi` is a 64 KB block selector

Byte address = `((addrHi << 8) | addrLo) * 256` = `addrHi * 65536 + addrLo * 256`.
So **`addrHi` selects a 64 KB block and `addrLo` a 256-byte page inside it** —
exactly SPI-flash block/page granularity. Confirmed by the write loop
(`0x332e81`): `r14 = pageLow << 8`, source `= dataBuf + r14`, so **image offset
`N*0x100` ↔ flash page `(hi, N)`**.

### 13.3 Region map (each parameter class has its own 64 KB block)

Recovered by resolving every `BuildRcvCardFlashOperation` call site to its
enclosing function and its literal `r8d` (addrHi):

| addrHi | region | written by |
|---|---|---|
| **0x07** | **basic parameters (.rcvbp container)** | `DoWriteToRcvForSeparate` |
| 0x0a | HDR gamma table, ROE multi-bright | `DoWriteHDRGammaTable` |
| 0x0b | module mapping table | `DoWriteModuleMappingTable` |
| 0x1c | basic-param overflow chunk **Two** | `DoWriteBasicParamExTwo` |
| 0x1e | factory bright/current param | `SaveFactoryBrightCurrentParam` |
| 0x1f | driver-chip params (SC6660/SC6618/XM11202G/ICND2260/ICND3065) | `DoWriteSC*Param` |
| 0x39 | route table Ex | `DoWriteRouteTableEx` |
| 0x3a | gamma calibration table | `DoWriteGammaCaliTable` |
| 0x3b | data remapping | `DoWriteDataRemappingParam` |
| 0x3c | gamma cali new-delta | `DoWriteGammaCaliNewDeltaTable` |
| 0xd6 | basic-param overflow chunk **One** | `DoWriteBasicParamExOne` |
| 0xd7 | HLG interpolation | `DoWriteHLGInterpolationTable` |
| 0xe0 | HLG 12-bit gamma | `DoWriteHLG12BitGammaTable` |
| 0xe2 / 0xe3 | multi-bright basic / gamma | `SaveMultiBright*Param` |
| 0xe5 | XYZ 12-bit gamma | `DoWriteXYZ12BitGammaTable` |
| 0xe7 | anti-pixel sequence | `DoWriteAntiPixelSequenceParam` |
| 0xe8 | shutter sync | `DoWriteShutterSyncParam` |
| 0xe9 | multi-module param | `DoWriteMultiModuleParam` |

**Firmware / FPGA is NOT in this table.** `DoSlowUpgradeRcv` /
`DoQuickUpgradeRcv` use a *different builder* — `BuildRcvCardFlashOperationEx`
@ `0x30b8e0` — with register-loaded (non-constant) opcode and address. That is a
useful structural separation: **the firmware path never goes through
`BuildRcvCardFlashOperation`**, so a guard that permits only that builder, only
opcodes {0x23, 0x44, 0x85}, and only `addrHi == 0x07` cannot reach the firmware
path at all.

### 13.4 Opcodes

| opcode | meaning | payload | flag (`payload[6]`) |
|---|---|---|---|
| **0x44** | read | none (structurally) | 1 |
| **0x23** | **erase** | none | 0 |
| **0x85** | write | 256 bytes | 0 |

### 13.5 The save sequence (`DoWriteToRcvForSeparate` @ `0x330220`)

Receiver indices come from a `std::vector<unsigned short>` at
`CBasicParamSendAndWriter+0x30/+0x38`, terminated in the caller by `0xFFFF`.

**Step 1 — erase**, once per receiver, via `CReceiverOP::ClearFlashSector` @ `0x3c3020`:

```
BuildRcvCardFlashOperation(&len,&buf, rcvIdx, opcode=0x23, hi=0x07, lo=0x00,
                           flag=0, dataptr=NULL, datalen=0)
usleep(5000)                     ; 0x1388 µs
```

Frame — 12 MAC bytes + 128 payload bytes = **140 bytes**:

```
11 22 33 44 55 66  22 22 33 44 55 66  06 00
00                       payload[2]
<idxMSB> <idxLSB>        payload[3..4]
23                       payload[5]  opcode = ERASE
00                       payload[6]  flag
07                       payload[7]  block 0x07
00                       payload[8]  page 0x00
00                       payload[9]
00 x 118                 payload[0x0a..0x7f]
```

**Step 2 — write**, one frame per 256-byte page (`0x332ede`–`0x332f13`):

```
BuildRcvCardFlashOperation(&len,&buf, rcvIdx, opcode=0x85, hi=0x07, lo=page,
                           flag=0, dataptr = image + page*0x100, datalen=0x100)
usleep(5000)
```

Frame — 12 + 266 = **278 bytes**:

```
11 22 33 44 55 66  22 22 33 44 55 66  06 00
00  <idxMSB> <idxLSB>  85  00  07  <page>  00
<256 bytes of image data at offset page*0x100>
```

**Step 3 — commit:** none. No commit/verify frame appears in this path; the
progress bookkeeping around it is UI only. Verification is done by reading back
(§12) and comparing.

Note the type byte is **0x06** for all three opcodes at `addrHi=0x07`, because
`0x23`, `0x44` and `0x85` all fail the `cl <= 0x22` test and take the computed
branch, which yields `6` whenever `addrHi < 8`.

### 13.6 Point 2 — VERDICT: yes, it is the length-prefixed .rcvbp, verbatim

**Confirmed** in `CSendAndSaveRcvParam::GetRcvParamBufForSPIFlash` (`0x1ec3a8`–`0x1ec459`):

```
len = GetBpFileLength(/*compressed=*/1)        ; 0x1ec3bb, esi = 1
word [image+0x10034] = 0x8000                  ; destination offset within the image
buf = new[len]; bzero
SaveBpToBuffer(fileMgr, buf, &len, /*compressed=*/1)   ; 0x1ec3fc, ecx = 1
if (len >= 0x6ffd) len = 0x6ffc                ; HARD CLAMP
memcpy(image + 0x8000 + 4, buf, len)           ; file body
word [image+0x10036] = len + 4                 ; total incl. prefix
dword [image + 0x8000] = len                   ; <-- u32 LE LENGTH PREFIX
```

Answering the specific questions:

* **The prefix counts the file only, not file+4.** `dword[image+0x8000] = len`
  where `len` is the value returned by `GetBpFileLength`/`SaveBpToBuffer`. The
  `+4` appears only in the separate *total size* field at `image+0x10036`.
* **It is the compressed variant** (both calls pass `1`), i.e. the 0x20-byte
  header form: 16-byte signature, `u32 version = 4`, `u32 compressed size`,
  `u32 decompressed size`, `u32 0`, then the zlib stream. This matches the
  container you read back byte-for-byte.
* **Image offset 0x8000 ↔ flash page `(0x07, 0x80)` = page 0x0780** — exactly
  where you found it. The mapping is self-consistent.
* **Maximum size 0x6FFC (28 668 bytes.)** Anything larger overflows into blocks
  0xd6 (`ExOne`) and 0x1c (`ExTwo`). Your file (~9.4 KB) and the card's copy
  (0x2398 = 9 112 B) are far below the clamp, so **only block 0x07 is involved**.
* **No page padding is applied to the blob itself** — but writes happen in whole
  256-byte pages, so the final partial page is written from whatever the image
  buffer holds there (zero, from the `bzero` of the image). Pad with zeros.

So you can write your own `.rcvbp` essentially verbatim: `u32 LE length` followed
by the compressed file, at image offset 0x8000. **No pack synthesis is needed.**

### 13.7 Point 3 — how it takes effect

`CReceiverOP::ReLoadLocalParam` @ `0x3b4b00` does exist and does build a
`BuildRcvCardFlashOperation` frame with a **5-byte payload** (`push 5`, dataptr =
a 5-byte local at `rbp-0x30`, flag = 0), sent with reply selector `edx = 0x807d`,
`r9d = 2`. **I could not resolve its opcode or address bytes** — they are
register-loaded from earlier branches I did not trace, so I will not guess them.

**Recommended: power-cycle the card.** It is guaranteed by the architecture
(config is read from flash at boot), costs nothing, and carries no risk of
sending a mis-decoded command. Treat "reload without reboot" as a later
convenience, not part of the first write.

### 13.8 Point 4 — safety

**A. The single most important finding: erase is whole-block, and you must
read-modify-write.**

`ClearFlashSector(..., hi=0x07)` issues opcode `0x23` at `(0x07, 0x00)` —
**page 0, i.e. the start of the block**, with no length parameter. Combined with
the 64 KB block addressing (§13.2) this is a standard SPI **64 KB block erase**:
it clears the *entire* block 0x07 (bytes 0x070000–0x07FFFF), not just the
parameter area at 0x8000.

The vendor tool gets away with this because it rebuilds the **whole 64 KB image**
and rewrites every page. **If you erase block 0x07 and write only the pages
around 0x8000, everything else in that block is left as 0xFF.**

> **Required procedure:** read all 256 pages of block 0x07 first (§12, opcode
> 0x44, safe), modify only the region at image offset 0x8000, erase, then write
> back **all** 256 pages. Never erase-then-partially-write.

I have **not** determined what occupies block 0x07 pages 0x00–0x7F. Reading them
is free and safe — do that before the first write and keep the dump.

**B. There is no redundant copy to fall back on.**

`GetRcvParamBackupExOne/TwoBufForSPIFlash` are misleadingly named: at `0x1ec9cb`
they are entered **only when `len >= 0x6ffd`**, and they store
`buf + 0x6ffc` onward — they are **overflow continuation chunks, not backups**.
For a config under 28 668 bytes they are never written. So block 0x07 holds the
only copy of the parameters.

**C. Failure mode if a write is interrupted: recoverable, not a brick —
provided the guard holds.**

After the erase, block 0x07 reads 0xFF until rewritten. An interrupted write
leaves an invalid/partial config, and the panel would not light. It is **not** a
brick, because:

* Discovery, the flash read/write command handler, and the Ethernet stack live in
  **firmware, in a different block**, reached only through
  `BuildRcvCardFlashOperationEx`. Nothing in the parameter write path touches it.
* Therefore the card still answers discovery (type 0x0700) and still accepts
  flash reads and writes after a failed attempt — you simply repeat the write.

The recovery path is only preserved if the firmware blocks are never written.
That is precisely what the guard below enforces.

**D. Recommended hard guard**

Refuse to transmit any frame unless **all** hold:

1. builder is `BuildRcvCardFlashOperation` layout (never the `Ex` firmware form);
2. `payload[5]` (opcode) ∈ {`0x44` read, `0x23` erase, `0x85` write};
3. `payload[7]` (addrHi) **== 0x07** — an allowlist of exactly one block;
4. for `0x85`: `datalen == 0x100` and the frame is 278 bytes;
5. for `0x23`: `payload[8] == 0x00` and no payload;
6. a dry-run mode that prints frames without sending, plus a full block-0x07 dump
   taken and saved *before* the first erase.

An allowlist on `addrHi` is strictly safer than a denylist: every other
parameter class, the calibration blocks, and the firmware all live at different
`addrHi` values and are unreachable by construction.

**E. Residual unknowns — state them before writing**

* Contents of block 0x07 pages 0x00–0x7F (read first).
* Whether the card validates the config (e.g. a checksum) before applying it; no
  such check was found, so a malformed blob may simply yield a dark panel.
* `ReLoadLocalParam`'s exact bytes (§13.7) — avoid, power-cycle instead.
* The erase is assumed to be a 64 KB block erase from the addressing granularity
  and the `lo = 0` argument. It could conceivably be a 4 KB sector erase; the
  read-modify-write procedure in **A** is correct and safe under *either*
  interpretation, which is why I recommend it unconditionally.

---

## 14. The 4-byte trailer — solved

**It is a CRC-32 with a non-standard initial value.** Found in the writer, then
verified against 18 independent files.

### 14.1 The algorithm

```
CRC-32, reflected, polynomial 0xEDB88320   (the ordinary CRC-32 polynomial)
initial register value : 0x00000000        <-- NOT 0xFFFFFFFF
final XOR              : none              <-- NOT ^0xFFFFFFFF
range                  : the whole file, offset 0 up to (not including) the trailer
storage                : little-endian, appended as the last 4 bytes
```

That single deviation — init 0 and no final XOR, instead of the usual
init 0xFFFFFFFF / xorout 0xFFFFFFFF — is why every standard `crc32` brute force
missed it.

Reference implementation:

```rust
fn trailer_crc(data: &[u8]) -> u32 {
    let mut t = [0u32; 256];
    for i in 0..256u32 {
        let mut c = i;
        for _ in 0..8 { c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 }; }
        t[i as usize] = c;
    }
    let mut c: u32 = 0;                       // init 0
    for &b in data { c = (c >> 8) ^ t[((c ^ b as u32) & 0xff) as usize]; }
    c                                          // no final xor
}
// append trailer_crc(file).to_le_bytes() to the end of the file
```

Equivalent one-liner against any stock zlib:
`crc = zlib_crc32(data, 0xFFFFFFFF) ^ 0xFFFFFFFF` (verified identical).

### 14.2 Where it comes from (`SaveBpToBuffer` @ `0x1ca810`, tail at `0x1cdd96`–`0x1cde05`)

The compressed header is written first (`0x1cdd5e`–`0x1cdd87`):

```
movdqu xmm0, [rcvBasicParamFileTagEx]   ; 16-byte signature
movdqu [rbx], xmm0
mov dword [rbx+0x10], 4                 ; version
mov dword [rbx+0x14], r15d              ; compressed size
mov dword [rbx+0x18], r13d              ; decompressed size
mov dword [rbx+0x1c], 0
add rbx, 0x20
```

then the CRC is computed over the buffer and appended:

```
xor esi, esi                            ; <-- CRC register starts at 0
loop:                                   ; unrolled 2 bytes/iteration
  movzx ecx, sil
  shr   esi, 8
  movzx eax, byte [rbx + rdx - 1]
  xor   ecx, eax
  xor   esi, dword [rbp + rcx*4 - 0x17580]    ; 256-entry u32 table
  ...
mov dword [rbx], esi                    ; <-- stored directly, NO final XOR
add r13d, 4                             ; length includes the trailer
```

`rbx` is set to `end - count`, so the range is the `count` bytes immediately
preceding the trailer position, and `r13d` (the returned file length) is
incremented by 4 — which is why the flash length prefix equals *file + 4* in the
sense that the trailer counts as part of the file. Your reading of the prefix was
right.

### 14.3 Verification: 18 / 18 files, zero failures

Applied to every `.rcvbp` in the vendor corpus plus the user's file, covering
**both container variants and two different signature families**:

| file | variant | signature | trailer |
|---|---|---|---|
| user's `P2.5-32S-128X64-SM16269S-256X384I` | compressed | 0xbe192020 | **0x128bebee** ✓ |
| `P2.5-320x160-2153-138-3840-256X384` | compressed | 0xbe192020 | 0xaba8f77e ✓ |
| `P2.5-2153-128512-2020.6.29` | uncompressed | 0x213f3acb | **0xd8b3a9d4** ✓ |
| `P2.5-2153-128512-2020.7.1` | uncompressed | 0x213f3acb | **0xcf92f78c** ✓ |
| `P2.5-64x32-32s-2053` | uncompressed | 0x213f3acb | **0x7bbae2eb** ✓ |
| + 13 more | uncompressed | 0x213f3acb | all ✓ |

Three of these are values from the coordinator's list of "high-entropy"
unknowns, now reproduced exactly. The rule is identical for the compressed
(0x20-header) and uncompressed (0x14-header) forms: always the whole file up to
the trailer.

**Self-check available:** the card's stored blob should satisfy
`trailer_crc(blob[0..9108]) == 0x5ac1e060` (from your trailer bytes `60 e0 c1 5a`).

### 14.4 Does anything validate it? — iSet does NOT

Scanned every function in the load path — `CHWParamReceiver::LoadFromBuffer`
@ `0x170e50`, `LoadBpFromBuffer` @ `0x1c48d0`, `LoadBpHeadFromBuffer` @ `0x1c49f0`,
`LoadBpBufFromBuffer` @ `0x1c5020`, `LoadFromBpFile` @ `0x1c45a0` — for CRC-style
table lookups (`xor r32, dword [table + idx*4]`), byte-shift accumulation, and
calls to `crc32`/`adler32`:

| function | CRC table lookups | `shr r,8` | crc32/adler calls |
|---|---|---|---|
| `LoadFromBuffer` | 0 | 0 | 0 |
| `LoadBpFromBuffer` | 0 | 0 | 0 |
| `LoadBpHeadFromBuffer` | 0 | 0 | 0 |
| `LoadBpBufFromBuffer` | 0 | 0 | 0 |
| `LoadFromBpFile` | 0 | 0 | 0 |

**Zero occurrences anywhere in the load path.** Two further structural
confirmations that the trailer is never even read:

1. `LoadBpFromBuffer` passes `srcLen = dword[buf+0x14]` (the *compressed size*) to
   `uncompress` — the trailer lies beyond that and is never handed to zlib.
2. The TLV walk in `LoadBpBufFromBuffer` exits at `0x1c5b84` on
   `cmp r15d, 4 ; jbe` — with 4 or fewer bytes remaining it stops **silently and
   successfully**. So in the uncompressed variant the trailing 4 bytes are simply
   left unconsumed, with no error path.

So iSet writes the trailer and never checks it.

**Caveat, stated plainly:** the parser that matters is the *card firmware*, which
is not available for static analysis (the only firmware images on hand are E320
FPGA `.hex` files, a different product and architecture). iSet's behaviour is
indirect evidence only.

**But the question is now moot:** since §14.1 reproduces the value exactly on
18/18 files, we should simply always compute the correct CRC. That removes the
dependency on whether anyone validates it — which is a far better position than
relying on a "nobody checks" argument.

**It is not a nonce or timestamp.** It is fully deterministic from content: the
two 18 766-byte corpus files that differ in exactly two content bytes produce
different trailers, and both are reproduced exactly by the algorithm above.

---

## 15. The layout / screen-size command (type 0x02) — RAM-only

Static analysis only. Nothing executed.

### 15.1 It is a real-time (RAM) command — safe to test immediately

`CReceiverOP::SendOrSaveLayout` @ `0x3b5990` → `DoSendOrSaveLayout` →
`CRcvLayoutSendAndWriter`. Its `DoSendSave` @ `0x37c840` has two clearly separated
phases:

```
phase 1 (RAM):   GetCardAreaParamPacks* ... -> SendRealTimePacks()
phase 2 (persist): DoWriteConnectionToEeprom / WriteBackUpConncetion / SaveSenderCardArea ...
```

Scanned for flash operations (`BuildRcvCardFlashOperation`, `ClearFlashSector`,
`BuildRcvStorage*`):

| function | flash ops |
|---|---|
| `DoSendSave` @ 0x37c840 | **0** |
| `PrepareData` @ 0x386530 | **0** |
| `GetRealTimePacks` @ 0x37d760 | **0** |

The only flash writes anywhere in the layout writer are
`WriteBackUpConncetion` @ `0x37f8ae` and `...ForBackup` @ `0x37ff42`, and both
target **region `addrHi = 0x1d`** — not block 0x07, and not page 0xF0.

**So sending the type-0x02 pack changes RAM only.** It cannot erase or write
flash, and it cannot make the current situation worse. It is the ideal thing to
test right now.

### 15.2 Frame layout (from `GetParamPacksLayout` @ `0x3b9540`)

Buffer base = `rbp-0x2850` = payload[0]; built at `0x3b9574`–`0x3b969c`:

```
bzero(payload+1, 0x503)
payload[0] = 2                       ; type byte

ebx = GetRcvMaxWidth()               ; receiver width
eax = GetRcvMaxHeight()              ; receiver height
ecx = ebx >> 8 ; edx = eax >> 8      ; MSBs
rsi = 0xd
loop:
   dword [payload + rsi - 9] = 0     ; entry: xOffset, yOffset
   byte  [payload + rsi - 5] = cl    ; width  MSB
   byte  [payload + rsi - 4] = bl    ; width  LSB
   byte  [payload + rsi - 3] = dl    ; height MSB
   byte  [payload + rsi - 2] = al    ; height LSB
   word  [payload + rsi - 1] = 0
   rsi += 0xa
until rsi == 0x50d
CSendControl(payload, 0x504)
```

Which gives, in payload coordinates:

```
payload[0]        = 0x02                     type
payload[1..3]     = 0
payload[4 + 10*i] for i = 0 .. 127           128 receiver entries, 10 bytes each:
      +0..1  xOffset  (big-endian u16)
      +2..3  yOffset  (big-endian u16)
      +4..5  width    (big-endian u16)
      +6..7  height   (big-endian u16)
      +8..9  reserved 0
payload length    = 0x504 = 1284 bytes
frame length      = 12 + 1284 = 1296 bytes
```

Entry count = `(0x50d - 0xd) / 0xa` = **128**, stride 10, spanning
payload[4 .. 0x503]. All 16-bit fields are **big-endian**, consistent with the
rest of the protocol.

**This independently confirms FPP's documented offsets.** Their `Data[]` starts at
payload[1], so their `Data[7]` = payload[8] = width MSB and `Data[9]` =
payload[10] = height MSB — exactly what the instructions above produce. Their
`Data[13]`/`Data[15]` (next receiver x/y offset) line up with entry 1's offset
fields at payload[14]/payload[16]. So FPP's header was right; you can now trust it.

### 15.3 The exact frame to restore 128x64

The initialisation loop writes the *same* width/height into all 128 entries and
zeroes every offset — so for a single receiver at (0,0) the initialisation alone
already produces the correct entry 0.

```
 0x00  11 22 33 44 55 66      dst MAC
 0x06  22 22 33 44 55 66      src MAC
 0x0c  02 00                  type 0x0200          <- payload[0..1]
 0x0e  00 00                  payload[2..3]
 0x10  00 00 00 00 00 80 00 40 00 00     entry 0: xOff=0 yOff=0 w=128 h=64
 0x1a  ... 127 more 10-byte entries ...
```

Two options for entries 1..127, in order of preference:

1. **Replicate the vendor initialisation** — every entry `00 00 00 00 00 80 00 40 00 00`.
   This is literally what the disassembled loop produces, so it is the
   behaviour most likely to be accepted.
2. Entry 0 populated, entries 1..127 all zero. More conservative in intent, but
   *not* what the code does — try it only if option 1 misbehaves.

Note the type word on the wire is `02 00`: `payload[0]=2`, `payload[1]=0`,
matching the `0x0700` / `0x0600` pattern of the other commands.

### 15.4 Why your page-0xF0 writes are refused (hypothesis, well-supported)

The parameter window is image offset `0x8000` + 4-byte prefix + max `0x6FFC`
= **exactly `0xF000`** (§13.6). Page 0xF0 begins precisely where the parameter
window ends — it is the next record, outside the window, which is consistent with
a firmware bounds check on opcode 0x85 writes.

Nothing in the vendor tool writes block 0x07 page 0xF0 via `BuildRcvCardFlashOperation`:

* `DoWriteToRcvForSeparate` writes only pages drawn from its parameter page list,
  which by construction covers the `0x80..0xEF` window.
* Every other `addrHi` in the region map (§13.3) is a *different* 64 KB block.
* The layout persistence path writes region `0x1d` and the EEPROM path — never
  `(0x07, 0xF0)`.

**Best explanation: the host never writes 0x07F000 directly — the card's firmware
writes it itself when it receives and persists a layout.** That accounts for both
observations: the page had valid content originally, and the host-side page write
is rejected now.

If that is right, the repair is not a raw page write at all: send the layout
(§15.3), and if you then want it persisted, use the layout *save* path rather
than trying to place the bytes yourself.

### 15.5 Correction to §12/§13: vtable slot names were misattributed

While tracing this I read the `CReceiverOP` vtable at `0x454c40` and got
`vt+0x610 = WriteDataToEepromFlashEx`, `vt+0x620 = ClearFlashSector` — which
contradicts §12 (where I called `[rax+0x620]` `ReadFlashToBuffer`) and §13 (where
I called `[rax+0x610]` `ClearFlashSector`).

The **argument shapes**, not the vtable read, are the reliable evidence:

* §12's `[rax+0x620]` call passes 8 arguments matching
  `ReadFlashToBuffer(u32,int,int,uchar,uchar,uint,uchar*,int)` exactly — and the
  frame it produced worked on hardware, which settles it empirically.
* §13's `[rax+0x610]` call passes `this` + 4 arguments, matching
  `ClearFlashSector(u32,u16,u16,u8)` and **not** the 6-argument
  `WriteDataToEepromFlashEx`.

So the frame-level conclusions in §12 and §13 stand; only the vtable-slot
*labels* were wrong. The likely cause is that these objects are a derived class
whose vtable is not the one at `0x454c40`. **Treat any vtable-slot name in this
document as indicative only; argument arity and the literal immediates are the
load-bearing evidence.**

### 15.6 What is still unknown

* The exact per-receiver patching that follows the initialisation loop (I read the
  init and the `CSendControl` wrap; a middle section may adjust entries for
  multi-receiver setups — irrelevant for a single 128x64 receiver).
* Whether the card persists the layout on its own or needs an explicit save
  command; I did not identify a distinct "save layout" frame type. FPP's
  documented `0x11` ("Save Config", 1283 data bytes) has *exactly* the same data
  length as this pack's `0x503`, which is suggestive of a save-variant sharing the
  structure — **unverified**, do not send it blind.
* The full semantic layout of page 0xF0 beyond the width/height fields you
  already identified. You have the 256 original bytes, so this only matters if a
  direct write path turns up.

---

## 16. Self-test, reload, and boot-apply

Static analysis only. Nothing executed.

### 16.1 Built-in screen test — `CReceiverOP::SetRcvCardTestMode` @ `0x3d54e0`

**This is a real-time command. It builds its buffer inline and sends via
`[deviceIO + 0x38]` — no flash builder, no erase, no `BuildRcvCardFlashOperation`
anywhere in the function. It cannot touch flash.** Safe to sweep freely.

Frame construction (`0x3d558a`–`0x3d5655`), buffer base = `rbp-0x140` = payload[0]:

```
byte  [payload+0x00] = 0x33            ; type
word  [payload+0x01] = 0
word  [payload+0x03] = rol(rcvIdx,8)   ; receiver index, BIG-endian
byte  [payload+0x05] = 0x09            ; fixed sub-command
byte  [payload+0x06] = param[0]        ; <-- TEST PATTERN SELECTOR
byte  [payload+0x07] = 0
byte  [payload+0x08..0x0a] = 24-bit BE ; trunc(K1 / f) * K2  from float param[4]  (speed/period)
byte  [payload+0x0b] = param[8]
word  [payload+0x0c] = rol(param[0x0a],8)   ; BE
word  [payload+0x0e] = rol(param[0x0c],8)   ; BE
word  [payload+0x10] = rol(param[0x0e],8)   ; BE
       rest zero
push 0x10b ; push buf ; call [deviceIO+0x38]
```

* **payload length = 0x10B = 267 bytes → frame = 12 + 267 = 279 bytes**
* type word on the wire is `33 00`
* send selector `edx = 0x807d`, `r9d = 2` (same as the reload command below)

Ready-to-send frame (receiver index 0, everything but the selector zero):

```
 0x00  11 22 33 44 55 66      dst MAC
 0x06  22 22 33 44 55 66      src MAC
 0x0c  33 00                  type 0x3300
 0x0e  00                     payload[2]
 0x0f  00 00                  payload[3..4]  receiver index 0, BE
 0x11  09                     payload[5]     fixed
 0x12  NN                     payload[6]     <-- PATTERN SELECTOR, sweep this
 0x13  00 ... 00              payload[7..0x10a], all zero  (total payload 267 B)
```

**The selector values are not recoverable statically.** The enum lives in the UI
layer, and I searched for it without success: `ScrnTest.dll` yields only
`NORMAL` / `RED`-family fragments with no numeric mapping, and the iSet binary's
only "Grayscale" hits are Qt print-dialog boilerplate. I will not invent the
mapping.

**Recommended sweep.** The command is RAM-only, so this is free: send
`payload[6] = 0x00 .. 0x0F` one at a time, a second or two apart. `0x00` is
almost certainly normal/off — use it to turn the test off. The strings in
`ScrnTest.dll` suggest the low values are the solid colours (red/green/blue),
with line/grayscale patterns above them. If any value lights the panel you have
your answer immediately; if none do across the whole sweep, the configuration is
still wrong and the pixel path is not the problem.

Leave the other fields zero on the first pass. `payload[8..0x0a]` is a
speed/period derived from a float and only matters for moving patterns; zero may
mean "as fast as possible" or may be ignored for static patterns.

### 16.2 Reload parameters from flash — opcode 0x79 (now pinned down)

`CReceiverOP::ReLoadLocalParam` @ `0x3b4b00`, call site `0x3b4b83`–`0x3b4bae`:

```
dword [rbp-0x30] = 0          ; 5-byte payload buffer
byte  [rbp-0x2c] = 0 or 1     ; branch-selected flag
mov cl, 0x79                  ; <-- OPCODE 0x79
movzx edx, r12w               ; receiver index
xor r8d, r8d                  ; addrHi = 0
xor r9d, r9d                  ; addrLo = 0
push 5 ; push dataptr ; push 0
BuildRcvCardFlashOperation(&len,&buf, rcvIdx, 0x79, 0, 0, flag=0, dataptr, 5)
```

Note `0x79` is **not** in the data-carrying opcode set `{0x85,0x77,0x66,0x52,0x42,0x32}`
(§13.1), so the builder **skips the memcpy** and the 5 bytes are never attached.
The frame is header-only, 128-byte payload:

```
 0x0c  06 00                  type 0x0600
 0x0e  00                     payload[2]
 0x0f  00 00                  payload[3..4]  receiver index, BE
 0x11  79                     payload[5]     opcode = reload
 0x12  00                     payload[6]     flag
 0x13  00                     payload[7]     addrHi = 0
 0x14  00                     payload[8]     addrLo = 0
 0x15  00                     payload[9]
 0x16  00 x 118               payload[0x0a..0x7f]
```

**Risk note:** this carries no data and cannot write anything, but `addrHi = 0`
is outside the `0x07` allowlist from §13.8-D, and `0x79` is not a read opcode. It
is not covered by the guard, so it is a deliberate exception — justified by the
function name and the empty payload, but worth a moment's thought before you send
it. If it works it saves you a power cycle on every iteration.

### 16.3 Does the card apply flash config at boot? — honest answer

**I cannot determine this from the host-side dylib.** Boot behaviour is firmware
behaviour, and the only firmware images available are E320 FPGA `.hex` files — a
different product and architecture.

What the static evidence *does* support:

* The card clearly reads *something* from flash at boot — your 1024x512 fallback
  after the erase proves a flash-backed record drives the discovery reply.
* The vendor tool always does **both**: `SendOrSave` sends the real-time packs
  *and* writes flash (§2, §13.5). It never relies on flash alone within a session.
  `DoSendSave` in the layout writer has the same two-phase shape (§15.1).

That the vendor never relies on flash alone is suggestive but not proof. **Item
16.1 settles it empirically and costs nothing:** if the built-in pattern lights
the panel, the flash config is being applied and your pixel/sync path is at
fault; if it stays dark with the config verified byte-for-byte, then either the
config is still wrong or the chip registers are only applied from the real-time
type-0x05 packs.

Either way, **sending the §10/§11 real-time packs is a cheap next experiment**
and I would do it regardless — it is RAM-only and it is exactly what the vendor
tool does before every session.

### 16.4 Discovery reply — what I can and cannot attribute

From `BuildDetectRcvCard` and the reply handling I can only confirm what §1/§12
already established: `payload[0]` card id (0x64 on your E120), `[1..2]` firmware
version, `[20..21]` columns, `[22..23]` rows, `[62]` controller number. Your live
capture is the better source for the rest.

I did **not** find a decoder that maps any reply byte to "configuration valid" or
"chip type", and I am not going to guess at offsets in a 1056-byte reply. The
richer diagnostic surface is elsewhere: the symbol table exposes
`CReceiverOP::DetectOneRcvInfo` @ `0x3aa640`, `ReadDeadPixelInfo` @ `0x3da230`,
`ReadEepromEMCInfo` @ `0x3b7b50` and `ExecutRcvCrcCheck` @ `0x3b2e00` — the last
being the most promising for "does the card think its config is intact", since it
implies a card-side CRC over stored data. None of these are traced yet; say the
word if item 16.1 does not resolve things and I will take `ExecutRcvCrcCheck`
apart next.

---

## 17. CRC check, output routing, and a hardware observation

Static analysis only. Nothing executed.

### 17.0 Read this first — the current measurement points away from configuration

Your numbers: total draw **~0.63 A**, and the E120 spec rates **the card alone at
0.6 A / 3.0 W**. So the panel is contributing roughly **0.03 A — essentially
nothing**.

A P2.5 128x64 module that is powered but idle still draws a real quiescent
current: its shift registers, driver ICs and decoder are energised even with all
LEDs off. A powered-but-misconfigured panel looks like *some* current and a dark
face. A panel drawing ~0 A is not a misconfigured panel — it is a panel that is
**not powered**.

This single hypothesis explains every observation at once, including the two that
configuration cannot explain:

* the card's **own physical test button** produces nothing — that path is entirely
  card-side and needs no host config to light a panel that has power;
* the panel stays dark across every config we write, verified byte-for-byte;
* total current equals the card's own rated draw;
* the card remains fully responsive throughout.

**The HUB75 ribbon carries signals, not panel power.** The panel has a separate
power input (typically a 4-pin/screw 5V feed) and at P2.5 128x64 it needs amps,
not milliamps. If only the receiving card is on the bench supply, this is exactly
what you would measure.

**Please check before any more protocol work:** is the panel's own 5V connector
energised, is it on the same supply, and does the supply have the headroom? A
quick test is to watch the current while pressing the card's test button — on a
powered panel that must move the meter substantially even if the pattern is
wrong. If it does not move, the problem is upstream of everything in this
document.

I flag this prominently because I have now spent several rounds decoding
protocol on the assumption that configuration was the blocker, and the power
figure is the first piece of evidence that is genuinely inconsistent with that
assumption.

### 17.1 Item 3 — output routing: probably NOT the cause

From your file's record 0x01 (§9 mapping applied to the real bytes):

| record 0x01 payload | value | meaning |
|---|---|---|
| +0x044 | **0x01** | `OBJ+0xb9` = `GetOutputCount` / `GetRealOutPutCount` = **1** |
| +0x058 | 0x10 | `SetHubType(16)` |
| +0x04e | 0x00 | `OBJ+0xbb` = `GetOutPutModel` |
| +0x036 | 0x4c | chip-library selector |
| +0x03a / +0x03c | 0x00 / 0x00 | `SetLineDir` = 0 |

**Output count is 1**, and I found **no field anywhere that names a physical
J-connector**. The connector is implied by output index, so a single output means
the first HUB75 group — **J1**, which is where your ribbon is. Line direction is
0 (no rotation/mirroring).

So item 3 does not explain the dark panel. That is a useful elimination: you can
stop chasing connector routing.

### 17.2 Item 1 — `ExecutRcvCrcCheck`: frame shape decoded, but not ready to send

`CReceiverOP::ExecutRcvCrcCheck` @ `0x3b2e00` calls
`BuildEnableCalcCrcEx` @ `0x30c1c0` with:

```
rcvIdx        = arg (u16)
arg4 (uchar)  = byte [param + 0x00]      <-- becomes the TYPE byte
arg5 (bool)   = 1
arg6 (bool)   = (byte[param+8] != 0)
arg7 (uchar)  = byte [param + 0x09]
arg8 (uchar*) = [param + 0x10]
arg9 (uint)   = dword [param + 0x04]
```

Builder body (`0x30c235`–`0x30c279`), 0x80-byte payload:

```
payload[0]      = arg4                 ; type byte — CALLER-SUPPLIED
payload[1..2]   = 0
payload[3]      = rcvIdx >> 8          ; BE
payload[4]      = rcvIdx & 0xff
payload[5]      = 0x82 - arg5          ; = 0x81 when arg5 = 1
payload[6..0xa] = 0
payload[0x0b]   = arg9 byte 0          ; 32-bit LITTLE-endian
payload[0x0c]   = arg9 byte 1
payload[0x0d]   = arg9 byte 2
payload[0x0e]   = arg9 byte 3
payload[0x0f]   = !arg6
payload[0x10]   = arg7
 (further bytes when arg7 != 0)
```

**I cannot give you a ready-to-send frame.** The type byte at `payload[0]`, the
32-bit value, and the two flags all come from `SExecuteRcvCrcCheckParam`, which is
populated by the UI layer — and `ExecutRcvCrcCheck` has **no callers inside this
dylib**, so the constants are not recoverable here. Opcode-slot `0x81` and the
`0x82 - flag` construction are the only literals.

I will not guess a type byte for a command whose semantics I cannot bound —
`ClearRcvCrcFlag` @ `0x3b2d00` sits in the same family, and a wrong guess in that
neighbourhood could clear card state rather than query it. Given §17.0 I would
not spend the risk budget here at all right now.

**On your underlying question — "has the card ever parsed our blob?" — I found no
validity byte or status word that answers it.** Nothing in the discovery reply
decoder maps to "config valid". The honest position is that we have no
card-side confirmation mechanism identified, and the 1024x512 fallback tells us
only that the card reads page 0xF0 at boot, exactly as you say.

### 17.3 Item 2 — SM16269 is not in the vendor chip library

Searched `ChipSetting.dll`'s chip class list and `ChipData/` (62 `pm_*.dat`
files) plus the iSet dylib:

* **No `SM16269` anywhere.** Nearest named entries are `CChipSettingSM16188`,
  `SM16219`, `SM16227`, `SM16237`, `SM16609`, `SM16803`.
* No `pm_*.dat` filename or content matches `16269`.

So there is **no `.dat` chip profile to decode** — that avenue is closed, and I
should say so plainly rather than send you after a file that does not exist.

The constructive reading: the pack path is named **`GetChipCustomPlusParamPack`**
and the accessor is **`GetChipCustomEX`** — *custom*. That is consistent with
SM16269 being carried as a **custom chip profile**, in which case record 0x84's
`(reg, R, G, B)` table in your file **is** the chip definition, and it is data you
already hold. That is the good outcome for buildability.

**Correction, same class of error as §15.5:** in §10.2 I said the 180-byte block
comes from `GetChipCustomEX()` via `[vtable+0x130]`. That is wrong —
`GetChipCustomEX` @ `0x16dc80` is a 12-byte accessor returning
`dword [OBJ+0xd4e1]`, a scalar. The 180-byte source is a different virtual
function on the object's actual (derived) vtable, which I have not identified.
**Treat §10.2's naming of that block as unverified.** The frame geometry in §10.2
(offsets and sizes) came from the literal instructions and still stands; only the
attribution of the source function was wrong.

I did not complete the trace from record 0x84 into `OBJ+0xd6d0`. Given §17.0 I
recommend resolving the power question before investing further here.

---

## 18. Scan mode in record 0x01 — resolved

### 18.1 The answer: `payload + 0x020` is the scan denominator

`GetScanMode()` @ `0x16e670` is a one-line accessor:

```
movzx eax, byte [rdi + 0xc1]      ; OBJ+0xc1, a single byte
```

and §9.2 showed record 0x01 `payload+0x020` feeding `SetScanMode(u8)` @ `0x131bb0`,
which writes that member. So the scan field is **one byte at record-0x01
`payload+0x020`**, holding the **scan denominator directly** — 16, 32 or 64, not a
log2 index and not a row count.

### 18.2 Verified against every unambiguously-named corpus file — 10/10

| file | name says | `payload+0x020` |
|---|---|---|
| `P2.5-16S-16169-64X64-160X160-KSL-V1.0` | 16 | **16** ✓ |
| `p2.5-2053+2018-128X64-16s` | 16 | **16** ✓ |
| `P2.5-128_64-32s-2038-138` | 32 | **32** ✓ |
| `P2.5-128x64-32S-6618+7258-3.80` | 32 | **32** ✓ |
| `P2.5-128x64-32S-9929+7258-3.0-75B` | 32 | **32** ✓ |
| `P2.5-64x32-32s-2053` | 32 | **32** ✓ |
| `P2.5-7347-57D-6464-32s-2121-2038-2012-138` | 32 | **32** ✓ |
| `P2.5-128x64-64S-9929+9737(mini)-3.15` | 64 | **64** ✓ |
| `P2.5-128x64-64S-9929+9739-3.15-75E` | 64 | **64** ✓ |
| `P2.5-128x64-64S-9929+9739-5.0-75B` | 64 | **64** ✓ |
| `P2.5-9929+9736-128x64-64S` | 64 | **64** ✓ |

Every file whose name states a scan matches, across all three values and across
marker bytes 0x08, 0x09 and 0x0a.

### 18.3 The installed config is ALREADY 1/16 scan

```
P2.5-32S-128X64-SM16269S-256X384I.rcvbp   ->  payload+0x020 = 0x10 = 16
```

Record 0x01 of the user's file, offset 0x20 onward:

```
+0x020: 10 08 00 0e 01 00 bc 00 ff ff ff 03 02 01 00 00
         ^^ scan = 16
```

**This is the only file in the whole corpus whose name disagrees with its
content.** The filename says `32S`; the payload says 16. Every other file agrees
with its name, which is what makes the rule trustworthy and this file the outlier.

**So the scan-mismatch hypothesis does not hold.** The config on the card already
specifies 1/16 scan, matching the datasheet's `O16S` / "1/16 duty". There is no
scan edit to make — the deliverable you asked for (byte edits turning 1/32 into
1/16) is a no-op, because the installed config is already 1/16.

Two more files corroborate: `P2.5-320x160-2153-138-3840-256X384.rcvbp` — the same
320x160mm / 128x64 P2.5 module as ours — also carries `payload+0x020 = 16`, as do
the two `P2.5-2153-128512` files.

### 18.4 `payload+0x001` is not scan

Your suspicion was right. Across the corpus `+0x000 ∈ {64,128}` and
`+0x001 ∈ {32,64}`, and they track each other rather than scan: the two 16-scan
files hold 32 and 64 respectively, and 32-scan files hold both 32 and 64. They
behave like module geometry fields, not scan. The 32→64 movement you saw in the
32S/64S pair was those two files also differing in geometry — coincidental to
that pair.

### 18.5 The "derived" fields are derived from clock, not from scan

Exact relationships, holding across **all 19 files**:

```
payload+0x04b  ==  payload+0x021
payload+0x049  ==  payload+0x021 // 2      (integer division)
```

But `payload+0x021` is **not** a function of scan. Observed values are 7, 8, 10,
12, 14, 15, 16 and 18, and files sharing a scan hold different values (32-scan
files show 7, 7, 7, 12, 15; 64-scan files show 12, 14, 18, 18). §9.2 maps
`payload+0x021` to `SetSerialClockFrequency(u16)` — a clock setting that varies
per panel/driver design.

So the `12->18` / `6->9` movement in your 32S-vs-64S diff was a **clock**
difference between those two designs that happened to accompany the scan change.
There is no scan→timing formula to apply. If you ever do change `+0x021`, keep
the two derived bytes consistent using the two identities above.

### 18.6 The marker byte does not change payload interpretation

**Definitive, from the dispatcher** (`LoadBpBufFromBuffer` @ `0x1c5b8e`):

```
ecx = dword [r12]        ; the whole 4-byte record header
ebx = cx                 ; length   <- bytes 0..1
eax = ecx >> 0x18        ; id       <- byte 3
al  = id + 0x7f          ; jump-table index
```

**Byte 2 — the marker — is never extracted or tested anywhere in the parser.** It
is read as part of the header dword and then discarded; only the length and the
id participate in dispatch, and the handler memcpys the record verbatim.

Corroborated empirically: the `+0x020` = scan rule in §18.2 holds across markers
0x08, 0x09 and 0x0a without exception.

**So your cross-family diffs are valid**, a 0x09-marker file is parsed exactly
like a 0x0a one, and no field translation is needed between them. (Caveat as
always: this is iSet's parser. The card firmware is not available to analyse.)

### 18.7 Where this leaves the diagnosis

With scan already correct at 1/16, output count 1 → J1 (§17.1), geometry 128x64
confirmed by the card's own discovery reply, and the blob verified byte-for-byte
in flash, **the configuration hypothesis is now substantially weakened.** The
remaining config-side unknown is the driver-chip question: the datasheet says
plain constant-current with no PWM chip named, the card's *original* config had
no record 0x84 at all, and `SM16269` appears nowhere in the vendor chip library
(§17.3) — so the file's chip table may describe a chip this panel does not have.

That is worth testing, and it is cheap: the corpus contains
`P2.5-320x160-2153-138-3840-256X384.rcvbp`, which is **the same 320x160mm 128x64
P2.5 module geometry, 1/16 scan, from the vendor's own library**. Installing that
file is a direct A/B against our SM16269S file and needs no byte editing.

---

## 19. Display-enable, lock, OE and current gain

Static analysis plus corpus inspection. Nothing executed.

### 19.1 Items 1 & 2 — there is NO display on/off or lock command in our topology

`CProcessorNicOP::SetScreenShowOnOrOff` @ `0x258960` and
`CProcessorNicOP::SetScreenLocked` @ `0x258970` are **stubs**:

```
0x258960  push rbp ; mov rbp,rsp ; xor eax,eax ; pop rbp ; ret
0x258970  push rbp ; mov rbp,rsp ; xor eax,eax ; pop rbp ; ret
```

They build no frame, send nothing, and return 0. Real implementations exist only
on the **sender/processor** classes:

* `CProcessorSOP::SetScreenShowOnOrOff` @ `0x261330`, `SetScreenLocked` @ `0x2615f0`
* `CProcessorZOP::SetScreenShowOnOrOff` @ `0x2f4f40`, `SetScreenLocked` @ `0x2f5060`

Those are S-series and Z-series **sender cards**, not receiving cards. `NicOP` —
the network-card sender path, which is exactly our topology — implements neither.

This cuts both ways, and the second direction is the useful one:

1. There is no display-enable frame for me to give you; none exists on this path.
2. **The card cannot be sitting in a software-blanked state waiting for an enable
   command, because LEDVISION/iSet never sends one when driving through a network
   card either.** If such a latch existed and defaulted to blanked, the vendor
   tool could never light this panel over Ethernet.

`IsScreenLocked` @ `0x2588b0` is likewise a `CProcessorNicInfo` accessor over
locally cached sender state — it queries the host's own model, not the card, so it
cannot tell you anything about the receiver.

### 19.2 Item 4 — current gains are NOT zero in either config

Three little-endian floats at record 0x01 `+0x0b4`, `+0x0b8`, `+0x0bc` (the
`SetCurrentByPercent(float,float,float)` / `GetCurrentPercent(float*,float*,float*)`
triple):

| config | +0x0b4 (R) | +0x0b8 (G) | +0x0bc (B) |
|---|---|---|---|
| installed `2153-138` | `00 00 80 3e` = **0.25** | `00 00 80 3e` = **0.25** | `00 00 00 3f` = **0.50** |
| `SM16269S` file | `cd cc cc 3d` = **0.10** | `cd cc cc 3d` = **0.10** | `cd cc cc 3d` = **0.10** |

**Both are non-zero**, so a zero stored current gain is ruled out as the cause.
The currently installed file asks for 25/25/50 % — modest but plainly visible.
(The SM16269S file's 10 % would have been dim, never invisible.)

### 19.3 Item 3 — OE: I could not resolve polarity, and I am not going to guess

The relevant accessors exist — `IsChipHasOE` @ `0x13e080`, `Is8nsOeEnable`
@ `0x145310`, `Get8nsOeEnableInfo` @ `0x168630`, `GetMinOE` @ `0x13e2b0`,
`HR_SetMinOE` @ `0x144b90` — and §7 maps pack `payload[0x3b]` to
`Get8nsOeEnableInfo` (member `OBJ+0xbd`), fed from record 0x01 `+0x050`.

**That byte is `0x01` in both configs**, so it does not differentiate them and it
is not an explanation for the current behaviour.

I could not establish which field, if any, carries OE *polarity*. These accessors
dereference a chip-library sub-object and my attempts to resolve them hit the same
vtable ambiguity that produced two wrong attributions earlier in this document
(§15.5, §17.3). Rather than produce a third speculative answer on a byte you would
act on, I am marking this **unresolved**.

### 19.4 What the current signature actually says

I want to put the measurement argument precisely, because I think it points
somewhere different from the "card scans, display blanked" reading.

The 0.428 A → 0.62 A step on first config write is real and is very likely the
card moving from idle to actively generating HUB75 output. That part of your
reading looks right. But note where it leaves the totals: the E120 spec rates
**the card alone at 0.6 A / 3.0 W**, so 0.62 A is the *card* working. The panel's
contribution is still ~0.

The decisive detail is the invariance you documented:

* full white vs full black — **identical**
* brightness 255 vs 8 — **identical**
* four different configs — **identical**
* during the card's own test-pattern sweep — **identical**

If the panel were powered and receiving drive, white-vs-black would move the
meter substantially — that is the single largest current swing an LED panel can
produce, and no configuration error suppresses it while leaving the card
scanning. Content-invariant and brightness-invariant current means **the LEDs are
never sourcing current at all**.

That is consistent with exactly two things, neither of which is configuration:

1. **The panel has no power on its own 5 V input** (the HUB75 ribbon carries
   signals, not panel power), or
2. **the drive signals are not reaching the panel** — ribbon seated on the wrong
   header or reversed, pin-1 orientation flipped, or a failed HUB75 buffer on the
   card.

A "software blank" would still be inconsistent with the card's own test button
doing nothing, and §19.1 shows no such blanking command exists on this path.

**The one measurement that discriminates:** put the meter on the *panel's own*
power feed, not the shared supply rail, and press the card's test button. If that
feed reads ~0 A, the panel is unpowered or unconnected and no byte in any config
will change it. If it reads a real quiescent current but nothing lights, the
drive path is suspect and the ribbon/orientation is the next thing to check.

I recognise you moved away from this line after §17, and I am raising it again
only because the white-vs-black invariance is new information since then and it
is the strongest single data point in the set. If that test comes back showing
real panel current, I will drop it and go straight at the OE polarity question
with the remaining leads in §19.3.

---

## 20. Restoring page 0xF0 — the linear-address flash path

Static analysis only. Nothing executed. **This section describes a write path;
read §20.5 before sending anything.**

### 20.1 The path you were missing

There is a **second flash builder with a completely different frame format**:

```
BulidEepromFlashOperation(unsigned int* outLen, unsigned char** outBuf,
                          unsigned short rcvIdx, unsigned char opcode,
                          unsigned int addr32, unsigned char* data,
                          unsigned int datalen)                     @ 0x30bdd0
```

(the vendor's own typo, "Bulid"). Reached from
`CReceiverOP::WriteDataToEepromFlash` @ `0x3b9d60` and
`ReadEepromBuffer` @ `0x3d54c0`.

The decisive difference: **it takes a 32-bit linear byte address**, not the
`(addrHi, addrLo)` 64 KB-block / 256-byte-page pair used by
`BuildRcvCardFlashOperation`. That is why your page-based writes to page 0xF0 were
refused while this path can reach it — the page-based window is bounded, this one
addresses flash directly.

### 20.2 Frame layout (from `0x30be38`–`0x30bea2`)

```
n = max(0x80, datalen + 0x12)
buf = new[n]; bzero(buf, n)

word [buf+0x00] = 0x0019           ; on the wire: 19 00   -> TYPE 0x1900
byte [buf+0x02] = 0
byte [buf+0x03] = rcvIdx >> 8      ; big-endian u16
byte [buf+0x04] = rcvIdx & 0xff
byte [buf+0x05] = opcode
byte [buf+0x06] = addr >> 24       ; 32-bit address, BIG-ENDIAN
byte [buf+0x07] = addr >> 16
byte [buf+0x08] = addr >> 8
byte [buf+0x09] = addr & 0xff
byte [buf+0x0a] = datalen >> 24    ; 32-bit length, BIG-ENDIAN
byte [buf+0x0b] = datalen >> 16
byte [buf+0x0c] = datalen >> 8
byte [buf+0x0d] = datalen & 0xff

; data-attach guard:
r15b = opcode + 0x7b
if (r15b <= 5 && r15b != 2)  memcpy(buf + 0x0e, data, datalen)
```

Header is **14 bytes (0x0e)**; the allocation is `datalen + 0x12`, leaving 4
zero bytes of slack after the data.

### 20.3 Opcodes on this path — same semantics as §13, verified at call sites

| opcode | `+0x7b` | carries data? | meaning |
|---|---|---|---|
| **0x85** | 0x00 | yes | write |
| **0x86** | 0x01 | yes | write (used by `WriteDataToEepromFlash`) |
| **0x44** | 0xBF | no (>5) | read |
| 0x87 | 0x02 | no (excluded) | — |

Sampled call sites confirm both in use: `0x3b3383` and `0x3b5f18` pass `0x85`;
`0x3b3a29`, `0x3b4409`, `0x3b6039` pass `0x44`. So the opcode meanings are
identical to the page-based path — **only the addressing and the type byte
differ.**

### 20.4 The exact frame to restore page 0xF0

Address `0x07F000`, 256 bytes, receiver index 0. Payload = 256 + 0x12 = **274
bytes**; frame = 12 + 274 = **286 bytes**.

```
 0x00  11 22 33 44 55 66      dst MAC
 0x06  22 22 33 44 55 66      src MAC
 0x0c  19 00                  type 0x1900          <- payload[0..1]
 0x0e  00                     payload[0x02]
 0x0f  00 00                  payload[0x03..04]    receiver index 0, BE
 0x11  85                     payload[0x05]        opcode = write
 0x12  00 07 F0 00            payload[0x06..09]    address 0x0007F000, BE
 0x16  00 00 01 00            payload[0x0a..0d]    length 0x00000100 = 256, BE
 0x1a  <256 bytes>            payload[0x0e..0x10d] your backup, verbatim
 0x11a 00 00 00 00            payload[0x10e..0x111] slack, zero
```

**No erase is needed.** The page currently reads `0xff` — already erased — and
writing to erased NOR flash only clears bits. Do **not** issue an erase; a 4 KB
sector erase (`ClearFlashSector4KBEx`) would take neighbouring content with it.

**Verify with the same builder, opcode 0x44**, `addr = 0x0007F000`,
`datalen = 0x100`, no data attached (payload = 0x80 bytes, frame 140):

```
 0x0c  19 00
 0x0e  00 | 00 00 | 44 | 00 07 F0 00 | 00 00 01 00 | 00 x 114
```

If `0x86` is refused, try `0x85` — both are write opcodes on this path and I
cannot tell statically which the firmware prefers for this region. Start with
`0x85`, since it is the opcode you already know this card accepts.

### 20.5 Safety — this path is OUTSIDE your existing guard

The §13.8-D guard allowlists `addrHi == 0x07` in the **page-based** frame. This is
a different frame type with a **32-bit linear address**, so that guard does not
constrain it at all, and a mistyped address here can reach *any* byte of flash
including firmware. Before sending, extend the guard:

1. frame type must be `0x1900`;
2. opcode ∈ {`0x44`, `0x85`, `0x86`};
3. **address clamped to exactly `0x0007F000 .. 0x0007F0FF`** — a hard range check,
   not a prefix test;
4. `datalen == 0x100` for the write, and the payload must be exactly your 256
   backup bytes;
5. dry-run print first, and re-read with opcode `0x44` immediately after.

Note `0x07F000` lies inside block `0x07`, the region you already own and have a
full pre-write image of — so this write stays within territory you can restore.

### 20.6 Item 4 — your own evidence already answers it

You do not need me for this one, and it is worth stating plainly: the card
reports 1024x512 again **after every power cycle**, despite the type-0x02 layout
having been accepted in RAM. That is direct proof that

* the boot path reads the **flash** record, and
* the RAM layout does not persist and does not influence what the card configures
  at boot.

So the type-0x02 command is not the wrong command — it is simply the wrong
*lifetime*. Restoring 0x07F000 is what makes the geometry survive a reboot, and
until it is restored every test has indeed started from a card that believes it
is driving a 1024x512 screen. Your reading of the regression is sound.

### 20.7 Item 3 — I did not decode the page's field layout

I have not traced what writes `0x07F000` field-by-field, so I cannot give you a
schema. What is established:

* offsets 6–7 = `00 80` = 128 and 8–9 = `00 40` = 64, big-endian, matching the
  discovery reply's cols/rows — your identification, and consistent with every
  other 16-bit field in this protocol being big-endian;
* discovery `payload[0x28]` tracking this page is your observation and I have no
  static evidence for or against it.

Since you hold the original 256 bytes, **replay them verbatim** — that is exactly
correct and needs no schema. Decoding the layout only matters if you later want to
change the geometry rather than restore it, and I would not spend effort there
until the panel is lit.

### 20.8 Recommended order

1. Read `0x07F000` with opcode `0x44` — confirms the read path and that it still
   reads `0xff`.
2. Write the 256 backup bytes with opcode `0x85`.
3. Read back and compare byte-for-byte.
4. **Power-cycle**, then run discovery — it should report 128x64 again.
5. Only then re-run the panel tests. Every earlier result was taken from a card in
   the 1024x512 state and should be treated as void.

---

## 21. Building the two type-0x05 packs

Static analysis only. Nothing executed.

### 21.1 The chip pack is SOLVED — it is record 0x84, verbatim, at offset +4

Two halves of the chain, joined:

**Loader** (`LoadBpBufFromBuffer` @ `0x1c8027`–`0x1c8064`) — record 0x84's local
buffer (which holds the record *including* its 4-byte header, §9.1) into the
chip-register sub-object at `OBJ[0xd6d0]`:

```
rax = qword [P + 0xd6d0]
movups [rax + 0x3c] <- [rbp-0x11d04]   ; local+0xF4 = rec84 payload+0xF0
movups [rax + 0x00] <- [rbp-0x11d40]   ; local+0xB8 = rec84 payload+0xB4
movups [rax + 0x10] <- [rbp-0x11d30]   ; local+0xC8 = rec84 payload+0xC4
movups [rax + 0x20] <- [rbp-0x11d20]   ; local+0xD8 = rec84 payload+0xD4
movups [rax + 0x30] <- [rbp-0x11d10]   ; local+0xE8 = rec84 payload+0xE4
```

**Pack builder** (`GetChipCustomPlusParamPack` @ `0x1ea2fb`–`0x1ea334`, §10.2)
— same sub-object back out into the pack:

```
pack+0xB8 <- rax[0x00] ; pack+0xC8 <- rax[0x10] ; pack+0xD8 <- rax[0x20]
pack+0xE8 <- rax[0x30] ; pack+0xF4 <- rax[0x3c]
```

Composing them gives a **constant +4 delta on all five blocks**:

| pack offset | record 0x84 payload offset | delta |
|---|---|---|
| 0xB8 | 0xB4 | +4 |
| 0xC8 | 0xC4 | +4 |
| 0xD8 | 0xD4 | +4 |
| 0xE8 | 0xE4 | +4 |
| 0xF4 | 0xF0 | +4 |

The pack is `0x104` = **260 bytes** = 4 header + 256, and record 0x84's payload is
**exactly 256 bytes**. The five independent blocks all land at +4, and the sizes
match exactly at both ends. So:

```
chip pack payload[0x00] = 0x05          ; type
chip pack payload[0x01] = 0x00
chip pack payload[0x02] = 0x00
chip pack payload[0x03] = 0x01          ; pack sub-index
chip pack payload[0x04 .. 0x103] = record 0x84 payload[0x00 .. 0xFF]   verbatim
```

**Frame = 12 MAC bytes + 260 payload = 272 bytes.** Build it directly from the
record you already hold; no chip library, no `pm_*.dat`, no transformation.

Two honest caveats:

* The `[0x04..0xB7]` half (180 bytes) is *inferred* by composition — I confirmed
  the `[0xB8..0x103]` half instruction-by-instruction, and the +4 delta plus the
  exact 256/256 size match makes the remainder near-certain, but I did not trace
  the vt+0x130 call that fills it (that is the slot I misattributed in §17.3).
  If the pack misbehaves, `[0x04..0xB7]` is where to look first.
* `ExchangeChipRegisterWhenColorChanged` @ `0x1ea370` runs afterwards and permutes
  registers when a colour swap is configured. Your record 0x01 `+0x0d0`-area
  colour-swap byte is 0, so this should be identity — but it is why a swapped
  panel would need the permutation applied.

### 21.2 The basic-param pack — joined table

`payload[0x00]=0x05`, `[0x03]=0x02` (second pack sub-index), `[0x04]=0xA8`.
Everything not listed is **zero** (the caller `bzero`s 0x103 bytes first, §7.3).
`R1+` = record 0x01 payload offset. All 16-bit fields **big-endian** (§7.1).

| pack | size | source | chain |
|---|---|---|---|
| 0x00 | 1 | `0x05` | constant |
| 0x03 | 1 | `0x02` | pack sub-index |
| 0x04 | 1 | `0xA8` | constant |
| 0x05–0x07 | 3 | **R1+0x028..0x02A** | → OBJ+0x94..0x96 → pack |
| 0x08–0x09 | 2 | module W/H bytes | order set by `GetLineDir()`; **see note** |
| 0x0a | 1 | `GetModuleCountInLineDir()` | OBJ+0xd4c0 |
| 0x0b | 1 | `GetRgbSelValue()` | unresolved member |
| 0x0c | 1 | `GetGrayLevel()`; `0x10` if 16-bit gray, `8` if `GetSplitSegment()==0x5c` | |
| 0x0d–0x0e | 2 BE | **`GetScanMode()` = R1+0x020** | the scan denominator (§18) — for you, **16** |
| 0x0f–0x10 | 2 BE | `GetOneScanLen()` | OBJ+0x68 — **from another record** |
| 0x11–0x12 | 2 BE | `GetCardScanLen()` | unresolved |
| 0x14 | 1 | `GetColorSwap()` (+add/or) | OBJ+0xd0 |
| 0x15 | 1 | `0x99` / `0x77` / `0x00` | branch-dependent; try **0x99** |
| 0x16 | 1 | **R1+0x018 bit 1** | → OBJ+0xc0 |
| 0x17 | 1 | **R1+0x018 bit 0** | → OBJ+0xbf |
| 0x22 | 1 | packed: `(a<<2)|(b<<5)` | inputs unresolved |
| 0x23–0x24 | 2 BE | `GetVoidPointCount()` | OBJ+0x6e — another record |
| 0x26 | 1 | **R1+0x03D & 0x0F** | → OBJ+0xb5 |
| 0x27 | 1 | **R1+0x03E** | → OBJ+0xb6 |
| 0x28 | 1 | OBJ+0xb7 | R1+0x03E (dup) |
| 0x2c | 2 | **R1+0x045** | → OBJ+0xc369 |
| 0x38 | 1 | OBJ+0xba | |
| 0x3a | 1 | **R1+0x04F** | → OBJ+0xbc |
| 0x3b | 1 | **R1+0x050** | → OBJ+0xbd (`Get8nsOeEnableInfo`) |
| 0x3d–0x3e | 2 BE | `GetCardScanLen()` / OBJ+0x44 | |
| 0x46 | 1 | **R1+0x24E** | → OBJ+0x74 |
| 0x47 | 1 | **R1+0x24F** | → OBJ+0x76 |
| 0x48 | 1 | OBJ+0x78 | |
| 0x49–0x4a | 2 | OBJ+0xd3c0 ← **R1+0x030** | |
| 0x4a | 1 | `GetModuleInputCount()` | OBJ+0xd4c0 |
| 0x4b | 1 | `GetHubType()` ← **R1+0x058** | your file: **0x10** |
| 0x74–0x82 | 15 | packed bitfields, masks `0x80/0xE0/0xC3/0xFC/0x3F` | **unresolved — zero and sweep** |
| 0x90 | 1 | `0x01` | constant |
| 0x91 | 1 | OBJ+0xdf16 (masked) | |
| 0x94 | 1 | `GetSpMoudleSetting()` & 0x3F | |
| 0x9f–0xa2 | 4 | shift-derived | unresolved |
| 0xa9 | 1 | `GetColorSwap()` | OBJ+0xd0 |
| 0xd8 | 1 | `GetCurrentPercent()`-derived | from the R/G/B floats at **R1+0x0B4/0B8/0BC** (§19.2) |
| 0xdb | 1 | OBJ+0xbe ← **R1+0x191** | |
| 0xe3 | 1 | `GetSumChipCurrent()` | |
| 0xed | 1 | `GetRealGrayLevel()` | |
| 0xf9–0xfb | 3 | OBJ+0xe61e / 0xe620 / 0xe61c | |
| 0xfc–0xfd | 2 | `GetChipOhmValB()` / `GetDeadPixelsCurrentGain()` | |

**Note on 0x08–0x09:** these come from `qword[OBJ+0x68]` (module width at +0x68,
height at +0x6a), and §9.2 proved `OBJ+0x68` is written from a *different record*,
not record 0x01. For a single 128x64 module the values are 128 and 64; the order
depends on `GetLineDir()` (0 in your file), so try `0x80,0x40` first and swap if
the image is transposed.

**Realistic assessment:** the basic pack is **not** fully derivable from the
records — several fields come from computed accessors whose inputs I could not
resolve, and the 0x74–0x82 bitfield block is the largest gap. It is sendable as a
best-effort with zeros in the gaps, and since it is RAM-only you can sweep. But I
would not expect it to be correct first try, and **I would send the chip pack
first and alone**, since that one is complete.

### 21.3 Send sequence, latching, and confirmation

**Order** (`GetParamPacksBasic` @ `0x31f1e0`, in construction order): chip-custom
pack (`[3]=1`) → data-swap → **basic param** (`[3]=2`, `[4]=0xA8`) → void table →
pixel-sequence packs → void-line info → gamma packs. `SendRealTimePacks`
@ `0x32cf40` transmits the vector in order with `usleep` between groups.

**Are the others mandatory?** I cannot prove which are required — the vendor always
sends the full list. The two you asked about are built unconditionally; the gamma
and calibration packs are guarded by feature checks (`IsEnableGammaCalibration`
etc.) and are plausibly optional for first light.

**Delay:** use **≥5 ms** between packs. The observed literals are `usleep(1000)`
in the flash read loop and `usleep(5000)` between flash writes; the real-time path
uses register-loaded delays I did not resolve, so 5 ms is the safe choice.

**Latching (your question 2):** I found **no commit or latch frame** for real-time
packs — nothing follows them in `SendRealTimePacks` but the next pack. The packs
appear to take effect on receipt. In practice: send chip pack → basic pack →
brightness `0x0A` → a `0x55` row + `0x01` sync, and judge by the panel.

**Confirmation (your question 3) — honest answer: I found none.** No discovery
byte, no query, no status word that reports whether real-time parameters were
applied. I looked for this in §17.2 as well and came up empty. The only
card-side "is my state good" candidate remains `ExecutRcvCrcCheck`, whose type
byte is caller-supplied and unrecoverable (§17.2). So there is no closed feedback
loop available — you are, as you feared, judging by the panel.

That makes the current meter your best instrument: **if the chip pack lands and
the drivers begin clocking, panel current should move even with black content.**
That is the signal to watch, and it is a far better discriminator than the dark
face.

### 21.4 On the "card never drives HUB75" hypothesis

I think the user's reading is the most economical explanation left, and §21.1 gives
you the cleanest possible test of it: a complete, first-principles chip pack built
from your own file with no library dependency and no guesswork. If the drivers are
uninitialised, that pack is what initialises them.

If sending it changes nothing — no current movement, no light — then the remaining
candidates are a card-side fault in the HUB75 output stage or a receiver that is
not applying real-time parameters at all, and neither is reachable from this
dylib. At that point I would want to compare against a second E120 or a different
panel before spending more effort on the protocol.

---

## 22. The remaining packs, and where else to look in flash

Static analysis only. Nothing executed.

### 22.1 Item 2 — the "+4 verbatim" pattern is NOT general

Short answer: **no.** The chip pack's `+4` was specific to record 0x84. The
general shape is *constant-offset block copies*, but **the delta differs per pack
and per block**, so each one has to be derived individually. Record 0x84 was the
lucky case where a single delta covered the whole payload.

Worked example — `GetDataSwapEx2ParamPack` @ `0x1ec700`. Pack side:

```
movups pack+0x04 <- P[0xd40c] ; pack+0x14 <- P[0xd41c]
movups pack+0x24 <- P[0xd42c] ; pack+0x34 <- P[0xd43c]
movups pack+0x44 <- P[0xd658] ; pack+0x54 <- P[0xd668]
movups pack+0x64 <- P[0xd678] ; pack+0x74 <- P[0xd688]
```

Loader side (`0x1c83c1`–`0x1c83f6` and `0x1c806a`–`0x1c809b`), converting the
`SRcvParamBasic` stack offsets to record-0x01 payload offsets (`payload = 0x330 -
var - 4`):

```
P[0xd40c] <- rec01 payload+0x19A      P[0xd658] <- rec01 payload+0x206
P[0xd41c] <- rec01 payload+0x1AA      P[0xd668] <- rec01 payload+0x216
P[0xd42c] <- rec01 payload+0x1BA      P[0xd678] <- rec01 payload+0x226
P[0xd43c] <- rec01 payload+0x1CA      P[0xd688] <- rec01 payload+0x236
```

Composing gives **two constant deltas, not one**:

| pack range | record 0x01 payload range | delta |
|---|---|---|
| 0x04 – 0x43 | **0x19A – 0x1D9** (64 B) | −0x196 |
| 0x44 – 0x83 | **0x206 – 0x245** (64 B) | −0x1C2 |

Both verified across four independent 16-byte blocks each. So you can build
those 128 bytes of the data-swap pack directly from record 0x01 today.

**Caveat:** `GetDataSwapEx2ParamPack` does not stop there — it continues into
`GetGammaTable` and `P[0xd3b0]`, so bytes beyond 0x84 carry gamma content I did
not trace. Send it with those zeroed and treat them as sweep territory.

### 22.2 Items 1 and 3 — what I did not get to

I need to be straight about coverage rather than pad this out. Of the packs you
listed I traced **only the data-swap pack** to record offsets. I did **not**
establish:

* `GetVoidTablePack`, `Get[Anti]VoidLineInfoPacks` — sources not traced;
* `GetPixelSequencePacks` — the **chunking rule, per-chunk header and indexing
  for record 0x03 are unresolved.** This is your item 3 and I have no answer. The
  function returns a count via an out-parameter, which is consistent with your
  chunking guess, but I did not decode how a 12 290-byte table is split or how
  each chunk is addressed. I will not guess at a header format for a 12 KB
  transfer;
* the 0x10 and 0x18 packs — I have their type bytes from §2.1 and nothing more;
* which gamma packs are mandatory.

### 22.3 Item 4 — which packs are mandatory: still unprovable

Unchanged from §21.3, and I want to flag it as a structural limit rather than
something more analysis will fix. The vendor builds the full list unconditionally
except for feature-gated gamma/calibration packs. Nothing in the host code
expresses "the card needs X before it will drive output" — that constraint, if it
exists, lives in firmware I cannot read. I can tell you what LEDVISION sends; I
cannot tell you what the card requires.

### 22.4 Other flash regions worth reading — concrete list

This is the most valuable thing in this section, and it is cheap for you. §13.3's
region map came from resolving every `BuildRcvCardFlashOperation` call site to its
literal `addrHi`. Each is a **64 KB block**; byte address = `addrHi × 0x10000`.
You have only ever read block 0x07.

| addrHi | byte address | holds | priority |
|---|---|---|---|
| **0x0b** | 0x0B0000 | **module mapping table** | **high** — mapping is exactly the "which output drives what" class |
| **0x1f** | 0x1F0000 | **driver-chip params** (SC6660/SC6618/XM11202G/ICND2260/3065) | **high** — chip init, our prime suspect |
| **0x1c** | 0x1C0000 | basic-param overflow chunk Two | high — may hold real data |
| **0xd6** | 0xD60000 | basic-param overflow chunk One | high — same |
| 0xe9 | 0xE90000 | multi-module param | medium |
| 0xe7 | 0xE70000 | anti-pixel sequence | medium |
| 0x39 | 0x390000 | route table Ex | medium |
| 0x3b | 0x3B0000 | data remapping | medium |
| 0x1e | 0x1E0000 | factory bright/current param | medium — factory area |
| 0x0a | 0x0A0000 | HDR gamma, ROE multi-bright | low |
| 0x3a / 0x3c | 0x3A0000 / 0x3C0000 | gamma calibration | low |
| 0xd7 / 0xe0 / 0xe5 | — | HLG / XYZ gamma | low |
| 0xe2 / 0xe3 | — | multi-bright basic / gamma | low |
| 0xe8 | 0xE80000 | shutter sync | low |

**Suggested sweep:** read the first 1 KB of blocks **0x0b, 0x1f, 0x1c, 0xd6**
using the §20 linear-address read frame (type `0x1900`, opcode `0x44`, 32-bit
big-endian address, no data attached). All-`0xFF` means never written; real
content means a region we have been ignoring. Widen your address clamp
deliberately to those four ranges, and keep it a **read-only** widening — do not
extend the write allowlist.

Two caveats: this list is what the *host* writes, so a region the card populates
itself (as page 0x07F000 appears to be) would not appear here. And I have no
evidence any of these blocks holds an "output enable" or "connector select" — that
was your hypothesis and I could not confirm it from the symbol names.

### 22.5 Where I think this stands

The chip pack was the strongest card I had: it is the one pack I could build
completely and independently from your own data, and it targets exactly the
mechanism — uninitialised drivers — that best explains zero output. Sending it
changed nothing.

Everything remaining on the protocol side is materially weaker: partially-traced
packs, unresolved chunking, and fields I would be asking you to sweep blind. I can
keep going, and §22.4 is a genuinely cheap next step, but I do not want to imply
the remaining surface is likely to contain the answer just because it is
unexplored.

The strongest untested hypothesis is still the one from §21.4: that the card is
not driving its outputs for a reason that is not configuration at all. The
cleanest discriminator remains a **second card or a second panel** — a swap test
separates "this card's output stage" from "this panel" from "our protocol" in one
measurement, and no amount of further static analysis can do that. If a spare
E120 or any other HUB75 panel is available, I would do that before decoding
`GetPixelSequencePacks`.

---

## 23. Firmware upgrade — gating questions answered

Static analysis only. **Nothing here should be executed as a flash without
reading §23.5 first.**

### 23.1 Item 4 — the variant is NOT determinable from the card. The gate cannot be satisfied this way.

I searched every binary we have for the variant names:

| binary | `PWM` | `Normal` | `LS0allDA` | `Golden` |
|---|---|---|---|---|
| `libCLTDevice.1.dylib` | none | none | none | none |
| `iSet` main binary | none | none | none | none |
| `LedAdmin2.dll` (LEDVISION) | none | none | none | none |

**The software has no concept of firmware variants at all.** `Normal` / `PWM` /
`LS0allDA` are Colorlight *filename conventions* from their release process, not
protocol-visible fields. There is no variant byte in the discovery reply because
there is no variant field anywhere in the stack.

On version numbering: the three files are FPGA **13.39**, **9.53**, **6.69** and
your card reports **10.81**. These are almost certainly **per-variant counters**
(three independent lineages), and 10.81 matches none of them — so your card runs
a build we do not have, and its number alone cannot place it in a lineage. 10.81
could be an older Normal or a newer PWM with equal plausibility.

Version display is via `GetRCVTypeVersionDesp` @ `0x39bab0`, which formats
`%d.%02d` from receiver-info bytes `+0x10/+0x14/+0x18/+0x1c/+0x20` — matching your
`0x0a,0x51` → "10.81". Card *type* dispatches on the first byte (yours `0x64`),
but that identifies the **model**, not the gateware build.

**So the hypothesis cannot be confirmed or killed before flashing** — which was
precisely the condition you set. See §23.6 for the one route that could still
settle it without risk.

### 23.2 Item 3 — golden/backup exists in the protocol, but I cannot confirm your card has it

Real capability flags exist:

* `CRcvUpgradeCmdManager::IsAllRcvHasGoldenUpgrade` @ `0x398ae0`
* `CRcvUpgradeCmdManager::IsAllRcvSupportGoldenUpgrade` @ `0x398b40`

`IsAllRcvHasGoldenUpgrade` reads **byte `+0x10` of each receiver-info element,
stride `0x1c`**, and requires it non-zero for every receiver:

```
cmp byte [rdx + 0x10], 0     ; first receiver
...
lea rdx, [rdx + 0x1c]        ; stride
cmp byte [rdx], 0            ; subsequent
```

Structurally this is consistent with the spec's claim of upgrade redundancy: there
is a **golden image** concept, and the tool checks whether the attached receivers
support it before choosing an upgrade strategy.

Corroborating: in `DoSlowUpgradeRcv` the write **address is not a constant in the
tool** — it comes from `SRcvUpgradeProgramInfo`, i.e. **the card tells the sender
where to write**. That is exactly how an A/B or golden-bank scheme is normally
arranged, and it means the host cannot easily target the wrong bank.

**But:** whether *your* card sets that flag is what the flag reports, and I did
not map receiver-info `+0x10` back to a discovery-reply offset. So I cannot tell
you your card is recoverable. Given you named this "the single most important
thing to understand before writing a byte", the honest answer is: **unresolved.**

### 23.3 Item 5 — there is NO compatibility gate

`CRcvUpgradeCmdManager::LoadUpgradeFile` @ `0x396120` opens the file, seeks to
end, allocates, and reads the whole thing into a buffer. **No header parsing, no
part-number check, no model comparison, no size validation against the card.**

`VerifyRcvInfo` @ `0x395fe0` — despite the promising name — compares *receivers to
each other* (building a key from info bytes `+0x16..+0x19` and requiring all
attached receivers to match), because upgrade is broadcast. It never compares the
file to the card.

`VerifyFileCrc` @ `0x396430` computes a CRC over the file for post-flash
verification, not for compatibility.

**Conclusion: iSet will not stop you flashing an incompatible image.** There is no
safety net in the tool, so any gate has to be one we impose ourselves.

### 23.4 File format — no conversion needed

`LoadUpgradeFile` reads the file **raw**. The 721 024 bytes go to the card exactly
as they sit on disk, including the leading `FF 00` and the ASCII
`Lattice Semiconductor Corporation Bitstream` header. Do not strip the header and
do not attempt an Intel-HEX parse — despite the `.hex` extension these are raw
ECP5 bitstream containers, and the tool treats them as opaque bytes.

### 23.5 The risk I want on the record before you flash

Four independent factors compound here, and I think they argue against flashing
right now:

1. **We cannot confirm the hypothesis** (§23.1). We would be flashing to test a
   theory we have no way to check first.
2. **We cannot confirm recoverability** (§23.2). The golden mechanism exists in
   the protocol; whether your card exposes it is unknown.
3. **There is no compatibility gate** (§23.3). Nothing will refuse a wrong image.
4. **Board model mismatch.** All three files are **E320 PCB6.0/6.1**. Your card is
   an **E120**. The evidence they share firmware is an observation about
   Colorlight's download page, not verified fact. Identical FPGA part
   (`LFE5U-25F-6CABGA256`) does **not** imply identical board pinout — and an ECP5
   bitstream is pinout-specific. Wrong pinout means the FPGA drives the wrong
   physical pins, which is both potentially unrecoverable and potentially
   electrically harmful to the board or the attached panel.

Point 4 is the one I would weigh most heavily. Everything else in this project so
far has been reversible: config writes had a backup, the page-0xF0 erase was
recoverable once we found the linear-address path. **A bad FPGA bitstream may not
be**, and unlike the config work we have no verified backup of the current
gateware.

### 23.6 What would actually settle this — dump the current bitstream first

There is a readback path (`QuickReadRcvUpgradeProgramParam` @ the `0x3a6980` area,
and the linear-address read frame from §20 reaches arbitrary flash). If we can dump
the card's *current* 721 KB gateware, it solves both gating questions at once:

* **It is the backup** — the thing we most lack. With a verified dump, a bad flash
  becomes recoverable by re-flashing the original, which changes the risk
  calculus completely.
* **It identifies the variant empirically.** Diff the dump against all three
  candidate files. Bitstreams for the same design differ far less from each other
  than from a different design; the closest match, and the pattern of differences,
  will tell us which lineage the card is running — answering §23.1 by measurement
  rather than by protocol field.

That is the next thing I would decode, and it is **read-only**, so it costs
nothing but time. If you want, point me at it and I will work out the exact
readback frames and the flash region (item 2, which I did not get to — the
upgrade address comes from the card rather than a constant, so I could not simply
read it off).

**My recommendation: do not flash until we have that dump.** If the dump turns out
to be impossible, then the decision becomes a genuine risk judgement about an
E320 image on an E120 board, and that is your call rather than mine — but you
should make it knowing there is no backup, no compatibility check, and no
confirmed golden bank.

---

## 24. CORRECTION: the type-0x1900 path is an EEPROM accessor, not a flash path

Static analysis only. **This section corrects §20 and explains the failing read.**

### 24.1 Item 3 — your read is not broken; the frame cannot address flash at all

I resolved every call site of `BulidEepromFlashOperation` @ `0x30bdd0` to its
enclosing function. **All seven are EEPROM operations:**

| call site | enclosing function |
|---|---|
| 0x3b3383 | `CReceiverOP::WriteEepromColorGamutCoef` |
| 0x3b3a29 | `CReceiverOP::ReadEepromPowerOffBrightCoef` |
| 0x3b4409 | `CReceiverOP::ReadEepromFullScreenSeamFactorEnable` |
| 0x3b5f18 | `CReceiverOP::WriteEepromCurrentBrightFlag` |
| 0x3b6039 | `CReceiverOP::ReadEepromCurrentBrightFlag` |
| 0x3b4e6f | `CReceiverOP::RealTimeWriteEepromFullScreenSeamFactorEnable` |
| 0x3c0acf | `CReceiverOP::WriteEepromNoInputShowInfo` |

And the addresses and lengths they pass are decisive:

```
ReadEepromCurrentBrightFlag        : addr = 0xFA , len = 1
ReadEepromPowerOffBrightCoef       : addr = 0xF6 , len = 1
ReadEepromFullScreenSeamFactorEnable: addr = 0x76 , len = 1
```

**Byte addresses in the range 0x00–0xFF, one byte at a time.** This is a small
I²C EEPROM on the receiver card — a few hundred bytes — not the 16 MB SPI flash.

That is exactly why your reads return identical data for every address: your
`0x0007F000` is being masked into a tiny address space, so every request lands in
the same place. The frame layout in §20 is *correct*; what was wrong was my
claim about **what device it addresses**. I named it "the linear-address flash
path" on the strength of the symbol name `EepromFlashOperation` without checking
the call sites, and that was a mistake — the same class of error as §15.5 and
§17.3, and I should have applied the lesson.

**Consequence: type 0x1900 cannot read or write SPI flash. It cannot dump the
bitstream.**

### 24.2 An implication you should check on the bench

Your page-0xF0 "restore" went out as type 0x1900, opcode 0x85, `addr=0x0007F000`,
`len=0x100`. If the EEPROM is 256 bytes, that address masks to 0 and **you wrote
your 256 backup bytes over the whole EEPROM**.

It evidently helped — discovery went back to 128x64 — which suggests the
screen-size record is EEPROM-resident and your backup happened to carry the right
values at the right offsets. But it also means **other EEPROM contents may have
been overwritten**: the call sites above show that region holds
`CurrentBrightFlag` (0xFA), `PowerOffBrightCoef` (0xF6) and
`FullScreenSeamFactorEnable` (0x76) among others.

Worth reading those three addresses back (opcode `0x44`, `len=1`, addresses
`0xFA`, `0xF6`, `0x76`) and sanity-checking them. They are brightness/seam flags,
so a clobbered value there is *another* candidate for a dark panel — and unlike
the gateware theory it is cheap to check right now.

### 24.3 Items 1 & 2 — the right tool is the page-based SPI read you already have

The path that genuinely reads SPI flash is the one from §12, which you have
already used successfully to dump all 64 KB of block 0x07:

```
type 0x0600, opcode 0x44, addrHi = block, addrLo = page, 1024-byte chunks
```

That addressing is `byte = addrHi*0x10000 + addrLo*0x100`, so with a one-byte
block index it reaches **16 MB** — the whole device, including wherever the
bitstream lives. No new frame format is needed.

**Finding the bitstream: scan for the Lattice magic.** Every one of the three
candidate files begins with:

```
FF 00 4C 61 74 74 69 63 65 20 53 65 6D 69 63 6F 6E 64 75 63 74 6F 72
      ^  L  a  t  t  i  c  e     S  e  m  i  c  o  n  d  u  c  t  o  r
```

So read **page 0 of each block, `addrHi = 0x00 .. 0xFF`, `addrLo = 0x00`**, 1 KB
each, and look for `FF 00 4C 61 74 74 69 63 65`. That is 256 read requests, all
read-only, and it will locate the bitstream region — and very likely a **second
copy**, which would be the golden bank and would answer §23.2 by observation.

Two expectations to calibrate against:

* A bitstream is `721024` bytes = `0xB0000` = **11 blocks**. So expect a run of
  ~11 consecutive blocks, and if golden exists, a second such run.
* It is **not** at block 0x00–0x0A, because your config occupies block 0x07 and
  the region map (§13.3) puts other parameters at 0x0A and 0x0B. Look higher —
  the large unmapped ranges are 0x0C–0x1B, 0x20–0x38, 0x3D–0xD5 and 0xEA–0xFF.

On `SRcvUpgradeProgramInfo` (asking the card where its firmware lives): I did not
decode it. Given the block scan is cheap, read-only, and answers the same question
by direct observation, I would do that first and only come back to the struct if
the scan is inconclusive.

### 24.4 Item 4 — not mapped, and I want to be clear about that

I did not map receiver-info `+0x10` back to a discovery-reply offset. The
receiver-info array is built by code I have not traced, and I am not going to
guess an offset into your 1056-byte payload — a wrong answer here would send you
looking at the wrong byte to decide whether a flash is recoverable.

If the block scan in §24.3 finds **two** bitstream-sized regions, that is direct
physical evidence of a golden bank and is worth more than the capability flag
anyway.

### 24.5 On the SM16380SC / OE-as-grayscale-clock note

I looked and found **nothing** in the upgrade code, or anywhere else in
`libCLTDevice`, that selects OE behaviour or a GCLK mode. That is consistent with
your research thread's conclusion rather than contradicting it: if OE free-running
as the grayscale clock is a property of the *gateware's* output state machine,
there would be no host-side field for it — which is precisely why no configuration
we send can fix it, and why the variant question matters.

I flag it as consistent-but-unconfirmed. I have no static evidence either way, and
§19.3's OE polarity question remains unresolved for the same reason: those
accessors bottom out in a chip-library sub-object I could not resolve.

### 24.6 Suggested order

1. Read EEPROM addresses `0x76`, `0xF6`, `0xFA` (type 0x1900, opcode 0x44,
   len 1) and check them against sane values — cheap, and a clobbered brightness
   flag is an alternative explanation for the dark panel.
2. Block-scan SPI flash for the Lattice magic (type 0x0600, opcode 0x44,
   `addrHi = 0x00..0xFF`, `addrLo = 0x00`). Read-only.
3. Dump whichever region(s) match — that is the backup, and the variant answer by
   diff.

---

## 25. Firmware flash — implementable spec

Static analysis. Confidence marked per item as requested.

### 25.0 Where the code lives (CONFIRMED)

`FwUpgrade2.dll` exports exactly **one** symbol, `CreateHwUpgrade`, and imports
exactly **one** function from `CLTDevice.dll`:
`GetHwDeviceManager()` → `IDeviceManager*`. It is a UI/orchestration layer; **all
frame construction happens in CLTDevice**, the Windows twin of the
`libCLTDevice.1.dylib` analysed throughout this document. So the dylib analysis
*is* the analysis of the flashing tool — the "two independent implementations"
you wanted to cross-check are the same implementation behind two front-ends.

### 25.1 Item 1 — the upgrade frame (HIGH confidence on layout, MEDIUM on field roles)

Builder: **`BuildRcvCardFlashOperationEx` @ `0x30b8e0`** — note this is the *Ex*
variant, distinct from the config-path builder, and its **type byte is
caller-supplied** rather than computed:

```
n = max(0x80, datalen + 0xa)
buf = new[n]; bzero
byte [buf+0x00] = arg4              ; TYPE BYTE  -> 0x26 at every upgrade call site
word [buf+0x01] = 0
byte [buf+0x03] = rcvIdx >> 8       ; big-endian
byte [buf+0x04] = rcvIdx & 0xff
byte [buf+0x05] = arg5              ; OPCODE
byte [buf+0x06] = stack arg (+0x18) ; 0 at the data-carrying sites
byte [buf+0x07] = arg6 (r9d)        ; address HIGH  (block)
byte [buf+0x08] = stack arg (+0x10) ; address LOW   (page)
byte [buf+0x09] = 0
memcpy(buf + 0x0a, data, datalen)   ; guarded on opcode
```

**Data-carrying call sites** (`0x3a4d36`, `0x3a4e38` in `DoSlowUpgradeRcv`) pass:

```
type    = 0x26
datalen = 0x100                      ; 256 bytes per chunk   <- CONFIRMED, literal push 0x100
data    = rbx                        ; firmware buffer + offset
payload[6] = 0
```

So the on-wire frame is **payload 0x10A = 266 bytes, frame = 12 + 266 = 278
bytes**, type word `26 00`.

**Write opcode (HIGH confidence):** at `0x3a4c05`–`0x3a4c12`:

```
mov eax, 0x85
mov ecx, 0x62
cmove ecx, eax        ; selected by a capability bit (dil & 4)
mov [var_9ch], ecx    ; -> feeds payload[5]
```

So the opcode is **`0x62`**, or **`0x85`** when the card advertises the
capability bit. `0x85` is the same write opcode as the config path (§13.4), which
is a reassuring cross-check. **Try `0x62` first**; it is the default branch.

**Delays (CONFIRMED, literal `usleep` arguments):**

| site | value | meaning |
|---|---|---|
| `0x3a4c4f` | `0x88B8` = **35 000 µs = 35 ms** | in the erase loop |
| `0x3a4e9f` | `0x3E8` = **1 000 µs = 1 ms** | between data chunks |
| `0x3a4ff7` | `0x7530` = **30 000 µs = 30 ms** | at completion |

**Erase step (MEDIUM):** a loop at `0x3a4c37`–`0x3a4c74` calls a device vtable
method `[rax+8]`, iterating `var_c8h` times with the 35 ms delay between
iterations. I did **not** resolve that vtable slot to a named function (see the
repeated vtable-misattribution problem in §15.5/§17.3/§24.1), so I cannot give
you the erase frame bytes. Given the erase count is a variable, it is very likely
one erase per 64 KB block over the image span.

**Completion (LOW):** I found no explicit completion frame — the sequence ends
with the 30 ms delay. `VerifyFileCrc` @ `0x396430` exists for post-flash
verification, and re-reading the region with the page-based read you already have
is the verification I would actually trust.

### 25.2 Item 2 — target address (MEDIUM-HIGH)

`payload[7]/[8]` are the **same page addressing as the config path**:
`byte address = payload[7] * 0x10000 + payload[8] * 0x100`.

At `0x3a4875`–`0x3a4878` the low byte is computed as
`(word[rbx+0x12] + chunk_index) & 0xff` — i.e. **a base page from card-supplied
info plus a running chunk counter**. So the tool asks the card for the base and
walks forward one 256-byte page per chunk.

**For your case:** you have measured the banks directly — primary at block 0x00,
golden at block 0x20. A 721 024-byte image is 0xB0000 = **11 blocks**, so the
primary occupies blocks **0x00–0x0A** and the golden **0x20–0x2A**.

**Write the primary only: `payload[7]` must stay in `0x00..0x0A`.** That leaves
golden as your in-hardware fallback, which is exactly what you want. Do not let
the address reach 0x20.

### 25.3 Item 3 — image sent verbatim (HIGH)

`LoadUpgradeFile` @ `0x396120` opens, seeks to end, allocates, reads the whole
file, and returns the buffer. **No header parse, no conversion, no stripping.**
The 721 024 bytes go out as-is including the leading `FF 00` and the ASCII
Lattice header — chunked into 2 816 frames of 256 bytes (`0xB0000 / 0x100`).

I did **not** examine the `.fw` container in `UpgradePack` (no E-series samples,
and the raw `.hex` path is what applies to us). If the card expected a `.fw`
wrapper, `LoadUpgradeFile` would have to parse one, and it does not — so raw is
right for our files.

### 25.4 The E120/E320 type question (IMPORTANT — read this)

I found the packed model table and enumerated it. **E120 and E320 are separate
entries** — the software distinguishes them as distinct card types:

```
... i7+ , E320P , E320 , K5+ , i9+ , E80 , K9+ , RI17 , ...
... K8S , E200 , E260 , GST32 , ... , RI21 , E120 , K8 , N6s , ...
```

**I could not reliably derive the numeric type for E120.** Anchoring the table
against `RcvPackInfo.xml` is inconsistent: one alignment makes `K8 = 101`
(matching the XML exactly) and would put `E120 = 100` — matching your card's
reported `0x64` — but the same alignment puts `E200/E260` two off from their XML
values. That is over-fitting on a coincidence, so **treat "type 100 = E120" as
plausible but unconfirmed.**

What this does *not* settle is gateware compatibility, and here your physical
evidence is much stronger than anything in the model table: identical
`Design name`, `Part`, `Rows`/`Cols`/`Bits` and header CRC across your dump and
the E320 files means it is the same design compiled at different dates. A
distinct *model type number* is a product/SKU distinction, not necessarily a
pinout distinction.

### 25.5 Item 4 — minimum viable procedure and guards

**Must happen, in order:**

1. Verify both bank dumps are on disk and their Lattice headers parse. You have
   this.
2. Confirm the target image size is exactly 721 024 bytes.
3. Erase the primary span (blocks 0x00–0x0A), 35 ms between erases.
4. Write 2 816 chunks of 256 bytes, `payload[7]:[8]` walking `0x0000` → `0x0AFF`,
   1 ms between chunks.
5. 30 ms settle.
6. **Read the region back with the page-based read and byte-compare against the
   file before power-cycling.** This is the step I would not skip — it is
   read-only, you already have the tooling, and it is the difference between
   "probably flashed" and "verified flashed".
7. Power-cycle.

**Must never happen — hard-code these:**

* `payload[7]` **outside `0x00..0x0A`**. Reaching `0x20..0x2A` destroys the golden
  bank, which is the one thing that makes this recoverable.
* Any write with type ≠ `0x26` on this path.
* Any erase whose count could exceed the 11-block span.
* Any `0x1900` frame during the flash — that is the EEPROM (§24.1) and has no
  business in this sequence.

**The failure that cannot be walked back:** the card stops answering Ethernet. That
happens if the *running* gateware is destroyed and the golden bank cannot take
over. Two implications: (a) never touch block 0x20–0x2A, and (b) if the card
boots from the primary unconditionally rather than falling back on a bad CRC,
then a half-written primary is fatal regardless of golden. **I could not determine
the boot/fallback rule** — it is in the bootloader, not in any host-side code —
so the golden bank is *probable* but not *proven* insurance.

Given that, the sequencing that minimises exposure is: erase and write in one
uninterrupted run, on a wired link with nothing else on the interface, on a
machine that will not sleep, with the panel's supply stable. The window of
vulnerability is between the first erase and the last verified chunk.

### 25.6 Confidence summary

| item | confidence |
|---|---|
| Frame layout (offsets, type 0x26, 256-byte chunks, 278-byte frame) | **high** |
| Write opcode 0x62 / 0x85 | **high** |
| Delays 35 ms / 1 ms / 30 ms | **high** (literal immediates) |
| Address = `payload[7]*0x10000 + payload[8]*0x100` | **medium-high** |
| Primary at blocks 0x00–0x0A, golden 0x20–0x2A | **high** (your measurement) |
| Image sent raw, header included | **high** |
| Erase frame bytes | **not determined** |
| Completion signalling | **not determined** — verify by readback |
| Boot/fallback rule (is golden real insurance?) | **not determined** |
| "type 100 = E120" | **plausible, unconfirmed** |

---

## 26. THE UNLOCK FRAME — `BuildHwProgramWritable2`

This is the missing step. Static analysis; confidence **high** — the layout is
five literal instructions and the call sites bracket the whole upgrade.

### 26.1 The frame

`BuildHwProgramWritable2(unsigned int* outLen, unsigned char** outBuf,
unsigned short rcvIdx, bool enable)` @ **`0x30aad0`**:

```
buf = new[0x80]; zeroed from +6 onward
word [buf+0x00] = 0x0023        ; on the wire: 23 00   -> TYPE 0x2300
byte [buf+0x02] = 0
byte [buf+0x03] = rcvIdx >> 8   ; big-endian
byte [buf+0x04] = rcvIdx & 0xff
neg  r14b                       ; r14b = the `enable` bool
byte [buf+0x05] = r14b          ; enable=1 -> 0xFF ;  enable=0 -> 0x00
outLen = 0x80
```

`neg` on a byte: `neg(1) = 0xFF`, `neg(0) = 0x00`. So **payload[5] is `0xFF` to
unlock and `0x00` to re-lock** — not 0x01.

**Payload 0x80 = 128 bytes, frame = 12 + 128 = 140 bytes.**

```
 0x00  11 22 33 44 55 66      dst MAC
 0x06  22 22 33 44 55 66      src MAC
 0x0c  23 00                  type 0x2300
 0x0e  00                     payload[2]
 0x0f  00 00                  payload[3..4]  receiver index, BE
 0x11  FF                     payload[5]     0xFF = UNLOCK, 0x00 = RELOCK
 0x12  00 x 122               payload[6..0x7f]
```

Sent via the same path and reply selector as everything else (`edx = 0x807d`,
`ecx = 0`, `r9d = 0` at `0x3a46f2`).

### 26.2 Why this is the answer to your bench result

`BuildHwProgramWritable2` has exactly three callers, and two of them bracket the
upgrade:

| call site | argument | meaning |
|---|---|---|
| `0x3a46bc` — **early in `DoSlowUpgradeRcv`** | `mov ecx, 1` | **unlock** (payload[5]=0xFF) |
| `0x3a4faa` — **late in `DoSlowUpgradeRcv`** | `xor ecx, ecx` | **re-lock** (payload[5]=0x00) |
| `0x3a5054` | — | `CReceiverOP::SendEnableHwProgramWritable` (the public API) |

The early call happens **before any erase or data frame**. That is precisely the
"step before the data frames" you deduced must exist, and its name —
*HwProgramWritable* — says exactly what it does: it makes the hardware program
region writable.

Your bench result is now fully explained: reads work everywhere, writes to the
parameter window work because that window is not protected, and writes to
`0x00–0x0A` are refused because the program region is **write-protected until
unlocked**. No opcode you tried could have worked without it.

(Note: the sibling `BuildHwProgramWritable` @ `0x30aa40`, type **0x2000**, is
byte-identical in layout but has **no callers** in this dylib — it targets a
different device class. Use `0x2300`.)

### 26.3 Item 3 — the opcode capability bit

At `0x3a4c01` the selector is `test dil, 4` where `edi = dword [rbp-0x5c]`, and at
`0x3a46ce` that slot is loaded from `r14d` — an argument passed into
`DoSlowUpgradeRcv` by its caller, ultimately out of `SSlowUpgradeRcvParam`. It is
**bit 2 (value 4)** of that word.

I did **not** trace it back to a discovery-reply offset, so I cannot tell you which
byte to read on your card. But the branch is unambiguous:

* bit clear → opcode **`0x62`** (the default / `cmove`-not-taken path)
* bit set → opcode **`0x85`**

Since you can now test in seconds, try `0x62` first **with the unlock applied** —
you tried it without, which is why it failed.

### 26.4 The complete ordered sequence

Confidence: unlock/relock **high**; chunking and delays **high**; erase **still
undetermined**.

```
1.  UNLOCK          type 0x2300, payload[5] = 0xFF          140-byte frame
2.  (erase)         see 26.5 — try skipping first
3.  for chunk i in 0 .. 2815:
        type 0x2600, opcode 0x62 (or 0x85),
        payload[7] = block, payload[8] = page,   page walks 0x0000..0x0AFF
        payload[0x0a..0x109] = 256 bytes of image
        278-byte frame
        usleep 1000                              ; 1 ms
4.  usleep 30000                                 ; 30 ms
5.  RELOCK          type 0x2300, payload[5] = 0x00          140-byte frame
6.  read back with type 0x0600 / opcode 0x44 and byte-compare
7.  power-cycle
```

### 26.5 Item 2 — the erase, and a cheap experiment

I still have not resolved the erase vtable call at `0x3a4c49` (`call [rax+8]`,
35 ms between iterations) — it is the same class of vtable indirection I have
misattributed three times, and I will not guess at frame bytes for an erase
aimed at the firmware region.

**But you can now settle it empirically in one test, safely:**

> Send **unlock (0x2300, 0xFF)**, then a single 0x2600 data write to
> **block 0x03 page 0x50** — the same target you have been using, which reads
> `0xFF` and is outside both bitstream banks — then read back.

* If the page takes the data: the unlock was the only missing piece, and **no
  erase is needed for already-erased flash** (writing to `0xFF` NOR only clears
  bits, exactly as in §20.4). That also means for the real flash you need an erase
  only because blocks 0x00–0x0A currently hold the old bitstream.
* If it still refuses: the erase is also a gate, and I will go back at the vtable
  slot with that knowledge.

Block 0x03 remains the right test target — it is not the running bitstream
(0x00–0x0A holds it, but 0x03 specifically reads 0xFF per your scan) and not
golden (0x20).

### 26.6 Item 4 — handshake / display state

I found **no** stop-display, maintenance-mode, or reset frame in either upgrade
path. The sequence in `DoSlowUpgradeRcv` goes straight from unlock into the
erase/write loop. The card-selection builders you asked about
(`BuildRcvCardParamSelectionPack` @ `0x30d3a0`, `BuildRcvCardIsSelectedInfo`
@ `0x30cab0`) are for choosing *which* receiver in a chain to target — relevant
for multi-card installs, not for a single card at index 0.

So: no handshake, no mode change, no display stop. **Unlock is the whole gate**,
as far as the host-side code shows.

### 26.7 Safety note that now matters more

With the unlock frame in hand, the write protection that has been silently
protecting you is gone. Two consequences:

* Send the **relock (`0x00`)** whenever you finish, including after a failed or
  aborted attempt. Leaving the program region unlocked means any subsequent
  addressing bug can reach the bitstream.
* The §25.5 guard on `payload[7] ∈ 0x00..0x0A` becomes load-bearing rather than
  belt-and-braces. Golden at 0x20–0x2A is your only fallback and there is nothing
  else stopping a stray write from reaching it.

---

## 27. The upgrade-descriptor query (read-only) — request and reply decoded

Static analysis. Confidence: request frame **high**; reply field offsets
**medium-high** (see §27.4 for the empirical anchor that makes this robust).

### 27.1 The request frame

`QuickDetectRcvUpgrade` @ `0x3a2f70` calls
`BuildQucikDetectRcvCardEx(&len, &buf, 9, ptr, 2)` @ `0x30a780`, which builds:

```
buf = new[0x110]; bzero(buf+0x0d, 0x103)
*(u64*)(buf+0x00) = 0x0001FFFFFF000007
*(u32*)(buf+0x08) = 0x97835743
byte [buf+0x0c] = arg3            ; = 9 at this call site
memcpy(buf+0x0d, arg4, min(arg5,0x103))   ; 2 bytes
outLen = 0x110
```

Little-endian expansion gives the wire bytes. **Payload 0x110 = 272 bytes,
frame = 12 + 272 = 284 bytes** (same size as ordinary discovery):

```
 0x00  11 22 33 44 55 66      dst MAC
 0x06  22 22 33 44 55 66      src MAC
 0x0c  07 00                  type 0x0700   (discovery family)
 0x0e  00                     payload[0x02]
 0x0f  FF FF FF               payload[0x03..0x05]   <- distinguishes it from plain discovery
 0x12  01 00                  payload[0x06..0x07]
 0x14  43 57 83 97            payload[0x08..0x0b]   <- magic 0x97835743 (LE)
 0x18  09                     payload[0x0c]         <- sub-command 9
 0x19  00 00                  payload[0x0d..0x0e]   <- 2 caller bytes; try 00 00
 0x1b  00 x 241               payload[0x0f..0x10f]
```

It is a **discovery-family frame (type 0x0700)** with `FF FF FF` at payload[3..5]
and the magic at payload[8..0x0b] marking it as the upgrade-info variant. Reply
selector at the send site is `0x8081`.

Read-only: it carries no data and no write opcode.

### 27.2 The reply layout

The parse loop (`0x3a327d`–`0x3a32a0`):

```
rax   = replyBuffer + 0x26      ; anchor
count = replyLen >> 0xB         ; i.e. replyLen / 2048
per receiver: rax += 0x800      ; 2048-byte record per receiver
```

So **each receiver occupies 0x800 = 2048 bytes** in the reply, and for receiver
`i` the anchor is `replyBuffer + 0x26 + i*0x800`. Fields, as offsets from the
**reply buffer** for receiver 0:

| reply offset | read as | → descriptor | meaning |
|---|---|---|---|
| **+0x0D** | byte | — | validity; **bit 1 must be set** or the record is skipped |
| +0x1D | word | +0x0d | |
| **+0x1F** | byte | +0x0f..+0x12 | **capability bits** (see below) |
| +0x20 | byte | +0x13 | |
| +0x22 | word | +0x14 | |
| **+0x24** | word | +0x16 | **declared length, high** |
| **+0x26** | word | +0x18 | **declared length, low** |

**Capability bits — all from the single byte at reply `+0x1F`:**

| bit | descriptor | meaning |
|---|---|---|
| 0 | +0x0f | supports-SDRAM-staging |
| **1** | **+0x10** | **has-golden-upgrade** ← what `IsAllRcvHasGoldenUpgrade` reads |
| 2 | +0x11 | supports-select-part |
| **3** | **+0x12** | **supports-golden-upgrade** |

So **one byte at reply `+0x1F` answers your golden question**: bit 1 set means the
card has a golden image; bit 3 means it supports golden upgrade.

### 27.3 The declared program length

`VerifyFileCrc` assembles `desc[0x16]<<16 | desc[0x17]<<8 | desc[0x18]`, and
`desc[0x16..0x19]` are the two words from reply `+0x24` and `+0x26`. Since the
words are stored little-endian into the descriptor, the length bytes appear on the
wire **most-significant first** at reply `+0x24`, `+0x25`, `+0x26`.

**Concrete predictions for your two candidate formats:**

| card declares | reply `+0x24 +0x25 +0x26` |
|---|---|
| `0x0B0000` (PWM 9.53 / LS0allDA 6.69 family) | `0B 00 00` |
| `0x0B0080` (Normal 13.39 family) | `0B 00 80` |

That is the decisive read. If the card declares `0x0B0080`, the PWM file is
structurally invalid for it and — as you say — the question closes without a
single write.

### 27.4 How to make this robust to my offsets being slightly off

My `+0x26` anchor is derived from `mov rax, [rbp-0x10050]; add rax, 0x26`, where
that buffer is the one handed to the send/collect call. Whether the framing layer
strips an Ethernet header before it (as it did in §12, where data began at
`recvBuf+0x0F`) I did not re-verify here.

**So anchor empirically instead:** dump the whole reply and **search for the byte
pattern `0B 00`**. A 24-bit length of ~721 KB has `0x0B` as its high byte, and
`0x0B 0x00` will be rare in an otherwise sparse reply. Once you find it:

* the byte *after* it is the low length byte (`00` → 0xB0000, `80` → 0xB0080);
* the capability byte is **5 bytes before** the `0x0B` (`+0x24 − 5 = +0x1F`);
* the validity byte is **0x17 bytes before** it.

That anchoring holds regardless of any constant header offset, because all the
field spacings come from literal instruction offsets and are certain.

### 27.5 On the unlock frame not working

Accepted — and your test was sound. Checking your description against §26: you
wrote "payload[3]=0xFF", and if your indexing starts after the 2-byte type word
then your payload[3] is wire offset 0x11, which is exactly where my payload[5]
sits. So you sent the right byte in the right place and it still refused.

That leaves the erase as the more likely remaining gate — which is what I
suspected in §26.5 and could not resolve. Given §27.3 may close the question
outright, I would spend the next read on the descriptor query before I spend more
effort on the write path.

### 27.6 On E120 ≠ E320

Your jump-table anchor at `0x39c41c` — index `(payload[0]+0x78)&0xFF`, entry 220
for `0x64` → "E120", with `E320`/`E320P` as separate entries — is a much better
derivation than my attempt in §25.4, which I flagged as over-fitted and
unconfirmed. I agree with your conclusion and would treat §25.4's "type 100 =
E120" as superseded by yours.

That does raise the risk of the three files materially, and it makes the
descriptor query more valuable still: a declared length that matches only one of
the two format families is evidence about what this card's bootloader expects,
independent of the SKU question.

---

## 28. The upgrade path, mapped end to end

This supersedes the fragments in §25 and §26. Confidence is marked per item;
anything I could not resolve is stated as such rather than omitted.

### 28.0 There are TWO upgrade strategies, and they are structurally different

| | **Quick** | **Slow** |
|---|---|---|
| entry | `DoQuickUpgradeRcv` @ `0x3a3610` | `DoSlowUpgradeRcv` @ `0x3a4600` |
| mechanism | **stages the image into the card's SDRAM**; the card programs its own flash | **host writes flash directly**, frame by frame |
| builder | `BuildSDRAMOperation` @ `0x30cd70`, type **0x1A00** | `BuildRcvCardFlashOperationEx` @ `0x30b8e0`, type **0x2600** |
| chunk | **1024 bytes** | 256 bytes |
| variants | `DoQuickUpgradeRcvBackup` @ `0x3a3e60`, `DoQuickUpgradeRcvNoBackup` @ `0x3a41a0` | single path |
| gate | `IsAllRcvSupportSDRAM` @ `0x395050` (descriptor `+0x0f`, reply bit 0 — §27.2) | fallback |

**This is the most important thing in this section.** The Quick path never writes
flash from the host at all — it uploads the image to card RAM and the card's own
firmware performs the programming. That means the *card* chooses the bank, applies
its own protection rules, and can validate before committing. It is
architecturally the safer path and it is the one the tool prefers when the card
advertises SDRAM support.

The Slow path is the fallback for cards without SDRAM staging, and it is the one
I specced in §25 — which is also the one whose frames your card rejected.

### 28.1 The SDRAM staging frame (Quick path) — type 0x1A00

`BuildSDRAMOperation(uint* outLen, uchar** outBuf, ushort rcvIdx, uchar mode,
int, uint addr, uchar* data, uint len, uchar, bool, bool)`:

```
n = (mode == 1) ? max(0x400, len) + 0x0e : 0x80
buf = new[n]; bzero
word [buf+0x00] = 0x001A          ; wire: 1A 00  -> TYPE 0x1A00
byte [buf+0x02] = 0
byte [buf+0x03] = rcvIdx >> 8     ; big-endian
byte [buf+0x04] = rcvIdx & 0xff
byte [buf+0x05] = mode
if (mode == 1) {                  ; data-carrying
    byte [buf+0x06] = 0
    byte [buf+0x07] = addr >> 16  ; 24-bit SDRAM address, BIG-endian
    byte [buf+0x08] = addr >> 8
    byte [buf+0x09] = addr
}
if (mode == 0x0a) dword [buf+0x06] = 1
byte [buf+0x0a] = len >> 24       ; 32-bit length, BIG-endian
byte [buf+0x0b] = len >> 16
byte [buf+0x0c] = len >> 8
byte [buf+0x0d] = len
if (mode == 1) memcpy(buf + 0x0e, data, len)
```

Header **0x0e = 14 bytes**. Data frame with 1024-byte chunk → payload
`0x40e = 1038`, frame **1050 bytes**. Reply selector `0x807d`.

`DoQuickUpgradeRcvBackup` issues three `BuildSDRAMOperation` calls with a
1024-byte chunk loop (`ebx = 0x400` at `0x3a401d`) — plausibly *begin*, *data*,
*commit* with different `mode` values, but **I did not resolve which mode value
each call uses**, so I cannot give you the begin/commit bytes. That is the single
biggest remaining gap in this section.

### 28.2 The Slow path, in exact order

From `DoSlowUpgradeRcv` @ `0x3a4600`, every call in address order:

| # | address | what | frame? |
|---|---|---|---|
| 1 | `0x3a464b` | `[rax+0xa8]` — fills 16 bytes, result XORed with `0x3163` | **no** — local licence/auth check |
| 2 | `0x3a46bc` | `BuildHwProgramWritable2(enable=1)` → send `0x3a470a` | **UNLOCK**, type 0x2300, payload[5]=0xFF |
| 3 | `0x3a479b` | `[rax+0x50]` with `esi=0x1e` | **no** — progress/timing |
| 4 | `0x3a48bd` | `BuildRcvCardFlashOperationEx`, 4 zero stack args → send `0x3a48f8` | type 0x2600, **no data** |
| 5 | `0x3a49de` | `BuildRcvCardFlashOperationEx`, 4 zero stack args → send `0x3a4a1c` | type 0x2600, **no data** |
| 6 | `0x3a4ae3`, `0x3a4b80` | `usleep` | |
| 7 | `0x3a4c62` | `usleep(0x88B8)` = **35 ms**, in a loop | erase pacing |
| 8 | `0x3a4ce9` | `[rax+0x50]` with `esi=0xf` | **no** — progress/timing |
| 9 | `0x3a4d36`, `0x3a4e38` | `BuildRcvCardFlashOperationEx`, `push 0x100` + data ptr | type 0x2600, **256-byte data** |
| 10 | `0x3a4ea4` | `usleep(0x3E8)` = **1 ms** | between chunks |
| 11 | `0x3a4f17` | `usleep` | |
| 12 | `0x3a4faa` | `BuildHwProgramWritable2(enable=0)` → send `0x3a4fdf` | **RELOCK**, payload[5]=0x00 |
| 13 | `0x3a4ffc` | `usleep(0x7530)` = **30 ms** | |

Steps 4 and 5 are the no-data frames that most likely correspond to the erase —
they sit between the unlock and the data loop and carry no payload. **They are
type 0x2600, which your card rejects**, so this is consistent with your finding
that the vendor's Ex frames do not work here while the ordinary config-path
frames do.

**Ex frame layout** (`0x30b8e0`), for completeness:

```
byte [buf+0x00] = arg4   ; type byte, 0x26 at every upgrade site
word [buf+0x01] = 0
byte [buf+0x03] = rcvIdx >> 8
byte [buf+0x04] = rcvIdx & 0xff
byte [buf+0x05] = opcode
byte [buf+0x06] = stack arg (+0x18)
byte [buf+0x07] = arg6
byte [buf+0x08] = stack arg (+0x10)
byte [buf+0x09] = 0
memcpy(buf+0x0a, data, len)
```

Note the field roles differ from the config-path frame — in the Ex frame the
address bytes are at **[7] and [8]**, not [5] and [6]. That, plus the different
type byte, is why none of your 0x2600 attempts landed.

### 28.3 Item 2 — which bank: the address comes from the CALLER

**Answer: neither hard-coded nor queried inside the upgrade function.** In
`DoSlowUpgradeRcv` the destination is built from
`word [SSlowUpgradeRcvParam + 0x12]` — a **base supplied by the caller** — plus a
running counter:

```
0x3a485a  movzx edx, word [rbx + 0x12]   ; rbx = &SSlowUpgradeRcvParam
0x3a485e  add   edx, r15d                ; r15d = counter, starts at 0 (0x3a4840)
0x3a4875  movzx eax, dl
0x3a4878  mov   [var_b0h], eax           ; -> one address byte
0x3a487e  mov   [var_84h], r15d          ; -> the other address byte
```

So the bank decision is made **above** this function, by
`CRcvUpgradeCmdManager` from the upgrade-descriptor data (§27), and passed down.
I traced it to the parameter struct and no further — **I did not resolve which
value the manager puts there**, so I cannot tell you "the vendor writes 0x200000"
or "the vendor writes 0x000000". That is an honest gap, and it is the question you
most wanted answered.

What the code *does* tell you: the Quick path's split into **Backup** and
**NoBackup** variants is the bank choice made explicit. A tool that has a
"backup" upgrade variant is one that knows about two banks and can target either.

Your inference that the card must boot from golden — because the config at
0x070000–0x07AFFF sits inside the primary image's address range and would corrupt
it — is sound and I have nothing in the host code that contradicts it.

### 28.4 Item 3 — the protected boot region: the host never unlocks it

I searched the full builder set for any second-level unlock, sector-protection
register write, or alternate opcode for low blocks. **There is exactly one
protection primitive in the entire library:** `BuildHwProgramWritable` (type
0x2000, no callers) and `BuildHwProgramWritable2` (type 0x2300, the receiver one).
Both take a single `bool` and set one byte. There is no address range, no sector
mask, no second level.

**Conclusion: the vendor has no mechanism to write blocks 0x00–0x02.** Combined
with your hardware finding that those blocks refuse erase even when unlocked, the
192 KB boot region is enforced by the card and the host tooling never touches it.

The implication you drew is right: **only the tail of the image is updatable.**
Whatever lives in 0x00–0x02 is fixed for the life of the card.

### 28.5 Item 4 — bootability: NOT determinable from host code

Which bank the bootloader tries first, what it validates, and how it decides to
fall back **is not present in any host-side code**. `libCLTDevice` sends frames
and parses replies; it contains no model of the card's boot behaviour. The
descriptor query (§27) reports capability *bits* — has-golden, supports-golden —
but not the boot policy.

**Stop probing for this on hardware.** It lives in the card's bootloader, which we
do not have and cannot read over this protocol. The only way to learn it is
behavioural inference from a card you are willing to risk, which is exactly what
you should not do.

### 28.6 Item 5 — Quick vs Slow, and which the UI uses

Difference: §28.0. Selection is gated on `IsAllRcvSupportSDRAM` @ `0x395050`,
which reads descriptor `+0x0f` — **bit 0 of the capability byte** at reply `+0x1F`
in §27.2. If every attached receiver sets it, the tool uses the SDRAM (Quick)
path; otherwise it falls back to Slow.

So **the descriptor query you are about to send also tells you which path
LEDUpgrade would take on your card** — bit 0 of that same byte. I did not trace
the UI default in `FwUpgrade2.dll` beyond this gate.

### 28.7 What this changes about the plan

Three things worth weighing before any further write:

1. **The Slow path is the wrong tool if your card supports SDRAM staging.** Check
   bit 0 of the capability byte first. If set, the vendor would never take the
   path you have been reverse-engineering.
2. **The Quick path is safer by construction** — the host uploads to RAM and the
   card programs itself, so bank selection and protection stay under the card's
   control. If bit 0 is set, that is the path to decode properly, and the gap in
   §28.1 (the mode values) is what to close next.
3. **Nothing can update blocks 0x00–0x02.** If the behaviour you need lives in
   the boot region, no firmware file and no protocol can reach it.

### 28.8 Confidence summary

| item | status |
|---|---|
| Quick vs Slow structural difference | **high** |
| SDRAM frame layout (type 0x1A00, header 0x0e, 1024-byte chunks) | **high** |
| Slow path call order, unlock/relock, delays | **high** |
| Ex frame layout and why 0x2600 failed on your card | **high** |
| Boot region: host has no unlock for 0x00–0x02 | **high** |
| SDRAM `mode` values for begin/data/commit | **NOT RESOLVED** |
| Which bank the manager targets | **NOT RESOLVED** — comes from caller |
| Bootloader validation / fallback policy | **NOT DETERMINABLE from host code** |
| LEDUpgrade UI default beyond the SDRAM gate | **not traced** |
