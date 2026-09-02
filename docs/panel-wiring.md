# Pixel map (record 0x03) and the P2.5 128x64 module wiring

The pixel map, `.rcvbp` record `0x0a03`, tells the card where framebuffer
pixel N lives on the driver chain. A wrong map scrambles every column
regardless of the pixel data. The map is compared as structure, not as a byte
diff: `scripts/mapstruct.py` collapses it into monotonic runs.

## Record format

```
[u16 count][3 bytes per entry: scan_line, slot, 0]
```

For this panel: 4096 entries = 16 scan lines x 256 chain slots. The entry
index is `row * width + col` over the stored height, `height / 2` = 32 rows,
because the module's two row-halves hang off separate hub data groups.

Tools:

| tool | output |
|---|---|
| `scripts/mapdump.py <file>` | every entry and the distinct values per byte |
| `scripts/mapstruct.py <file>...` | the map as monotonic runs, comparable between files |

## Wiring of this module

The chain is walked in 64-column blocks, alternating between the two
row-halves:

```
[lower cols 0-63][upper cols 0-63][lower cols 64-127][upper cols 64-127]
```

| | cols 0-63 | cols 64-127 |
|---|---|---|
| rows 0-31 (upper) | slots 64-127 | slots 192-255 |
| rows 32-63 (lower) | slots 0-63 | slots 128-191 |

As a formula, with `groups = stored_height / scan`:

```
slot = (col / blk) * (groups * blk) + group * blk + col % blk      # blk = 64
```

`[mapping] block = 64` in the panel spec selects this wiring. With it the
generated table equals the reference file's record 0x03 byte for byte.

## Contiguous wiring (not this module)

With `blk = width` the formula collapses to `group * width + col`: each data
group gets one contiguous 128-slot half of the chain. That is the majority
wiring across the vendor config corpus and is the generator's default. It is
not this module's wiring; measured: flashed to the card it scrambles every
column.

`tests/fixtures/p25-128x64-fixed.rcvbp` and the "consensus donor" fixture
both carry the contiguous table. They are this repository's own artefacts,
not vendor ground truth. The reference file is
`third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`; its records
match the card's factory flash.

## Scan rate

The reference file's name says `32S`; the panel label says `O16S`. The file
content gives 16:

* the mapping's scan-line byte takes 16 distinct values (0-15);
* driver register `0x02` reads `0x0f` = scan - 1.

The parser prints "main param block: width 128, scan 1/32" for both the
reference file and the generated one. That field is not the module scan;
the two values disagreeing is normal.

## Driver registers (record 0x84)

The record is a flat stream of 4-byte groups: register address, then one
value per colour. Grouping it in 3s produces plausible-looking nonsense.
Decode and compare with `scripts/chipregs.py a.rcvbp b.rcvbp`.

`config/chips/sm16269s-factory.toml` holds the values from the reference
file. The generic `config/chips/sm16269.toml` (the vendor tool's "Default
Parameter" set) differs in eleven registers, several governing greyscale and
blanking.

## Secondary chip id

The reference file carries no secondary chip id: record 0x01 `+0x0E9` and
`+0x205` are both zero. A sub-id of `0x14D` would declare max scan 64 on a
1/16 module, because the vendor's ResetIS rule lets the sub-id override max
scan.

## Regeneration from TOML

With `block = 64`, the factory registers and no sub-id, `rxp config gen`
reproduces the reference file record for record from TOML alone, with no
donor file. Pinned by `the_reference_config_is_regenerated_record_for_record`
in `crates/rcvbp/tests/factory.rs`. The one intended difference is
screen size: the reference file is compiled for a 256x384 wall of twelve
modules; the bench spec has one.

## Dependencies

The mapping alone does not make the panel render. Also required: record 0x01
`+0x02F` = 1, the measured frame order, configuration from flash, and the
phantom positions `width..2*width` gated through the void-line column table
(this is what makes an all-black frame go dark). Each setting and its
measurement: [rendering.md](rendering.md).
