// `node --test tests/wall.test.ts`: the grid of src/lib/wall.ts, both ways.
import { test } from "node:test";
import assert from "node:assert/strict";
import { cardSize, chainOrder, CORNERS, DIRECTIONS, gridOf, layoutFromGrid, provisionLine, screenSize, type Corner, type Direction, type Grid } from "../src/lib/wall.ts";

const grid = (over: Partial<Grid> = {}): Grid => ({
  module: { width: 128, height: 64 },
  perCard: { columns: 2, rows: 4 },
  cards: { columns: 3, rows: 2 },
  start: "top-left",
  direction: "rows",
  serpentine: false,
  ...over,
});

test("screen and card sizes follow the counts", () => {
  assert.deepEqual(cardSize(grid()), { width: 256, height: 256 });
  assert.deepEqual(screenSize(grid()), { width: 768, height: 512 });
});

test("card windows tile the screen at the card size", () => {
  const c = layoutFromGrid(grid());
  assert.equal(c.width, 768);
  assert.equal(c.height, 512);
  assert.equal(c.receivers.length, 6);
  assert.deepEqual(c.receivers[0], { index: 0, x: 0, y: 0, width: 256, height: 256 });
  assert.deepEqual(c.receivers[5], { index: 5, x: 512, y: 256, width: 256, height: 256 });
});

test("panels sit on their card at module steps, unturned", () => {
  const c = layoutFromGrid(grid());
  assert.equal(c.panels.length, 6 * 8);
  const card4 = c.panels.filter((p) => p.receiver === 4);
  assert.equal(card4.length, 8);
  assert.deepEqual(card4[0], { receiver: 4, receiver_x: 0, receiver_y: 0, x: 256, y: 256, width: 128, height: 64, rotation: "none", flip_x: false, flip_y: false, max_brightness: 255 });
  assert.deepEqual(card4[7], { receiver: 4, receiver_x: 128, receiver_y: 192, x: 384, y: 448, width: 128, height: 64, rotation: "none", flip_x: false, flip_y: false, max_brightness: 255 });
});

// A 3 x 2 grid of cards, each chain order as [column,row] cells.
const cases: Record<string, [number, number][]> = {
  "top-left rows": [[0, 0], [1, 0], [2, 0], [0, 1], [1, 1], [2, 1]],
  "top-left rows serpentine": [[0, 0], [1, 0], [2, 0], [2, 1], [1, 1], [0, 1]],
  "top-left columns": [[0, 0], [0, 1], [1, 0], [1, 1], [2, 0], [2, 1]],
  "top-left columns serpentine": [[0, 0], [0, 1], [1, 1], [1, 0], [2, 0], [2, 1]],
  "top-right rows": [[2, 0], [1, 0], [0, 0], [2, 1], [1, 1], [0, 1]],
  "top-right rows serpentine": [[2, 0], [1, 0], [0, 0], [0, 1], [1, 1], [2, 1]],
  "top-right columns": [[2, 0], [2, 1], [1, 0], [1, 1], [0, 0], [0, 1]],
  "top-right columns serpentine": [[2, 0], [2, 1], [1, 1], [1, 0], [0, 0], [0, 1]],
  "bottom-left rows": [[0, 1], [1, 1], [2, 1], [0, 0], [1, 0], [2, 0]],
  "bottom-left rows serpentine": [[0, 1], [1, 1], [2, 1], [2, 0], [1, 0], [0, 0]],
  "bottom-left columns": [[0, 1], [0, 0], [1, 1], [1, 0], [2, 1], [2, 0]],
  "bottom-left columns serpentine": [[0, 1], [0, 0], [1, 0], [1, 1], [2, 1], [2, 0]],
  "bottom-right rows": [[2, 1], [1, 1], [0, 1], [2, 0], [1, 0], [0, 0]],
  "bottom-right rows serpentine": [[2, 1], [1, 1], [0, 1], [0, 0], [1, 0], [2, 0]],
  "bottom-right columns": [[2, 1], [2, 0], [1, 1], [1, 0], [0, 1], [0, 0]],
  "bottom-right columns serpentine": [[2, 1], [2, 0], [1, 0], [1, 1], [0, 1], [0, 0]],
};

for (const start of CORNERS)
  for (const direction of DIRECTIONS)
    for (const serpentine of [false, true]) {
      const name = `${start} ${direction}${serpentine ? " serpentine" : ""}`;
      test(`chain ${name}`, () => {
        assert.deepEqual(chainOrder({ columns: 3, rows: 2 }, start, direction, serpentine), cases[name]);
        // The layout carries the order as card indices, and reads back to the same grid.
        const g = grid({ start, direction, serpentine });
        const c = layoutFromGrid(g);
        cases[name]!.forEach(([col, row], i) => {
          const r = c.receivers.find((q) => q.index === i)!;
          assert.deepEqual([r.x / 256, r.y / 256], [col, row], `card ${i}`);
        });
        assert.deepEqual(gridOf(c), g);
      });
    }

test("a single card reads back as the plain grid", () => {
  const g = grid({ cards: { columns: 1, rows: 1 }, start: "bottom-right" as Corner, direction: "columns" as Direction, serpentine: true });
  assert.deepEqual(gridOf(layoutFromGrid(g)), grid({ cards: { columns: 1, rows: 1 } }));
});

test("a layout the grid cannot express is null", () => {
  const c = layoutFromGrid(grid());
  assert.notEqual(gridOf(c), null);
  const turned = structuredClone(c);
  turned.panels[3]!.rotation = "cw90";
  assert.equal(gridOf(turned), null);
  const moved = structuredClone(c);
  moved.receivers[1]!.x = 300;
  assert.equal(gridOf(moved), null);
  const gap = structuredClone(c);
  gap.width = 1024;
  assert.equal(gridOf(gap), null);
  const reindexed = structuredClone(c);
  reindexed.receivers[1]!.index = 7;
  assert.equal(gridOf(reindexed), null);
  const missing = structuredClone(c);
  missing.panels.pop();
  assert.equal(gridOf(missing), null);
  assert.equal(gridOf({ width: 128, height: 64, receivers: [], panels: [], max_brightness: 255 }), null);
});

test("optional fields default as the JSON does", () => {
  const c = layoutFromGrid(grid({ cards: { columns: 2, rows: 1 } }));
  const bare = JSON.parse(JSON.stringify(c)) as typeof c;
  for (const r of bare.receivers) if (r.x === 0) delete (r as Partial<typeof r>).x;
  for (const p of bare.panels) {
    delete (p as Partial<typeof p>).rotation;
    if (p.receiver_y === 0) delete (p as Partial<typeof p>).receiver_y;
  }
  assert.deepEqual(gridOf(bare), grid({ cards: { columns: 2, rows: 1 } }));
});

test("the provision line names the spec, the position and the chain index", () => {
  assert.equal(provisionLine("config/panels/p25-128x64-sm16269s.toml", 256, 64, 3), "rxp provision --spec config/panels/p25-128x64-sm16269s.toml --position 256,64 --index 3 --commit");
});
