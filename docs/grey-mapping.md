# Grey mapping: how 8-bit input becomes an N-bit word, and where the black floor is not

Static analysis of the vendor libraries, answering the four questions raised by
"Black is not off" in [rendering-recipe.md](rendering-recipe.md). Nothing was
executed; every address below is a byte offset in a file on disk.

Primary source: the macOS build `libCLTDevice.1.dylib` (iSet 7, C++ symbols
intact). Addresses are its `__TEXT` VAs, which equal file offsets. Secondary:
`CLTNic.dll` (x86, base `0x10000000`) and LEDSetting 2.2.6's `CLTInterface.dll`
/ `ScreenBeautifyAssistant.dll`.

**Headline.** The 8-bit → N-bit map is a host-built table that lives in the
card's SPI flash and is also sendable live as type-`0x76` packs. Its value at
input 0 is **exactly zero**, verified three ways, so it is *not* the black
floor. The inconsistency that is real, and that no vendor build can produce, is
**grey depth 12**: for chip `0x14C` the vendor derives the depth from two
driver-register bytes and the smallest value it can ever produce is **13**. Our
image pairs a grey byte of 12 with a **14-level** scan table, which schedules
two bit-plane slots (levels 12 and 13, together ~75 % of the frame's lit time)
that the 12-bit pixel word cannot supply. That is the leading candidate.

---

## 1. Where the 8-bit → N-bit mapping lives

### 1.1 It is built on the host, not in the card

`CHWParamRcvGeneral::CalGamaTable()` @ **`0x1311d0`** constructs a
`CGammaTableCalculator` and calls
`CGammaTableCalculator::CalGamaTable()` @ **`0x11ae30`**. That function fills
`GetGamaTable()`'s buffer with **256 entries of 24-bit big-endian** (`0x300`
bytes per channel), then `memcpy`s the first `0xC0` bytes to `+0x300`
(`0x11b2ce`–`0x11b2e1`) and continues for the other channels. There are
10-bit (1024-entry) and 12-bit (4096-entry) variants:

| accessor | entries | buffer |
|---|---|---|
| `CHWParamRcvGeneral::Get8bitGammaTalbe(uchar*)` @ `0x14ca90` | 256 x 3 ch | `0x900` |
| `…::Get10bitGammaTable(uchar*)` @ `0x14d2c0` | 1024 x 3 | `0x2400` |
| `…::Get12bitGammaTable(uchar*)` @ `0x14d340` | 4096 x 3 | `0x9000` |

Which one applies is chosen by the virtual slot **`vt+0x5A8`** on
`CHWParamRcvGeneral` (call sites `0x19b5dd`, `0x1ed60d`, `0x11aefa`): return 2
selects the 10-bit table, 4 the 12-bit table, anything else the 8-bit table.
Record 0x01 flag word #1 bit18 (`vt+0x5A8()==2`) and bit31 (`==4`) are the
serialised form. **Our card's stored value is the 8-bit table** — see §1.4.

### 1.2 The pack: type `0x76`

`CRcvCommandManager::GetSendCMD_GamaTable(uchar**, int*, int, int&)` @
**`0x19b570`** builds the send list. Every pack is **`0x487` bytes**:
`buf[0] = 0x76` (`movb $0x76, -0xd980(%rbp)` @ `0x19b5fe`), then
`bzero(buf+1, 0x486)`.

| pack offset | size | meaning | citation |
|---|---|---|---|
| `0x00` | 1 | type `0x76` | `0x19b5fe` |
| `0x01`–`0x02` | 2 | zero | `bzero` `0x19b5f4` |
| `0x03` | 1 | table index | `movb %bl, (%r12)` `0x1e7b50` (r12 = pack+3) |
| `0x04` | 1 | table kind: `0` = 8-bit, `4` = 12-bit | `movb $0x4, 0x1(%r12)` `0x1e7b54`; left 0 on the 8-bit path |
| `0x05`–`0x06` | 2 | zero | |
| `0x07` … | ≤`0x480` | payload, 24-bit BE entries | `0x1e7a67`, `0x1e7b5f` |
| … `0x486` | | zero pad | |

* **8-bit** (`Get8bitGamaTablePack(SGamaTablePackEx*)` @ **`0x1e7970`**): three
  packs, index `0/1/2` at `0x1e7a58`, `0x1e7a6c`, `0x1e7a89`; payload lengths
  `0x480`, `0x240`, `0x240` sliced from the `0x900` buffer at source offsets
  `0`, `0x480`, `0x6C0`.
* **12-bit** (`Get12BitGamaTablePack` @ **`0x1e7ae0`**): **48** packs, index
  `0..47`, `0x300` payload bytes each, covering the `0x9000` buffer
  (`0x1e7b50`–`0x1e7baf`).
* **10-bit** (`Get10BitGamaTablePack` @ `0x1e7be0`): 8 packs, dispatched at
  `0x19bc14`.

### 1.3 How the table is computed (`CalGamaTable`, `0x11ae30`)

Symbols resolved from the lazy-bind table: `0x3fb6e6` = `_ldexp`,
`0x3fb782` = `_pow`, `0x3fb74c` = `_logl`, `0x3fb662` = `_expl`,
`0x3fb740` = `_log10f`.

