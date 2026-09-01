#!/usr/bin/env python3
"""Generate the compiled parameter image (flash 0x70000..0x78000) from a .rcvbp.

Model (see compiled-format.md): the image is page-structured, 128 x 256-byte
pages. Three page classes:

  GENERATED  pages we can synthesize from the .rcvbp records with proven,
             byte-exact transforms:
               0x18-0x27  gamma LUTs: two copies of u16-BE identity ramp
                          0x2000..0x23FF (factory gamma records are all zero)
               0x30-0x5F  pixel mapping table: record 0x0a03 payload[2:],
                          4096 x 3-byte entries, bytes 1,2 swapped (u16 LE->BE)
  TEMPLATE   pages copied from the factory compiled block, gated on the
             source records being byte-identical to the factory .rcvbp:
               0x00 (basic pack body), 0x05 (r01-tail page), 0x60, 0x63
  FF / 00    never-written (0xFF) and written-zero pages, from the factory map.

The generator REFUSES to run if any gating record differs from the factory
copy, because the template pages contain computed fields (incl. an unresolved
dword at page0+0xFC) that we cannot recompute. This makes the output provably
byte-exact for any input whose records match the factory config -- which
includes P2.5-32S-128X64-SM16269S-256X384I.rcvbp (verified identical).

Usage:
  compile_rcvbp.py <input.rcvbp> <out-compiled.bin> [--full-block <out-64k.bin>]

--full-block also emits the entire 64KB block-7 image: compiled image at 0,
u32-LE length-prefixed input file verbatim at 0x8000, 0xFF elsewhere
(page 0xF0, the EEPROM-mapped screen record, is NOT part of flash and is
emitted as 0xFF; the card ignores flash writes there anyway).
"""
import struct, sys, zlib

FACTORY_DUMP = '/Users/amd/e120/firmware/card-dumps/primary-region.bin'

# Pages that must be copied from the factory compiled block (computed fields).
TEMPLATE_PAGES = [0x00, 0x05, 0x60, 0x63]
# Pages left erased (0xFF) by the vendor tool.
FF_PAGES = [0x09, 0x0D, 0x0E, 0x0F] + list(range(0x28, 0x30)) + list(range(0x64, 0x68))
# Records whose bytes feed the template pages; must equal the factory copy.
GATING_RECORDS = [0x0a01, 0x0a84, 0x0a8a, 0x0aca, 0x0a8e, 0x0a83, 0x0a89]


def load_rcvbp_records(data: bytes):
    """Parse the compressed-variant .rcvbp into {type: payload} (first wins)."""
    if len(data) < 0x20:
        raise ValueError('too short for a .rcvbp')
    blob = zlib.decompress(data[0x20:])
    raw_len = struct.unpack_from('<I', data, 0x18)[0]
    if len(blob) != raw_len:
        raise ValueError(f'inflated {len(blob)} != header {raw_len}')
    recs, off = {}, 0
    while off + 4 <= len(blob):
        size = struct.unpack_from('<H', blob, off)[0]
        rtype = (blob[off + 2] << 8) | blob[off + 3]
        if size < 4:
            break
        recs.setdefault(rtype, blob[off + 4:off + size])
        off += size
    return recs


def factory_reference():
    d = open(FACTORY_DUMP, 'rb').read()
    compiled = d[0x70000:0x78000]
    n = struct.unpack_from('<I', d, 0x78000)[0]
    return compiled, d[0x78004:0x78004 + n]


def gamma_pages() -> bytes:
    ramp = b''.join(struct.pack('>H', 0x2000 + i) for i in range(0x400))
    return ramp + ramp  # pages 0x18-0x1F and 0x20-0x27


def mapping_pages(rec03: bytes) -> bytes:
    count = struct.unpack_from('<H', rec03, 0)[0]
    if count != 0x1000 or len(rec03) != 2 + 3 * count:
        raise ValueError(f'unexpected 0x0a03 shape: count={count:#x} len={len(rec03)}')
    out = bytearray()
    for i in range(count):
        a, b, c = rec03[2 + 3 * i:5 + 3 * i]
        out += bytes((a, c, b))  # 3-byte entry: flag, then u16 LE -> BE
    return bytes(out)


def compile_image(rcvbp_path: str) -> bytes:
    file_bytes = open(rcvbp_path, 'rb').read()
    recs = load_rcvbp_records(file_bytes)
    factory_compiled, factory_rcvbp = factory_reference()
    fact = load_rcvbp_records(factory_rcvbp)

    for rt in GATING_RECORDS:
        a, b = fact.get(rt), recs.get(rt)
        if a != b:
            raise SystemExit(
                f'record {rt:#06x} differs from the factory config; the '
                f'template pages cannot be recomputed for it -- refusing')
    for rt, pl in recs.items():
        if rt not in fact and any(pl):
            raise SystemExit(
                f'record {rt:#06x} is new and non-zero; unknown compiled '
                f'consequence -- refusing')

    img = bytearray(b'\xff' * 0x8000)
    for pg in range(0x80):
        img[pg * 0x100:(pg + 1) * 0x100] = b'\x00' * 0x100
    for pg in FF_PAGES:
        img[pg * 0x100:(pg + 1) * 0x100] = b'\xff' * 0x100
    for pg in TEMPLATE_PAGES:
        img[pg * 0x100:(pg + 1) * 0x100] = factory_compiled[pg * 0x100:(pg + 1) * 0x100]
    img[0x1800:0x2800] = gamma_pages()
    img[0x3000:0x6000] = mapping_pages(recs[0x0a03])
    return bytes(img)


def load_rcvbp_records_from_flash(raw: bytes):
    """The factory flash copy is a complete .rcvbp file, parse it the same way."""
    return load_rcvbp_records(raw)


def full_block(rcvbp_path: str, img: bytes) -> bytes:
    f = open(rcvbp_path, 'rb').read()
    if len(f) >= 0x6ffd:
        raise SystemExit('rcvbp exceeds the vendor 0x6FFC clamp')
    blk = bytearray(b'\xff' * 0x10000)
    blk[0:0x8000] = img
    struct.pack_into('<I', blk, 0x8000, len(f))
    blk[0x8004:0x8004 + len(f)] = f
    return bytes(blk)


if __name__ == '__main__':
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    src, dst = sys.argv[1], sys.argv[2]
    img = compile_image(src)
    open(dst, 'wb').write(img)
    print(f'wrote {dst} ({len(img):#x} bytes)')
    if '--full-block' in sys.argv:
        out = sys.argv[sys.argv.index('--full-block') + 1]
        open(out, 'wb').write(full_block(src, img))
        print(f'wrote {out} (64KB block-7 image)')
