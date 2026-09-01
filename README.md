# e120

Drive a Colorlight E120 LED receiving card directly from a Mac over raw
Ethernet — no vendor software involved.

The card speaks a layer-2 protocol with hardcoded MAC addresses and no IP, so
everything here works on raw Ethernet frames via `/dev/bpf`.

## Workspace layout

| Crate | Role |
|---|---|
| `e120-proto` | The Colorlight wire protocol: frame construction for discovery, pixel rows, sync/latch, and brightness. Pure logic, no I/O. |
| `e120-net` | Transport. Raw Ethernet send/receive over BPF, plus a reader for pcap capture files. Protocol-agnostic: it moves opaque frames. |
| `e120-rcvbp` | Parser for Colorlight `.rcvbp` receiver-parameter files (both the compressed and uncompressed variants). |
| `e120-cli` | The `e120` binary tying the above together. |

## Usage

Raw Ethernet access needs permission on the BPF devices:

```sh
sudo chmod o+rw /dev/bpf*      # resets on reboot
```

```sh
e120 discover                  # probe for a card; prints firmware and detected size
e120 listen                    # passively dump frames (debugging)

e120 test rgb                  # test patterns: gradient | rows | border | rgb
e120 fill ff8000               # solid colour
e120 image picture.png         # display an image, scaled to the panel
e120 brightness 128
e120 blank

e120 rcvbp panel.rcvbp         # inspect a receiver-parameter file
e120 pcap-summary cap.pcap     # summarise Colorlight traffic in a capture
```

Global options: `--iface en24 --width 128 --height 64 --order bgr --brightness N`.
Add `--hold` to `test` and `image` to refresh continuously.

## Documentation

* [`docs/rcvbp-format.md`](docs/rcvbp-format.md) — the `.rcvbp` file format.
* [`docs/config-protocol.md`](docs/config-protocol.md) — the receiver
  configuration protocol, reverse-engineered from vendor binaries by static
  analysis.

## Status

Discovery, pixel, sync, and brightness frames are implemented and verified on
the wire. Sending receiver configuration to the card — which a panel needs
before its driver chips will light — is still in progress; see
`docs/config-protocol.md`.
