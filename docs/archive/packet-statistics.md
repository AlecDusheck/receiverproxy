# Per-receiver packet statistics — how LEDVISION reads them

**Answer in one line:** there is **no dedicated statistics query**. The counters
ride in the ordinary discovery reply (request type `0x0700`, reply type `0x08xx`),
as four big-endian `u32` at payload offsets **37, 41, 45 and 97**; the only
statistics-specific frame in the vendor library is the **clear** command,
frame type **`0x0900`**.

Static reading of `libCLTDevice.1.dylib` (iSet 7 macOS build, C++ symbols
intact) — the same binary `docs/receiver-identity.md` and `docs/eeprom-map.md`
were derived from — plus the LEDVISION 9.6 language resources and a stripped
read of `LedAdmin.dll`. Nothing was executed; nothing touched the network.

Paths:

```
.../scratchpad/iset7pkg/iSet.pkg/Payload/iSet.app/Contents/Frameworks/lib/libCLTDevice.1.dylib
.../scratchpad/libCLTDevice.asm                     (otool -tv of the above)
.../scratchpad/ledvision/$_15_/Language/Hw/iSeries/Multi_eng.ini
.../scratchpad/ledvision/$_15_/x64/Bin/LedAdmin.dll
```

All addresses below are `libCLTDevice.1.dylib` virtual addresses unless stated.

---

## 0. Convention

Every frame builder in this library allocates the **payload starting at the
EtherType slot**, i.e. `buf[0]` is frame offset 12. This document uses:

* `buf[i]` = frame offset `12 + i` (what the builders write);
* `payload[i]` = frame offset `14 + i` (what `crates/e120-proto/src/discovery.rs`
  calls `p[i]` — the byte after the two type bytes).

So `payload[i] == buf[i + 2]`.

The **reply** parser uses a third base: `CReceiverOP::DetectOneRcvInfo`
@ `0x3aa640` receives into an 0x800-byte buffer at `rbp-0x830` and then

```
3aa750  leaq -0x823(%rbp), %rsi     ; = rxframe + 13
3aa757  leaq -0xa08(%rbp), %rdi
3aa75e  movl $0x1d2, %edx           ; 466 bytes
3aa763  callq _memcpy
```

so `CReceiverInfo::InitRcvBuf08Ex`'s `buf` is **`rxframe + 13`**, one byte
before `payload[0]`. Its `buf[n]` is our `payload[n-1]`. That is confirmed
independently: it reads the card type at its `buf[1]` and the detected size at
`buf[0x15]`/`buf[0x17]`, which are our `payload[0]` and `payload[20]`/`[22]` —
exactly the fields `parse_discovery_response` already reads. **HIGH.**

---

## 1. The request frame

### 1.1 The plain discovery request — this is all you need

`BuildDetectRcvCard(unsigned int*, unsigned char**, unsigned short)`
@ `0x30a370`:

```
30a390  movl $0x110, %edi            ; allocate 272 bytes
30a398  callq __Znam
30a3a0  leaq 0x5(%rax), %rdi
30a3a4  movl $0x10b, %esi
30a3a9  callq ___bzero               ; zero buf[5..0x10f]
30a3ae  movw $0x7, (%rbx)            ; buf[0..1] = 07 00
30a3b3  movb $0x0, 0x2(%rbx)         ; buf[2]    = 00
30a3ba  movb %ah, 0x3(%rbx)          ; buf[3]    = index >> 8
30a3bd  movb %al, 0x4(%rbx)          ; buf[4]    = index & 0xff
30a3c0  movl $0x110, (%r14)          ; length 272
```

| frame off | bytes | meaning |
|---|---|---|
| 0–5 | `11 22 33 44 55 66` | dest MAC (vendor constant) |
| 6–11 | `22 22 33 44 55 66` | src MAC |
| 12–13 | `07 00` | frame type |
| 14 | `00` | reserved |
| 15–16 | `xx xx` | **receiver index, big-endian** (`FFFF` = broadcast) |
| 17–283 | `00` × 267 | zero pad; payload is always 272 bytes |

Total frame **284 bytes**. This is byte-for-byte what
`e120-proto::discovery()` already emits (`frame([0x07,0x00], &[0u8; 270])`)
with index `0x0000`. **HIGH.**

Sent by `DetectOneRcvInfo` @ `0x3aa6b5`/`0x3aa720` through the device-IO hook
at vtable `+0x48` with reply selector `edx = 0xff08` and a **600 ms** timeout
(`pushq $0x258` @ `0x3aa714`).

