# How this module is wired, and how record 0x03 says it

The pixel map (record `0x0a03`) is the card's answer to "where does framebuffer
pixel N physically live". Getting it wrong scrambles every column no matter how
correct the pixel data is, and it is not something you can eyeball from a byte
diff — decode it as structure with `scripts/mapstruct.py`.

## The record

```
[u16 count][3 bytes per entry: scan_line, slot, 0]
```

For this panel: 4096 entries = **16 scan lines x 256 chain slots**. The index is
`row * width + col` over the *stored* height (`height / 2` = 32 rows), because
the module's two row-halves hang off separate hub data groups.

`scripts/mapdump.py <file>` prints entries and the distinct values per byte;
`scripts/mapstruct.py <file>...` collapses it into monotonic runs, which is what
makes two configs comparable.

## The wiring this module actually has

The chain is walked in **64-column blocks**, alternating between the two
row-halves:

```
[lower cols 0-63][upper cols 0-63][lower cols 64-127][upper cols 64-127]
```

| | cols 0–63 | cols 64–127 |
|---|---|---|
| rows 0–31 (upper) | slots 64–127 | slots 192–255 |
| rows 32–63 (lower) | slots 0–63 | slots 128–191 |

As a formula, with `groups = stored_height / scan`:

```
slot = (col / blk) * (groups * blk) + group * blk + col % blk      # blk = 64
```

## The wiring we assumed, and why it was wrong

With `blk = width` the formula collapses to `group * width + col` — each data
group gets one contiguous 128-slot half of the chain. That is the majority
wiring across the vendor corpus and remains the generator's default, but it is
**not** this module's wiring, and it is what had been flashed to the card.

The difference had been recorded as an unreproducible "outlier" in the seller's
file, and pinned by a test asserting our own table was right. It is not an
outlier; it is a property of the module. `[mapping] block = 64` in the panel
spec selects it, and with that the generated table is the seller's byte-for-byte.

**Both `tests/fixtures/p25-128x64-fixed.rcvbp` and the "consensus donor" carry
the contiguous table and are our own artefacts, not vendor ground truth.** The
file that shipped with the panel is
`third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`.

## Reading scan from the file, not the label

The factory file's *name* says `32S`; the panel's label says `O16S`. The file
itself says 16, twice:

* the mapping's scan-line byte takes 16 distinct values (0–15);
* driver register `0x02` reads `0x0f` = scan − 1.

Our parser also prints "main param block: width 128, scan 1/32" for both this
file and ours — that field is not the module scan and the two disagreeing is
normal, not a bug.

## Driver registers (record 0x84)

The record is a flat stream of **4-byte groups**: register address, then one
value per colour. Decode and compare with `scripts/chipregs.py a.rcvbp b.rcvbp`.
Grouping it in 3s produces plausible-looking nonsense — that mistake cost a
round of analysis.

`config/chips/sm16269s-factory.toml` holds the values read out of the seller's
file. The generic `sm16269.toml` (the vendor tool's "Default Parameter" set)
disagrees in eleven registers, several governing greyscale and blanking.

The factory writes **no secondary chip id** (record 0x01 `+0x0E9` and `+0x205`
both zero). Claiming `0x14D` would declare max scan 64 on a 1/16 module, via the
vendor's own ResetIS rule where the sub-id overrides max scan.

## Status

With `block = 64`, the factory registers and no sub-id, `gen-config` reproduces
the seller's shipped file **record-for-record from TOML alone**, no donor —
pinned by `the_sellers_config_is_regenerated_record_for_record`. The only
intended difference is screen size: they compiled for a 256x384 wall of twelve
modules, we have one.

That is necessary but **not sufficient**: the panel still does not render
correctly, and the card's own test patterns fail too, so the seller's
configuration is itself not right for this module. See
[firmware-16.53-bench-result.md](firmware-16.53-bench-result.md).
