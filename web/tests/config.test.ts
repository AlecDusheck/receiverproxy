// `node --test tests/config.test.ts`: the build-time loader against the
// repository's config/ files and the crate that embeds the same files.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cards, firmware, FORMATS, imageLocation, panels, root } from "../src/lib/server/config.ts";

const repo = root();

test("the bench spec is first, verified, and reads its chip library", () => {
  const list = panels(repo);
  const bench = list[0]!;
  assert.equal(bench.path, "config/panels/p25-128x64-sm16269s.toml");
  assert.equal(bench.name, "p25-128x64-sm16269s");
  assert.equal(bench.meta.status, "verified");
  assert.equal(bench.meta.pitch_mm, 2.5);
  assert.deepEqual(bench.module, { width: 128, height: 64, scan: 16 });
  assert.equal(bench.chip.family_id, 0x14c);
  assert.equal(bench.chip.library, "config/chips/sm16269s.toml");
  assert.deepEqual(bench.formats, ["rcvbp"]);
  assert.equal(bench.meta.maker, "Eager LED");
  assert.equal(bench.meta.product, "P2.5-O16S-SMD1415-128x64-E");
  assert.match(bench.meta.url ?? "", /^https:\/\/eager-led\.com\//);
  assert.match(bench.meta.datasheet ?? "", /\.pdf$/);
  assert.match(bench.meta.image ?? "", /\.jpg$/);
  assert.equal(bench.meta.image_source, "eager-led.com product photo");
  assert.equal(bench.chip.vendor, "Sunmoon");
  assert.match(bench.chip.datasheet ?? "", /sm16269\.pdf$/);
  assert.equal(list[1]!.path, "config/panels/p25-2x128x64-chain.toml");
  assert.equal(list[1]!.meta.status, "verified");
  assert.ok(list.slice(2).every((p) => p.meta.status === "derived"), "the derived specs follow the verified ones");
  assert.equal(new Set(list.map((p) => p.name)).size, list.length);
});

test("a derived spec without pitch fills the meta defaults", () => {
  const p = panels(repo).find((x) => x.path === "config/panels/104x104-52s-dp5525.toml")!;
  assert.equal(p.meta.pitch_mm, undefined);
  assert.equal(p.meta.status, "derived");
  assert.equal(p.meta.sources, 7);
  assert.equal(p.chip.name, "DP5525");
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
  assert.equal(e120!.image, "cards/e120.jpg");
  assert.equal(e120!.image_source, "eager-led.com product photo");
  assert.match(e120!.datasheet ?? "", /colorlight-e120\.pdf$/);
  assert.equal(e120!.limits.max_width, 1024);
  assert.equal(e120!.memory.parameter_block, 7);
  assert.deepEqual(e120!.memory.guarded[0], { from: "16.53", blocks: [0, 1, 2, 8] });
  assert.deepEqual(e120!.memory.boot_image[0], ["basic_pack", 0]);
  assert.equal(e120!.firmware.sdram_staging, true);
});

test("the firmware manifest lists every archived image and locates them at base_url", () => {
  const fw = firmware(repo);
  assert.equal(fw.images.length, 129);
  assert.equal(fw.base_url, "https://assets.receiverproxy.com");
  const first = fw.images[0]!;
  assert.equal(first.version, "16.53");
  assert.equal(fw.size, 721024);
  assert.equal(fw.prefix, "firmware/colorlight/e-series");
  assert.deepEqual(imageLocation(fw, first), { href: `${fw.base_url}/${fw.prefix}/${first.name}`, remote: true });
  assert.deepEqual(imageLocation({ base_url: "", prefix: fw.prefix, size: fw.size, images: [] }, first), { href: `third-party/firmware/${first.name}`, remote: false });
});