### 1.2 The "extended detect" family (for completeness)

Four other builders share one layout, discovered by comparing
`BuildDetectRcvCardInfo` @ `0x30a460`, `BuildQucikDetectRcvCard` @ `0x30a710`,
`BuildDetectRcvMonitorExInfo` @ `0x30a4f0` and `BuildDetectLastRcvCardInfo`
@ `0x30a5c0` (the last two load their headers from `__TEXT,__const` at
`0x4240a0` and `0x4240c0`, dumped below):

```
0x4240a0: 07 00 00 ff ff ff 01 0e 00 00 01 00 02 00 03 00
0x4240b0: 04 00 05 00 06 00 07 00 08 00 09 00 0a 00 0b 00
          (+ movabs 0x97835743000D000C at buf[0x20])
0x4240c0: 07 00 00 ff ff ff 01 02 54 00 55 00 43 57 83 97
```

| buf | size | meaning |
|---|---|---|
| 0–1 | 2 | `07 00` type |
| 2 | 1 | `00` |
| 3–4 | 2 | receiver index BE (`FF FF` broadcast) |
| 5 | 1 | `FF` — "extended request" marker |
| 6 | 1 | `01` broadcast form, `00` in the indexed form |
| 7 | 1 | `N` = number of 16-bit item ids that follow |
| 8 … | 2N | item ids, **little-endian** u16 |
| … | 4 | magic `43 57 83 97` |
| … | 1 | **sub-command** |
| … | — | zeros to 272 bytes total |

Observed sub-commands: `0x07` receiver monitor-ex (with N=14, ids 0…13),
`0x09` upgrade-descriptor query (N=0 — this is exactly `upgrade_info()` in
`discovery.rs`, which independently confirms the layout), `0xF3` "last receiver
index" (N=2, ids `0x0054`,`0x0055`, `CReceiverOP::DetectLastRcvInfo`
@ `0x3cb460`, `movl $0xf3,%edx` @ `0x3cb4c6`).

**None of these sub-commands is a statistics query.** `BuildDetectRcvCardInfo`
has no caller at all in this build. **HIGH** (the layout), **HIGH** (that no
statistics sub-command exists in this library).

---

## 2. The reply

Type bytes `08 xx` at frame 12–13 (`0x08` is what the reply selector `0xff08`
masks on; our capture shows `08 05`, and `crates/e120-proto` already keys on
`08 05`). The parser copies **466 bytes from frame offset 13**, so a full reply
is at least 479 bytes.

`CReceiverInfo::InitRcvBuf08Ex(unsigned char*, SRcvCardInfo*)` @ `0x39d670` is
the whole decoder. Selected fields, by *payload* offset (`= buf - 1`):

