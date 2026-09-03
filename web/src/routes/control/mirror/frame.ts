// The wire format POST /show/frame reads, built in the browser: the 12-byte
// header `sources::raw::Header` writes (`RXP\0`, version 1, a reserved byte,
// then width, height and fps little-endian), and rgb24 from the canvas.
import type { Fit } from "$api/types";

export function header(width: number, height: number, fps: number): Uint8Array {
  const b = new Uint8Array(12);
  b.set([0x52, 0x58, 0x50, 0x00]); // RXP\0
  b[4] = 1; // version
  const view = new DataView(b.buffer);
  view.setUint16(6, width, true);
  view.setUint16(8, height, true);
  view.setUint16(10, fps, true);
  return b;
}

/** `getImageData`'s rgba without the alpha. */
export function rgb24(rgba: Uint8ClampedArray): Uint8Array {
  const out = new Uint8Array((rgba.length / 4) * 3);
  for (let i = 0, o = 0; i < rgba.length; i += 4, o += 3) {
    out[o] = rgba[i]!;
    out[o + 1] = rgba[i + 1]!;
    out[o + 2] = rgba[i + 2]!;
  }
  return out;
}

/**
 * `drawImage`'s nine arguments for one fit: the source rectangle and the
 * destination rectangle, as `[sx, sy, sw, sh, dx, dy, dw, dh]`.
 * `stretch` fills, `contain` letterboxes, `cover` crops the source.
 */
export function place(fit: Fit, sw: number, sh: number, w: number, h: number): [number, number, number, number, number, number, number, number] {
  if (!sw || !sh) return [0, 0, 0, 0, 0, 0, 0, 0];
  if (fit === "stretch") return [0, 0, sw, sh, 0, 0, w, h];
  if (fit === "contain") {
    const scale = Math.min(w / sw, h / sh);
    const dw = Math.max(1, Math.round(sw * scale));
    const dh = Math.max(1, Math.round(sh * scale));
    return [0, 0, sw, sh, Math.round((w - dw) / 2), Math.round((h - dh) / 2), dw, dh];
  }
  const scale = Math.max(w / sw, h / sh);
  const cw = Math.min(sw, Math.round(w / scale));
  const ch = Math.min(sh, Math.round(h / scale));
  return [Math.round((sw - cw) / 2), Math.round((sh - ch) / 2), cw, ch, 0, 0, w, h];
}
