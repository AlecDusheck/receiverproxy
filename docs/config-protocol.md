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

