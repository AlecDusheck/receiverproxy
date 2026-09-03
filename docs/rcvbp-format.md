# The `.rcvbp` receiver-parameter file format

`.rcvbp` is the vendor's receiver-parameter container. Sources: the file
bytes of the vendor corpus, and the disassembly of
`CHWParamReceiver::LoadFromBuffer` @ `0x170e50` in `libCLTDevice.1.dylib`;
the two agree. Implemented in `crates/rcvbp`. Record 0x01's fields are
decoded byte by byte in [`record-0x01-fields.md`](record-0x01-fields.md),
which is the authority for field names.

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

The payload is a flat TLV stream that tiles the buffer exactly (measured:
zero slack across 19 files; 89 070 bytes for the compressed sample):

```
[u16 size, little-endian, includes this 4-byte header]
[u8  marker]   0x0a in newer files, 0x09 in older ones; 0x00 on some records
[u8  id]
[payload; size - 4 bytes]
```

### Records

| id | Contents |
|---|---|
| `0x01` | Main receiver parameters: geometry, scan, timing, coefficients (764-byte payload) |
| `0x03` | Pixel/row mapping table: a 3-byte header then 4096 three-byte entries ([panel-wiring.md](panel-wiring.md)) |
| `0x84` | Driver-chip register table: `(register, R, G, B)` quads |
| `0x8a` | Secondary parameters; mirrors the screen size |
| `0xca` | Cabinet geometry: u16 width, u16 scan |
| `0x83`, `0x89` | Small RGB coefficient records (10 bytes each) |
| `0x07`, `0x86`, `0x8d`, `0x8e`, `0x8f`, `0x91`, `0x95`, `0xcd`, `0xd8`, `0xda` | Gamma and calibration tables; all zero in an uncalibrated profile |

In `P2.5-32S-128X64-SM16269S-256X384I.rcvbp` only records `0x01`, `0x03`,
`0x84`, `0x8a`, `0xca` and the two small coefficient records carry data, about
13 KB of the 89 KB total. The rest is empty tables.

## Record `0x01` field positions

Offsets are within the record payload. The corpus column records how the
byte varies across 18 P2.5 configuration files that differ in scan, driver
chip and module size with other parameters fixed. The name column is the
decoded accessor from [`record-0x01-fields.md`](record-0x01-fields.md).

| Offset | Name | Corpus variation |
|---|---|---|
| `+0x000` | module width (`GetMoudleWidth`) | 128 on 128-wide modules, 64 on 64-wide; `0x40`/`0x80` tracks the filename |
| `+0x001` | module height as stored (`GetMoudleHeight`) | 32 to 64 across an otherwise identical 32S/64S pair |
| `+0x020` | scan denominator (`GetScanMode`) | changes with `+0x001` |
| `+0x021`, `+0x04b` | serial clock frequency (`SetSerialClockFrequency`) and its duplicate | 12 to 18 across the 32S/64S pair; also shifts with driver chip |
| `+0x023` | gray level (`GetGrayLevel`) | shifts with both scan and chip |
| `+0x049` | serial clock / 2 | shifts with both scan and chip |
| `+0x05a` to `+0x069` | swap block 0 (`ResetSwapData`), 16 entries | wholly reordered across the 32S/64S pair |
| `+0x0ae` to `+0x0b1` | f32 minimum OE (`HR_SetMinOE`) | `0x4372b3e9` vs `0x420dcf4e` between chip 9929 and 6618 |
| `+0x0fc` to `+0x105` | chip-specific block | populated for chip 9929, zeroed for 6618 |

`+0x001` is not the scan denominator; the scan denominator is `+0x020`
(decoded from `GetScanMode`; day-one pack body `[0x07] = 0x10 = 16`).

The driver chip id is a 16-bit value split across `+0x036` (low byte) and
`+0x204` (high byte). Chip identity on the card is expressed through that id,
record `0x84`'s register table and the timing values above.

## Relationship to the wire protocol

The file is not replayed verbatim onto the wire. The vendor tool parses it
into a `CHWParamReceiver` object and re-serializes that into typed packets:
the 256-byte basic pack and the real-time pack push, described in
[fpga/parameter-path.md](fpga/parameter-path.md). The record ids above are
file-format ids, not packet types.
