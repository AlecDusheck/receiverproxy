// The build-time loader behind the prerendered pages: config/panels/**/*.toml,
// config/cards/*.toml and config/firmware.toml read from the repository with
// node and parsed with smol-toml. The field names are the files' own, and the
// gallery entries take the shape the WASM `gallery()` returns (api/types.ts
// `Entry`), so the table is the same whichever produced it. No WASM runs at
// build time.
import { readdirSync, readFileSync, existsSync } from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";
import { parse } from "smol-toml";
import type { Entry, Format, Meta } from "../../api/types";

type Table = Record<string, unknown>;

/** The repository root: the nearest ancestor of `from` holding config/firmware.toml. */
export function root(from = process.cwd()): string {
  let dir = resolve(from);
  for (;;) {
    if (existsSync(join(dir, "config", "firmware.toml"))) return dir;
    const up = dirname(dir);
    if (up === dir) throw new Error(`config/firmware.toml: not found above ${from}`);
    dir = up;
  }
}

function tomlFiles(dir: string): string[] {
  const out: string[] = [];
  const walk = (d: string) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith(".toml")) out.push(p);
    }
  };
  walk(dir);
  return out.sort();
}

const str = (v: unknown, d = ""): string => (typeof v === "string" ? v : d);
const num = (v: unknown, d = 0): number => (typeof v === "number" ? v : d);
const strs = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
const table = (v: unknown): Table => (v && typeof v === "object" && !Array.isArray(v) ? (v as Table) : {});

/** `panelspec::Meta` with its defaults: a spec without `[meta]` counts as mined from nothing. */
function meta(t: Table): Meta {
  const m: Meta = {
    status: str(t.status) === "tested" ? "tested" : "generates",
    origin: str(t.origin) === "bench" ? "bench" : "mined",
    sources: num(t.sources),
    examples: strs(t.examples),
    vendors: strs(t.vendors),
  };
  if (typeof t.pitch_mm === "number") m.pitch_mm = t.pitch_mm;
  if (typeof t.agreement === "number") m.agreement = t.agreement;
  if (typeof t.notes === "string") m.notes = t.notes;
  return m;
}

/**
 * The codec registry as `rxp config formats` prints it (`rcvbp::codecs()`).
 * There is no data file for it; this list is the same table by hand, and
 * `pnpm test` pins it against the crate's format list.
 */
export const FORMATS: Format[] = [{ name: "rcvbp", vendor: "Colorlight", extension: "rcvbp", generate: true, import: true }];

export type Panel = Entry & { toml: string; mined: boolean };

/** Every panel spec under config/panels, non-mined first, then mined, each by path. */
export function panels(repo = root()): Panel[] {
  const dir = join(repo, "config", "panels");
  const list = tomlFiles(dir).map((file): Panel => {
    const toml = readFileSync(file, "utf8");
    const t = parse(toml) as Table;
    const path = relative(repo, file);
    const mod = table(t.module);
    const chipPath = str(table(t.chip).library);
    const chip = chipPath ? (parse(readFileSync(join(repo, chipPath), "utf8")) as Table) : {};
    return {
      path,
      name: str(t.name, basename(file, ".toml")),
      meta: meta(table(t.meta)),
      module: { width: num(mod.width), height: num(mod.height), scan: num(mod.scan) },
      chip: { library: chipPath, name: str(chip.name, chipPath), family_id: num(chip.family_id) },
      formats: FORMATS.filter((f) => f.generate).map((f) => f.name),
      toml,
      mined: path.startsWith(join("config", "panels", "mined") + "/"),
    };
  });
  const names = new Set<string>();
  for (const p of list) {
    if (names.has(p.name)) throw new Error(`${p.path}: name ${p.name} is used twice`);
    names.add(p.name);
  }
  return list.sort((a, b) => Number(a.mined) - Number(b.mined) || a.path.localeCompare(b.path));
}

