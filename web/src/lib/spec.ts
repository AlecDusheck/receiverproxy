// PanelSpec <-> TOML. Parses the flat `[table]` form config/panels/*.toml use
// (scalars, arrays of numbers, quoted keys) and emits it in the same order.

export type PanelSpec = {
  name: string;
  module: { width: number; height: number; scan: number; serial_clock: number | null; gray_bits: number | null; line_dir: number; data_groups: number };
  screen: { width: number; height: number };
  chip: { library: string };
  color: { swap: number; source: [number, number, number] };
  current: { gains: [number, number, number, number]; percent: [number, number, number] };
  timing: { gamma: number; refresh_hz: number; gclock: number; min_oe: number; luminance_level: number; oe_8ns: boolean };
  mapping: { reversed_groups: boolean; reversed_lines: boolean; block: number | null; gate_phantom_positions: boolean };
  boot: { arm_at_boot: boolean };
  overrides: { offset: string; value: number }[];
  /** Tables the form does not edit ([meta] and any other), kept as parsed and re-emitted. */
  extra: Record<string, Record<string, Value>>;
};

export const defaultSpec = (): PanelSpec => ({
  name: "panel",
  module: { width: 128, height: 64, scan: 16, serial_clock: null, gray_bits: null, line_dir: 0, data_groups: 1 },
  screen: { width: 128, height: 64 },
  chip: { library: "" },
  color: { swap: 3, source: [2, 1, 0] },
  current: { gains: [43, 43, 43, 43], percent: [0.1, 0.1, 0.1] },
  timing: { gamma: 2.8, refresh_hz: 60, gclock: 0x14, min_oe: 0.0001, luminance_level: 188, oe_8ns: true },
  mapping: { reversed_groups: true, reversed_lines: false, block: null, gate_phantom_positions: true },
  boot: { arm_at_boot: false },
  overrides: [],
  extra: {},
});

export type Value = number | boolean | string | Value[];
export type Tables = Record<string, Record<string, Value>>;
const KNOWN = new Set(["", "module", "screen", "chip", "color", "current", "timing", "mapping", "boot", "record01_overrides"]);

function scalar(s: string): Value {
  s = s.trim();
  if (s === "true") return true;
  if (s === "false") return false;
  if (/^"([^"]*)"$/.test(s) || /^'([^']*)'$/.test(s)) return s.slice(1, -1);
  if (s.startsWith("[")) {
    if (!s.endsWith("]")) throw new Error(`unterminated array: ${s}`);
    const inner = s.slice(1, -1).trim();
    return inner ? inner.split(",").map((x) => x.trim()).filter(Boolean).map(scalar) : [];
  }
  if (/^0x[0-9a-f_]+$/i.test(s)) return parseInt(s.replace(/_/g, ""), 16);
  if (/^[+-]?(\d[\d_]*)(\.\d+)?([eE][+-]?\d+)?$/.test(s)) return Number(s.replace(/_/g, ""));
  throw new Error(`cannot parse value: ${s}`);
}

