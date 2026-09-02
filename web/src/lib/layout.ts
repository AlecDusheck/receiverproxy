// Canvas helpers for the Wall editor. `validateJs` and `example` are the
// JS forms of the WASM module's `validate_layout` and `layout_example`;
// api/ops.ts uses them until the module is loaded.
import type { Canvas, Panel, Receiver, Rotation } from "../api/types";

export const rotated = (p: Panel): [number, number] =>
  p.rotation === "cw90" || p.rotation === "ccw90" ? [p.height, p.width] : [p.width, p.height];

export function snapSize(c: Canvas): number {
  const sizes = c.panels.flatMap((p) => [p.width, p.height]);
  return sizes.length ? Math.max(1, Math.min(...sizes)) : 16;
}

export const snap = (v: number, g: number) => Math.round(v / g) * g;
export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function addReceiver(c: Canvas): Receiver {
  const index = c.receivers.reduce((m, r) => Math.max(m, r.index + 1), 0);
  const w = c.receivers[0]?.width ?? 128;
  const h = c.receivers[0]?.height ?? 64;
  const x = c.receivers.reduce((m, r) => Math.max(m, (r.x ?? 0) + r.width), 0);
  const r: Receiver = { index, x, y: 0, width: w, height: h };
  c.receivers.push(r);
  if (x + w > c.width) c.width = x + w;
  if (h > c.height) c.height = h;
  return r;
}

export function addPanel(c: Canvas, receiver: number): Panel {
  const r = c.receivers.find((q) => q.index === receiver) ?? c.receivers[0];
  const ref = c.panels[0];
  const w = ref?.width ?? r?.width ?? 128;
  const h = ref?.height ?? r?.height ?? 64;
  const mine = c.panels.filter((p) => p.receiver === receiver);
  const rx = mine.reduce((m, p) => Math.max(m, (p.receiver_x ?? 0) + rotated(p)[0]), 0);
  const p: Panel = {
    receiver: r?.index ?? receiver,
    receiver_x: rx,
    receiver_y: 0,
    x: (r?.x ?? 0) + rx,
    y: r?.y ?? 0,
    width: w,
    height: h,
    rotation: "none",
    flip_x: false,
    flip_y: false,
  };
  c.panels.push(p);
  return p;
}

// Panel position in screen space is x,y; place() keeps receiver_x/y consistent.
export function place(c: Canvas, p: Panel, x: number, y: number) {
  const r = c.receivers.find((q) => q.index === p.receiver);
  p.x = x;
  p.y = y;
  if (r) {
    p.receiver_x = x - (r.x ?? 0);
    p.receiver_y = y - (r.y ?? 0);
  }
}

export function validateJs(c: Canvas): string {
  for (const r of c.receivers) {
    if ((r.x ?? 0) + r.width > c.width || (r.y ?? 0) + r.height > c.height)
      return `receiver ${r.index}: ${r.x ?? 0},${r.y ?? 0} ${r.width}x${r.height} exceeds the canvas ${c.width}x${c.height}`;
  }
  for (const [i, p] of c.panels.entries()) {
    const r = c.receivers.find((q) => q.index === p.receiver);
    if (!r) return `panel ${i}: receiver ${p.receiver} is not defined`;
    const [w, h] = rotated(p);
    if (p.x + p.width > c.width || p.y + p.height > c.height) return `panel ${i}: ${p.x},${p.y} ${p.width}x${p.height} exceeds the canvas`;
    if ((p.receiver_x ?? 0) + w > r.width || (p.receiver_y ?? 0) + h > r.height)
      return `panel ${i}: ${w}x${h} at ${p.receiver_x ?? 0},${p.receiver_y ?? 0} does not fit receiver ${r.index} (${r.width}x${r.height})`;
  }
  return "ok";
}

export function example(cols: number, rows: number, w: number, h: number): Canvas {
  const c: Canvas = { width: cols * w, height: rows * h, receivers: [], panels: [] };
  for (let r = 0; r < rows; r++)
    for (let col = 0; col < cols; col++) {
      const index = r * cols + col;
      const x = col * w;
      const y = r * h;
      c.receivers.push({ index, x, y, width: w, height: h });
      c.panels.push({ receiver: index, receiver_x: 0, receiver_y: 0, x, y, width: w, height: h, rotation: "none", flip_x: false, flip_y: false });
    }
  return c;
}

export const ROTATIONS: Rotation[] = ["none", "cw90", "ccw90", "rot180"];

// Every field present, as `e120 card layout-example` prints it.
export function normalize(c: Canvas): Canvas {
  return {
    width: c.width,
    height: c.height,
    receivers: c.receivers.map((r) => ({ index: r.index, x: r.x ?? 0, y: r.y ?? 0, width: r.width, height: r.height })),
    panels: c.panels.map((p) => ({
      receiver: p.receiver,
      receiver_x: p.receiver_x ?? 0,
      receiver_y: p.receiver_y ?? 0,
      x: p.x,
      y: p.y,
      width: p.width,
      height: p.height,
      rotation: p.rotation ?? "none",
      flip_x: p.flip_x ?? false,
      flip_y: p.flip_y ?? false,
    })),
  };
}
