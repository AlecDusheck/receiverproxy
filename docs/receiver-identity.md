# Control area: how a receiver card knows which pixels are its own

A Colorlight receiver stores only the incoming pixels that fall inside its
control area, a rectangle `(startX, startY) -> (endX, endY)` held in the
card's EEPROM at address `0x02`. `rxp provision --position x,y` writes it.
With the record erased (`startX = startY = 0xFFFF`) the card drops every
pixel while reporting a healthy 128x64 to `rxp discover`.

Source: static analysis of `libCLTDevice.1.dylib` (iSet 7 macOS build, C++
symbols intact) and E120 flash dumps. Addresses are in that binary.

---

## 1. Record layout: EEPROM `0x02`, 42 bytes

`CReceiverOP::WriteEepromCtrlAreaOffset` @ `0x3b2fc0` builds it. The first
five `u16` are byte-swapped to big-endian (`movzwl` / `rolw $0x8` / `movw` at
`0x3b3000`-`0x3b303b`); the remaining 32 bytes are copied raw (`movups` at
`0x3b303b`/`0x3b303f`). The write path is called with `%r8d = 2` (EEPROM
address) and `[rsp] = 0x2a` (42 bytes) at `0x3b304f`/`0x3b3066`.

| EEPROM | size | field | note |
|---|---|---|---|
| `0x02` | u16 BE | startX | left edge of this card's window in the screen |
| `0x04` | u16 BE | startY | top edge |
| `0x06` | u16 BE | endX | `startX + width`, an end coordinate, not a width |
| `0x08` | u16 BE | endY | `startY + height` |
| `0x0a` | u16 BE | reserved, always 0 | |
| `0x0c`-`0x2b` | 32 B | connection blob, low half; not resolved | the vendor writes zeros in the default path |

A companion 32-byte blob goes to EEPROM `0x92`
(`CReceiverOP::WriteEepromExtendDataOffset` @ `0x3c1820`,
`movl $0x92, %r8d`, `pushq $0x20`).

`endX`/`endY` are end coordinates: `CRcvLayoutSendAndWriter::PrepareData`
@ `0x386530` computes them by addition:

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

For a single card at the origin the end coordinates equal the size, which is
why `docs/compiled-image-format.md` and `crates/cli/src/screen.rs` call
offsets `0x06`/`0x08` "width/height". That reading holds only while
`startX = startY = 0`.

`colorlight::eeprom::control_area(x, y, w, h)` builds the record as
`(x, y, x+w, y+h)`; `control_area_is_big_endian_corners` pins the bytes.

## 2. Vendor write paths

Two paths, both ending at the same 42 bytes at EEPROM `0x02`:

1. Screen Connection / cabinet layout.
   `CRcvLayoutSendAndWriter::DoWriteConnectionToEeprom` @ `0x37edf0` walks
   an array at `this+0x23e8` (stride `0x58`, count at `this+0x23fc`),
   byte-swaps the five `u16` and writes 42 bytes to EEPROM `0x02`
   (`0x37ef53: movl $0x2a,(%rsp)`, `0x37ef5c: movl $0x2,%r8d`), then the
   32-byte tail to EEPROM `0x92` (`0x37f0bf`). The receiver index is forced
   to `0xFFFF` (broadcast) when it is 0 (`0x37ef1a: movl $0xffff,%ebx`).
   The array elements come from `PrepareData` above, i.e. from the cabinet's
   position in the layout editor.

2. Ordinary "save parameters to receiver".
   `CRcvCommandManager::GetRcvParam_SaveCMDData` @ `0x1a1010` ends its
   command list with `GetSaveCMD_CtrlAreaParam` @ `0x1a4fc0` (call site
   `0x1a13fa`, guarded by the flag at `[rbx+0xc]`), which writes the
   degenerate default: `startX = startY = 0`,
   `endX = CHWParamRcvGeneral::GetMaxWidth()`, `endY = GetMaxHeight()`, blob
   all zero, followed by the same 32 zero bytes at EEPROM `0x92` and a reload
   command.

Every vendor save of a configuration to a receiver therefore rewrites the
control area. A block-0x07 image write on its own does not; the erase clears
the mirror. `rxp provision` reads the records before the block write and
writes them back after.

### `SaveSingleRcvCtrlArea`: the three-frame sequence

`CReceiverOP::SaveSingleRcvCtrlArea` @ `0x3b5820` is the whole operation for
one card: three frames with 100 ms between them.

