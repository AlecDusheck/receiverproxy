# Pixel wire format

The frames that carry pixels to the card: the row packet (type `0x55`), the
latch frame (type `0x0107`) and the brightness frame (type `0x0A`). The byte
layouts are those of Colorlight's own sender library, `CLTNic.dll`, read
statically; nothing was executed. `crates/colorlight/src/pixel.rs` emits the
same bytes and the proto tests pin them.

## Sources

| Tag | File | Notes |
|---|---|---|
| **x86** | `<scratch>/ledup/x52b/x86/Bin/CLTNic.dll` (2025-04-30, 1 892 904 B) | 32-bit, `__stdcall`, decorated exports carry argument sizes. Image base `0x10000000`. Every address below is from this build unless tagged x64. |
| **x64** | `<scratch>/ledvision/$_15_/x64/Bin/CLTNic.dll` (2024-06-10, LEDVISION 9.6) | Image base `0x180000000`. Confirms the templates are unchanged in the newest release. |

`<scratch>` is a scratch directory outside the tree. The vendor installers
(`LEDVISION_Setup_x64_9.6.49150.exe`, LEDUpgrade x52b) unpack with `7z x`;
no vendor program is run.

There is no macOS build of CLTNic. `libCLTDevice.1.dylib` (iSet) holds the
device and configuration side only: no `Nic_*` symbols, no picture sender.
Six copies of CLTNic exist across the vendor packages; all export the same 25
symbols, and the x86 and x64 builds agree byte for byte on the frame
templates.

CLTNic is the only sender library LEDVISION ships (`$_15_/x64/Bin/` has no
alternative), so there is one pixel format for the whole product line.

---

## 1. The pixel-data frame

### 1.1 Frame template

The frame header is a template built once in the pcap-wrapper constructor and
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

x64 `fcn.1800030f0`, `0x180003104`–`0x180003130`: identical values at
`rcx+0x78`/`+0x84`/`+0x85`/`+0x89`. The template base is `+0x44` (x86) /
`+0x78` (x64). All offsets below are relative to that base, so they are frame
offsets.

### 1.2 Layout

| Offset | Size | Value | Meaning |
|---|---|---|---|
| 0 | 6 | `11 22 33 44 55 66` | destination MAC (the card) |
| 6 | 6 | `22 22 33 44 55 66` | source MAC (the sender) |
| 12 | 1 | `0x55` | frame type / first EtherType byte |
| 13 | 1 | row >> 8 | row index, high byte |
| 14 | 1 | row & 0xFF | row index, low byte |
| 15 | 1 | xoff >> 8 | first pixel index in the row, high byte |
| 16 | 1 | xoff & 0xFF | first pixel index, low byte |
| 17 | 1 | count >> 8 | pixels in this packet, high byte |
| 18 | 1 | count & 0xFF | pixels in this packet, low byte |
| 19 | 1 | `0x08` | fixed marker |
| 20 | 1 | `0x88` | fixed marker |
| 21 | 3·count | pixel bytes | 3 bytes per pixel |

21-byte header, three 16-bit big-endian fields at offsets 13, 15 and 17, the
`08 88` marker at 19–20, pixels from 21. Total frame length `21 + 3·count`.

Patch sites, x86 `fcn.10003580` (the row packetizer):

| Field | Store | Address |
|---|---|---|
| row hi/lo | `mov [edi+0x51], al` / `mov [edi+0x52], bl` | `0x10003630`, `0x10003652` |
| xoff hi/lo | `mov [edi+0x53], al` / `mov [edi+0x54], al` | `0x100036a4`, `0x100036b7` (full packet); `0x10003788`, `0x1000379b` (tail) |
| count hi/lo | `mov word [edi+0x55], 0xf101` (= `01 F1` = 497 BE) | `0x1000369b` (full packet) |
| count hi/lo | `mov [edi+0x55], al` / `mov [edi+0x56], al` | `0x10003763`, `0x10003777` (tail) |

The `0xf101` store fixes the endianness: 497 decimal is written as the byte
pair `01 F1` at offsets 17,18, big-endian, after the row and offset fields.

### 1.3 Frame length

Three independent computations agree on the 21-byte header:

* `0x10003684` / `0x10003692`: for a maximum-size packet the pcap header's
  `caplen` and `len` are both `0x5E8` = 1512 = 21 + 3·497.
* `0x10003749`: for the tail packet, `len = (count + 7) * 3` = `3·count + 21`.
* `fcn.100067d0` (send-queue sizing, `0x10006848`–`0x10006897`) uses
  `0x5F8` = 1528 bytes per full packet and a `0x25` = 37-byte per-packet
  overhead. 1528 − 1491 = 37 = 16-byte `pcap_pkthdr` + 21-byte frame header.

