# "Screen Connection" on the wire — the card-area pack (type 0x0200)

What LEDVISION/iSet actually transmits when the user lays cabinets out and
presses Send or Save. Static analysis of `libCLTDevice.1.dylib` (iSet 7 macOS,
C++ symbols intact). Nothing was executed or transmitted.

Read with [`docs/receiver-identity.md`](receiver-identity.md), which covers the
*persisted* half (the EEPROM control area). This file covers the *volatile*
half.

## 1. Call path — HIGH

```
CReceiverOP::SendOrSaveLayout            0x3b5990   (spawns a thread)
  DoSendOrSaveLayout                     0x3b5ae0   builds CRcvLayoutSendAndWriter
    CRcvLayoutSendAndWriter::SendOrSave  0x37adf0   device-type dispatch
      DoSendSave(int, bool)              0x37c840   the ordinary (no VX/X100/eV4) case
```

`DoSendSave`, in order:

1. `SavePortBackup`, `CalculateRcvCount`
2. if `this+0x21` (send-packs): `GetCardAreaParamPacks(layout, m_packs, 0, this+0x20)` @ `0x37c8ec`
3. if `this+0x20 == 0` (save): `PrepareData()` @ `0x37ca4b`
4. if `this+0x21`: **`SendRealTimePacks()` @ `0x37ca14` — this is where frames go out**
5. sender/processor branches (`dt == 1/2/4`) — not reached with no sender box
6. if save: `DoWriteConnectionToEeprom()` @ `0x37cace`, then `WriteBackUpConncetion()`

So **Send = the packs below; Save = the packs below *plus* the EEPROM writes.**

## 2. Framing — HIGH

`CSendControl::CSendControl(u8* buf, u32 len)` @ `0x257330`:

```
25735c  leal 0xc(%r15), %r12d              ; m_len = len + 12
25737e  memcpy(m_buf + 12, buf, len)
25738d  movabsq $0x2222665544332211, %rax
257397  movq %rax, (%r13)                  ; 11 22 33 44 55 66 22 22
25739b  movl $0x66554433, 0x8(%r13)        ; 33 44 55 66
```

Independently rebuilt by the transport, `CCmdAnalysisProtocolNic::CommandPack`
@ `0x257fb0`, command `0x807D` branch at `0x25814d` — same two MAC constants,
same `+0xc` payload copy.

**Frame = dst `11:22:33:44:55:66` | src `22:22:33:44:55:66` | payload.** The
payload's first two bytes occupy the EtherType slot, exactly like the `07 00`
detect frame (`BuildDetectRcvCard` @ `0x30a370`, `movw $0x7,(%rbx)`).

`SendRealTimePacks` @ `0x37ec00` hands `buf+0xC, len-0xC` to
`CDeviceSetIO::SendData` with `cmd = 0x807D`, `r9d = 2`, plus sender and port
indices — which the `0x807D` handler ignores entirely. **Nothing about the port
or the sender appears in the frame**, so these are receiver-directed frames we
can emit ourselves.

## 3. `SCardAreaPack` — 1284 bytes — HIGH

`GetCardAreaParamPacks` @ `0x38a1c0` allocates 8 stack packs, stride `0x504`,
each initialised at `0x38a316`/`0x38a327`:
`memset(pack+1, 0, 0x503); pack[0] = 0x02`.

| off | size | meaning |
|---|---|---|
| 0x000 | 1 | `0x02` — type low byte |
| 0x001 | 1 | `0x00` — type high byte (wire type reads `02 00`) |
| 0x002 | 1 | `0x00` |
| 0x003 | 1 | pack index (`movb %dl, 0x3(%r12)` @ `0x38dc68`) |
| 0x004 | 1280 | 128 entries × 10 bytes |

### Entry — left / top / right / bottom, big-endian

`GetCardAreaPacksEx` @ `0x38da60`, cursor `%rbx` starting at `pack+0xD` and
stepping 10 (`0x38dcdd`):

```
38dca0  movzwl -0x85(%rbp), %edi     ; width
38dca7  addw   %cx, %di              ; right  = startX + width
38dcb5  movb %dh, -0x9(%rbx)         ; +0 left   hi
38dcb8  movb %sil,-0x8(%rbx)         ; +1 left   lo
38dcbc  addw %ax, %cx                ; bottom = startY + height
38dcbf  movb %ah, -0x7(%rbx)         ; +2 top    hi
38dcc2  movb %al, -0x6(%rbx)         ; +3 top    lo
38dcc7  movb %ah, -0x5(%rbx)         ; +4 right  hi
38dcca  movb %al, -0x4(%rbx)         ; +5 right  lo
38dccd  movb %ch, -0x3(%rbx)         ; +6 bottom hi
38dcd0  movb %cl, -0x2(%rbx)         ; +7 bottom lo
38dcd3  movw $0x0, -0x1(%rbx)        ; +8..9 = 0
```

