# Adding a receiving card

How a receiving card is described to `rxp`, how the description is checked
against the card, how a new card or panel is brought up on the bench, and
what a second vendor needs. The E120 is the worked example throughout:
`config/cards/e120.toml` is the only model file, and the values quoted here
are its.

## 1. The model file

One TOML file per card in `config/cards/`, embedded into the `receivers`
crate at build time (`crates/receivers/build.rs`); nothing reads the
directory at run time. `receivers::models()` lists them tested first, then
by name; `default_model()` is the first tested one, which is what offline
generation lays the boot image out for when no card is named. Every field
is required unless marked optional.

| field | meaning |
|---|---|
| `name` | what `--card` takes, matched without regard to case (`E120`) |
| `vendor` | the maker, as printed by `rxp card models` (`Colorlight`) |
| `family` | the protocol family; `colorlight` is the only one implemented (section 4) |
| `id` | the first byte of the discovery reply; `rxp discover` prints it as `id=0x64` |
| `status` | `tested` (driven on a bench), `generates` (configurations build, never driven), `unsupported` |
| `notes` | optional free text shown nowhere but the file |
| `image` | optional: a photo for the web app's Cards pages, relative to `web/static` (`cards/e120.jpg`) |
| `image_source` | optional: where the photo came from, shown as its caption (`eager-led.com product photo`) |
| `[[tested]]` | one entry per panel driven on a bench with this card; a `tested` card has at least one, the others none |
| `tested.panel` | the panel spec it was driven with, relative to the repository root (`config/panels/p25-128x64-sm16269s.toml`); must be a file the build embeds |
| `tested.firmware` | the image the card ran, by its name in `config/firmware.toml` (`E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex`); the version comes from the manifest entry |
| `limits.max_width` | pixels across the card's control area, from the specification (1024) |
| `limits.max_height` | pixels down it (192) |
| `limits.hub_ports` | HUB75 headers on the card (12) |
| `limits.chain` | optional: cards on one chain when the specification states it |
| `memory.block_bytes` | flash erase-block size (0x10000) |
| `memory.primary_bank` | flash address of the primary bitstream bank (0x000000) |
| `memory.bank_bytes` | one bank's length; the block count is `bank_bytes / block_bytes` rounded up (0x0B0000, 11 blocks) |
| `memory.golden_bank` | flash address of the fallback bank; outside every write allowlist (0x200000) |
| `memory.parameter_block` | block index of the 64 KB boot image the card applies at power-on (0x07); must lie inside the primary bank |
| `memory.eeprom_mirror` | flash address the card mirrors its EEPROM to; the control area and screen-size record live there (0x07F000) |
| `[[memory.guarded]]` | optional, repeated: blocks a firmware version range write-protects from the host page-write path |
| `guarded.from` | first version the entry covers (`16.53`) |
| `guarded.to` | optional last version, inclusive; open-ended when absent |
| `guarded.blocks` | the block indices (`[0x00, 0x01, 0x02, 0x08]`) |
| `memory.boot_image.basic_pack` | offset of the 256-byte basic-parameter pack inside the parameter block (0x0000) |
| `memory.boot_image.data_swap` | offset of the data-swap pack (0x0500) |
| `memory.boot_image.module_positions` | offset of the module-position table (0x0600) |
| `memory.boot_image.chip_page` | offset of the chip-register page, record 0x84 verbatim (0x0900) |
| `memory.boot_image.void_line` | offset of the void-line packs (0x1000) |
| `memory.boot_image.void_line_columns` | offset of the void-line column table (0x1400) |
| `memory.boot_image.anti_void` | offset of the anti-void-line counters (0x1800) |
| `memory.boot_image.mapping` | offset of the pixel map, three bytes per entry (0x3000) |
| `memory.boot_image.scan_table` | offset of the scan table (0x6000) |
| `memory.boot_image.rcvbp` | offset of the length-prefixed embedded `.rcvbp` (0x8000); `config_page()` is the parameter block's first page plus this |
| `memory.boot_image.map_entries` | pixel-map entries the mapping region holds (4096) |
| `memory.boot_image.rcvbp_max` | largest embedded `.rcvbp` the card accepts (0x6FFC) |
| `firmware.image_pattern` | the vendor's image file names with `{version}` for `major.minor` (`E320_PWM_FPGA{version}_*.hex`); `provision --firmware` reads the version it must wait for from the file name through this |
| `firmware.sdram_staging` | true when the card stages an image in SDRAM and programs the guarded blocks itself (type 0x1a00) |

Where the values come from: the id byte from `rxp discover`; the limits and
port count from the vendor specification; the banks, the parameter block and
the mirror from a flash dump (`rxp flash dump-range`, then `rxp flash
scan` to see where the bitstream headers and the `.rcvbp` signature sit);
the guarded blocks from a firmware install that leaves blocks differing
after the host path ([provisioning.md](provisioning.md)); the boot-image
offsets from the vendor tool's image writer ([compiled-image-format.md](compiled-image-format.md)),
or from a block dump when the card was configured by the vendor tool. The
`receivers` tests refuse a parameter block outside the primary bank, a golden
bank inside it, a shared id or name, and a `tested` status without a
`[[tested]]` entry.