The record copied into the pcap send queue (`0x100036c0`–`0x100036e2`) is
`16 + 16 + 4 + 1 = 37` bytes: pkthdr, then frame bytes 0–15, then 16–19, then
byte 20.

### 1.4 Maximum pixels per packet

497 (`0x1F1`), hard-coded at `0x10003660`, `0x10003676`, `0x100036ba`,
`0x10003678` and in the queue sizing at `0x10006848`. A row wider than 497 is
split into `floor(w/497)` full packets of 497 plus one tail packet of
`w mod 497`. FPP's `CL_MAX_PIXL_PER_PACKET` has the same value.

### 1.5 Minimum pixel count

`0x1000359b`–`0x100035a7`: if the screen width is below 16, the packet's
`count` is raised to 16 and the extra pixels are zero-filled
(`0x10003722`/`0x100037ff` call `memset`). `pixel.rs` does not implement this
rule; it does not apply at 128 px wide.

### 1.6 Row field and screen number

`0x100035c5`–`0x100035e3` computes a row base from the screen number
(`node+8`, the `Nic_CreateScreen` screen id, 1…256):

```
if (n <= 9)  base = (n - 1) << 12          ; 0, 4096, 8192, …
else         base = 320 * (n - 9) + 0x8000
```

The transmitted row field is `base + y` (`add ebx, [var_ch]` at
`0x1000361f`). For a single screen created as number 1 the base is 0 and the
field is plain `y`. The high nibble of the 16-bit row field is therefore a
screen/port selector; a screen number other than 1 shifts every row by 4096.

The `row` and `x-offset` fields are absolute coordinates in the whole display.
Each receiver keeps the pixels inside its own window, held in the 42-byte
EEPROM record at address `0x02` ([receiver-identity.md](receiver-identity.md)).

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

8 bits per channel, 3 bytes per pixel, copied verbatim from the low three
bytes of a 32-bit source pixel.

Which colour those three bytes carry is decided by the caller, not by CLTNic:
`Nic_CreateScreen` (`0x10001ec0` → `fcn.10005f80`) takes no pixel-format
argument, and `fcn.10006540` allocates the buffer as `width*height*4`
(`0x1000659f`–`0x100065c7`). LEDVISION feeds it GDI/GDI+ 32bpp surfaces,
whose memory order is B,G,R,A, so the wire order is B,G,R. Inferred from the
caller's surface format, not fixed in CLTNic. `rxp --order` defaults to
`bgr`; measured: `bgr` renders the right colours on the bench panel.

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

One video frame for one screen is one pcap send-queue burst, in this order:

1. Latch frame, type `0x0107`, 112 bytes, first.
2. Brightness frame, type `0x0A`, 77 bytes.
3. All pixel row packets, row 0 upward, and within a row left to right.

Steps 1–2 are emitted only for the screen selected by
`Nic_SetSendParamScreenNumber` (global `0x101b16b8`, default 1 at
`0x10005a64`). With a single screen they are emitted on every frame.

Because the latch frame leads the burst, it latches the previous frame's row
data. Over a continuous stream this is equivalent to rows-then-latch; the
difference shows only on the first burst.

### 2.1 Pacing

None. The whole burst goes to `pcap_sendqueue_transmit` with `sync = 0`
(`fcn.10003850` → `fcn.100038f0`, `0x10003883` pushes 0 as the sync flag), so
packets go out back to back at line rate with no timestamp pacing. After the
burst `Run` issues three `Sleep(0)` calls (`0x1000901a`–`0x10009024`) and the
thread runs at `THREAD_PRIORITY_ABOVE_NORMAL` (`0x10005cd1`). There is no
inter-packet delay.

### 2.2 The latch frame (type 0x0107, 112 bytes)

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
| 38 | 1 | channel gain 0 (R) | `0x100034d7`, `[bright+4]` |
| 39 | 1 | channel gain 1 (G) | `0x100034de`, `[bright+5]` |
| 40 | 1 | channel gain 2 (B) | `0x100034e5`, `[bright+6]` |
| 41–111 | 71 | `0x00` | `memset` at `0x100034b6` |

Byte-identical to what `pixel::sync_gains()` emits.

`bright` is the 7-byte block at `0x101b1664` written by `Nic_SetBrightness`
(`_Nic_SetBrightness@24` at `0x10001f60`, which forwards arguments 2–4 to the
worker `fcn.100062c0` at `0x10001f74` and drops arguments 5 and 6). Its caller
`_CLTProcessorSetBrightness@12` (CLTDevice `0x10177fa0`) computes
`M = percent / 100.0f` (`0x10178004`, divisor `100.0f` at `0x10462c68`) and
`cR/cG/cB = roundf(255 * k)`, where `k` is `1.0f` at neutral colour
temperature (`0x10178206`) or a Kelvin-to-RGB table lookup (`fcn.100065b0`,
16-byte records `{int K; float r,g,b;}` from `0x104b85f0`). `fcn.1013d1f0`
is `pow(double, double)` (Intel SSE2 libm: `stmxcsr` at `0x1013d1f3`,
mantissa split `psrlq xmm0,0x2c` at `0x1013d244`, reciprocal table
`0x10181390`); `0x10125173` is `roundf` (half away from zero).