> **This corrects `docs/archive/config-protocol.md` §15.2**, which read the
> entry as `xOffset, yOffset, width, height`. It is
> **left, top, right, bottom** — an exclusive-edge rectangle. The two readings
> coincide only when the card sits at the origin. Corroborated three ways:
> `GetParamPacksLayout` @ `0x3b9660` fills `0, 0, rcvMaxWidth, rcvMaxHeight`;
> `CBasicParamSendAndWriter::GetParamPacksLayoutDefault` @ `0x325f50` is
> identical; and `DoWriteConnectionToEeprom` @ `0x37ee87` byte-swaps the same
> five `u16` into the EEPROM control area, whose fields are provably
> start/end (`docs/receiver-identity.md` §1).

Other rules, all HIGH:

* Coordinates are made relative to the port's bound origin
  (`0x38dd35`/`0x38dd42`, from `CRcvLayout::GetInitedPortBound` @ `0x1b5900`).
  Zero for a single card at (0,0).
* **The receiver's identity is positional**: entry *i* is the *i*-th card in the
  port chain (`GetInitedSenderPortRcvList` @ `0x38daf2`, slot `-1` → entry left
  zero). There is no receiver-index field inside the pack.
* Unused entries are **not zero** — the filled prefix is replicated across all
  128 slots by a doubling `memcpy` loop at `0x38dd6d`–`0x38ddc1`.
* Pack count = `ceil(nRcv / 128)`, capped at 8.
* Zero-receiver fallback (`0x38dde6`) writes entry 0 =
  `00 00 00 00 00 80 00 80` (a 0x8000 × 0x8000 rectangle).

## 4. `SOutputOffset` — 69 bytes, type `0x1100` — HIGH structure

Built only when the "send" flag is set (`0x38a5c0`); allocated `n * 0x45` and
initialised `record[0] = 0x11`, `record[3..5] = 0xFFFF` (`0x38a62e`–`0x38a67e`).

| off | size | meaning |
|---|---|---|
| 0x00 | 1 | `0x11` |
| 0x01 | 2 | `00 00` |
| 0x03 | 2 | receiver index, big-endian (default `FF FF`) — **medium** |
| 0x05 | 64 | copy of `SRcvRegionData + 0x0F` — **NOT RESOLVED**, zero in ordinary use |

Frame length `12 + 69 = 81`.

## 5. `SRcvRegionData` — the layout editor's per-card record — HIGH for +0..+0xE

`sizeof = 0x5F`. From `CRcvLayout::GetRcvRegion` @ `0x1b0180` and `ResetIndex`
@ `0x1b12f0`:

| off | type | field |
|---|---|---|
| +0x00 | u16 | index |
| +0x02 | u8 | flags (bit 2 = skip, bit 3 = out-of-offset) |
| +0x03 | u16 | sender |
| +0x05 | u16 | port |
| +0x07 | u16 | startX |
| +0x09 | u16 | startY |
| +0x0B | u16 | width |
| +0x0D | u16 | height |
| +0x0F | 0x50 | opaque — NOT RESOLVED |

## 6. The frame to send for one 128x64 card at (0,0)

Frame length **1296**:

```
  0  11 22 33 44 55 66            dst MAC
  6  22 22 33 44 55 66            src MAC
 12  02                           pack type
 13  00                           (type high byte)
 14  00
 15  00                           pack index
 16  00 00  00 00  00 80  00 40  00 00     entry 0: left 0, top 0, right 128, bottom 64
 26  ... the same 10 bytes 127 more times ...
```

With the repo's CLI:

```
e120 raw-send --type 0200 --pad 1282 --payload \
  0000$(python3 -c "print('00000000008000400000'*128)")
```

**RAM only.** `docs/archive/config-protocol.md` §15.1 verified that
`DoSendSave`, `PrepareData` and `GetRealTimePacks` contain zero flash
operations; the only flash writes in the whole layout writer are
`WriteBackUpConncetion` (`0x37f8ae`) at `addrHi = 0x1d`. So this frame cannot
erase or write anything and is safe to try immediately.

## 7. Correction to the repo

`e120_proto::discovery::set_layout` builds a 98-byte payload with fields at
`p[0..2]`, `p[6..10]`, `p[12..20]` taken from FPP's header comment. **That does
not match the vendor pack** in length (98 vs 1282), in field order, or in
meaning. It should be replaced with the 1284-byte pack above.
