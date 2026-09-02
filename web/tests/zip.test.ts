import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { crc32, zip } from "../src/lib/zip.ts";

test("crc32 matches the known value", () => {
  assert.equal(crc32(new TextEncoder().encode("123456789")), 0xcbf43926);
});

test("unzip reads the archive back", () => {
  const files = [
    { name: "a.rcvbp", bytes: new Uint8Array([1, 2, 3, 4, 5]) },
    { name: "b.toml", bytes: new TextEncoder().encode('name = "x"\n') },
  ];
  const dir = mkdtempSync(join(tmpdir(), "zip-"));
  const path = join(dir, "out.zip");
  writeFileSync(path, zip(files));
  execFileSync("unzip", ["-q", "-o", path, "-d", dir]);
  for (const f of files) {
    assert.deepEqual(new Uint8Array(readFileSync(join(dir, f.name))), f.bytes);
  }
  assert.match(execFileSync("unzip", ["-t", path]).toString(), /No errors/);
});