Setup:

```
maxGray = ldexp(1.0, GetGrayLevel()) - 1.0        ; 0x11ae6e .. 0x11ae83
outMax  = (1 << 24) - 1  = 16777215               ; 0x11ae88 .. 0x11aec3
method  = *(u8*)(obj + (vt5A8 in {2,4} ? 0xB0 : 0xAC))   ; 0x11aef0 .. 0x11af54
                                                  ; record 0x01 flag word #2 bits 3/4
L       = (method == 1 || method == 2) ? 0.0
                                       : 255.0 / maxGray  ; 0x11af5e .. 0x11af70
L       = max(0.0, min(0.5, L))                   ; 0x11af81 .. 0x11af96
gamma g = *(f32*)(obj + 0x98)                     ; record 0x01 +0x01C; ours 2.8
```

Per entry `i` in `0..255`, `x = i / 255.0` (`0x11b05d`, divisor `255.0` @
`0x3fc1d8`):

* **method 3** (HDR/PQ-ish, `cmpb $0x3, (%rbx)` `0x11b057`): `x < 0` → the
  `expl` branch; `x <= 0.5` → `y = x*x/3` (`0x11b095`, `1/3` @ `0x4000f8`);
  otherwise `y = (expl((x - 0.559904)*2.79574) + 0.28466892) / 12`
  (`0x11b140`–`0x11b168`). **`y(0) = 0`.**
* **otherwise** (`0x11b0b0`):
  * `if (1e-5 > x) y = 0` — `movsd 1e-05, %xmm3; ucomisd %xmm4, %xmm3; ja`
    @ `0x11b0b4`–`0x11b0c0`. **This is the input-0 case: `y(0) = 0.0`,
    unconditionally.**
  * `g < 1.1` → `y = pow(x, g) + 0.0` (`0x11b0f3`, addend `0.0` @ `0x3fc820`).
  * `g >= 1.1` → the **low-grey linearisation**, `0x11b175`–`0x11b20e`:

    ```
    xt = pow(L/g, 1/(g-1))        ; the x where d/dx(x^g) == L
    yt = pow(xt, g)
    yl = L * xt
    y  = (x < xt) ? L * x
                  : yl + (x^g - yt) * (1 - yl) / (1 - yt)
    ```

    A straight line of slope `L` from the origin up to the tangent point, then
    the gamma curve rescaled to stay continuous. **`y(0) = L*0 = 0`.**

Then (`0x11b220`–`0x11b252`): `v = round_half_away(y * outMax)`, clamped to
`0xFFFFFE`. For `i == 255` only, if
`IsNeedDropGrayWhenCalGammaTable()` and `grayLevel >= 10`, the top entry is
reduced by `2^(grayLevel-10)` LSBs of the grey field (`0x11b264`–`0x11b2c9`) —
this is where the code shows that **the card takes the top `grayLevel` bits of
the 24-bit word**: `cl = 24 - grayLevel` (`0x11b2ac`).
Finally (`0x11aff0`) the value is quantised, `v = (v >> (8 - shake2)) << (8 - shake2)`,
and stored big-endian as `v>>16, v>>8, v`.

**Verified value at input 0: `00 00 00`.** Even the "bypass" branch, taken when
`IsHighRefreshValid() && OBJ+0xDEF0` (`0x1e7993`–`0x1e79e8`), sets the whole
table to `0xFF` *and then explicitly zeroes entry 0 of each channel*
(`movw $0, 0x7(%rbx)`, `movb $0, 0x9(%rbx)`, likewise at `0x307`/`0x3C7`).

Neither the luminance level (record `+0x026`) nor the current percents
(record `+0x0B4/B8/BC`) enter this function; the only record-0x01 inputs are
the f32 gamma at `+0x01C`, the derived grey level, and the calc-method bits of
flag word #2. Current gains/percents reach the card through the **basic pack**
(`0x34`–`0x37`, `0xD8`), not the gamma table — see §3.

### 1.4 Where the card keeps it: SPI flash, and the E120 on this bench has one

`CSendAndSaveRcvParam::GetRcvGammaTableBufForSPIFlash(…, bool)` @ **`0x1ed5a0`**
(thin wrapper `CHWParamRcvGeneral::…` @ `0x154c30`) builds the flash image:

| offset | size | content | citation |
|---|---|---|---|
| `0x0000` | 1 | gamma as packed BCD: `(int)g << 4 \| (int)(g*10 - 10*(int)g)` | `0x1ed5d4`–`0x1ed62c`, `0x1ed695` |
| `0x0001` | 1 | `OBJ+0xB4` | `0x1ed5fe`, `0x1ed699` |
| `0x0002` | 1 | table kind: `2` if `vt5A8==4`, `1` if `==2`, else `0` | `0x1ed60d`–`0x1ed69e` |
| `0x0003` | 1 | `1` (version) | `0x1ed6a3` |
| `0x0004` | 1 | result of `vt+0x5B8 / 0x5D0 / 0x5E8` (selected by kind) | `0x1ed687`, `0x1ed6a9` |
| `0x0005`–`0x000F` | 11 | zero | `0x1ed6ae`, `0x1ed6b7` |
| `0x0010`–`0x090F` | `0x900` | the 8-bit table (3 packs concatenated) | `0x1ed723`–`0x1ed766` |
| `0x0910`… | `0x2400` | 10-bit table, if kind 1/2 | `0x1ed7dc` |
| `0x3000`… | `0x9000` | 12-bit table, if kind 2 | `0x1ed845` |
| `0x10000` | 2+2 x n | index: `(offset, length)` u16 pairs for header / 8-bit / 10-bit / 12-bit | `0x1ed689`, `0x1ed717`, `0x1ed7cb`, `0x1ed839` |
| `0x10010` | 2 | total size in 256-byte pages, rounded up | `0x1ed86d`–`0x1ed8c7` |

