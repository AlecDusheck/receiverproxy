# Handoff

A P2.5 128x64 SM16269S panel on a Colorlight E120, driven over raw Ethernet
from the Rust CLI in this repo. Index of the documentation:
[`docs/README.md`](docs/README.md). Read
[`docs/retracted-findings.md`](docs/retracted-findings.md) before concluding
anything from a measurement, then [`docs/bench.md`](docs/bench.md).

## What works (2026-09-02)

* The panel renders sent content from flash after a power-cycle: black
  0.466 A (LEDs off), greys monotonic, every test pattern intact, the same
  result on three of three power-cycles. Every setting and the measurement
  behind it: [`docs/rendering.md`](docs/rendering.md).
* The wire protocol is byte-exact against the vendor sender
  ([`docs/pixel-protocol.md`](docs/pixel-protocol.md)); the proto tests pin it.
* The whole configuration is generated from `config/panels/<panel>.toml` with
  no donor file, and the reference `.rcvbp`, the factory basic pack and the
  factory boot image regenerate byte for byte under test
  (`crates/e120-rcvbp/tests/factory.rs`).
* Firmware 16.53 is installed; `e120 discover` reports it.

## Commands

```
sudo chmod o+rw /dev/bpf*                                   # after every reboot
cargo build
e120 discover                                               # firmware version, detected size
e120 provision --spec config/panels/p25-128x64-sm16269s.toml \
    --firmware third-party/firmware/E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex \
    --position 0,0 --commit                                 # then power-cycle
e120 show image picture.png --hold
e120 show video clip.mp4 --loop
scripts/bench.py run --boot --spec config/panels/p25-128x64-sm16269s.toml \
    --brightness 40 black white top left                    # every experiment
scripts/flash-review.py <dump>                              # after any flash operation
```

`--position x,y` is the cabinet's place in the wall (the EEPROM control
area); a wall is one card per panel, each provisioned with its position, then
`e120 show video --layout wall.json` ([`docs/provisioning.md`](docs/provisioning.md)).

## Rig rules

* PSU: read it and power-cycle it through `scripts/psu.sh` (10-minute
  auto-off); never change voltage or current settings. Brightness ≤ 40 until
  content is right; never flash while `ka3005p status` shows `CH1: Cc`.
* Vendor software: inspect, never execute.
* Configure from flash. `config send` (RAM) lands on about one boot in three
  and is for experiments only.

## Open items

1. Row band order reads reversed on the rotated panel (`line_dir`,
   `reversed_lines` in the spec). Not measured.
2. A faint flicker seen by eye; the 30 fps camera cannot resolve it. Re-assess
   by eye now that black is black.
3. EEPROM records `0x041`, `0x042`, `0x092` did not take through opcode
   `0x85` and are still `0xFF`.
4. `e120_proto::discovery::set_layout` sends the 98-byte FPP form, not the
   vendor's 1284-byte card-area pack
   ([`docs/archive/screen-connection-wire.md`](docs/archive/screen-connection-wire.md));
   it is off by default and provisioning does not need it.
5. Serial clock 8 (the reference file) versus 15 (chip default): not swept.

## Assets

`card-dumps/primary-region.bin` (day-one dump, firmware 10.81),
`build/snapshot-*/`, `third-party/firmware/` (16.53 is the only image naming
SM16269SH), `third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp` (the reference
`.rcvbp`: its records match the card's day-one flash under test; where it came
from is not recorded, and its name says a 256x384 wall), `analysis/`. The test fixture
`crates/e120-rcvbp/tests/fixtures/p25-128x64-fixed.rcvbp` is our own
construction, not vendor ground truth. History:
[`docs/archive/handoff-history.md`](docs/archive/handoff-history.md).