// Leading comment block of a TOML file, `#` stripped.
export function header(toml: string): string {
  const out: string[] = [];
  for (const line of toml.split("\n")) {
    if (line.startsWith("#")) out.push(line.replace(/^#\s?/, ""));
    else if (line.trim() === "" && out.length === 0) continue;
    else break;
  }
  return out.join("\n").trim();
}

export function parseToml(toml: string): Tables {
  const t: Tables = { "": {} };
  let cur = t[""]!;
  const lines = toml.split("\n");
  for (let i = 0; i < lines.length; i++) {
    let line = lines[i]!.trim();
    if (!line || line.startsWith("#")) continue;
    const th = /^\[([^\]]+)\]$/.exec(line);
    if (th) {
      cur = t[th[1]!.trim()] ??= {};
      continue;
    }
    // Multi-line arrays: join until the bracket closes.
    while (line.includes("[") && !line.includes("]") && i + 1 < lines.length) line += " " + lines[++i]!.trim();
    const kv = /^("([^"]+)"|[A-Za-z0-9_.-]+)\s*=\s*(.*)$/.exec(line);
    if (!kv) throw new Error(`line ${i + 1}: expected key = value`);
    const key = kv[2] ?? kv[1]!;
    let v = kv[3]!;
    if (!v.trim().startsWith('"')) v = v.replace(/\s#.*$/, "");
    cur[key] = scalar(v);
  }
  return t;
}

const num = (v: Value | undefined, d: number): number => (typeof v === "number" ? v : d);
const opt = (v: Value | undefined): number | null => (typeof v === "number" ? v : null);
const bool = (v: Value | undefined, d: boolean): boolean => (typeof v === "boolean" ? v : d);
const nums = <N extends number>(v: Value | undefined, d: number[]): number[] =>
  Array.isArray(v) && v.length === d.length && v.every((x) => typeof x === "number") ? (v as number[]) : d;

export function fromToml(toml: string): PanelSpec {
  const t = parseToml(toml);
  const d = defaultSpec();
  const g = (k: string) => t[k] ?? {};
  const mod = g("module"), scr = g("screen"), chip = g("chip"), col = g("color"), cur = g("current"), tim = g("timing"), map = g("mapping"), boot = g("boot");
  const root = t[""]!;
  return {
    name: typeof root.name === "string" ? root.name : d.name,
    module: {
      width: num(mod.width, d.module.width),
      height: num(mod.height, d.module.height),
      scan: num(mod.scan, d.module.scan),
      serial_clock: opt(mod.serial_clock),
      gray_bits: opt(mod.gray_bits),
      line_dir: num(mod.line_dir, 0),
      data_groups: num(mod.data_groups, 1),
    },
    screen: { width: num(scr.width, d.screen.width), height: num(scr.height, d.screen.height) },
    chip: { library: typeof chip.library === "string" ? chip.library : "" },
    color: { swap: num(col.swap, 3), source: nums(col.source, [2, 1, 0]) as [number, number, number] },
    current: {
      gains: nums(cur.gains, [43, 43, 43, 43]) as PanelSpec["current"]["gains"],
      percent: nums(cur.percent, [0.1, 0.1, 0.1]) as PanelSpec["current"]["percent"],
    },
    timing: {
      gamma: num(tim.gamma, 2.8),
      refresh_hz: num(tim.refresh_hz, 60),
      gclock: num(tim.gclock, 0x14),
      min_oe: num(tim.min_oe, 0.0001),
      luminance_level: num(tim.luminance_level, 188),
      oe_8ns: bool(tim.oe_8ns, true),
    },
    mapping: {
      reversed_groups: bool(map.reversed_groups, true),
      reversed_lines: bool(map.reversed_lines, false),
      block: opt(map.block),
      gate_phantom_positions: bool(map.gate_phantom_positions, true),
    },
    boot: { arm_at_boot: bool(boot.arm_at_boot, false) },
    overrides: Object.entries(g("record01_overrides")).map(([offset, value]) => ({ offset, value: typeof value === "number" ? value : 0 })),
    extra: Object.fromEntries(Object.entries(t).filter(([k, v]) => !KNOWN.has(k) && Object.keys(v).length)),
  };
}

const hex2 = (n: number) => "0x" + (n & 0xff).toString(16).padStart(2, "0");
const f = (n: number) => (Number.isInteger(n) ? `${n}.0` : String(n));
const emit = (v: Value): string => (Array.isArray(v) ? `[${v.map(emit).join(", ")}]` : typeof v === "string" ? JSON.stringify(v) : String(v));

export function toToml(s: PanelSpec): string {
  const o: string[] = [`name = ${JSON.stringify(s.name)}`, "", "[module]"];
  if (s.module.gray_bits !== null) o.push(`gray_bits = ${s.module.gray_bits}`);
  o.push(`width = ${s.module.width}`, `height = ${s.module.height}`, `scan = ${s.module.scan}`, `line_dir = ${s.module.line_dir}`, `data_groups = ${s.module.data_groups}`);
  if (s.module.serial_clock !== null) o.push(`serial_clock = ${s.module.serial_clock}`);
  o.push("", "[screen]", `width = ${s.screen.width}`, `height = ${s.screen.height}`);
  o.push("", "[chip]", `library = ${JSON.stringify(s.chip.library)}`);
  o.push("", "[color]", `swap = ${s.color.swap}`, `source = [${s.color.source.join(", ")}]`);
  o.push("", "[current]", `gains = [${s.current.gains.join(", ")}]`, `percent = [${s.current.percent.join(", ")}]`);
  o.push(
    "",
    "[timing]",
    `gamma = ${f(s.timing.gamma)}`,
    `refresh_hz = ${f(s.timing.refresh_hz)}`,
    `gclock = ${hex2(s.timing.gclock)}`,
    `min_oe = ${s.timing.min_oe}`,
    `luminance_level = ${s.timing.luminance_level}`,
    `oe_8ns = ${s.timing.oe_8ns}`,
  );
  o.push("", "[mapping]", `reversed_groups = ${s.mapping.reversed_groups}`, `reversed_lines = ${s.mapping.reversed_lines}`);
  if (s.mapping.block !== null) o.push(`block = ${s.mapping.block}`);
  if (!s.mapping.gate_phantom_positions) o.push("gate_phantom_positions = false");
  o.push("", "[boot]", `arm_at_boot = ${s.boot.arm_at_boot}`);
  if (s.overrides.length) {
    o.push("", "[record01_overrides]");
    for (const ov of s.overrides) o.push(`"${ov.offset}" = ${hex2(ov.value)}`);
  }
  for (const [name, table] of Object.entries(s.extra)) {
    o.push("", `[${name}]`);
    for (const [k, v] of Object.entries(table)) o.push(`${/^[A-Za-z0-9_-]+$/.test(k) ? k : JSON.stringify(k)} = ${emit(v)}`);
  }
  return o.join("\n") + "\n";
}