**Ground truth from this card.** An earlier dump of flash block 9 page 0
(`<scratch>/gamma/flash_block9_page0_9.bin`, 2560 bytes) reads

```
28 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00   <- header
00 00 00 00 04 00 00 08 00 00 0c 00 ...           <- table @ +0x10
```

which decodes as gamma **2.8**, `OBJ+0xB4` = 0, kind **0 (8-bit table)**,
version 1 — exactly the layout above. And bytes `0x10`..`0x90F` are
**byte-for-byte identical** to the table this formula produces for gamma 2.8 at
**14-bit** grey (`<scratch>/gamma/gamma8_2p8_14bit.bin`; 2304/2304 bytes match,
zero differences). Entry `i` in the linear segment is `i * 0x400`, and
`(2^24-1)/(2^14-1) = 1024.0` — the linear slope `L = 255/16383` in 24-bit
units, i.e. "output = input, in 14-bit units".

**So: the 8→N map is real, it is on this card, it was computed for 14-bit grey,
and its entry 0 is `00 00 00`.** Reading it at 12-bit (top 12 bits, `>> 12`)
still gives 0 for input 0 and `0xFFF` for input 255. It cannot be the floor.

### 1.5 The one additive offset — and why it does not apply

`CSendAndSaveRcvParam::CalGammaTableAfterCompensation(uchar*, int)` @
**`0x1e5f90`** adds a constant to gamma-table entries (vectorised
`paddd %xmm0` at `0x1e613e`–`0x1e6150`), where the constant is
`OBJ+0xE13C << (24 - GetEditGammaBitsExtend())` (`0x1e5fdb`–`0x1e5ff8`). It is
called from all three pack builders (`0x1e7a53`, `0x1e7b39`, `0x1e7c36`).

It is gated (`0x1e5fc4`, `0x1e5fd2`) on `GetScreenShakeParam` returning
`p1 != 0 && p2 == 6`, and returns unmodified otherwise. It also *skips entry 0*
— the SIMD loop starts at byte offset 3 (`movl $0x3, %eax` @ `0x1e6029`).
`OBJ+0xE13C` is serialised at `SaveBpToBuffer 0x1caee1` / loaded at
`LoadBpBufFromBuffer 0x1c787f`, which by the established stack-slot mapping is
**record 0x01 payload `+0x1ED`** — a byte not previously in
`docs/record-0x01-fields.md`. Confidence: high for the transport, medium for
the payload offset.

*Verdict: an additive low-grey lift, host-side, off by default, and it does not
touch input 0. Not the floor.*

---

## 2. Grey depth: what the vendor actually derives for chip `0x14C`

### 2.1 The file's grey byte is discarded

`CHWParamRcvGeneral::GetGrayLevel()` @ **`0x13aff0`** starts from `OBJ+0x80`
(record 0x01 `+0x023`), then:

1. `0x13b015`–`0x13b042`: if `(stored - 1) <= 0x13`, test
   `GetSupporttedGray() & (1 << (stored-1))` — mask table at **`0x401420`**,
   which is simply `bit(n-1)` for grey `n`. If the bit is clear,
   `OBJ+0x80 = GetDefaultGray()`.
2. `GetSupporttedGray()` @ **`0x13a5f0`** dispatches on the chip id
   (`OBJ+0xD3C4`) through a jump table at **`0x13aa7c`**. **Chip `0x14C`'s entry
   is `0x13aa0e`, the epilogue, with `r12` left at 0 from
   `xorl %r12d, %r12d` @ `0x13a609` — the supported-grey mask for `0x14C` is
   `0x0000`.** So the test always fails and the stored value is always replaced.
3. `GetDefaultGray()` @ **`0x13c0e0`**, jump table **`0x13c410`**: chip `0x14C`
   jumps straight to `0x13c3ee` with the pre-loaded default
   `movl $0x10, %r15d` @ `0x13c10e` — **16**.
4. `GetGrayLevelCalType()` for `0x14C` is **12** (`0xFAF50`; already documented
   in `docs/chip-libraries-non-sh.md` §4). The cal-type-12 arm at `0x13b264`
   then **overwrites** `OBJ+0x80` again — from the driver registers.

### 2.2 The register formula (cal type 12, chip `0x14C`)