The boot-image offsets are the same for every card that runs the E320
gateware line; a card whose vendor tool writes a different image needs
its own values and, where the regions differ in shape, its own builders in
`crates/rcvbp/src/image/`.

## 2. The probe

```
rxp card probe [--card NAME] [--out DIR] [--index N] [--wait S]
```

Read-only: the frames it sends are one discovery and flash reads, which
carry no data. It discovers the card, takes the model from the id byte (or
from `--card`, for a card whose id no file carries yet), reads the first
1024 bytes of each firmware bank, the whole parameter block and the EEPROM
mirror, and prints one line per claim:

```
ok           discovery id 0x64
ok           reported size within limits: 128x64
ok           primary bank at 0x000000: bitstream header
ok           golden bank at 0x200000: bitstream header
ok           basic pack at +0x0000: marker and CRC
ok           embedded .rcvbp at +0x8000: 9431 bytes, 17 records
ok           mapping at +0x3000: written
ok           scan table at +0x6000: written
ok           chip page at +0x0900: record 0x84
ok           basic pack against record 0x01: scan 1/16, 12 bits, screen 128x64
ok           eeprom mirror at 0x07f000: control area 0,0-128,64
not checked  guarded blocks 00,01,02,08 on 16.53: checking means writing
not checked  12 hub ports: not readable
not checked  firmware via SDRAM staging: checking means installing firmware
```

A `mismatch` line names what the model expected and what the card holds
(`expected 0x64, seen 0x65`). The summary goes to stderr; the exit code is 1
when any line is a mismatch, so the probe can gate a script. `--out DIR`
writes `parameter-block.bin` and `eeprom-mirror.bin` there; without it
nothing touches the disk.

What each line checks: the id byte against `id`; the reported size against
`limits`; a Lattice bitstream header at the start of each bank; the basic
pack's marker and CRC at `boot_image.basic_pack`; a length within
`rcvbp_max` and a parsable file at `boot_image.rcvbp`; the mapping and scan
table regions not erased; the chip page erased (drivers not armed at boot)
or equal to the embedded file's record 0x84; the pack's scan, grey depth and
screen size against the embedded record 0x01; the mirror programmed, with
its control area. The guarded blocks stay `not checked`: proving a block
is write-protected means writing to it. The checker is a pure function over
the bytes read (`ops::probe::check_block`), unit-tested against the image
`rxp config gen` builds for the bench spec.

On a card configured by the vendor tool the chip page and the embedded
file reflect that tool's version: the older tool leaves the chip page
erased, so `chip page: erased, drivers not armed at boot` is the expected
reading for a factory card, not a fault.

## 3. The bench loop

The rig, the meters and their limits are in [bench.md](bench.md); the rule
that matters here is that a claim is recorded only with its measurement. A
new card, or a new panel on a known card, goes through the loop below. The
supply is only ever switched through `scripts/psu.sh`, brightness stays at
or below 40 until content is right, and nothing is flashed while the supply
shows constant-current.

1. **Probe.** `rxp discover` for the id byte and firmware; `rxp card probe`
   for the model, with `--card` naming the closest existing model when the
   id is new. Every `mismatch` is a value to measure before anything is
   written.
2. **Snapshot.** `rxp flash snapshot --dir snapshots/<card>-<date>` for
   the primary and golden banks, and `rxp flash dump-range` for the whole
   flash of a new card. Keep them outside the repository; they are the only
   way back.
3. **Model file.** Copy `config/cards/e120.toml`, set the id byte, the
   limits from the specification and the addresses from the dump, and
   `status = "generates"`. `cargo test -p receivers` and `rxp card models`
   accept it; `rxp card probe` now runs against it without `--card`.
4. **Spec.** A panel spec in `config/panels/` ([building-a-config.md](building-a-config.md)),
   started from the closest class in `config/panels/mined/` or from a vendor
   file for the exact module when one exists. `rxp config gen --card NAME
   --spec ...` builds the file, the pack, the boot image and the sources
   report offline.
5. **Provision.** `rxp provision --card NAME --spec ... --position 0,0`
   prints the plan; with `--commit` it snapshots, installs firmware when
   given, writes the boot image, rewrites the EEPROM records and verifies
   the control area. Power-cycle; the card configures itself from flash.
   `rxp card probe` afterwards should read all `ok`, and
   `scripts/flash-review.py` names every run of block 7 that differs from
   the reference dump.
6. **Capture.** `scripts/bench.py locate` once per rig to find the panel in
   frame, then `scripts/bench.py run --boot --spec ... --brightness 20 black
   white top left` for the first look: one looping stream, every condition
   photographed and metered mid-segment, the first condition repeated as
   the control. An all-black frame that stays lit is conclusive on its own.