| payload | size / type | → `SRcvCardInfo` | instruction | meaning |
|---|---|---|---|---|
| 0 | u8 | `+0x10` | `0x39d6b5` | card type / model id |
| 1 | u8 | `+0x14` | `0x39d778` | firmware major |
| 2 | u8 | `+0x18` | `0x39d783` | firmware minor |
| 3 | s8 | `+0x5c` (float) | `0x39d7cf` | temperature, integer part |
| 4 | u8 | ″ | `0x39d7da` | temperature, fraction (`×0.01`) |
| 5 | u8 | `+0x64` (float) | `0x39d7fe` | supply voltage, integer part |
| 6 | u8 | ″ | `0x39d80c` | voltage, fraction (`/10`) |
| 7–8 | u16 BE | `+0xea` | `0x39db3c` | (unnamed) |
| 16–17 | **u16 BE** | `+0x38` | `0x39d714` | **control-area startX** |
| 18–19 | **u16 BE** | `+0x3c` | `0x39d724` | **control-area startY** |
| 20–21 | **u16 BE** | `+0x40` = `endX-startX` | `0x39d734` | **control-area endX** |
| 22–23 | **u16 BE** | `+0x44` = `endY-startY` | `0x39d746` | **control-area endY** |
| 28 | u8 | `+0x24` | `0x39d763` | |
| 29–36 | 8 B | `+0x25` | `0x39d758` | serial / MAC-like blob |
| **37–40** | **u32 BE** | **`+0x9c`** | `0x39d93c` `movl -0x5d2(%rbp),%ecx` / `bswapl` / `movl %ecx,0x9c(%r15)` | **counter A** |
| **41–44** | **u32 BE** | **`+0xa0`** | `0x39d94b` `movl -0x5ce(%rbp),%r8d` / `bswapl` / `movl %r8d,0xa0(%r15)` | **counter B** |
| **45–48** | **u32 BE** | **`+0x68`** | `0x39d82e` `movl -0x5ca(%rbp),%ecx` / `bswapl` / `movl %ecx,0x68(%r15)` | **counter C** |
| 65–68 | u32 raw (LE, not swapped) | `+0x89` | `0x39dcfb` | IPv4-shaped; byte-sum-zero flag stored at `+0x87` |
| 80 | u8 | `+0x105` | `0x39dbfd` | |
| 81 | u8 | `+0xa8` (float) | `0x39d8c0` | |
| 82 | u8 | `+0xa4` (float, scaled) | `0x39d899` | |
| 83–84 | u16 BE | `+0x0c` | `0x39d6a5` | receiver id / index echo |
| 86, 90, 95 | u8 pairs | `+0x70…0x7c` (4 floats) | `0x39d83a`–`0x39d894` | four more analogue readings |
| **97–100** | **u32 BE** | **`+0x6c`** | `0x39d7c3` `movl -0x596(%rbp),%eax` / `bswapl` / `movl %eax,0x6c(%r15)` | **counter D** |
| 101 | BCD | `+0xac` (`2000+`) | `0x39d975` | date/RTC year |
| 102–106 | BCD | `+0xae…0xb8` | `0x39d997`–`0x39da0c` | date/RTC month/day/h/m/s |
| 114 | u8 flag | `+0x34` | `0x39d7a4` | |
| 115 | u8 | `+0x30` | `0x39d76d` | HUB / board type candidate |
| 125 | u8 | `+0x86`,`+0x88` | `0x39d95c` | |
| 155–161 | u8 flags | `+0xd7…0xdd` | `0x39daab`+ | capability bits |
| 162–169 | 8 B | `+0xde` | `0x39db12` | sub-version block (`M3`/`LCD`/`ARM %d.%02d`, see `0x3c8e9b`) |
| 429 | struct | — | `0x39e5c9` | `SRcvDetectCustomAnswer` tail, parsed by `InitRcvBuf08CustomDetect` |

Two things are worth flagging beyond the statistics question:

* **payload 16–23 is a read-back of the EEPROM control area.** The parser
  computes `+0x40 = payload[20..21] - payload[16..17]` and
  `+0x44 = payload[22..23] - payload[18..19]`. `parse_discovery_response` in
  `crates/e120-proto/src/discovery.rs` reads `payload[20..23]` as
  "cols/rows" — that is `endX/endY`, correct only while `startX = startY = 0`,
  exactly the trap `docs/receiver-identity.md` §1 documents for EEPROM `0x02`.
  **The discovery reply gives you `startX`/`startY` for free at payload 16–19**
  and would have caught the empty-window fault immediately. **HIGH.**
* Everything is **big-endian** except the IPv4-shaped word at payload 63–66,
  which is copied raw.

### 2.1 What the counters are called

LEDVISION's receiver table has exactly these columns
(`$_15_/Language/Hw/iSeries/Multi_eng.ini`):

```
IDS_DEVICE_INFO_INDEX=Index
IDS_DEVICE_INFO_TYPE=Type
IDS_DEVICE_INFO_SUPPORT_CHIP=Supported Chip
IDS_DEVICE_INFO_NETWORK_PACKET_COUNT=Network Packet
IDS_DEVICE_INFO_ERROR_PACKET_COUNT=Error Packet
IDS_DEVICE_INFO_ERROR_RUN_TIME=Run Time
IDS_DEVICE_INFO_ERROR_HUB_TYPE=HUB Type
IDS_DEVICE_INFO_RESET_NETWORK_PACKET=Reset Network Packet
LedAdmin_HW_TotalPack=Network Packet
LedAdmin_HW_TotalPackE=Error Packet
LedAdmin_HW_TotalPackERatio= Error Ratio
LedAdmin_HW_TotalTime=Run Time
```

So the UI shows exactly three per-receiver counters — **Network Packet, Error
Packet, Run Time** — plus a derived Error Ratio. Those three must be three of
the four `u32` above.