`SChipCustomPlus` is the 256-byte record-0x84 payload at object `+0xD4E5`
(`docs/chip-control-block.md` §row "SChipCustomPlus"); record 0x84 is a flat
stream of `[reg, R, G, B]` quads, so byte `k` of the payload is at
`OBJ+0xD4E5+k`. For the factory table (`config/chips/sm16269s-factory.toml`)
payload byte 5 is reg `0x03`'s R value and byte 21 is reg `0x07`'s R value —
i.e. `OBJ+0xD4EA` = **reg 0x03**, `OBJ+0xD4FA` = **reg 0x07**.

The `0x14C` arm, `0x13b46a`–`0x13b5d8`:

```
if (GetChipType() != 0x14C) keep the generic log2 value
gpart = 1 << ((reg07 >> 3) & 3)                 ; 0x13b48f .. 0x13b4a6 (ldexp)
if (secondary chip id == 0x14D)                 ; 0x13b4ab
    gpart *= 64.0                               ; const @ 0x3fc7f8
    m      = *(int*)(0x3fe460 + ((reg03 >> 4) & 0xC))   ; {64, 32, 32, 128}
else
    gpart *= 128.0                              ; const @ 0x3fc7f0
    m      = (reg03 < 0x40) ? 64 : 32           ; 0x13b598 .. 0x13b5aa
n = m * gpart
grey = n < 0x1000 ? 12 : n < 0x2000 ? 13 : n < 0x4000 ? 14 : n < 0x8000 ? 15 : 16
                                                ; 0x13b5ad .. 0x13b5d4
```

(The generic arm it overrides, `0x13b367`–`0x13b3b2`, is
`ceil(log10f(clamp(((OBJ+0xD4EA & 0x7F)+1) * OBJ+0xD4FE * 4, <= 0x10000)) / 0.30103f)`
— `_log10f` @ `0x3fb740`, `log10(2)` @ `0x401390`. It is dead for `0x14C`.)

A final adjustment at `0x13b1f8`–`0x13b20d`: if `IsSupportNormalScanMethod()`
and `OBJ+0xDF8C`, the returned value is `OBJ+0x80 - 1`.

### 2.3 The consistent (grey byte, reg 0x03, reg 0x07) triples

Sub-variant id `0x0000` (ours). `b = (reg07 >> 3) & 3`:

| reg 0x03 | b | m | gpart | n | **grey** |
|---|---|---|---|---|---|
| `>= 0x40` | 0 | 32 | 128 | `0x1000` | **13** |
| `< 0x40` | 0 | 64 | 128 | `0x2000` | **14** |
| `>= 0x40` | 1 | 32 | 256 | `0x2000` | **14** |
| `< 0x40` | 1 | 64 | 256 | `0x4000` | **15** |
| `>= 0x40` | 2 | 32 | 512 | `0x4000` | **15** |
| `< 0x40` | 2 | 64 | 512 | `0x8000` | **16** |
| `>= 0x40` | 3 | 32 | 1024 | `0x8000` | **16** |
| `< 0x40` | 3 | 64 | 1024 | `0x10000` | **16** |

Our registers (`0x03 = 0x3F`, `0x07 = 0x04`, so `b = 0`) give
`64 x 128 = 0x2000` → **14**, which is what the factory basic pack carries
(`+0x08 = 0x0E`) and what the flash gamma table was computed for.

> **The minimum product is `32 x 128 = 0x1000`, so `n < 0x1000` is
> unreachable. Grey depth 12 is not a value the vendor tool can produce for
> chip `0x14C` under any register combination.** The lowest reachable value is
> 13, at reg `0x03 >= 0x40` (the chip's 13-bit mode) with reg `0x07` bits 4:3
> clear. Verified.

### 2.4 `IsNeed16BitGrayWhenSend()` — what it actually changes

`CChipTypeClassify::IsNeed16BitGrayWhenSend()` @ **`0xFB710`** returns true for
chip ids `0x85 0xBB 0xC1 0xC2 0xCC 0xCE 0xCF 0xD6 0xE2 0xE5 0xE6 0x10D 0x110
0x118 0x11A 0x12B 0x12C 0x12F 0x132 0x135 0x137 0x13C 0x14C 0x157 0x15D`
(the `cmpl` immediates in `0xFB72E`–`0xFB977`, all falling to `0xFB98E`).
The `CHWParamRcvGeneral` wrapper is `0x160A00`.

**It has exactly one caller in the whole library**: `GetBasicParam` @
`0x1dfb50`, at **`0x1dfefa`**:

```
1dfeef: callq GetGrayLevel()
1dfef4: movb  %al, 0xc(%rbx)         ; pack[0x0C] = derived grey (14 for us)
1dfefa: callq IsNeed16BitGrayWhenSend()
1dff01: je    0x1dff07
1dff03: movb  $0x10, 0xc(%rbx)       ; pack[0x0C] = 16
1dff0d: callq GetChipType()
1dff10: cmpw  $0x5c, %ax
1dff16: movb  $0x8,  0xc(%rbx)       ; chip 0x5C only: pack[0x0C] = 8
```

So the flag changes **one byte of the basic pack and nothing else**. It does
not select a different scan table, a different gamma table, or different chip
registers: `CalGamaTable`, `CalScanTalbeDefault` and `GetChipCustomPlusParamPack`
all use `GetGrayLevel()` (14), unaffected. Word packing on the sender side is a
card/FPGA matter and is not touched here. Verified.

Consequence: the PC tool's "send"/"save" path for chip `0x14C` always ships a
basic pack whose grey byte is **`0x10`** while the tables it ships alongside are
built for **14**. The card's own compiler (which produced our factory pack from
a `.rcvbp`) does not apply the rule and emits `0x0E`. Both are documented in
`docs/chip-libraries-non-sh.md` §4; this note adds the reason the two differ.

