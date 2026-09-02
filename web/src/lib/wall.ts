// The Wall's grid: a screen of cards, each card a rectangle of panels, the
// cards chained in an order. `layoutFromGrid` writes the layout JSON
// (`Canvas`, docs/ui.md section 4) and `gridOf` reads one back, or null when
// the layout is not a grid. Pure; pinned by tests/wall.test.ts.
import type { Canvas, Panel, Receiver } from "../api/types";

export type Corner = "top-left" | "top-right" | "bottom-left" | "bottom-right";
export type Direction = "rows" | "columns";
export type Size = { width: number; height: number };
export type Count = { columns: number; rows: number };
export type Grid = {
  /** One panel's pixels. */
  module: Size;
  /** Panels per card. */
  perCard: Count;
  cards: Count;
  start: Corner;
  direction: Direction;
  serpentine: boolean;
};

export const CORNERS: Corner[] = ["top-left", "top-right", "bottom-left", "bottom-right"];
export const DIRECTIONS: Direction[] = ["rows", "columns"];

export const cardSize = (g: Grid): Size => ({ width: g.perCard.columns * g.module.width, height: g.perCard.rows * g.module.height });
export const screenSize = (g: Grid): Size => {
  const c = cardSize(g);
  return { width: g.cards.columns * c.width, height: g.cards.rows * c.height };
};

const positive = (n: number) => Number.isInteger(n) && n > 0;
export const gridValid = (g: Grid) =>
  [g.module.width, g.module.height, g.perCard.columns, g.perCard.rows, g.cards.columns, g.cards.rows].every(positive);

/**
 * The grid cell `[column, row]` of each card in chain order. The chain
 * starts at `start`, runs along `direction` (rows: left or right first;
 * columns: up or down first) and, when serpentine, turns back on each line.
 */
export function chainOrder(cards: Count, start: Corner, direction: Direction, serpentine: boolean): [number, number][] {
  const fromRight = start.endsWith("right");
  const fromBottom = start.startsWith("bottom");
  const range = (n: number, reverse: boolean) => Array.from({ length: n }, (_, i) => (reverse ? n - 1 - i : i));
  const out: [number, number][] = [];
  if (direction === "rows") {
    range(cards.rows, fromBottom).forEach((row, line) => {
      for (const col of range(cards.columns, fromRight !== (serpentine && line % 2 === 1))) out.push([col, row]);
    });
  } else {
    range(cards.columns, fromRight).forEach((col, line) => {
      for (const row of range(cards.rows, fromBottom !== (serpentine && line % 2 === 1))) out.push([col, row]);
    });
  }
  return out;
}

/** The layout JSON for a grid: card `index` is its place in the chain. */
export function layoutFromGrid(g: Grid): Canvas {
  const card = cardSize(g);
  const screen = screenSize(g);
  const c: Canvas = { width: screen.width, height: screen.height, receivers: [], panels: [] };
  chainOrder(g.cards, g.start, g.direction, g.serpentine).forEach(([col, row], index) => {
    const x = col * card.width, y = row * card.height;
    c.receivers.push({ index, x, y, width: card.width, height: card.height });
    for (let pr = 0; pr < g.perCard.rows; pr++)
      for (let pc = 0; pc < g.perCard.columns; pc++) {
        const rx = pc * g.module.width, ry = pr * g.module.height;
        c.panels.push({ receiver: index, receiver_x: rx, receiver_y: ry, x: x + rx, y: y + ry, width: g.module.width, height: g.module.height, rotation: "none", flip_x: false, flip_y: false });
      }
  });
  return c;
}

const same = <T extends object>(a: T, b: T, keys: (keyof T)[]) => keys.every((k) => a[k] === b[k]);

/** The grid a layout was made from, or null when no grid produces it. */
export function gridOf(c: Canvas): Grid | null {
  const r0 = c.receivers[0], p0 = c.panels[0];
  if (!r0 || !p0) return null;
  if (![c.width, c.height, r0.width, r0.height, p0.width, p0.height].every(positive)) return null;
  if (c.width % r0.width || c.height % r0.height || r0.width % p0.width || r0.height % p0.height) return null;
  const cards: Count = { columns: c.width / r0.width, rows: c.height / r0.height };
  const perCard: Count = { columns: r0.width / p0.width, rows: r0.height / p0.height };
  if (c.receivers.length !== cards.columns * cards.rows) return null;
  if (c.panels.length !== c.receivers.length * perCard.columns * perCard.rows) return null;

  // Every card: the same size, on a cell, one card per cell, indices 0..n-1.
  const cell = new Map<string, Receiver>();
  const byIndex = new Map<number, Receiver>();
  for (const r of c.receivers) {
    const x = r.x ?? 0, y = r.y ?? 0;
    if (!same(r, r0, ["width", "height"]) || x % r0.width || y % r0.height) return null;
    const key = `${x / r0.width},${y / r0.height}`;
    if (cell.has(key) || byIndex.has(r.index)) return null;
    cell.set(key, r);
    byIndex.set(r.index, r);
  }
  if ([...byIndex.keys()].some((i) => i < 0 || i >= c.receivers.length)) return null;

  // Every panel: the same size, unturned, on a cell of its card, one per cell.
  const slot = new Set<string>();
  for (const p of c.panels) {
    const r = byIndex.get(p.receiver);
    if (!r || !same(p, p0, ["width", "height"]) || (p.rotation ?? "none") !== "none" || p.flip_x || p.flip_y) return null;
    const rx = p.receiver_x ?? 0, ry = p.receiver_y ?? 0;
    if (p.x !== (r.x ?? 0) + rx || p.y !== (r.y ?? 0) + ry || rx % p0.width || ry % p0.height) return null;
    const key = `${p.receiver}:${rx / p0.width},${ry / p0.height}`;
    if (slot.has(key)) return null;
    slot.add(key);
  }

  // The chain: the first corner, direction and turn that give these indices.
  for (const start of CORNERS)
    for (const direction of DIRECTIONS)
      for (const serpentine of [false, true]) {
        const order = chainOrder(cards, start, direction, serpentine);
        if (order.every(([col, row], i) => cell.get(`${col},${row}`)?.index === i))
          return { module: { width: p0.width, height: p0.height }, perCard, cards, start, direction, serpentine };
      }
  return null;
}

/** The panels a card drives, in the layout's order. */
export const cardPanels = (c: Canvas, index: number): Panel[] => c.panels.filter((p) => p.receiver === index);

/** The command that gives a card its window: what the drawing shows for a selected card. */
export const provisionLine = (spec: string, x: number, y: number) => `rxp provision --spec ${spec} --position ${x},${y} --commit`;
