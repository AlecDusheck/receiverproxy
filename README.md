# e120

Drive a Colorlight E120 LED receiving card directly from a Mac over raw
Ethernet — no vendor software involved.

The card speaks a layer-2 protocol with hardcoded MAC addresses and no IP, so
everything here works on raw Ethernet frames via `/dev/bpf`.

## Workspace layout

| Crate | Role |
|---|---|
| `e120-proto` | The wire protocol: discovery, pixel rows, sync, brightness, layout, test mode, flash and EEPROM access, and parameter packs. Pure logic, no I/O. |
| `e120-net` | Transport. Raw Ethernet send/receive over BPF, plus a pcap reader. Protocol-agnostic: it moves opaque frames. |
| `e120-rcvbp` | Parser and writer for Colorlight `.rcvbp` configuration files, both container variants, including the CRC trailer. |
| `e120-canvas` | Wall topology: map one image onto any arrangement of panels, at any size, rotation or mirroring, across any number of receivers. |
| `e120-video` | Frame sources: video via ffmpeg, stills, and built-in test patterns. |
| `e120-driver` | Joins topology, protocol and transport so anything driving the wall behaves identically. |
| `e120-cli` | The `e120` binary. |

## Setup

Raw Ethernet access needs permission on the BPF devices:

```sh
sudo chmod o+rw /dev/bpf*      # resets on reboot
```

Global options: `--iface en24 --width 128 --height 64 --order bgr --brightness N`.

## Driving the panel

```sh
e120 discover                  # probe for a card; prints firmware and detected size
e120 pattern rgb               # rgb | border | rows | gradient | white
e120 fill ff8000               # solid colour
e120 image picture.png         # a still, scaled to the wall
e120 play clip.mp4 --loop      # video, via ffmpeg
e120 blank
```

Add `--hold` to keep refreshing. `--layout wall.json` drives a multi-panel wall;
`e120 layout-example` prints one to adapt.

## Configuration

```sh
e120 read-config --out card.rcvbp    # what the card currently holds
e120 rcvbp panel.rcvbp               # inspect a config file
e120 config-diff a.rcvbp b.rcvbp     # compare two
e120 config-build --base a.rcvbp --copy-from b.rcvbp --copy 0a84 --out new.rcvbp
e120 write-config panel.rcvbp        # dry run; add --commit to install
e120 send-params panel.rcvbp         # push parameters into RAM, no flash, no reboot
```

## Flash and firmware

Reads are always safe. Writes need `--commit` and are confined by guards in
`e120-proto`: configuration writes touch only the parameter block, firmware
writes only the primary bank, and the golden backup bank is unreachable by
construction.

```sh
e120 scan-flash                          # find bitstreams and config in flash
e120 dump-flash --block 0 --blocks 11 --out primary.bin
e120 upgrade-info                        # what image the bootloader expects
e120 flash-firmware image.hex --backup primary.bin --commit
```

**Always snapshot first.** Restoring firmware erases the configuration, because
the configuration lives inside the firmware image's address range, so
`restore all` sequences the pieces in the order that leaves the card whole.

```sh
e120 snapshot --dir before-flash
e120 restore all --dir before-flash --commit
```

## What we learned about this hardware

Colorlight builds a **different FPGA bitstream per LED driver chip**. There is
no runtime setting for it — the driver-chip protocol is implemented in gateware.
A card whose firmware was built for one driver IC will not light a panel using
another, no matter how it is configured. See [`firmware/README.md`](firmware/README.md).

`E320` in a firmware filename is a **platform name, not a product name**:
Colorlight ships one gateware line for the E80, E120, 5A-75B, 5A-75E and E320,
and names every build after the E320. This card's factory flash is byte-identical
to `E320_PCB6.0_PWM_FPGA10.81_20230907.hex`.

Flash layout, and the write-protect that gates every firmware write, are
documented in [`firmware/README.md`](firmware/README.md).

## Documentation

* [`docs/config-protocol.md`](docs/config-protocol.md) — the wire protocol and
  configuration format, reverse-engineered from vendor binaries by static
  analysis.
* [`docs/rcvbp-format.md`](docs/rcvbp-format.md) — the `.rcvbp` file format.
* [`firmware/README.md`](firmware/README.md) — firmware images, flash layout,
  and the upgrade procedure.

Firmware can only be installed through the card's own SDRAM staging path
(`e120 upgrade install`); direct flash writes cannot reach the bitstream regions
at all. `firmware/README.md` has the layout and why.

## Status

Discovery, pixel, sync and brightness frames are verified on the wire, and the
configuration path reads and writes correctly.

The card now holds `E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex`, built
for the SM16269S driver ICs this panel uses, installed via SDRAM staging and
verified byte-for-byte against both bitstream regions. Its configuration is
restored alongside it: 15 records, width 128, scan 1/16, driver-chip register
table present. Awaiting a power cycle to load the new bitstream.
