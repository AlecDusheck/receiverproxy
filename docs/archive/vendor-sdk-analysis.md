# Working out protocol details from the vendor SDK

> Archived. The findings this method produced live in
> [rcvbp-format.md](../rcvbp-format.md), [record-0x01-fields.md](../record-0x01-fields.md),
> [compiled-image-format.md](../compiled-image-format.md),
> [pixel-protocol.md](../pixel-protocol.md) and [eeprom-map.md](../eeprom-map.md);
> the open questions listed at the end were answered by those pages or by the
> bench. The unpack paths below were scratch directories and no longer exist.

Everything known about the wire protocol and the `.rcvbp` format came from
reading Colorlight's own shipped libraries. `config-protocol.md` in this
directory is the first output of that work; this file is the method.

## Never execute the vendor software

Unpack it, read it, reimplement it. Do not run it, install it, or `dlopen` it:
not the installer, not the app, not the DLLs. This is a standing rule (see
`third-party/README.md`).

## What is on disk

LEDVISION 9.6 unpacks with `7z x` (no execution involved):

```sh
7z x -y LEDVISION_Setup_x64_9.6.49150.exe -o<dir>
```

| Path | What it is |
|---|---|
| `$_15_/x64/Bin/CLTNic.dll` | The sender. Builds and transmits the raw-Ethernet frames via a winpcap send queue. Exports `Nic_SendScreenPicture`, `Nic_SendScreenBlackPicture`, `Nic_SetBrightness`, `Nic_SetScreenSize`, `Nic_SetTestModeIndex`, `Nic_SetScreenConnectionStyle`, `Nic_Write`. |
| `$_15_/x64/Bin/CLTDevice.dll` | Device/config side: `.rcvbp` loading, parameter packs, flash and EEPROM operations, the firmware upgrade path. Most of `config-protocol.md` came from the macOS build of this library. |
| `$_15_/x64/Bin/ChipData/` | Per-vendor driver-chip data (`AXS`, `DP`, `LS`, `chipone`, `cks`, `xm`) plus `custom_gamma/*.csv`. |
| `$_15_/config_files/` | A large tree of vendor `.rcvbp` files by manufacturer and pitch; known-good examples to diff against. |
| `$_15_/x64/Bin/LEDSetting.exe` | The configuration UI, where "send" vs "save to flash" is driven from. |

The macOS build, `libCLTDevice.1.dylib` from the iSet 7 package, keeps
C++-mangled symbols and is far easier to navigate than the stripped Windows
DLLs. Every address in `config-protocol.md` and in the format pages refers to
it.

## Method that worked

1. Start from exported symbol names. `CLTDevice` keeps meaningful C++ names
   (`CReceiverOP::SetRcvCardTestMode`, `BuildSDRAMOperation`,
   `CSendAndSaveRcvParam::Get*ParamPack`). Demangle with `c++filt`, then
   follow the call graph out from the operation of interest.
2. Read frame builders literally. They are formulaic: allocate a buffer,
   `bzero`, write a type byte, write a receiver index big-endian, write an
   opcode, `memcpy` a body, call the send hook. Transcribe every byte offset
   into a table.
3. Anchor on constants. Frame types (`0x0107`, `0x55`, `0x0AFF`, `0x1A00`,
   `0x2300`), the `08 88` pixel marker, the hardcoded MACs
   `11:22:33:44:55:66` / `22:22:33:44:55:66`. On the stripped Windows x64
   build these did not appear as immediate stores, so byte-pattern hunting
   alone found nothing; go via symbols.
4. Cross-check against FPP. `ColorLight-5a-75.cpp` in FalconChristmas/fpp is
   an independent, working implementation of the display path for this card
   family. When it and the vendor code disagree, the hardware decides, and it
   has gone both ways.
5. Mark confidence per claim: high, medium, NOT RESOLVED. Hours were lost to
   treating an inferred field as established.

## Open questions at the time, and where they went

* The save-to-flash frame: made unnecessary by building the boot image on
  the host (`e120 config gen`, [compiled-image-format.md](../compiled-image-format.md)).
* The built-in test-pattern selector values: `e120 card test-mode <n>`
  reaches the generator; the enum is still not recovered statically.
* `basic_pack` field sources: decoded in
  [record-0x01-fields.md](../record-0x01-fields.md).
* SDRAM staging mode bytes: the upgrade path works with `0x03` program and
  `0x05` erase (`crates/e120-proto/src/upgrade.rs`); firmware 16.53 was
  installed through it.
