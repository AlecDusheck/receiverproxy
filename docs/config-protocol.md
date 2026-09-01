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