**Which is which is NOT RESOLVED.** The library never names them: `SRcvCardInfo`
has no accessors, the public `CReceiverOP::DetectReceiverCardsInfo` @ `0x3c8b70`
(which flattens `SRcvCardInfo` into the 188-byte `SReceiverCardInfo` for the
SDK) copies temperature and a status byte but **not** any of these four fields,
and the consumer is `LedAdmin.dll` — a stripped MSVC x64 build of a different
CLTDevice generation whose struct offsets do not match the macOS build, so
cross-matching `+0x9c`/`+0xa0` there produced only false positives.

What the code *does* establish:

* they form **two adjacent pairs**: `+0x9c`/`+0xa0` (payload 37 / 41) and
  `+0x68`/`+0x6c` (payload 45 / 97);
* `+0x68`/`+0x6c` sit inside the analogue-monitoring block of the struct
  (`+0x5c` temperature, `+0x64` voltage, `+0x70…0x7c` four more floats), which
  argues for that pair being the environment/run-time side;
* `+0x9c`/`+0xa0` sit in the flags/identity region.

Reconciling with the bench numbers in the brief:

* payload 37–40 (`+0x9c`) advances during pixel streaming and **not** during
  brightness-only traffic. A pure "total packets received" counter would move
  for brightness frames too, so `+0x9c` is more likely a **video/pixel-specific**
  counter than the plain network-packet total. The magnitude also disagrees with
  a raw packet count: ~60 counts for ~20 000 packets and ~300 frames is neither.
* payload 45–48 (`+0x68`) free-runs at ~5/s when idle. That is consistent with
  either a **run-time tick** (200 ms units) or a **total-received-packets**
  counter being fed by background/poll traffic on the link.

Both readings survive the evidence. **Do not treat either as established.**

#### The experiment that settles it

Two isolated deltas, each read with `e120 discover` before and after:

1. **Link-only:** send nothing but exactly 100 discovery requests over ~20 s.
   Whichever field advances by ≈100 is *Network Packet* (total received);
   whichever advances by ≈20 (or ≈100 at 5 Hz) independent of the request count
   is *Run Time*.
2. **Pixel-only:** send exactly 1000 type-`0x55` pixel packets in one burst with
   no other traffic. The field that advances by exactly 1000 is the packet
   total; a field that advances by a small fixed number is frame- or
   block-scoped.

Run `e120 debug send --type 0900 --pad 270 --payload 00ffff` (§4) first so both
runs start from zero.

---

## 3. Is any of this in the ordinary 0x07/0x08 discovery exchange? — **Yes, all of it**

There is no separate statistics request anywhere in `libCLTDevice`. The
complete list of `Build*` frame constructors (`nm -g | c++filt | grep '^.* T Build'`,
66 functions) contains no packet/statistics detector; the only statistics
symbol is the clear command in §4. `CReceiverOP` has no `Detect*Statistical*`
or `Detect*PackCount*` member. The receiver "monitor" path
(`BuildDetectRcvMonitorExInfo` @ `0x30a4f0`,
`CReceiverOP::DetectOneReceiverMonitorExInfo` @ `0x3aea80`) is a **different**
thing: it is sent with command selector `0x807b`, its reply is a stream of
64-byte records each beginning `0xEE` with a validity bit at record byte 7, and
`CReceiverInfo::InitRcvMonitorExInfo08Ex` @ `0x39e810` extracts a single 8-byte
field from record+15 — module temperature/humidity monitoring, not packets.
**HIGH.**

So: the answer to question 3 is that the counters are **only** in the 0x08
reply, and the two `u32`s the bench saw at payload 37–40 and 44–48 are
`InitRcvBuf08Ex`'s `+0x9c` (payload 37, `0x39d93c`) and `+0x68` (payload 45,
`0x39d82e`) respectively. The bench's "44–48" is one byte off; the field is
**45–48**, and there is a third at 41–44 and a fourth at 97–100 the bench had
not spotted. **HIGH** on the offsets, **NOT RESOLVED** on the names.

---

## 4. Clear statistics

`CReceiverOP::ResetRcvStatisticalData(unsigned int, int)` @ `0x3b34e0`:

```
3b3521  movl $0xffff, %edx                  ; receiver index = broadcast
3b3526  callq BuildClearPackCount(unsigned int*, unsigned char**, unsigned short)
3b3545  movl $0x807d, %edx                  ; send selector, no reply expected
3b354c  movl $0x2, %r9d
3b3556  callq *0x38(%rbx)                   ; CDeviceSetIO::SendData
```

`BuildClearPackCount` @ `0x30a9d0` is byte-identical to `BuildDetectRcvCard`
apart from the type word:

