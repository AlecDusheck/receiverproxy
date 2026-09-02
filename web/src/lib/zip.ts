// A zip of stored (uncompressed) files: enough for a handful of small
// config files, and no dependency.

const CRC: Uint32Array = (() => {
  const t = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[i] = c >>> 0;
  }
  return t;
})();

export function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC[(c ^ b) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export type ZipEntry = { name: string; bytes: Uint8Array };

/** The entries as one zip archive, stored, in the order given. */
export function zip(entries: ZipEntry[]): Uint8Array {
  const enc = new TextEncoder();
  const parts: Uint8Array[] = [];
  const dir: Uint8Array[] = [];
  let offset = 0;
  for (const e of entries) {
    const name = enc.encode(e.name);
    const sum = crc32(e.bytes);
    const local = new Uint8Array(30 + name.length);
    const v = new DataView(local.buffer);
    v.setUint32(0, 0x04034b50, true);
    v.setUint16(4, 20, true); // version needed
    v.setUint16(8, 0, true); // stored
    v.setUint32(14, sum, true);
    v.setUint32(18, e.bytes.length, true);
    v.setUint32(22, e.bytes.length, true);
    v.setUint16(26, name.length, true);
    local.set(name, 30);
    parts.push(local, e.bytes);

    const central = new Uint8Array(46 + name.length);
    const c = new DataView(central.buffer);
    c.setUint32(0, 0x02014b50, true);
    c.setUint16(4, 20, true);
    c.setUint16(6, 20, true);
    c.setUint16(10, 0, true);
    c.setUint32(16, sum, true);
    c.setUint32(20, e.bytes.length, true);
    c.setUint32(24, e.bytes.length, true);
    c.setUint16(28, name.length, true);
    c.setUint32(42, offset, true);
    central.set(name, 46);
    dir.push(central);
    offset += local.length + e.bytes.length;
  }
  const dirSize = dir.reduce((n, d) => n + d.length, 0);
  const end = new Uint8Array(22);
  const v = new DataView(end.buffer);
  v.setUint32(0, 0x06054b50, true);
  v.setUint16(8, entries.length, true);
  v.setUint16(10, entries.length, true);
  v.setUint32(12, dirSize, true);
  v.setUint32(16, offset, true);

  const total = offset + dirSize + end.length;
  const out = new Uint8Array(total);
  let at = 0;
  for (const p of [...parts, ...dir, end]) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}
