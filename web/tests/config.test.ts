// `node --test tests/config.test.ts`: the build-time loader against the
// repository's config/ files and the crate that embeds the same files.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cards, firmware, FORMATS, imageLocation, panels, root } from "../src/lib/server/config.ts";

const repo = root();

test("the bench spec is first, tested, and reads its chip library", () => {
  const list = panels(repo);
  const bench = list[0]!;
  assert.equal(bench.path, "config/panels/p25-128x64-sm16269s.toml");
  assert.equal(bench.name, "p25-128x64-sm16269s");
  assert.equal(bench.meta.status, "tested");
  assert.equal(bench.meta.origin, "bench");
  assert.equal(bench.meta.pitch_mm, 2.5);
  assert.deepEqual(bench.module, { width: 128, height: 64, scan: 16 });
  assert.equal(bench.chip.family_id, 0x14c);
  assert.equal(bench.chip.library, "config/chips/sm16269s-factory.toml");
  assert.deepEqual(bench.formats, ["rcvbp"]);
  assert.equal(bench.mined, false);
  assert.ok(list.slice(1).every((p) => p.mined), "mined specs follow the bench spec");
  assert.equal(new Set(list.map((p) => p.name)).size, list.length);
});

test("a mined spec without pitch fills the meta defaults", () => {
  const p = panels(repo).find((x) => x.path === "config/panels/mined/104x104-52s-dp5525.toml")!;
  assert.equal(p.meta.pitch_mm, undefined);
  assert.equal(p.meta.origin, "mined");
  assert.equal(p.meta.sources, 7);
  assert.equal(p.chip.name, "DP5525 (mined)");
});

test("the format table matches the codec registry the crate pins", () => {
  // crates/rcvbp-wasm/src/api.rs asserts the bench entry's formats are ["rcvbp"].
  const api = readFileSync(join(repo, "crates", "rcvbp-wasm", "src", "api.rs"), "utf8");
  assert.match(api, /assert_eq!\(bench\.formats, \["rcvbp"\]\)/);
  assert.deepEqual(
    FORMATS.map((f) => f.name),
    ["rcvbp"],
  );
});

test("the E120 model file reads whole", () => {
  const [e120] = cards(repo);
  assert.equal(e120!.name, "E120");
  assert.equal(e120!.id, 0x64);
  assert.equal(e120!.status, "tested");
  assert.equal(e120!.tested[0]!.panel, "config/panels/p25-128x64-sm16269s.toml");
  assert.equal(e120!.limits.max_width, 1024);
  assert.equal(e120!.memory.parameter_block, 7);
  assert.deepEqual(e120!.memory.guarded[0], { from: "16.53", blocks: [0, 1, 2, 8] });
  assert.deepEqual(e120!.memory.boot_image[0], ["basic_pack", 0]);
  assert.equal(e120!.firmware.sdram_staging, true);
});

test("the firmware manifest lists five images and locates them in the repository", () => {
  const fw = firmware(repo);
  assert.equal(fw.images.length, 5);
  assert.equal(fw.base_url, "");
  const first = fw.images[0]!;
  assert.equal(first.version, "16.53");
  assert.equal(first.size, 721024);
  assert.deepEqual(imageLocation(fw, first), { href: `third-party/firmware/${first.name}`, remote: false });
  assert.deepEqual(imageLocation({ base_url: "https://x/", images: [] }, first), { href: `https://x/${first.name}`, remote: true });
});