```
30a9f0  movl $0x110, %edi         ; 272-byte payload
30aa00  leaq 0x5(%rax), %rdi
30aa04  movl $0x10b, %esi
30aa09  callq ___bzero
30aa0e  movw $0x9, (%rbx)         ; buf[0..1] = 09 00
30aa13  movb $0x0, 0x2(%rbx)
30aa1a  movb %ah, 0x3(%rbx)       ; index >> 8
30aa1d  movb %al, 0x4(%rbx)       ; index & 0xff
30aa20  movl $0x110, (%r14)
```

| frame off | bytes | meaning |
|---|---|---|
| 12–13 | `09 00` | frame type |
| 14 | `00` | reserved |
| 15–16 | `ff ff` | receiver index BE — the vendor always broadcasts |
| 17–283 | `00` × 267 | pad |

Send selector `0x807d` with `r9d = 2` is the same fire-and-forget path used by
`ReLoadLocalParam` (`docs/screen-connection-wire.md`), so **no reply is
expected**. The frame carries no write opcode and no data, so it cannot touch
flash or EEPROM. **HIGH.**

---

## 5. Ready-to-run

Read the statistics (this is just discovery — `e120 discover` already sends it;
use `debug send` when you want the raw reply bytes):

```sh
e120 debug send --type 0700 --pad 270 --payload 000000 --show 128
```

284-byte frame: `07 00 | 00 | 00 00 | 267 × 00`. Replace `00 00` with the
receiver index, or `ff ff` to broadcast.

Parse the reply (`p = eth_frame[14..]`, all big-endian):

```
p[0]        u8    card type
p[1], p[2]  u8    firmware major, minor
p[3], p[4]  s8,u8 temperature   = p[3] + p[4]*0.01
p[5], p[6]  u8    voltage       = p[5] + p[6]/10
p[16..18]   u16   control-area startX
p[18..20]   u16   control-area startY
p[20..22]   u16   control-area endX      (repo currently calls this "cols")
p[22..24]   u16   control-area endY      (repo currently calls this "rows")
p[37..41]   u32   counter A   -> SRcvCardInfo+0x9c
p[41..45]   u32   counter B   -> SRcvCardInfo+0xa0
p[45..49]   u32   counter C   -> SRcvCardInfo+0x68
p[97..101]  u32   counter D   -> SRcvCardInfo+0x6c
p[115]      u8    HUB/board type candidate
```

Three of A–D are LEDVISION's *Network Packet*, *Error Packet* and *Run Time*;
which is which is unresolved (§2.1).

Clear the counters on every card:

```sh
e120 debug send --type 0900 --pad 270 --payload 00ffff --wait 0
```

284-byte frame `09 00 | 00 | ff ff | 267 × 00`. For one card only, put its
index in place of `ffff`.

---

## 6. What could not be resolved

* **The names of the four counters.** The library never labels them; the
  labelling lives in `LedAdmin.dll`, a stripped build against a different
  `SRcvCardInfo` layout. §2.1 gives the experiment that decides it in two
  measurements.
* **The fourth counter.** Only three appear in the UI; one of A–D is something
  else entirely (a second time base, a dropped-frame count, a CRC-error count).
* **Whether the card counts *all* Ethernet frames or only Colorlight ones**, and
  whether "error packet" means CRC/FCS errors, sequence gaps, or checksum
  failures. Nothing in the host library says; the counting happens in the
  gateware.
* **The second reply type byte.** The vendor matches on `0xff08` (high byte
  `0x08`) and never inspects `frame[13]`; our capture shows `05`, but whether
  that byte varies by card family is untested.
* **Whether `0x0900` clears all four counters or only the packet pair.** The
  symbol is `ResetRcvStatisticalData` / `BuildClearPackCount` and the UI button
  is "Reset Network Packet"; the frame carries no selector, so it is one
  all-or-nothing reset — but which fields it resets can only be seen on the
  bench.
* **`CReceiverOP::DetectR8BitErrorRate` / `DetectNewR8BitErrorRate`**
  (`0x3ca380`, `0x3ca5b0`, builders at `0x30d650`/`0x30d6f0`) were not analysed.
  They are the "R8 Error Rate" tool for R8-class *sender/fibre* hardware, a
  different link layer from the E120's plain Ethernet, and the E120 datasheet's
  "bit error detection" wording maps onto the packet counters above, not onto
  these.
