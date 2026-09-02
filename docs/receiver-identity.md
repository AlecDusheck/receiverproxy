# How a receiver card is told where it is

A Colorlight receiver only stores incoming pixels that fall inside its
**control area**, a rectangle `(startX, startY) → (endX, endY)` held in the
card's on-board EEPROM at address `0x02`. `e120 provision --position x,y`
writes it. On 2026-09-01 this card had `startX = startY = 0xFFFF` with
`endX = 128, endY = 64`, an empty window: it stored nothing while reporting a
healthy 128x64 to `discover`. §4 records how that happened.

Static analysis of `libCLTDevice.1.dylib` (iSet 7 macOS build, C++ symbols
intact) plus the repo's own flash dumps. Nothing was executed, nothing was
transmitted.

---

## 1. The record: EEPROM `0x02`, 42 bytes (HIGH)

`CReceiverOP::WriteEepromCtrlAreaOffset` @ `0x3b2fc0` builds it. The first five
`u16` are byte-swapped to big-endian (`movzwl` / `rolw $0x8` / `movw` at
`0x3b3000`–`0x3b303b`), the remaining 32 bytes are copied raw (`movups` at
`0x3b303b`/`0x3b303f`). It then calls the write path with
`%r8d = 2` (EEPROM address) and `[rsp] = 0x2a` (42 bytes) at
`0x3b304f`/`0x3b3066`.

| EEPROM | size | field | note |
|---|---|---|---|
| `0x02` | u16 BE | **startX** | left edge of this card's window in the screen |
| `0x04` | u16 BE | **startY** | top edge |
| `0x06` | u16 BE | **endX** | `startX + width`, an *end coordinate*, not a width |
| `0x08` | u16 BE | **endY** | `startY + height` |
| `0x0a` | u16 BE | reserved, always 0 | |
| `0x0c`–`0x2b` | 32 B | connection blob, low half, **NOT RESOLVED** | vendor writes zeros in the default path |

A companion 32-byte blob goes to **EEPROM `0x92`**
(`CReceiverOP::WriteEepromExtendDataOffset` @ `0x3c1820`,
`movl $0x92, %r8d`, `pushq $0x20`).

`endX`/`endY` are end coordinates, proven by `CRcvLayoutSendAndWriter::PrepareData`
@ `0x386530`, which computes them by *addition*:

```
386b21  callq CRcvLayout::GetRcvRegion(u16, SRcvRegionData&)
386b26  movzwl -0x89(%rbp),%eax     ; region.x
386b2d  subl   -0xc0(%rbp),%eax     ; minus the port's origin
386b3a  movzwl -0x87(%rbp),%ecx     ; region.y
386b41  subl   -0xb8(%rbp),%ecx
386b5d  movw %ax, 0xc(%rdx,%rsi)    ; startX
386b62  movw %cx, 0xe(%rdx,%rsi)    ; startY
386b67  addw -0x85(%rbp),%ax        ; + region.width
386b6e  movw %ax, 0x10(%rdx,%rsi)   ; endX
386b73  addw -0x83(%rbp),%cx        ; + region.height
386b7a  movw %cx, 0x12(%rdx,%rsi)   ; endY
```

For a single card at the origin the two readings coincide, which is why
`docs/compiled-image-format.md` and `crates/e120-cli/src/screen.rs` have been
calling offsets `0x06`/`0x08` "width/height". That reading is right only while
`startX = startY = 0`.

## 2. Who writes it (HIGH)

Two paths, both ending at the same 42 bytes at EEPROM `0x02`:

1. **Screen Connection / cabinet layout.**
   `CRcvLayoutSendAndWriter::DoWriteConnectionToEeprom` @ `0x37edf0` walks an
   array at `this+0x23e8` (stride `0x58`, count at `this+0x23fc`), byte-swaps
   the five `u16` and writes 42 bytes to EEPROM `0x02`
   (`0x37ef53: movl $0x2a,(%rsp)`, `0x37ef5c: movl $0x2,%r8d`), then the
   32-byte tail to EEPROM `0x92` (`0x37f0bf`). The receiver index is forced to
   **`0xFFFF` (broadcast) when it is 0** (`0x37ef1a: movl $0xffff,%ebx`).
   The array elements come from `PrepareData` above, i.e. straight from the
   cabinet's position in the layout editor.

2. **Ordinary "save parameters to receiver".**
   `CRcvCommandManager::GetRcvParam_SaveCMDData` @ `0x1a1010` ends its command
   list with `GetSaveCMD_CtrlAreaParam` @ `0x1a4fc0`
   (call site `0x1a13fa`, guarded by the flag at `[rbx+0xc]`), which writes the
   **degenerate default**: `startX = startY = 0`,
   `endX = CHWParamRcvGeneral::GetMaxWidth()`,
   `endY = GetMaxHeight()`, blob all zero, followed by the same 32 zero bytes
   at EEPROM `0x92` and a reload command.