With `Mb = roundf(M * 255)`:

| global | frame byte | formula | citations |
|---|---|---|---|
| `0x101b1667` | `0x0107` byte 35 (master) | `Mb` | `0x100062d4`, `0x100062e3`, `0x10006336` |
| `0x101b1668` | `0x0107` byte 38 (R gain) | `round(cR * Mb / 255)` | `0x1000637d`, `0x10006385`, `0x10006393`, `0x100063c9` |
| `0x101b1669` | `0x0107` byte 39 (G gain) | `round(cG * Mb / 255)` | `0x10006355`–`0x100063d7` |
| `0x101b166a` | `0x0107` byte 40 (B gain) | `round(cB * Mb / 255)` | `0x10006322`–`0x100063e2` |

The gains are linear: no gamma, no offset. At neutral colour temperature all
three equal `Mb`, which is what `pixel::sync(b)` sends (`gains = [b; 3]`).
`pow` is used only for the type-`0x0A` frame's bytes 13–15 (§2.3).

The gain bytes are live: measured on the bench panel, a gain sweep of
0/4/12/40/120 with all-black content changed the supply current
0.47/0.71/0.75/0.86/1.08 A while the black floor was present
([rendering.md](rendering.md)).

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

`[bright+0..2]` are gamma-shaped: `gk = round(255 * (ck/255)^0.4)`,
`Mg = round(255 * M^0.4)`, byte = `round(gk * Mg / 255)` (`0x10006306`,
`0x100063e7`, `0x10006424`, `0x10006469`; stores `0x10006513`,
`0x1000651e`, `0x10006529`). `pixel::brightness(b)` sends `b, b, b, 0xFF`
without the `pow` shaping; the frame layout is otherwise byte-identical.

The vendor sends this frame on every video frame, after the latch frame and
before the rows. `Wall::show` sends it first in every refresh.

---

## 3. Card-model independence

CLTNic has no notion of card model, scan mode, driver-chip family or panel
geometry. Nothing in the pixel path depends on any of them:

* The only branches in the send path are the screen-rotation switch
  (`fcn.100068e0` `0x100069f0`, 4 cases, a local framebuffer transform), the
  test-pattern switch (`fcn.10006540` `0x100065e4`, host-side rendering), and
  the 497-pixel packet split.
* The frame templates are compile-time constants in the constructor. There is
  no second template, no lookup table, no configuration read.
* The x86 (2025) and x64 (LEDVISION 9.6, 2024) builds have identical
  templates and identical constants.
* `$_15_/x64/Bin/` ships exactly one sender DLL, used for every product.

The only things that vary with configuration are the screen number (§1.6) and
the pixel count per row.

---

## 4. `Nic_SendScreenBlackPicture`

`0x10001e60` → `fcn.10006670`. It builds no special frame. It:

1. looks the screen up by id (`0x100066e3`),
2. takes a free frame node (`fcn.10006b80`, `0x1000674f`),
3. `memset(node->pixels, 0, width * height * 4)` (`0x1000675a`–`0x1000676d`),
4. pushes it to the send queue (`fcn.10006aa0`, `0x10006776`).

The wire result is an ordinary full frame: latch frame, brightness frame, then
every row packet with all-zero pixel bytes.

`Nic_SendScreenPicture` (`0x10001e40`) routes to the same path when the show
flag (`0x101a7470`, set by `Nic_SetScreenShowOnOff`) is 0.

There is exactly one pixel path; a black panel is the full sequence with zero
pixels.

---

## 5. `Nic_SetScreenSize` and `Nic_SetScreenConnectionStyle`

Both are purely host-side. Video is not gated on either. Neither transmits a
packet.

* `Nic_SetScreenSize(id, w, h)` (`0x10001f00` → `fcn.10006170`). Rejects
  dimensions above `0x100000`, finds the screen, calls two setters
  (`fcn.10008850`, `fcn.100088f0`) that store width and height in the screen
  object, then calls `fcn.100067d0` + `fcn.10002e80`, which only free and
  reallocate the local pcap send queue to fit the new worst-case burst size
  (`fcn.10002e80` calls `pcap_sendqueue_destroy` at `[obj+0x28]` and
  `pcap_sendqueue_alloc` at `[obj+0x18]`).

