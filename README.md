# e120

Drive a Colorlight E120 LED receiving card directly from a Mac over raw
Ethernet — no vendor software involved. The card speaks a layer-2 protocol
with hardcoded MAC addresses and no IP, so everything here works on raw
Ethernet frames via `/dev/bpf`.

## Workspace

| Crate | Role |
|---|---|
| `e120-proto` | The wire protocol: discovery, pixel rows, sync, brightness, layout, test mode, flash/EEPROM access, parameter packs. Pure logic. |
| `e120-net` | Raw Ethernet over BPF, plus a pcap reader. |
| `e120-rcvbp` | The `.rcvbp` config format (parse/write/CRC), the compiled boot-image builder, and the panel-spec generator. |
| `e120-canvas` | Wall topology: one image onto any arrangement of panels. |
| `e120-video` | Frame sources: video via ffmpeg, stills, test patterns. |
| `e120-driver` | Joins topology, protocol and transport. |
| `e120-cli` | The `e120` binary. |

## Setup

```sh
sudo chmod o+rw /dev/bpf*      # resets on reboot
cargo build
```

Global options: `--iface en24 --width 128 --height 64 --order bgr --brightness N`.

## Configuring a panel

A panel is described once, declaratively, and everything the card needs is
generated from it — see [`docs/building-a-config.md`](docs/building-a-config.md).

```sh
e120 gen-config --spec config/panels/p25-128x64-sm16269s.toml --out-dir build
e120 restore-flash build/p25-128x64-sm16269s-block7.bin --commit   # install the boot image
e120 screen-size --set 128x64 --commit
e120 reload-params --full                                          # apply without a power cycle
e120 send-params --spec config/panels/p25-128x64-sm16269s.toml            # or push the RAM packs directly
```

Inspecting configs: `e120 rcvbp file.rcvbp`, `e120 config-diff a b`,
`e120 read-config --out card.rcvbp` (what the card holds).

## Driving the panel

```sh
e120 discover                  # firmware version and detected size
e120 set-layout                # tell the card its size (RAM; needed each boot)
e120 test rgb --hold           # rgb | border | rows | gradient
e120 fill ff8000 --hold
e120 image picture.png --hold
e120 play clip.mp4
e120 blank
```

`--layout wall.json` drives a multi-panel wall (`e120 layout-example`).

## Bench

`scripts/bench.py boot` powers the panel on without railing the supply,
`scripts/bench.py capture` turns flicker into a measured waveform, `scripts/bench.py capture`
takes strobe-proof stills, `scripts/bench.py run --boot` / `ab.sh` run one experiment with
current readings and photos.

## Flash and firmware

Reads are always safe; writes need `--commit` and are confined by guards in
`e120-proto` (config writes reach only the parameter block; firmware only
through the card's own SDRAM staging path). Snapshot before flashing:

```sh
e120 snapshot --dir before
e120 upgrade install image.hex --commit
e120 restore all --dir before --commit
```

Layout, images and procedure: [`third-party/README.md`](third-party/README.md).

## Documentation

* [`docs/building-a-config.md`](docs/building-a-config.md) — the generator, what is derived from where, honest limits.
* [`docs/record-0x01-fields.md`](docs/record-0x01-fields.md) — every byte of the main parameter record.
* [`docs/compiled-image-format.md`](docs/compiled-image-format.md) — the boot image, region by region.
* [`docs/rcvbp-format.md`](docs/rcvbp-format.md) — the `.rcvbp` container.
* [`docs/vendor-sdk-analysis.md`](docs/vendor-sdk-analysis.md) — how findings were extracted from the vendor binaries.
* [`docs/archive/config-protocol.md`](docs/archive/config-protocol.md) — the original analysis log (superseded where the documents above disagree).
* [`HANDOFF.md`](HANDOFF.md) — current state and next steps.