So every time LEDVISION/iSet saves a configuration to a receiver it also
rewrites the control area. The first config path here (build a 64 KB block-7
image, erase, write 256 pages) did not, and the erase wiped the mirror;
`e120 provision` reads the records before and writes them back after.

### `SaveSingleRcvCtrlArea`: the three-frame sequence

`CReceiverOP::SaveSingleRcvCtrlArea` @ `0x3b5820` is the whole operation for
one card, and it is exactly three frames with 100 ms between them:

| site | target | effect |
|---|---|---|
| `0x3b58da` | `WriteEepromCtrlAreaOffset` | 42 B → EEPROM `0x02` |
| `0x3b590b` | `WriteEepromExtendDataOffset` | 32 B → EEPROM `0x92` |
| `0x3b5946` | `ReLoadLocalParam` (arg `0x04`) | type `0x0600`, opcode `0x77`, data `01 01 00 00 00` |

`SRcvCtrlArea` is `{u32 startX; u32 startY; u32 endX; u32 endY; u8* ext; u16 extLen;}`;
the `pshufb` mask at `0x3b5880` (`00 01 04 05 08 09 0c 0d`) takes the low `u16`
of four consecutive `u32`s.

## 3. Is any of this in the `.rcvbp`? No (HIGH)

The control area is **not** in the `.rcvbp`, not in record 0x01, and not in the
compiled parameter image at flash `0x70000`. It lives only in the EEPROM (and
its flash mirror at `0x07F000`). Two independent confirmations:

* `CHWParamRcvGeneral::SetRcvsInCabinetPos` @ `0x15fdb0`, the only
  position-shaped thing that *does* ride in the parameter blob, is a
  rows×cols grid of receivers **inside one cabinet**, a plain `memcpy` into
  `this+0xe5fc`. It is not a screen coordinate.
* `CRcvParamManager::SetCabinetSettingParam` @ `0x390360` stores only the
  cabinet's own pixel size (`SetMaxWidth`, `SetMaxHeight`), no origin.

`CReceiverOP::SetMarkRcvPositionEnable` @ `0x3c2e70` is a red herring: it is the
volatile "highlight this card on the wall" toggle, a type `0x3300` frame with no
flash or EEPROM opcode anywhere.

## 4. The state this card was found in (HIGH)

Both dumps below were taken with page-addressed (type `0x0600`) flash reads, so
they are the flash mirror of the EEPROM at `0x07F000`.

| | startX | startY | endX | endY | blob `0x0c..0x2b` | `0x41` | `0x42` | `0x4c` |
|---|---|---|---|---|---|---|---|---|
| day-one dump (as shipped) | 0 | 0 | 128 | 64 | all `00` | `00` | `00` | `02` |
| snapshot after the erase | **65535** | **65535** | 128 | 64 | all `FF` | `FF` | `FF` | `FF` |

The window was `X ∈ [65535, 128)` and `Y ∈ [65535, 64)`, empty. No received
pixel could be inside it, so nothing was written to the frame buffer and the
panel kept displaying whatever it already held; an all-black frame changed
nothing.

How it got that way: a whole-block erase of block 0x07 also clears the mirror
sector (`docs/fpga/flash-layout.md` §4 records `primary-after-restore.bin`
differing from the vendor image in exactly `0x07F000`–`0x07FFFF`, all
`0xFF`). Then `e120 card screen-size --set 128x64` wrote the 256-byte record
back with only bytes `0x06`–`0x09` patched and everything else left as the
`0xFF` it had just read, restoring the size and leaving the offsets at
`0xFFFF`. It refuses a record that reads as erased.

## 5. Answers to the four questions

1. **What does Screen Connection send/write?** Two things. A real-time RAM pack
   ([archive/screen-connection-wire.md](archive/screen-connection-wire.md))
   and, on save, the **42-byte EEPROM record at address `0x02`** documented
   above, plus its 32-byte companion at `0x92`, followed by a reload
   (`0x0600`, opcode `0x77`, data `01 01 00 00 00`). It is *not* a `.rcvbp`
   record and *not* a flash-block write.
2. **Is there a per-card position, and where?** Yes: `startX`/`startY` at EEPROM
   `0x02`/`0x04`, big-endian `u16`. There is no separate "receiver index" or
   "screen number" stored on the card; the index only appears as the frame's
   addressing field (`payload[3..4]`, `0xFFFF` = broadcast).
3. **What does the card do with the row field?** It compares it against this
   window. The transmitted row is a *screen-global* row (`docs/pixel-protocol.md`
   §1.6), and the card keeps a pixel only when the row is in `[startY, endY)`
   and the column in `[startX, endX)`. That filtering rule is inferred from the
   record's shape and from the fact that FPP addresses rows globally and relies
   on LEDVISION having set the window (**MEDIUM**); the firmware itself is not
   available for static analysis. What is HIGH is that the window exists, that
   it is per-card, and that an erase leaves it empty.