---

## 3. The 0x0107 channel-gain bytes (resolved)

Traced in `CLTNic.dll` (x86 build, base `0x10000000`). `fcn.1013d1f0` — the
call `docs/pixel-protocol.md` §2.2 left open — is **`pow(double, double)`**
(Intel SSE2 libm: `stmxcsr` `0x1013d1f3`, mantissa split
`psrlq xmm0,0x2c` `0x1013d244`, reciprocal table `0x10181390`). `0x10125173`
is **`roundf`** (half away from zero).

`_Nic_SetBrightness@24` @ `0x10001f60` forwards only args 2–4 to the worker
`fcn.100062c0` (`0x10001f74`); args 5 and 6 are dropped. Its caller
`_CLTProcessorSetBrightness@12` (CLTDevice `0x10177fa0`) computes
`M = percent / 100.0f` (`0x10178004`, divisor `100.0f` @ `0x10462c68`) and
`cR/cG/cB = roundf(255 * k)` where `k` is `1.0f` for neutral colour temperature
(`0x10178206`) or a Kelvin→RGB table lookup (`fcn.100065b0`, 16-byte records
`{int K; float r,g,b;}` from `0x104b85f0`).

Then, with `Mb = roundf(M * 255)`:

| global | frame | formula | citations |
|---|---|---|---|
| `0x101b1667` | `0x0107` **byte 35** (the inert "master") | `Mb` | `0x100062d4`, `0x100062e3`, `0x10006336` |
| `0x101b1668` | `0x0107` **byte 38 = R gain** | `round(cR * Mb / 255)` | `0x1000637d`, `0x10006385`, `0x10006393`, `0x100063c9` |
| `0x101b1669` | `0x0107` **byte 39 = G gain** | `round(cG * Mb / 255)` | `0x10006355`–`0x100063d7` |
| `0x101b166a` | `0x0107` **byte 40 = B gain** | `round(cB * Mb / 255)` | `0x10006322`–`0x100063e2` |

**There is no gamma and no offset on these bytes — they are strictly linear.**
The `pow(x, 0.4)` calls in the same function feed a *different* triple, bytes
13/14/15 of the 77-byte **type-`0x0A`** brightness frame
(`gk = round(255*(ck/255)^0.4)`, `Mg = round(255*M^0.4)`, byte = `round(gk*Mg/255)`;
`0x10006306`, `0x100063e7`, `0x10006424`, `0x10006469`, stores `0x10006513`/
`0x1000651e`/`0x10006529`).

Neutral colour temperature therefore gives
`byte35 = byte38 = byte39 = byte40 = round(255 * percent/100)`; our bench value
12 is 4.7 % brightness. **No minimum-brightness or black-level parameter feeds
these bytes**: both frames were enumerated byte-for-byte (`0x0107` built at
`fcn.10003460`, 112 B; `0x0A` at `0x100034f8`, 77 B) and every other byte is a
literal zero from the `memset`s at `0x100034b6` / `0x1000352e`.
`Nic_SetBrightness` writes exactly 7 bytes at `0x101b1664` and nothing else in
CLTNic touches them.

R/G/B **current gain** (record `+0x032/033/034`, object `+0xD3B9..BB`,
accessors `0x16e140`/`0x16e150`/`0x16e160`, reset default `0x2B2B2B2B` at
`0x12fb37`) and **current percent** (record `+0x0B4/B8/BC`, via
`GetCurrentPercent` `0x13e920` → `CChipCurrentCalculator::GetCurrentPercent`
`0xcca80`) reach the card only through the basic pack (`0x34`–`0x37`, `0xD8`) —
they are configuration, not part of the sync frame. Confidence: high.

---

## 4. Black-level / low-grey machinery: an exhaustive negative

Searched `syms.txt`, `sym/*.syms`, `dylib.strings`, `full.asm`, all
`Multi_*.ini`, and the Windows binaries under `ledvision/$_15_/x64/Bin/`.

**Not found anywhere**: `MinGray`, `MinGrey`, `MinBright`, `GrayOffset`,
`GrayStart` (receiver-side), `DarkLevel`, `Speckle`, `黑电平`, `最低亮度`.
`BlackLevel*` appears only in `LedAdmin.dll` as sender-side video processing.

**Found but not applicable to `0x14C`:**

