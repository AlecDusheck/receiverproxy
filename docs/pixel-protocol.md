# The vendor's pixel wire format (recovered from CLTNic)

What Colorlight's own sender puts on the wire for pixel data, recovered by
static reading of `CLTNic.dll`. Nothing was executed.

## Sources

| Tag | File | Notes |
|---|---|---|
| **x86** | `<scratch>/ledup/x52b/x86/Bin/CLTNic.dll` (2025-04-30, 1 892 904 B) | 32-bit, `__stdcall`, decorated exports carry argument sizes. Image base `0x10000000`. Every address below is from this build unless tagged x64. |
| **x64** | `<scratch>/ledvision/$_15_/x64/Bin/CLTNic.dll` (2024-06-10, LEDVISION 9.6) | Image base `0x180000000`. Used only to confirm the templates are unchanged in the newest release. |

`<scratch>` is a scratch directory outside the tree (the vendor packages
unpack with `7z x`; see [archive/vendor-sdk-analysis.md](archive/vendor-sdk-analysis.md)).

There is **no** macOS build of CLTNic; `libCLTDevice.1.dylib` (iSet) contains the
device/config side only: no `Nic_*` symbols, no picture sender. Six copies of
CLTNic exist across the vendor packages; all export the same 25 symbols and
the x86 and x64 builds agree byte-for-byte on the frame templates.

CLTNic is the **only** sender library LEDVISION ships (`$_15_/x64/Bin/` has no
alternative), so there is one pixel format for the whole product line.

---

## 1. The pixel-data frame

### 1.1 Where it comes from

The frame header is a *template* built once in the pcap-wrapper constructor and
patched in place for each packet.

x86 `fcn.10002ae0`, `0x10002b07`–`0x10002b5f` (object base `edi`):

```
0x10002b07  mov dword [edi+0x44], 0x44332211    ; 11 22 33 44
0x10002b11  mov dword [edi+0x48], 0x22226655    ; 55 66 22 22
0x10002b1a  mov dword [edi+0x4c], 0x66554433    ; 33 44 55 66
0x10002b3d  mov byte  [edi+0x50], 0x55          ; type
0x10002b21  mov dword [edi+0x51], 0            ; six patched bytes
0x10002b28  mov dword [edi+0x55], 0x88080000    ; 00 00 08 88
```

x64 `fcn.1800030f0`, `0x180003104`–`0x180003130`, identical values at
`rcx+0x78`/`+0x84`/`+0x85`/`+0x89`. The template base is `+0x44` (x86) /
`+0x78` (x64); all offsets below are relative to that base, i.e. **they are
frame offsets**.

### 1.2 Layout

| Offset | Size | Value | Meaning | Confidence |
|---|---|---|---|---|
| 0 | 6 | `11 22 33 44 55 66` | destination MAC (the card) | high |
| 6 | 6 | `22 22 33 44 55 66` | source MAC (the sender) | high |
| 12 | 1 | `0x55` | frame type / first EtherType byte | high |
| 13 | 1 | row >> 8 | row index, high byte | high |
| 14 | 1 | row & 0xFF | row index, low byte | high |
| 15 | 1 | xoff >> 8 | first pixel index in the row, high byte | high |
| 16 | 1 | xoff & 0xFF | first pixel index, low byte | high |
| 17 | 1 | count >> 8 | pixels in this packet, high byte | high |
| 18 | 1 | count & 0xFF | pixels in this packet, low byte | high |
| 19 | 1 | `0x08` | fixed marker | high |
| 20 | 1 | `0x88` | fixed marker | high |
| 21 | 3·count | pixel bytes | 3 bytes per pixel | high |

So: **21-byte header, three 16-bit big-endian fields at offsets 13, 15 and 17,
the `08 88` marker at 19–20, pixels from 21.** Total frame length
`21 + 3·count`.

The patch sites, x86 `fcn.10003580` (the row packetizer):

| Field | Store | Address |
|---|---|---|
| row hi/lo | `mov [edi+0x51], al` / `mov [edi+0x52], bl` | `0x10003630`, `0x10003652` |
| xoff hi/lo | `mov [edi+0x53], al` / `mov [edi+0x54], al` | `0x100036a4`, `0x100036b7` (full packet); `0x10003788`, `0x1000379b` (tail) |
| count hi/lo | `mov word [edi+0x55], 0xf101` (= `01 F1` = 497 BE) | `0x1000369b` (full packet) |
| count hi/lo | `mov [edi+0x55], al` / `mov [edi+0x56], al` | `0x10003763`, `0x10003777` (tail) |

