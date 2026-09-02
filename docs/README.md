# Research log

Everything learned bringing up a Colorlight E120 with a P2.5 128x64
SM16269S panel, by topic. Start with [`../HANDOFF.md`](../HANDOFF.md).

## How to use the card
| doc | what |
|---|---|
| [architecture.md](architecture.md) | the map: spec → boot image → card → frames, which crate owns each step, where every measured default lives |
| [provisioning.md](provisioning.md) | bring a fresh card to a working state in one command; multi-panel positioning |
| [rendering-recipe.md](rendering-recipe.md) | every configuration value that matters for this panel, with what each alternative did |
| [building-a-config.md](building-a-config.md) | the TOML spec → `.rcvbp` + boot image generator |
| [bench-measurement.md](bench-measurement.md) | how to measure on this rig without fooling yourself; `scripts/bench.py` |
| [retracted-findings.md](retracted-findings.md) | every claim we later disproved, and why |

## The card's data
| doc | what |
|---|---|
| [rcvbp-format.md](rcvbp-format.md) | the `.rcvbp` container and records |
| [record-0x01-fields.md](record-0x01-fields.md) | record 0x01 byte by byte |
| [compiled-image-format.md](compiled-image-format.md) | the boot image at flash block 7, region by region |
| [panel-wiring.md](panel-wiring.md) | record 0x03 (pixel map) structure; this module's 64-column interleave |
| [chip-control-block.md](chip-control-block.md) | the 20-byte SChipControl: the driver-chip protocol descriptor |
| [chip-libraries-non-sh.md](chip-libraries-non-sh.md) | non-addressed S-PWM chips (SM16169S) and their custom block |
| [grey-mapping.md](grey-mapping.md) | how input values become data words; gamma tables; the 12-bit field table |
| [black-floor.md](black-floor.md) | the void-line displacement table, decoded — and the floor it removes |
| [eeprom-map.md](eeprom-map.md), [receiver-identity.md](receiver-identity.md), [screen-connection-wire.md](screen-connection-wire.md) | the EEPROM records; the control area; the Screen Connection frames |
| [packet-statistics.md](packet-statistics.md) | the discovery reply's counters |

## Wire and firmware
| doc | what |
|---|---|
| [pixel-protocol.md](pixel-protocol.md) | the vendor sender's frames, recovered from CLTNic.dll |
| [firmware-16.53-bench-result.md](firmware-16.53-bench-result.md) | installing 16.53; what it fixed |
| [fpga-gateware.md](fpga-gateware.md), [fpga/](fpga/) | the ECP5 bitstream: format, pinout, output stage, block RAM, microcode, flash layout |
| [own-gateware/PLAN.md](own-gateware/PLAN.md) | feasibility study for a gateware of our own (insurance; not needed) |
| [vendor-sdk-analysis.md](vendor-sdk-analysis.md), [archive/](archive/) | where the vendor material is and the first session's long-form notes |
