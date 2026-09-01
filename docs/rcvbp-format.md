# The `.rcvbp` receiver-parameter file format

Derived independently two ways — by structural analysis of the file bytes, and
by disassembly of `CHWParamReceiver::LoadFromBuffer` @ `0x170e50` in
`libCLTDevice.1.dylib` — which agree. Implemented in `src/rcvbp.rs`.

## File header (32 bytes)

```
0x00  16 bytes  signature, identifies the variant
0x10  u32       version (4 in every file seen)
0x14  u32       compressed size    (compressed variant only)
0x18  u32       decompressed size  (compressed variant only; used as zlib destLen)
0x1c  u32       reserved (0)
```

Two variants exist, distinguished by signature:

| Signature | Storage | Record stream |
|---|---|---|
| `20 20 19 be 74 23 43 45 b1 c7 93 03 9b 83 ae ab` | zlib | at file offset `0x20`, inflates to the size at `0x18` |
| `cb 3a 3f 21 52 07 3d 45 a8 d6 08 43 5f 7a 6c d5` | inline | starts at file offset `0x14`, followed by a 4-byte trailer |

## Record stream

The payload is a flat TLV stream that tiles the buffer exactly (verified: zero
slack across all 19 files tested, 89 070 bytes for the compressed sample):

```
[u16 size, little-endian, includes this 4-byte header]
[u8  marker]   0x0a in newer files, 0x09 in older ones; 0x00 on some records
[u8  id]
[payload; size - 4 bytes]
```

### Records observed

| id | Contents |
|---|---|
| `0x01` | Main receiver parameters — geometry, scan, timing, coefficients |
| `0x03` | Pixel/row mapping table: a 3-byte header then 4096 three-byte entries |
| `0x84` | Driver-chip register table: `(register, R, G, B)` quads |
| `0x8a` | Secondary parameters |
| `0xca` | Cabinet geometry: u16 width, u16 scan |
| `0x83`, `0x89` | Small RGB coefficient records (10 bytes each) |
| `0x07`, `0x86`, `0x8d`, `0x8e`, `0x8f`, `0x91`, `0x95`, `0xcd`, `0xd8`, `0xda` | Gamma and calibration tables; all zero in an uncalibrated profile |

For `P2.5-32S-128X64-SM16269S-256X384I.rcvbp`, only records `0x01`, `0x03`,
`0x84`, `0x8a`, `0xca` and the two small coefficient records carry data — about
13 KB of the 89 KB total. Everything else is empty tables.

## Record `0x01` field positions

Established empirically by diffing a corpus of 18 P2.5 configuration files that
vary in scan, driver chip, and module size while holding other parameters fixed.
Offsets are within the record payload.

| Offset | Field | Evidence |
|---|---|---|
| `+0x000` | Module width | 128 on 128-wide modules, 64 on 64-wide, 0x40/0x80 tracks the filename |
| `+0x001` | Scan denominator | 32 → 64 across an otherwise identical 32S/64S pair |
| `+0x020` | Scan (second copy) | changes with `+0x001` |
| `+0x021`, `+0x04b` | Timing derived from scan | 12 → 18 across the 32S/64S pair; also shifts with driver chip |
| `+0x023`, `+0x049` | Timing | shifts with both scan and chip |
| `+0x05a`–`+0x069` | 16-entry row-order permutation | wholly reordered across the 32S/64S pair |
| `+0x0ae`–`+0x0b1` | f32, changes with driver chip | `0x4372b3e9` vs `0x420dcf4e` between chip 9929 and 6618 |
| `+0x0fc`–`+0x105` | Chip-specific block | populated for chip 9929, zeroed for 6618 |

The driver chip is not identified by a single ID byte here; chip identity is
expressed through record `0x84`'s register table plus these timing values.

## Relationship to the wire protocol

The file is **not** replayed verbatim onto the wire. iSet parses it into a
`CHWParamReceiver` object and re-serializes that into typed packets — see
`config-protocol.md`. The record IDs above are file-format IDs, not packet types.