* `Nic_SetScreenConnectionStyle(id, style)` (`0x10001f10` → `fcn.100060f0`).
  Validates `style ∈ {0,1,2,3}` (`0x10006139`–`0x10006148`) and does one
  store: `mov [eax+0x10], esi` (`0x10006159`). That field is read in
  `fcn.100068e0` at `0x100069e3` and drives a 4-case switch (`0x100069f5`)
  that copies the user's image into the node buffer straight (`memcpy`, case
  0), reversed (case 1, 180°), or transposed (cases 2/3, `fcn.10007890` /
  `fcn.10007810`). It is a framebuffer rotation, not a cabling descriptor.

The card's window is configured from the device side (`CLTDevice` /
`libCLTDevice`), not from `CLTNic`, in two forms:

* a volatile card-area pack, wire type `02 00`, 1284 bytes: byte 3 is the
  pack index, then 128 entries of 10 bytes carrying left/top/right/bottom as
  big-endian u16 (`GetCardAreaParamPacks` at libCLTDevice `0x38a1c0`, sent by
  `SendRealTimePacks` at `0x37ec00`). A power cycle discards it. Provisioning
  does not use it; `rxp debug send --type 0200` can emit it by hand
  ([receiver-identity.md](receiver-identity.md) §6.0);
* a persisted 42-byte record in the card's EEPROM at address `0x02`
  ([receiver-identity.md](receiver-identity.md)). Measured: with that record
  erased (`startX = startY = 0xFFFF`) the card drops every pixel while
  `discover` reports a healthy 128x64.

### 5.1 `Nic_SetTestModeIndex`

Host-side as well. `0x10001fa0` stores the index at `0x101b16c8` and
re-sends. `fcn.10006540` (`0x100065cc`–`0x100065ff`) uses it to render a
pattern into the local framebuffer (`fcn.10006d00` for 1–4, `fcn.10006f80`
for 5–9, `fcn.10006e20` for 10–13), which is then sent as ordinary pixel rows.
LEDVISION never asks the card's built-in generator to draw. `rxp card
test-mode <n>` does, over the type-`0x33` frame.

---

## 6. Worked example: one 128-pixel row of a 128×64 panel

Screen created as number 1, row `y = 0`, white pixels, connection style 0.

* `width = 128`, ≥ 16 so no padding (§1.5), < 497 so a single tail packet.
* row field = `base(1) + 0` = `0x0000`
* xoff = `0x0000`, count = `0x0080`
* frame length = `21 + 3·128` = 405 bytes

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

Full vendor burst for one frame at brightness 0xFF:

```
1.  112 B  11 22 33 44 55 66 | 22 22 33 44 55 66 | 01 07 | 00×21 |
           FF | 05 00 | FF FF FF | 00×71
2.   77 B  11 22 33 44 55 66 | 22 22 33 44 55 66 | 0A | FF FF FF FF | 00×60
3.   64 × 405 B  row packets as above, rows 0…63
    (transmitted back-to-back, no pacing)
```

---

## 7. `crates/colorlight/src/pixel.rs` against the vendor

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
| max per packet | 497 (`MAX_PIXELS_PER_PACKET`) | same |
| frame length | `21 + 3n` (405 at n=128) | same |
| latch frame | 112 B, `01 07`, bright at 35, gains at 38–40, `0x05` at 36 | same |
| brightness frame | 77 B, `0A`, b,b,b,`FF` at 13–16 | same layout; vendor bytes 13–15 are `pow`-shaped (§2.3) |
| row field | plain `y` (screen number 1) | `base(n) + y` |
| padding below 16 px | not implemented | count raised to 16 |

The row bytes are what Colorlight sends, and CLTNic has no product branch
(§3). A layout with the payload starting at frame offset 14 (406-byte rows)
is wrong: measured, it turns the panel into a 5 Hz strobe. The white-versus-
black current difference (3.1 A → 4.4 A) that once suggested that layout was
the card's per-run state toggle, read without a same-content control
([retracted-findings.md](retracted-findings.md)).

The sequence differs from the vendor's, not the bytes. The vendor sends
latch, brightness, rows, back to back, latching the previous burst. Measured
on this card with firmware 16.53 ([rendering.md](rendering.md)): brightness,
64 row packets, a 500 µs gap, then three latch frames. One latch never starts
the display; two render and decay; three hold. `driver::Timing::default()`
carries those values.

---

## 8. Limits and unresolved

* Wire colour order B,G,R is inferred from the caller's 32bpp surface format
  (§1.7); CLTNic itself fixes no colour order.
* The second 12-byte template at `+0x69` (x86) / `+0x9d` (x64), bytes
  `11 55 44 33 22 11 11 55 44 33 22 22`: not resolved. No read reference was
  found anywhere in the DLL; possibly dead.
* The per-row change-flag array (`node+0x1c` gate, `node+0x20` u16 array,
  `0x10003601`–`0x10003619`) lets the vendor skip unchanged rows. The
  skipping mechanism is clear; the producer of the flags is in the caller,
  not in CLTNic, and is not traced.