* `CChipParamCalculator::GetChipBlackFieldTime()` @ `0xf6550` tests only
  `0x15D` (DP3254, `0xf6565`) and `0xE4` (MBI5264, `0xf656c`) and returns
  `0.0` for everything else (`xorps` @ `0xf658f`). The whole `CalBlackTime*`
  family (`0x1f5b40`, `0x1f5ba0`, `0x1f4ab0`, `0x1f5be0`, `0x1f5dc0`,
  `0x1f63f0`, `0x1f6620`, `0x1f6070`) is **camera-shutter black-*field timing*
  for refresh maths, not a black level** — cf. `IDS_ERROR_BLACKTIME`,
  "black time >= 2*(Tdt1+Tdt2)".
* `GetRemoveLowGrayMode()` @ `0x16e3c0` reads `OBJ+0xE62C`, the same slot as
  `GetChipCurrentVerifyVal` @ `0x16f310`; never packed, never serialised. Dead.
* The ~200 `CICNChipRegData::*LowGray*` symbols (`0x083940`–`0x08e9c0`),
  `CICND3065ChipData::*FirstLineDarkCompensation*` and
  `CXM11202GRegData::*LowAshCompensation*` are Chipone / Xinmao only, delivered
  by chip-specific packs (`0x1f6900`, `0x1f6a40`, `0x1e9220`) our card never
  receives.
* `CSC6618Lib::SetGammaStart6618` / `CSC6660Lib::SetGammaStart` are the
  sender-side scaler chips.
* "Low Gray Spot / 低灰麻点" (`Multi_eng.utf8.ini:175, 355, 448, 491-492, 690`)
  exists for DP5525/MBI/ICN/LS9937 pages — **there is no 麻点 control on any
  SM16169SH or SM16269 page**.

**Found and reachable, but only as raw chip registers.** The SM16169SH /
SM16269 parameter dialogs `IDD20172`–`IDD20174`
(`Multi_eng.utf8.ini:1101-1160`) do expose:

| string id | label | Chinese |
|---|---|---|
| `IDD20173_20421` | Low Gray Uniformity | 低灰均匀性 |
| `IDD20173_20858` | Low Gray Compensation 1 | 低灰补偿1 |
| `IDD20174_21566` | Low Gray Compensation 2 | 低灰补偿2 |
| `IDD20173_20866` | Low Gray Improvement | 低灰条纹改善 |
| `IDD20173_21088` | Blanking Level | 消隐等级 |
| `IDD20173_21089` | Blanking Ghost Level | 消文字鬼影等级 |
| `IDD20173_21090/91` | Blanking Time1 Coarse/Fine | 消隐时间1粗调/微调 |
| `IDD20174_21092` | First-Line Dark Compensation | 第一扫偏暗补偿等级 |

These are bits inside the 180-byte `SChipCustomPlus` blob shipped by
`GetChipCustomPlusParamPack` @ `0x1ea2b0` (`memcpy` `0xB4` bytes to `pack+4` @
`0x1ea2ea`, plus `0x50` bytes from `OBJ+0xD6D0` to `pack+0xB8`) — i.e. record
0x84, which the bench has already swept register by register with no effect.
The macOS dylib has **no per-field accessors for SM16169SH** (only
`SSM16169SHChipCustomPlus::GetScanCycleLevel()` @ `0xf0840`). The field/bit
layout would have to come from
`ScreenBeautifyAssistant.dll` (`CSM16169SH_LowGrayStripeDlg`, resource section
`//SM16169SH芯片参数区` in its `Multi_chi.ini`, lines 845-863). **NOT RESOLVED.**

`SetGrayCompensation(int, int)` @ `0x13e340` / `Get…` @ `0x13e390`: a 16-entry
x 4-bit array at `OBJ+0xC369`; the setter has a clobber bug
(`movb %al, 0xc369(%rdi)` @ `0x13e388` overwrites entries 0/1 on every call);
only the first word leaves the host (`GetBasicParam 0x1e2aba`/`0x1e2ac1` →
pack `0x2C`-`0x2D`; record 0x01 `+0x045`). **No caller anywhere in the dylib.**
Meaning: **NOT RESOLVED** — treat it as unknown, not as a low-grey control.
The adjacent byte `OBJ+0xC368` goes to **EEPROM byte `0x71`**
(`GetEepromPacks 0x1e8043`), which is missing from `docs/eeprom-map.md`.

`ChipData/custom_gamma/` contains `custom_gamma_SM16169_0.csv` and `_1.csv`
(259 lines: `GAMMA_TABLE_FILE`, `GrayLevel=16`, `IsRGBSeparate=0`, then 256
16-bit decimals). `_0` is a uniform step of 8; `_1` is a low-grey stretch.
**Both start at 0.** No SM16269 or SM16169SH file exists, and their loader is
Windows-only (`ChipSetting.dll`/`HwCommon2.dll`); no string from these files
appears in the macOS dylib.

> **Summary of §4: every low-grey mechanism in the vendor stack *adds* light at
> low grey. None subtracts, and there is no "black = off" control for this
> chip.**

---

## 5. What the black floor most likely is

Ranked, with what supports each.

### 5.1 Leading candidate — the scan table has more bit-plane levels than the pixel word has bits

`crates/e120-rcvbp/src/image/scan_table.rs` hard-codes `let gray = 14u32` and
uses the vendor's **14-bit** field-table block, while our basic pack declares
grey **12**. The scan table is a list of `(level, time/8)` entries: with
`gray = 14` it contains entries for levels **12 and 13**, whose bit times are
`2^12` and `2^13` times the minimum OE — together roughly **75 % of the frame's
total lit time**.

