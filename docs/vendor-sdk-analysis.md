# Working out protocol details from the vendor SDK

Everything we know about the wire protocol and the `.rcvbp` format came from
reading Colorlight's own shipped libraries. `docs/config-protocol.md` is the
output of that work; this file is the *method*, so the next person can extend it
rather than start over.

## Delegate this to an Opus 5 subagent

**Do not do this work in the main loop.** Spawn a subagent explicitly on Opus 5
and consume only its findings:

```
Agent(
  subagent_type: "general-purpose",
  model: "opus",
  description: "Locate the save-to-flash frame builder",
  prompt: "<see the prompt template below>"
)
```

Two reasons. Smaller models running file inspection inline have repeatedly
tripped automated content classifiers and derailed the session — the user asked
for this split directly. And these files are large; a subagent keeps thousands
of lines of tool output out of the main context, returning a short structured
answer instead.

Keep the main loop on hardware: bring-up, measurement, code, A/B tests.

## Never execute the vendor software

Unpack it, read it, reimplement it. Do not run it, install it, or `dlopen` it —
not the installer, not the app, not the DLLs. This is a standing rule from the
user (see `firmware/README.md` and the session memories). The one time an app
was launched without asking, it was the worst moment of the project.

## What is on disk

LEDVISION 9.6 is already unpacked at:

```
/private/tmp/claude-501/-Users-amd-e120/261c3dad-ba97-45d2-8ea3-ab7a950a8ff9/scratchpad/ledvision/
```

If that scratchpad is gone, re-extract with `7z x` (no execution involved):

```sh
7z x -y ~/Downloads/LEDVISION_Setup_x64_9.6.49150.exe -o<dir>
```

| Path | What it is |
|---|---|
| `$_15_/x64/Bin/CLTNic.dll` | The sender. Builds and transmits the raw-Ethernet frames via a winpcap send queue. Exports `Nic_SendScreenPicture`, `Nic_SendScreenBlackPicture`, `Nic_SetBrightness`, `Nic_SetScreenSize`, `Nic_SetTestModeIndex`, `Nic_SetScreenConnectionStyle`, `Nic_Write`. |
| `$_15_/x64/Bin/CLTDevice.dll` | Device/config side: `.rcvbp` loading, parameter packs, flash and EEPROM operations, the firmware upgrade path. Most of `docs/config-protocol.md` came from the macOS build of this library. |
| `$_15_/x64/Bin/ChipData/` | Per-vendor driver-chip data (`AXS`, `DP`, `LS`, `chipone`, `cks`, `xm`) plus `custom_gamma/*.csv`. Worth checking for SM16269 support, which was absent from older releases. |
| `$_15_/config_files/` | A large tree of vendor `.rcvbp` files by manufacturer and pitch. Useful as known-good examples to diff against. |
| `$_15_/x64/Bin/LEDSetting.exe` | The configuration UI, where "send" vs "save to flash" is driven from. |

The earlier work used a **macOS** build, `libCLTDevice.1.dylib`, whose symbols
are C++-mangled and far easier to navigate than the stripped Windows DLLs. If
you can find that build again, prefer it — every address in
`docs/config-protocol.md` refers to it.

## Method that worked

1. **Start from exported symbol names.** `CLTDevice` keeps meaningful C++ names
   (`CReceiverOP::SetRcvCardTestMode`, `BuildSDRAMOperation`,
   `CSendAndSaveRcvParam::Get*ParamPack`). Demangle with `c++filt`, then follow
   the call graph out from the operation you care about.
2. **Read frame builders literally.** They are formulaic: allocate a buffer,
   `bzero`, write a type byte, write a receiver index big-endian, write an
   opcode, `memcpy` a body, call the send hook. Transcribe every byte offset
   into a table — that is what §16.1 and §28.1 of the protocol doc are.
3. **Anchor on constants.** Frame types (`0x0107`, `0x55`, `0x0AFF`, `0x1A00`,
   `0x2300`), the `08 88` pixel marker, the hardcoded MACs
   `11:22:33:44:55:66` / `22:22:33:44:55:66`. On the stripped Windows x64 build
   these did **not** appear as immediate stores, so byte-pattern hunting alone
   found nothing useful — go via symbols instead.
4. **Cross-check against FPP.** `ColorLight-5a-75.cpp` in FalconChristmas/fpp
   is an independent, working implementation of the display path for this card
   family. When it and our reading of the vendor code disagree, the hardware
   decides — and it has surprised us both ways.
5. **Mark confidence honestly.** `docs/config-protocol.md` labels findings
   *high* / *medium* / **NOT RESOLVED**. Several hours were lost to treating an
   inferred field as established. Keep doing this.

## Prompt template for the subagent

```
REPO: /Users/amd/e120 (Rust CLI driving a Colorlight E120 LED receiving card
over raw layer-2 Ethernet). Read HANDOFF.md and docs/config-protocol.md first
for what is already known and how findings should be written up.

VENDOR SDK (unpacked, do NOT execute anything):
  <path>/$_15_/x64/Bin/CLTNic.dll     -- sender, builds the ethernet frames
  <path>/$_15_/x64/Bin/CLTDevice.dll  -- config, flash, parameter packs

GOAL: <the one specific thing, e.g. "find the frame that makes the card compile
its .rcvbp into the parameter image at flash 0x70000 -- LEDVISION's save-to-
flash. docs/config-protocol.md section 3 names types 0x11, 0x1F, 0x26, 0x31,
0x32, 0x76 from a capture but no payload layout.">

METHOD: start from exported symbol names and demangle them; follow the call
graph to the frame builder; transcribe every byte offset. Anchor on known
constants. Do not pattern-match on raw bytes alone -- it does not work on these
stripped x64 builds.

RETURN: the exact byte layout as a table (offset, size, value/source, meaning),
the symbol and address it came from, a ready-to-run `e120 raw-send` command that
constructs it, and a confidence level per field. Say "NOT RESOLVED" rather than
guessing -- an inferred field presented as fact has cost this project hours.
```

## Open questions worth handing over

* **The save-to-flash frame.** The single highest-value unknown: it would let
  the card compile its own parameter image instead of us synthesising one. See
  `HANDOFF.md` §7.
* **The built-in test-pattern selector values.** §16.1 states the enum is not
  recoverable statically; the frame layout is confirmed but the values are not.
* **`basic_pack` field sources.** §21.2's joined table leaves the `0x74`–`0x82`
  bitfield block unresolved, and the version built from that table stopped the
  drivers arming — see `HANDOFF.md` §6.
* **SDRAM staging mode bytes.** §28.1 confirms `mode == 1` carries data but
  never resolved which values mean begin and commit. Our upgrade path works with
  inferred values (`0x03` program, `0x05` erase), which is worth confirming.