export type Tested = { panel: string; firmware: string };
export type Guard = { from: string; to?: string; blocks: number[] };
export type Card = {
  path: string;
  name: string;
  vendor: string;
  family: string;
  id: number;
  status: string;
  notes?: string;
  tested: Tested[];
  limits: { max_width: number; max_height: number; hub_ports: number; chain?: number };
  memory: {
    block_bytes: number;
    primary_bank: number;
    bank_bytes: number;
    golden_bank: number;
    parameter_block: number;
    eeprom_mirror: number;
    guarded: Guard[];
    boot_image: [string, number][];
  };
  firmware: { image_pattern: string; sdram_staging: boolean };
};

/** The card models in config/cards, tested first, then by name (`receivers::models()`). */
export function cards(repo = root()): Card[] {
  const list = tomlFiles(join(repo, "config", "cards")).map((file): Card => {
    const t = parse(readFileSync(file, "utf8")) as Table;
    const lim = table(t.limits);
    const mem = table(t.memory);
    const fw = table(t.firmware);
    const card: Card = {
      path: relative(repo, file),
      name: str(t.name),
      vendor: str(t.vendor),
      family: str(t.family),
      id: num(t.id),
      status: str(t.status),
      tested: (Array.isArray(t.tested) ? t.tested : []).map((x) => ({ panel: str(table(x).panel), firmware: str(table(x).firmware) })),
      limits: { max_width: num(lim.max_width), max_height: num(lim.max_height), hub_ports: num(lim.hub_ports) },
      memory: {
        block_bytes: num(mem.block_bytes),
        primary_bank: num(mem.primary_bank),
        bank_bytes: num(mem.bank_bytes),
        golden_bank: num(mem.golden_bank),
        parameter_block: num(mem.parameter_block),
        eeprom_mirror: num(mem.eeprom_mirror),
        guarded: (Array.isArray(mem.guarded) ? mem.guarded : []).map((g) => {
          const gt = table(g);
          const guard: Guard = { from: str(gt.from), blocks: Array.isArray(gt.blocks) ? gt.blocks.map((b) => num(b)) : [] };
          if (typeof gt.to === "string") guard.to = gt.to;
          return guard;
        }),
        boot_image: Object.entries(table(mem.boot_image)).map(([k, v]) => [k, num(v)]),
      },
      firmware: { image_pattern: str(fw.image_pattern), sdram_staging: fw.sdram_staging === true },
    };
    if (typeof t.notes === "string") card.notes = t.notes;
    if (typeof lim.chain === "number") card.limits.chain = lim.chain;
    return card;
  });
  const rank = (c: Card) => (c.status === "tested" ? 0 : 1);
  return list.sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name));
}

export type Image = { name: string; version: string; kind: string; pcb?: string; chips: string[]; size: number; sha256: string };
export type Firmware = { base_url: string; images: Image[] };

/** config/firmware.toml: the manifest `rxp firmware list` prints. */
export function firmware(repo = root()): Firmware {
  const t = parse(readFileSync(join(repo, "config", "firmware.toml"), "utf8")) as Table;
  return {
    base_url: str(t.base_url),
    images: (Array.isArray(t.image) ? t.image : []).map((x) => {
      const i = table(x);
      const img: Image = { name: str(i.name), version: str(i.version), kind: str(i.kind), chips: strs(i.chips), size: num(i.size), sha256: str(i.sha256) };
      if (typeof i.pcb === "string") img.pcb = i.pcb;
      return img;
    }),
  };
}

/** Where a manifest image is: `base_url/NAME` when the manifest names a base, else its path in the repository. */
export function imageLocation(fw: Firmware, img: Image): { href: string; remote: boolean } {
  return fw.base_url ? { href: `${fw.base_url.replace(/\/$/, "")}/${img.name}`, remote: true } : { href: `third-party/firmware/${img.name}`, remote: false };
}