If the card indexes the pixel word by the `level` byte from the scan table, a
12-bit word has no bits 12 and 13. Whatever those slots read (adjacent pixel
bits, stale SDRAM, an undriven bus) is displayed with three quarters of the
frame's weight — which predicts exactly what the bench sees:

* a **per-pixel** pattern, present on an all-black frame;
* **static plus flickering** components (single frames correlate ~0.6 with each
  other, ~0.88 with the average) — fixed memory contents plus refresh churn;
* scaling with the channel gains (it is real lit time, not leakage);
* **immune to every driver register** — it is an FPGA-side scheduling artefact,
  not a chip setting;
* immune to `[current] percent`, to double-buffer writes, and to grey-depth
  changes on the driver side;
* the right magnitude: the floor is `(0.75-0.47)/(1.75-0.47) ≈ 22 %` of full
  scale, i.e. those two planes lit at roughly 30 % density.

Status: **inferred**, but every premise is verified — the 14 in the scan table
is in our own source, the 12 in the pack is in our own config, and §2.3 shows
the vendor never pairs them.

### 5.2 The vendor's own gray-12 field table (the fix to test)

`CScanCalculator::InitFieldTable16Segment(uint, int, float, float, void*)` @
**`0x1d5cf0`** hand-codes one block per top level. The dispatch is
`jmpq *(0x1d722c + 4*(top-1))` (`0x1d61f7`–`0x1d6212`), where `top = gray - 1`:

| gray | top | index | block |
|---|---|---|---|
| 12 | 11 | 10 | **`0x1d653f`** |
| 13 | 12 | 11 | `0x1d66d4` |
| 14 | 13 | 12 | `0x1d678a` (the one we ship) |
| 15 | 14 | 13 | `0x1d6be8` |
| 16 | 15 | 14 | `0x1d6933` |

Level record layout, recovered from the store offsets and cross-checked against
the already-verified gray-14 block: record base = `level * 0x104`, segment id at
`base + 8`, slot `k` enable at `base + 0xC + 4k`.

Decoding `0x1d653f`–`0x1d66cf` gives the **gray-12, 16-segment, style-0 field
table** (levels 0..10; the caller sets level 11 = the top = id 16 / `0xFFFF`,
as it does for gray 14):

```rust
/// InitFieldTable16Segment, jump table 0x1d722C entry 10 (top = 11, gray 12).
const FIELD_TABLE_16SEG_GRAY12: [(u32, u32); 11] = [
    (1, 1 << 3),                    // 0x1d666f id, 0x1d66a8 slot
    (1, 1 << 5),                    // 0x1d6676, 0x1d66af
    (1, 1 << 7),                    // 0x1d6680, 0x1d66b9 (rcx = rbx+0x230)
    (1, 1 << 11),                   // 0x1d668a, esi = 0x344
    (1, 1 << 13),                   // 0x1d6694, edx = 0x450
    (1, 1 << 15),                   // 0x1d669e, eax = 0x55c
    (2, (1 << 1) | (1 << 9)),       // 0x1d6651, 0x1d665b, 0x1d6665
    (4, 0x4444),                    // 0x1d661f .. 0x1d6647
    (4, 0x1111),                    // 0x1d65f0 .. 0x1d6615
    (8, 0xAAAA),                    // 0x1d6596 .. 0x1d65e6
    (8, 0x5555),                    // 0x1d653f .. 0x1d658c
];
```

Sanity: the shape matches the gray-14 block exactly (levels 0..5 in single
segments, then 2, 4, 4, 8, 8, and the top in 16), and every store in the block
maps to a valid `(level, slot)` with none left over.

### 5.3 Runner-up — a grey-depth/word-width mismatch at the driver

