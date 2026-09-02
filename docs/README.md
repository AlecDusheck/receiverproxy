# Documentation

Colorlight E120 with a P2.5 128x64 SM16269S panel, driven from the Rust CLI in this repo.

## Start here

| doc | what |
|---|---|
| [../CLAUDE.md](../CLAUDE.md) | orientation: hardware, commands, rig rules, code rules |
| [architecture.md](architecture.md) | spec → boot image → card → frames; which crate owns each step; where every measured default lives |
| [provisioning.md](provisioning.md) | bring a card to a working state in one command; multi-panel positioning |
| [rendering.md](rendering.md) | every setting that matters, its value, what pins it, what happened when it was wrong |
| [retracted-findings.md](retracted-findings.md) | claims this project recorded and later disproved |
| [building-a-config.md](building-a-config.md) | the TOML spec → `.rcvbp` + boot image generator |

## Formats

| doc | what |
|---|---|
| [pixel-protocol.md](pixel-protocol.md) | the row, latch and brightness frames, recovered from CLTNic.dll |
| [rcvbp-format.md](rcvbp-format.md) | the `.rcvbp` container and its records |
| [record-0x01-fields.md](record-0x01-fields.md) | record 0x01 byte by byte |
| [compiled-image-format.md](compiled-image-format.md) | the boot image at flash block 7, region by region |
| [panel-wiring.md](panel-wiring.md) | record 0x03 (pixel map) and this module's 64-column interleave |
| [eeprom-map.md](eeprom-map.md) | the EEPROM access frame and address map |

## Card internals

| doc | what |
|---|---|
| [chip-control-block.md](chip-control-block.md) | the 20-byte SChipControl: the driver-chip protocol descriptor |
| [chip-libraries-non-sh.md](chip-libraries-non-sh.md) | non-addressed S-PWM chips (SM16169S) and their SChipCustom block |
| [receiver-identity.md](receiver-identity.md) | the EEPROM control area: how a card knows which pixels are its own |
| [fpga-gateware.md](fpga-gateware.md) | the ECP5 bitstream in two pages; detail in [fpga/](fpga/README.md) |

## Bench

| doc | what |
|---|---|
| [bench.md](bench.md) | the rig, how measurements are taken, the meters and their limits, `scripts/bench.py` |

## Archive

| doc | what |
|---|---|
| [archive/handoff-history.md](archive/handoff-history.md) | how the bring-up went, day by day |
| [archive/black-floor.md](archive/black-floor.md) | the void-line table decode that removed the black floor |
| [archive/grey-mapping.md](archive/grey-mapping.md) | gamma table and grey-depth decode; both ruled out as the floor |
| [archive/firmware-16.53-bench-result.md](archive/firmware-16.53-bench-result.md) | installing 16.53 and the state right after |
| [archive/packet-statistics.md](archive/packet-statistics.md) | the discovery reply's counters |
| [archive/screen-connection-wire.md](archive/screen-connection-wire.md) | the vendor's card-area pack (not adopted) |
| [archive/vendor-sdk-analysis.md](archive/vendor-sdk-analysis.md) | how the vendor libraries were read |
| [archive/config-protocol.md](archive/config-protocol.md) | the first long-form analysis log; superseded where the pages above disagree |
| [fpga/](fpga/README.md) | gateware analysis detail |