| site | target | effect |
|---|---|---|
| `0x3b58da` | `WriteEepromCtrlAreaOffset` | 42 B -> EEPROM `0x02` |
| `0x3b590b` | `WriteEepromExtendDataOffset` | 32 B -> EEPROM `0x92` |
| `0x3b5946` | `ReLoadLocalParam` (arg `0x04`) | type `0x0600`, opcode `0x77`, data `01 01 00 00 00` |

`SRcvCtrlArea` is `{u32 startX; u32 startY; u32 endX; u32 endY; u8* ext; u16 extLen;}`;
the `pshufb` mask at `0x3b5880` (`00 01 04 05 08 09 0c 0d`) takes the low
`u16` of four consecutive `u32`s.

## 3. Absence from the `.rcvbp` and the parameter image

The control area is not in the `.rcvbp`, not in record 0x01, and not in the
compiled parameter image at flash `0x70000`. It lives only in the EEPROM and
its flash mirror at `0x07F000`. Two confirmations:

* `CHWParamRcvGeneral::SetRcvsInCabinetPos` @ `0x15fdb0`, the only
  position-shaped field in the parameter blob, is a rows x cols grid of
  receivers inside one cabinet, a plain `memcpy` into `this+0xe5fc`. It is
  not a screen coordinate.
* `CRcvParamManager::SetCabinetSettingParam` @ `0x390360` stores only the
  cabinet's own pixel size (`SetMaxWidth`, `SetMaxHeight`), no origin.

`CReceiverOP::SetMarkRcvPositionEnable` @ `0x3c2e70` is the volatile
"highlight this card on the wall" toggle, a type `0x3300` frame with no flash
or EEPROM opcode. It is unrelated to position storage.

## 4. Erased state

Both dumps below are page-addressed (type `0x0600`) flash reads of the EEPROM
mirror at `0x07F000`.

| | startX | startY | endX | endY | blob `0x0c..0x2b` | `0x41` | `0x42` | `0x4c` |
|---|---|---|---|---|---|---|---|---|
| day-one image | 0 | 0 | 128 | 64 | all `00` | `00` | `00` | `02` |
| after block-0x07 erase and `screen-size --set 128x64` | 65535 | 65535 | 128 | 64 | all `FF` | `FF` | `FF` | `FF` |

The erased window is `X in [65535, 128)` and `Y in [65535, 64)`, empty. No
received pixel is inside it, nothing reaches the frame buffer, and the panel
keeps whatever it already holds; an all-black frame changes nothing.
Measured: frames accepted, packet counter advancing, current changing,
`discover` reporting 128x64, no sent content displayed.

Mechanism: a whole-block erase of block 0x07 clears the mirror sector
(`primary-after-restore.bin` differs from the vendor image in exactly
`0x07F000`-`0x07FFFF`, all `0xFF`; [fpga/flash-layout.md](fpga/flash-layout.md)).
`rxp card screen-size --set 128x64` then writes the 256-byte record back
with only bytes `0x06`-`0x09` patched and everything else as the `0xFF` it
read, restoring the size and leaving the offsets at `0xFFFF`. The command
refuses a record that reads as erased.

## 5. Facts by subject

### Screen Connection

Two things go to the card. A volatile RAM pack, type `0x0200` (section 6.0),
and on save the 42-byte EEPROM record at address `0x02` plus its 32-byte
companion at `0x92`, followed by a reload (`0x0600`, opcode `0x77`, data
`01 01 00 00 00`). Neither is a `.rcvbp` record or a flash-block write.

### Per-card position

`startX`/`startY` at EEPROM `0x02`/`0x04`, big-endian `u16`. There is no
separate "receiver index" or "screen number" stored on the card; the index
appears only as the frame's addressing field (`payload[3..4]`, `0xFFFF` =
broadcast).

### The row field

The transmitted row is a screen-global row ([pixel-protocol.md](pixel-protocol.md)
section 1.6). The card keeps a pixel only when the row is in
`[startY, endY)` and the column in `[startX, endX)`. This filtering rule is
inferred from the record's shape and from FPP addressing rows globally while
relying on LEDVISION to set the window; the firmware is not available for
static analysis. That the window exists, is per-card, and is left empty by
an erase is established.

### Display-on command

No command beyond the pixel, latch and brightness frames is required. Two
adjacent EEPROM flags are not start/stop controls: `0x41` "no input show
info" and `0x42` "turn on screen show" (both `0x00` at the day-one, both
`0xFF` after an erase). `Nic_SetTestModeIndex` renders host-side and never
asks the card ([pixel-protocol.md](pixel-protocol.md) section 5.1), so the
vendor's test mode says nothing about the card.