Our registers derive **14** (§2.3) while the pack says **12**. If the FPGA
shifts 12 bits per channel per pixel into a chip configured for 14-bit S-PWM
(reg `0x03 = 0x3F`, bit 6 clear), the chips' shift registers are 2 bits short
per pixel and every latch loads a rotated stream. This is a coherent story for
"grey depth 14 or 16 in the pack makes pixel data not display at all" (the
FPGA's frame-buffer geometry changes with the pack byte) but a weaker story for
a floor on an all-zero frame, since a steady black stream should flush to zero.
Status: **inferred, second choice.**

### 5.4 Ruled out

* A gamma/LUT with a non-zero origin. **Ruled out** — §1.3 (three code paths
  all give 0), §1.4 (the table on this card's flash starts `00 00 00`),
  §1.5 (the only additive offset skips entry 0 and is gated off).
* Any vendor black-level / low-grey pack. **Ruled out** — §4.
* A minimum-brightness parameter behind the channel gains. **Ruled out** — §3.
* The card using a *default* table when none is sent: not applicable, this card
  has a real one in flash, byte-identical to the vendor formula.

---

## 6. The experiment

**Primary — make the scan table's level count match the pack's grey byte.**

1. Add `FIELD_TABLE_16SEG_GRAY12` (§5.2) to
   `crates/e120-rcvbp/src/image/scan_table.rs` and take `gray` from the spec
   (`rec.gray()`), instead of the hard-coded `14`. `init_field_table`'s
   `top = gray - 1 = 11` check still holds; `render` then emits **no entry with
   `level >= 12`**.
2. Regenerate and reflash:
   ```sh
   e120 gen-config --spec config/panels/p25-128x64-sm16269s.toml --out-dir build
   e120 restore-flash build/<name>-block7.bin --commit
   # power-cycle (arm_at_boot), then measure
   ```
3. Keep everything else identical — grey byte 12, registers `0x03 = 0x3F`,
   `0x07 = 0x04`, gains 12.

**Predicted result if §5.1 is right:** the all-black draw drops from
~0.75 A toward the gain-0 floor of ~0.47 A, the speckle disappears, and white
stays near 1.74–1.76 A (the level weights still sum to full scale, they are
just spread over 12 planes instead of 14). A partial drop means only one of the
two phantom planes was being driven.

**Predicted result if §5.1 is wrong:** black is unchanged, which also settles
it — move to the secondary test.

**Secondary — run a vendor-consistent (grey, reg 0x03, reg 0x07) triple.**
The two the vendor can actually emit for our silicon, from §2.3:

| variant | reg 0x03 | reg 0x07 | grey byte | field table |
|---|---|---|---|---|
| A (what the registers already say) | `0x3F` | `0x04` | **14** | gray-14, `0x1d678a` — already shipped |
| B (13-bit chip mode) | `0x7F` (bit 6 set) | `0x04` | **13** | gray-13, `0x1d66d4` — needs transcribing |

Variant A costs nothing to retry now that the failure mode is understood: the
current image already carries the 14-level scan table and the 14-bit flash
gamma table, so setting the pack grey byte to `0x0E` makes all three agree for
the first time. `rendering-recipe.md` records that 14 "makes pixel data not
display at all", but that was measured with the grey byte alone changed and
several other things (notably `+0x02F` and the frame order) not yet settled — it
is worth one clean repeat.

**Cheap diagnostic, no reflash.** If levels 12/13 are the culprit, deleting just
those entries from the scan table should be enough. Zero the `(level, time)`
quads whose level byte is `0x0C` or `0x0D` in the scan-table region of the
block-7 image, leave everything else alone, and reflash only that page. If black
goes dark, §5.1 is confirmed without touching the solver.

**Instrumentation.** `scripts/psu.sh` for the current, an averaged webcam snap
for the speckle (`docs/bench-measurement.md`); compare against the recorded
gain sweep 0/4/12/40/120 → 0.47/0.71/0.75/0.86/1.08 A.

---

## 7. Confidence register

| claim | status |
|---|---|
| Gamma pack type is `0x76`, `0x487` bytes, index at `+3`, kind at `+4`, payload at `+7` | **verified** (`0x19b5fe`, `0x1e7b50`, `0x1e7b54`, `0x1e7b5f`) |
| Table entry for input 0 is exactly 0 in every host path | **verified** (`0x11b0b4`–`0x11b0c0`; bypass path `0x1e79bb`; compensation skips index 0 at `0x1e6029`) |
| The table is `y = L*x` below the tangent point, `L = 255/(2^depth - 1)` | **verified** (`0x11af63`, `0x11b175`–`0x11b1c4`) |
| This card's flash holds that table, gamma 2.8, 14-bit, header per `0x1ed5a0` | **verified** — 2304/2304 bytes match, header decodes field for field |
| Flash region = block 9 (`0x90000`) | **medium** — from the dump's filename; the content and header are certain |
| `GetSupporttedGray(0x14C) = 0`, `GetDefaultGray(0x14C) = 16` | **verified** (jump tables `0x13aa7c`, `0x13c410`) |
| Grey for `0x14C` = bucket(`m * (128 << ((reg07>>3)&3))`), `m = reg03 < 0x40 ? 64 : 32` | **verified** (`0x13b46a`–`0x13b5d8`); constants `0x3fc7f0`, `0x3fe460` |
| Grey 12 is unreachable for `0x14C`; minimum is 13 | **verified** (arithmetic on the above) |
| `IsNeed16BitGrayWhenSend` changes only basic-pack `0x0C` | **verified** — one caller, `0x1dfefa` |
| `0x0107` gain bytes = `round(c_k * round(255*pct/100) / 255)`, no gamma, no offset | **verified** (CLTNic `0x100062c0`) |
| No black-level / min-grey parameter exists for this chip | **verified negative** (§4) |
| `SetGrayCompensation` semantics | **NOT RESOLVED** |
| `SChipCustomPlus` bit layout for 低灰均匀性 / 低灰补偿 / 消隐等级 | **NOT RESOLVED** — chase `ScreenBeautifyAssistant.dll` |
| Record 0x01 `+0x1ED` = `OBJ+0xE13C` gamma additive offset | **medium** |
| EEPROM byte `0x71` = `OBJ+0xC368` | **medium** |
| `FIELD_TABLE_16SEG_GRAY12` values | **verified** decode; **untested** on hardware |
| The floor is the two phantom bit-plane levels | **inferred** — §5.1 |
