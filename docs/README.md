# Documentation

Reference for the Colorlight E120 receiving card, its formats, its protocol,
its gateware, and the `rxp` tool that drives it. The reference module
throughout is a P2.5 128x64 SM16269S panel on an E120 running firmware
16.53: the one card-and-panel pair whose values have been driven and
measured.

CLAUDE.md: orientation for contributors ([../CLAUDE.md](../CLAUDE.md)).

## Platform

| doc | what |
|---|---|
| [architecture.md](architecture.md) | spec to boot image to card to frames; which crate owns each step; where each measured default lives |
| [provisioning.md](provisioning.md) | `rxp provision`: snapshot, firmware, configuration, EEPROM, verify; multi-panel positioning |
| [rendering.md](rendering.md) | what a working configuration needs: every setting that affects the picture, its value, what it does, what goes wrong when it is wrong |
| [bench.md](bench.md) | bringing up a card: the rig, the one-run-with-control method, the meters and their limits, `scripts/bench.py` |
| [building-a-config.md](building-a-config.md) | the TOML panel spec, the chip library, `rxp config gen` |
| [cards.md](cards.md) | adding a receiving card: the model file field by field, `rxp card probe`, the bench loop for a new card or panel, adding a vendor through the `Protocol` and `Codec` traits |
| [ui.md](ui.md) | the web UI, `rxp ui` and its JSON API, the WASM module: the contract they are built against |
| [ui-design.md](ui-design.md) | the web app's design: principles, layout, type, colour tokens, components, states, the review checklist |

## Formats

| doc | what |
|---|---|
| [rcvbp-format.md](rcvbp-format.md) | the `.rcvbp` container and its records |
| [record-0x01-fields.md](record-0x01-fields.md) | record 0x01 byte by byte |
| [compiled-image-format.md](compiled-image-format.md) | the boot image at flash block 7, region by region |
| [panel-wiring.md](panel-wiring.md) | record 0x03 (pixel map) and the 128x64 module's 64-column interleave |
| [eeprom-map.md](eeprom-map.md) | the EEPROM access frame and address map |

## Protocol

| doc | what |
|---|---|
| [pixel-protocol.md](pixel-protocol.md) | the row, latch and brightness frames, as emitted by CLTNic.dll |
| [receiver-identity.md](receiver-identity.md) | the EEPROM control area: which incoming pixels a card keeps |

## Driver chips

| doc | what |
|---|---|
| [chip-control-block.md](chip-control-block.md) | the 20-byte SChipControl block and the other chip-protocol fields of record 0x01 |
| [chip-libraries-non-sh.md](chip-libraries-non-sh.md) | non-addressed S-PWM chips (SM16169S) and their SChipCustom block |

## Gateware

| doc | what |
|---|---|
| [fpga-gateware.md](fpga-gateware.md) | the ECP5 bitstream in two pages |
| [fpga/README.md](fpga/README.md) | index of the gateware analysis: bitstream, flash, pinout, block RAM, pixel path, output stage, unresolved points |