The `0x1f101` store settles the endianness: 497 decimal is written as
the byte pair `01 F1` at offsets 17,18: big-endian, and it lands *after* the
row and offset fields, not before.

### 1.3 Independent confirmation of the 21-byte header

Two places compute lengths and both agree:

* `0x10003684` / `0x10003692`: for a maximum-size packet the pcap header's
  `caplen` and `len` are both set to `0x5E8` = **1512** = 21 + 3·497.
* `0x10003749`: for the tail packet, `len = (count + 7) * 3` = `3·count + 21`.
* `fcn.100067d0` (send-queue sizing, `0x10006848`–`0x10006897`) uses
  `0x5F8` = 1528 bytes per full packet and a `0x25` = **37**-byte per-packet
  overhead. 1528 − 1491 = 37 = 16-byte `pcap_pkthdr` + 21-byte frame header.

The record copied into the pcap send queue (`0x100036c0`–`0x100036e2`) is
exactly `16 + 16 + 4 + 1 = 37` bytes: pkthdr, then frame bytes 0–15, then
16–19, then byte 20.

### 1.4 Maximum pixels per packet

**497** (`0x1F1`), hard-coded at `0x10003660`, `0x10003676`, `0x100036ba`,
`0x10003678` and again in the queue sizing at `0x10006848`. A row wider than
497 is split into `floor(w/497)` full packets of 497 plus one tail packet of
`w mod 497`. Confidence: high. (Same value as FPP's `CL_MAX_PIXL_PER_PACKET`.)

### 1.5 Minimum pixel count: a padding rule we do not implement

`0x1000359b`–`0x100035a7`: if the screen width is below 16, the packet's
`count` is raised to 16 and the extra pixels are zero-filled
(`0x10003722`/`0x100037ff` call `memset`). Irrelevant at 128 px wide, but it is
a real rule. Confidence: high.

### 1.6 The row field is not just `y`

`0x100035c5`–`0x100035e3` computes a row base from the screen number
(`node+8`, i.e. the `Nic_CreateScreen` screen id, 1…256):

```
if (n <= 9)  base = (n - 1) << 12          ; 0, 4096, 8192, …
else         base = 320 * (n - 9) + 0x8000
```

and the transmitted row field is `base + y` (`add ebx, [var_ch]` at
`0x1000361f`). For a single screen created as number **1** the base is 0 and
the field is plain `y`. Confidence: high. Consequence: the high nibble of the
16-bit row field is a screen/port selector; using a screen number other than 1
would shift every row by 4096.

### 1.7 Pixel byte order and depth

`0x10003704`–`0x10003713` (and the identical `0x100037e1` in the tail):

```
mov ax, word [esi]        ; source bytes 0,1
mov [ecx], ax
mov al, byte [esi+2]      ; source byte 2
mov [ecx+2], al
add esi, 4                ; source advances 4 bytes
add ecx, 3
```

**8 bits per channel, 3 bytes per pixel, copied verbatim from the low three
bytes of a 32-bit source pixel.** Confidence: high.

Which *colour* those three bytes are is decided by the caller, not by CLTNic:
`Nic_CreateScreen` (`0x10001ec0` → `fcn.10005f80`) takes no pixel-format
argument, and `fcn.10006540` allocates the buffer as `width*height*4`
(`0x1000659f`–`0x100065c7`). LEDVISION feeds it GDI/GDI+ 32bpp surfaces, whose
memory order is B,G,R,A, so the wire order is almost certainly **B,G,R**.
Confidence: **medium**, inferred from the caller's conventions, not proven in
CLTNic. Our `ColorOrder` enum stays useful.

---

## 2. Per-frame packet sequence

Driven by `fcn.10002f10` (x86 `0x10002f10`), called from
`CSendThread::Run` (`method.CSendThread.virtual_4`, `0x10008e00`) at
`0x10009007`:

```
queue->len = 0                              ; 0x10002f4e
if (screen == g_sendParamScreenNumber)      ; flag from Run @ 0x10008ffc
    fcn.10003460()                          ; 0x10002f59  -> two frames, see below
fcn.10003580(node)                          ; 0x10002f6a  -> all row packets
fcn.10003850(pcap_handle, queue)            ; 0x10002f7c  -> pcap_sendqueue_transmit
```

So one video frame for one screen is **one pcap send-queue burst**, in this
order:

1. **Display / latch frame**, type `0x0107`, 112 bytes, *first*, not last.
2. **Brightness frame**, type `0x0A`, 77 bytes.
3. **All pixel row packets**, row 0 upward, and within a row left to right.

Steps 1–2 are emitted only for the screen selected by
`Nic_SetSendParamScreenNumber` (global `0x101b16b8`, defaults to 1 at
`0x10005a64`). With a single screen they are emitted on every frame.

Because the latch frame *leads* the burst, it latches the **previous** frame's
row data. Over a continuous stream this is equivalent to rows-then-latch;
the difference only shows on the very first burst.

Confidence: high.

### 2.1 Pacing

None. The whole burst goes to `pcap_sendqueue_transmit` with `sync = 0`
(`fcn.10003850` → `fcn.100038f0`, `0x10003883` pushes 0 as the sync flag), so
packets go out back-to-back at line rate with no timestamp pacing. After the
burst `Run` issues three `Sleep(0)` calls (`0x1000901a`–`0x10009024`) and the
thread runs at `THREAD_PRIORITY_ABOVE_NORMAL` (`0x10005cd1`). There is no
inter-packet delay anywhere. Confidence: high.

### 2.2 The display / latch frame (type 0x0107, 112 bytes)

`fcn.10003460`, `0x10003460`–`0x100034f6`. Built on the stack, `caplen = len =
112` (`0x1000348e`, `0x10003495`), copied to the queue as 32 dwords.

| Offset | Size | Value | Source |
|---|---|---|---|
| 0 | 6 | `11 22 33 44 55 66` | `0x10003471`, `0x1000347b`, `0x10003487` |
| 6 | 6 | `22 22 33 44 55 66` | ″ |
| 12 | 1 | `0x01` | `0x1000349c` |
| 13 | 1 | `0x07` | `0x100034c4` |
| 14–34 | 21 | `0x00` | `0x100034a0`, `0x100034a5`, `0x100034ac` |
| 35 | 1 | master brightness | `0x100034d0`, = byte `[bright+3]` |
| 36 | 1 | `0x05` | `0x100034b0` |
| 37 | 1 | `0x00` | ″ |
| 38 | 1 | channel gain 0 | `0x100034d7`, `[bright+4]` |
| 39 | 1 | channel gain 1 | `0x100034de`, `[bright+5]` |
| 40 | 1 | channel gain 2 | `0x100034e5`, `[bright+6]` |
| 41–111 | 71 | `0x00` | `memset` at `0x100034b6` |

This is byte-identical to what `pixel::sync()` emits. Confidence: high.

`bright` is the 7-byte block at `0x101b1664` written by `Nic_SetBrightness`
(`fcn.100062c0`). `[bright+3] = round(master_float * 255)` (`0x10006300`,
`0x10006336`). The per-channel bytes `[bright+4..6]` are
`round(c_k * round(255 * percent / 100) / 255)` with `c_k = 255` at neutral
colour temperature: strictly linear, no gamma and no offset
(`fcn.1013d1f0` is `pow`, used only for the type-0x0A frame's bytes 13–15;
[archive/grey-mapping.md](archive/grey-mapping.md) §3).

### 2.3 The brightness frame (type 0x0A, 77 bytes)

Same function, `0x100034f8`–`0x1000356a`. `caplen = len = 77` (`0x10003518`,
`0x1000351f`), 23 dwords + 1 byte.

| Offset | Size | Value | Source |
|---|---|---|---|
| 0–11 | 12 | the two MACs | `0x100034fb`–`0x10003509` |
| 12 | 1 | `0x0A` | `0x10003526` |
| 13 | 1 | `[bright+0]` | `0x10003542` |
| 14 | 1 | `[bright+1]` | `0x10003549` |
| 15 | 1 | `[bright+2]` | `0x10003555` |
| 16 | 1 | `0xFF` | `0x1000352a` |
| 17–76 | 60 | `0x00` | `memset` at `0x1000352e` |

Byte-identical to `pixel::brightness()`. Confidence: high. The vendor sends
it on every video frame, after the latch frame and before the rows;
`Wall::show` sends it first in every refresh.

---

## 3. Does anything depend on the card model?

**No.** Confidence: high.

CLTNic has no notion of card model, scan mode, driver-chip family or panel
geometry. Grep-level and call-graph-level evidence:

* The only branches in the whole send path are: the screen-rotation switch
  (`fcn.100068e0` `0x100069f0`, 4 cases, a purely local framebuffer transform),
  the test-pattern switch (`fcn.10006540` `0x100065e4`, host-side rendering),
  and the 497-pixel packet split.
* The frame templates are compile-time constants in the constructor. There is
  no second template, no lookup table, no configuration read.
* The x86 (2025) and x64 (LEDVISION 9.6, 2024) builds have identical
  templates and identical constants.
* `$_15_/x64/Bin/` ships exactly one sender DLL, used for every product.

The only thing that varies with configuration is the screen *number* (§1.6) and
the pixel *count* per row.

---

## 4. `Nic_SendScreenBlackPicture`: the minimal correct sequence

`0x10001e60` → `fcn.10006670`. It does **not** build a special frame. It:

1. looks the screen up by id (`0x100066e3`),
2. takes a free frame node (`fcn.10006b80`, `0x1000674f`),
3. `memset(node->pixels, 0, width * height * 4)` (`0x1000675a`–`0x1000676d`),
4. pushes it to the send queue (`fcn.10006aa0`, `0x10006776`).

The wire result is therefore an ordinary full frame: latch frame, brightness
frame, then every row packet with all-zero pixel bytes. Confidence: high.

`Nic_SendScreenPicture` (`0x10001e40`) routes to exactly this path when the
show flag (`0x101a7470`, set by `Nic_SetScreenShowOnOff`) is 0.

So "make the panel go black" is not a shortcut; it is the full sequence with
zero pixels. There is exactly one pixel path.

---

## 5. `Nic_SetScreenSize` and `Nic_SetScreenConnectionStyle`: neither is on the wire

Both are **purely host-side**. Video is *not* gated on them. Confidence: high.

* `Nic_SetScreenSize(id, w, h)` (`0x10001f00` → `fcn.10006170`). Rejects
  dimensions above `0x100000`, finds the screen, calls two setters
  (`fcn.10008850`, `fcn.100088f0`) that store width and height in the screen
  object, then calls `fcn.100067d0` + `fcn.10002e80`, which *only* frees and
  reallocates the local pcap send queue to fit the new worst-case burst size
  (`fcn.10002e80` calls `pcap_sendqueue_destroy` at `[obj+0x28]` and
  `pcap_sendqueue_alloc` at `[obj+0x18]`). **No packet is transmitted.**

* `Nic_SetScreenConnectionStyle(id, style)` (`0x10001f10` → `fcn.100060f0`).
  Validates `style ∈ {0,1,2,3}` (`0x10006139`–`0x10006148`) and does one store:
  `mov [eax+0x10], esi` (`0x10006159`). That field is read in `fcn.100068e0` at
  `0x100069e3` and drives a 4-case switch (`0x100069f5`) that copies the user's
  image into the node buffer either straight (`memcpy`, case 0), reversed
  (case 1, 180°), or transposed (cases 2/3, `fcn.10007890`/`fcn.10007810`).
  It is a **framebuffer rotation**, not a cabling descriptor sent to the card.
  **No packet is transmitted.**

The card's window is configured from the *device* side, `CLTDevice` /
`libCLTDevice`, not from `CLTNic`: a volatile type-`0x0200` card-area pack
([archive/screen-connection-wire.md](archive/screen-connection-wire.md)) and
a persisted 42-byte record in the card's EEPROM at address `0x02`
([receiver-identity.md](receiver-identity.md)). With that record erased the
card drops every pixel while reporting a healthy size.

### 5.1 Bonus: `Nic_SetTestModeIndex` is host-side too

`0x10001fa0` stores the index at `0x101b16c8` and re-sends. `fcn.10006540`
(`0x100065cc`–`0x100065ff`) uses it to *render a pattern into the local
framebuffer* (`fcn.10006d00` for 1–4, `fcn.10006f80` for 5–9, `fcn.10006e20`
for 10–13) which is then sent as ordinary pixel rows. LEDVISION never asks the
card's built-in generator to draw; `e120 card test-mode <n>` does, over the
type-0x33 frame. Confidence: high.

---

## 6. Worked example: one 128-pixel row of a 128×64 panel

Screen created as number 1, row `y = 0`, white pixels, connection style 0.

* `width = 128`, ≥ 16 so no padding (§1.5), < 497 so a single tail packet.
* row field = `base(1) + 0` = `0x0000`
* xoff = `0x0000`, count = `0x0080`
* frame length = `21 + 3·128` = **405 bytes**

```
offset  bytes
  0     11 22 33 44 55 66          dst MAC
  6     22 22 33 44 55 66          src MAC
 12     55                         type
 13     00 00                      row   = 0     (BE)
 15     00 00                      xoff  = 0     (BE)
 17     00 80                      count = 128   (BE)
 19     08 88                      marker
 21     FF FF FF  FF FF FF  …      128 × 3 bytes
404     (last pixel byte)
```

Row 1 is the same with bytes 13,14 = `00 01`. Rows run 0…63.

Full burst for one frame at brightness 0xFF:

```
1.  112 B  11 22 33 44 55 66 | 22 22 33 44 55 66 | 01 07 | 00×21 |
           FF | 05 00 | FF FF FF | 00×71
2.   77 B  11 22 33 44 55 66 | 22 22 33 44 55 66 | 0A | FF FF FF FF | 00×60
3.   64 × 405 B  row packets as above, rows 0…63
    (transmitted back-to-back, no pacing)
```

---

## 7. What `crates/e120-proto/src/pixel.rs` sends

| Field | `pixel.rs` (and FPP `ColorLight-5a-75`) | Vendor |
|---|---|---|
| dst MAC | `11:22:33:44:55:66` | same |
| src MAC | `22:22:33:44:55:66` | same |
| offset 12 | `0x55` | same |
| offsets 13–14 | row, BE | same |
| offsets 15–16 | xoff, BE | same |
| offsets 17–18 | count, BE | same |
| offsets 19–20 | `08 88` | same |
| offset 21+ | 3 B/pixel | same |
| max per packet | 497 | same |
| frame length | `21 + 3n` (405 at n=128) | same |
| sync frame | 112 B, `01 07`, bright at 35/38–40, `0x05` at 36 | same |
| brightness frame | 77 B, `0A`, b,b,b,`FF` at 13–16 | same |

The row format is byte for byte what Colorlight sends; there is no product
branch anywhere in CLTNic (§3). A one-byte-shifted layout (payload starting
at frame offset 14, 406-byte rows) was tried once on the strength of a
white-versus-black current reading (3.1 A → 4.4 A) taken without a control;
the difference was the card's per-run state toggle
([retracted-findings.md](retracted-findings.md)) and the vendor layout is
what the proto tests pin.

What differs is the sequence, not the bytes. The vendor sends latch,
brightness, rows, back to back, latching the previous burst. Measured on
this card ([rendering.md](rendering.md)): brightness, rows, a 500 µs gap,
then three latch frames; one latch never starts the display and two decay.
The row field is plain `y` (screen number 1); no padding applies at 128 px;
the wire colour order is BGR by default (`--order`).

---

## 8. Confidence summary

| Item | Confidence |
|---|---|
| 21-byte pixel header, fields at 13/15/17 BE, `08 88` at 19–20 | **high**: three independent confirmations (template, patch sites, two length computations) |
| 497 pixels/packet, `21 + 3n` frame length | **high** |
| Row field = `(screenNo−1)<<12 + y` for screenNo ≤ 9 | **high** |
| `count` padded to ≥ 16 below 16 px wide | **high** |
| 8 bpc, 3 bytes/pixel from a 32-bit source | **high** |
| Wire colour order is BGR | **medium**: depends on the caller's surface format, not fixed in CLTNic |
| Burst order latch → brightness → rows, one transmit, no pacing | **high** |
| Latch frame 112 B and brightness frame 77 B layouts | **high**: identical to our current code |
| No card-model / scan-mode / chip-family branch anywhere | **high** |
| `SetScreenSize` and `SetScreenConnectionStyle` send nothing | **high** |
| `SetTestModeIndex` renders host-side, never asks the card | **high** |
| Per-channel derivation of brightness bytes `[bright+4..6]` | **high**: linear, traced in CLTNic `0x100062c0` |
| Purpose of the second 12-byte template at `+0x69` (x86) / `+0x9d` (x64), bytes `11 55 44 33 22 11 11 55 44 33 22 22` | **NOT RESOLVED**: no read reference found anywhere in the DLL; possibly dead |
| Meaning of the per-row change-flag array (`node+0x1c` gate, `node+0x20` u16 array, `0x10003601`–`0x10003619`) that lets the vendor skip unchanged rows | **medium**: the mechanism is clear, the producer of the flags is in the caller, not CLTNic |