7. **Meters.** The supply current per condition from the run, the averaged
   captures (`bench.py capture`, `compare`, `tile`), and `rxp discover` for
   the reported size. A difference is a finding only when it exceeds what the
   control shows against itself, and only after the run is repeated.
8. **Adjust.** Change one spec field, or one `record01_overrides` byte, per
   run; rerun from step 5 (`flash restore-block` plus a power-cycle when only
   the boot image changed, `rxp config send` for a RAM-only try that lands
   on about one boot in three). Record the value, the run and its readings
   in [rendering.md](rendering.md); record what did not work in
   [retracted-findings.md](retracted-findings.md).

When the panel renders from flash after a power-cycle, the model file
gains a `[[tested]]` entry naming the spec and the firmware, `status`
becomes `tested`, and `rxp card models --markdown` regenerates the README's
matrix (a unit test fails until it is pasted between the `<!-- tested -->`
markers).

## 4. Adding a vendor

The vendor-specific code is in two crates, each behind one trait. A second
vendor is a protocol crate implementing `Protocol` in `colorlight`'s place
and a codec crate implementing `Codec` in `rcvbp`'s place; `panelspec`
(the spec and chip library), `receivers` (the model files, with the new
`family`), `wall`, `sources` and `driver` stay as they are.

`colorlight::Protocol`, frame builders and reply parsers only; sockets,
timing and sequencing stay in `ops` and `driver`:

| method | builds or parses |
|---|---|
| `discover(&self) -> Vec<u8>` | the discovery request |
| `discovery_reply(&self, frame) -> Option<DiscoveryInfo>` | the card a reply describes: id byte, firmware, reported size, index |
| `row(&self, buf, row, x, rgb, order)` | one row packet into a caller-owned buffer, screen row and x offset |
| `latch(&self, brightness) -> Vec<u8>` | the frame that applies the rows sent since the last one |
| `brightness(&self, brightness) -> Vec<u8>` | the brightness frame sent before the rows |
| `flash_read(&self, index, page) -> Vec<u8>` | one chunk of flash at a 256-byte page index |
| `flash_reply(&self, frame) -> Option<&[u8]>` | the flash bytes a reply carries |
| `flash_write(&self, map, index, block, page, data) -> Result<Vec<u8>, WriteError>` | one page of the parameter block, refused outside the `FlashMap` |
| `eeprom_write(&self, addr, data) -> Vec<u8>` | one EEPROM record at its own address and length |

`Colorlight` implements it over the crate's free functions, and a test pins
the two to the same bytes. The `FlashMap` allowlists are built from the
model file (`ops::model::flash_map`), so a new vendor's write frames are
confined the same way.

`rcvbp::Codec`, the configuration format:

| method | does |
|---|---|
| `format(&self) -> Format` | the registry entry: `name` (what `--format` takes), `vendor`, `extension`, and whether `generate` and `import` are implemented |
| `matches(&self, file) -> bool` | true when the file starts with the format's signature; `rcvbp::detect(file)` asks each codec in turn |
| `generate(&self, spec, chip) -> Result<Encoded>` | the file the card's tooling loads for a `PanelSpec` and its `ChipLibrary`, plus one source line per byte range placed |
| `inspect(&self, file) -> Result<Vec<String>>` | one line per record of a file, as `rxp config info` lists them |
| `import(&self, file, chips) -> Result<(PanelSpec, Vec<String>)>` | the spec that regenerates the file and the fields it could not recover, by name; `chips` maps a chip id to a library `(path, text)`. The default fails as not implemented; a codec whose `Format::import` is true overrides it |

`RcvbpCodec` implements it over `rcvbp::spec::generate`, `Rcvbp::from_bytes`
and `rcvbp::spec::spec_from_rcvbp`.
The registry is `rcvbp::codecs()`, a static list of the implementations;
`rcvbp::formats()` iterates their entries, `rcvbp::codec(name)` looks one
up, failing with the known names, and `rcvbp::detect(file)` picks one by
signature. `rxp config formats` and the site's
format list print that table; `rxp config gen --format NAME` and the WASM
`generate(spec_toml, format)` look the name up before generating;
`rxp config import FILE` and the WASM `import(bytes, format?)` detect it
unless told. A new codec is one more element in `codecs()`.
The boot image is not part of the trait: it is the E320 gateware line's
flash layout (`rcvbp::image`, laid out from `boot_image`), and a vendor
whose card loads the file directly needs none.

The command modules in `ops` call the Colorlight functions directly today;
the traits are the surface a second implementation fills in, and the call
sites to dispatch through them are the ones that name `protocol::` in
`crates/ops/src/*.rs` and the frame builders `driver` calls. Until a second
card exists that dispatch is a change with one implementation, and stays out.