## 6. Frames

`rxp provision --position x,y` writes the control area, its companion and
every other record from the read-back set, one record at a time, then saves
(opcode `0x87`) and reloads (`0x77`). `scripts/eeprom-restore.py` rewrites
records from the day-one dump. The frames below are what those send, for
doing it by hand with `rxp debug send`.

### 6.0 The RAM-only card-area pack

Screen Connection's `Send` pushes the same rectangle as a volatile pack. It
writes nothing; a power cycle undoes it:

```
rxp debug send --type 0200 --pad 1282 \
  --payload 0000$(python3 -c "print('00000000008000400000'*128)")
```

1296-byte frame: type `02 00`, pack index 0, then 128 entries of
`left=0, top=0, right=128, bottom=64`.

### 6.1 Persist: EEPROM `0x02`, 42 bytes

MACs `11:22:33:44:55:66` / `22:22:33:44:55:66`. Payload length is
`max(0x80, dataLen + 0x12)` = `0x80`, so each frame is 140 bytes. Receiver
index `0000`, or `FFFF` to broadcast as the vendor does.

```
rxp debug send --type 1900 --pad 126 --payload \
00000085000000020000002a000000000080004000000000000000000000000000000000000000000000000000000000000000000000
```

| frame off | bytes | meaning |
|---|---|---|
| 12 | `19 00` | type |
| 14 | `00` | |
| 15 | `00 00` | receiver index, BE |
| 17 | `85` | write |
| 18 | `00 00 00 02` | EEPROM address 2 |
| 22 | `00 00 00 2a` | 42 bytes |
| 26 | `00 00` | startX = 0 |
| 28 | `00 00` | startY = 0 |
| 30 | `00 80` | endX = 128 |
| 32 | `00 40` | endY = 64 |
| 34 | `00 00` | reserved |
| 36 | 32 x `00` | blob (the day-one value) |
| 68 | 72 x `00` | pad to a 128-byte payload |

### 6.2 Companion blob: EEPROM `0x92`, 32 bytes

```
rxp debug send --type 1900 --pad 126 --payload \
0000008500000092000000200000000000000000000000000000000000000000000000000000000000000000
```

### 6.3 Reload

```
rxp debug send --type 0600 --pad 126 --payload 00000077000000000101000000
```

The vendor's `ReLoadLocalParam` form: opcode `0x77`, data `01 01 00 00 00`.
A power-cycle has the same effect. `rxp card reload --full` emits an
equivalent frame.

### 6.4 Verify

```
rxp card screen-size
```

The first 16 bytes must read `00 00 00 00 00 00 00 80 00 40 00 00 ...`. The
erased card reads `00 00 ff ff ff ff 00 80 00 40 ff ff ...`.

### 6.5 DayOne flags

`0x41` and `0x42` are single bytes, `00` at the day-one:

```
rxp debug send --type 1900 --pad 126 --payload 00000085000000410000000100
rxp debug send --type 1900 --pad 126 --payload 00000085000000420000000100
```

Neither takes through opcode `0x85`; both read back `0xFF`. The panel
renders regardless.

### 6.6 Read without writing

Opcode `0x44` is not in the data-attaching set, so this frame cannot modify
the card. 42 bytes from EEPROM 2:

```
rxp debug send --type 1900 --pad 126 --payload 00000044000000020000002a
```

Layout `00 00 00 | 44 | 00 00 00 02 | 00 00 00 2a`; the length field is the
number of bytes to return.

## 7. Command behaviour

* `rxp card screen-size --set` reads and writes all 256 bytes from EEPROM 0.
  It refuses a record that reads as erased. It prints the end coordinates as
  a size, which is right only while `startX = startY = 0`.
* `rxp flash restore-block` writes block 0x07 from an image whose page
  `0xF0` is erased, so it clears the mirror every time; page `0xF0` refusing
  the write is expected. `rxp provision` reads the EEPROM records before
  the block write and rewrites them after. By hand: run
  `scripts/eeprom-restore.py --commit`, then check with
  `scripts/flash-review.py`.
* `parse_discovery_response` (`crates/colorlight/src/discovery.rs`) reads
  reply payload bytes 20-23 as cols/rows. Those are `endX`/`endY`;
  `startX`/`startY` sit at payload 16-19 and are not read. `rxp discover`
  therefore reports a healthy size while the control area is erased.