4. **Is there a separate "display on" / "normal mode" command?** No command is
   required beyond what we already send. Two EEPROM flags are adjacent and worth
   restoring to their factory values, but neither is a start/stop:
   `0x41` "no input show info" and `0x42` "turn on screen show" (both `0x00` at
   the factory, both `0xFF` after an erase). `Nic_SetTestModeIndex` renders host-side and
   never asks the card (`docs/pixel-protocol.md` §5.1), so the built-in
   generator being inert is expected, not a symptom.

## 6. Writing the record

`e120 provision --position x,y` writes the control area, its companion and
every other record from the read-back set, one record at a time, then saves
(opcode `0x87`) and reloads (`0x77`). `scripts/eeprom-restore.py` rewrites
records from the day-one dump. The frames below are what those send, for
doing it by hand with `e120 debug send`.

### 6.0 The RAM-only card-area pack

`Send` in Screen Connection pushes the same rectangle as a volatile pack
([archive/screen-connection-wire.md](archive/screen-connection-wire.md)). It
writes nothing and a power cycle undoes it:

```
e120 debug send --type 0200 --pad 1282 \
  --payload 0000$(python3 -c "print('00000000008000400000'*128)")
```

1296-byte frame: type `02 00`, pack index 0, then 128 entries of
`left=0, top=0, right=128, bottom=64`.

### 6.1 Persist it: EEPROM `0x02`, 42 bytes

Send with the usual MACs `11:22:33:44:55:66` / `22:22:33:44:55:66`. Payload
length is `max(0x80, dataLen + 0x12)` = `0x80`, so each frame is **140 bytes**.
Receiver index `0000` (or `FFFF` to broadcast, as the vendor does).

```
e120 debug send --type 1900 --pad 126 --payload \
00000085000000020000002a000000000080004000000000000000000000000000000000000000000000000000000000000000000000
```

which lays out as:

| frame off | bytes | meaning |
|---|---|---|
| 12 | `19 00` | type |
| 14 | `00` | |
| 15 | `00 00` | receiver index, BE |
| 17 | `85` | write |
| 18 | `00 00 00 02` | EEPROM address 2 |
| 22 | `00 00 00 2a` | 42 bytes |
| 26 | `00 00` | **startX = 0** |
| 28 | `00 00` | **startY = 0** |
| 30 | `00 80` | **endX = 128** |
| 32 | `00 40` | **endY = 64** |
| 34 | `00 00` | reserved |
| 36 | 32 × `00` | blob (matches the factory value) |
| 68 | 72 × `00` | pad to a 128-byte payload |

### 6.2 Restore the companion blob: EEPROM `0x92`, 32 bytes

```
e120 debug send --type 1900 --pad 126 --payload \
0000008500000092000000200000000000000000000000000000000000000000000000000000000000000000
```

### 6.3 Make it take effect

```
e120 debug send --type 0600 --pad 126 --payload 00000077000000000101000000
```
(the vendor's `ReLoadLocalParam` form: opcode `0x77`, data `01 01 00 00 00`),
or simply power-cycle. `e120 card reload --full` already emits an equivalent
frame.

### 6.4 Verify

```
e120 card screen-size
```
The first 16 bytes must read `00 00 00 00 00 00 00 80 00 40 00 00 ...`; the
erased card read `00 00 ff ff ff ff 00 80 00 40 ff ff ...`.

### 6.5 The factory flags

`0x41` and `0x42` are single bytes and were `00` at the factory:

```
e120 debug send --type 1900 --pad 126 --payload 00000085000000410000000100
e120 debug send --type 1900 --pad 126 --payload 00000085000000420000000100
```

Neither took through opcode `0x85` on this card; both still read `0xFF`, and
the panel renders regardless.

### 6.6 Reading the record without writing anything

Opcode `0x44` is not in the data-attaching set, so this frame cannot modify the
card. 42 bytes from EEPROM 2:

```
e120 debug send --type 1900 --pad 126 --payload 00000044000000020000002a
```

i.e. `00 00 00 | 44 | 00 00 00 02 | 00 00 00 2a`; the length field is simply how
many bytes to return.

## 7. What the commands do

* `e120 card screen-size --set` reads and writes all 256 bytes from
  EEPROM 0; it refuses a record that reads as erased. It prints the
  end coordinates as a size, which is right only while `startX = startY = 0`.
* `e120 flash restore-block` writes block 0x07 from an image whose page
  `0xF0` is erased, so it clears the mirror every time; page `0xF0` refusing
  the write is expected. `e120 provision` reads the EEPROM records before
  the block write and rewrites them after; by hand, run
  `scripts/eeprom-restore.py --commit` and check with `scripts/flash-review.py`.
* `parse_discovery_response` reads payload 20–23 as cols/rows; those are
  `endX`/`endY`, and `startX`/`startY` sit at payload 16–19 unread
  ([archive/packet-statistics.md](archive/packet-statistics.md) §2).
