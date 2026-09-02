// Shapes from docs/ui.md sections 2 to 4.

export type Card = {
  controller: number;
  card_id: number;
  ver_major: number;
  ver_minor: number;
  cols: number;
  rows: number;
};

export type Line = { stream: "out" | "err"; text: string };
export type Outcome = { lines: Line[]; files: string[] };
export type GatedOutcome = Outcome & { committed: boolean };

export type JobKind =
  | "provision"
  | "firmware/install"
  | "flash/snapshot"
  | "flash/restore"
  | "show/video"
  | "show/hold";

export type Job = {
  id: string;
  kind: JobKind;
  state: "running" | "done" | "failed" | "cancelled";
  started: string;
  finished: string | null;
  lines: Line[];
  error: string | null;
  result: GatedOutcome | Outcome | null;
};

export type Health = { version: string; iface: string; cards: Card[] };
export type Settings = { iface: string; brightness: number };
export type Fit = "stretch" | "contain" | "cover";
export type PatternName = "rgb" | "border" | "rows" | "gradient" | "white";

export type GenFiles = {
  name: string;
  files: { rcvbp: string; basic_pack: string; block7: string | null; sources_txt: string };
  sources: string[];
  notes: string[];
};

// Wall layout, the structure e120_canvas reads.
export type Rotation = "none" | "cw90" | "ccw90" | "rot180";
export type Receiver = { index: number; x?: number; y?: number; width: number; height: number };
export type Panel = {
  receiver: number;
  receiver_x?: number;
  receiver_y?: number;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation?: Rotation;
  flip_x?: boolean;
  flip_y?: boolean;
};
export type Canvas = { width: number; height: number; receivers: Receiver[]; panels: Panel[] };

// WASM surface.
export type Generated = {
  name: string;
  rcvbp: Uint8Array;
  basic_pack: Uint8Array;
  block7: Uint8Array | null;
  sources: string[];
  notes: string[];
};

export type Record01 = {
  module_width: number;
  module_height_stored: number;
  scan: number;
  serial_clock: number;
  gray: number;
  luminance_level: number;
  max_width: number;
  max_height: number;
  grid: [number, number];
  line_dir: number;
  split_segment: number;
  segments: number;
  min_oe: number;
  hr_style: number;
  hr_scan_style: number;
  chip_id: number;
  swap_ramp: Uint8Array;
  chip_custom: Uint8Array;
};

export type RecordInfo = {
  offset: number;
  type: string;
  id: number;
  length: number;
  nonzero: number;
  empty: boolean;
  description: string;
  fields: Record01 | null;
};

export type Inspection = { version: number; cabinet: [number, number] | null; records: RecordInfo[] };

export type Diff = {
  a_records: number;
  b_records: number;
  only_a: string[];
  only_b: string[];
  records: { type: string; len_a: number; len_b: number; offsets: number[] }[];
};

export type Libraries = {
  chips: { path: string; name: string; toml: string }[];
  panels: { path: string; name: string; toml: string; mined: boolean }[];
};

export type WasmModule = {
  generate(spec_toml: string): Generated;
  inspect(rcvbp: Uint8Array): Inspection;
  diff(a: Uint8Array, b: Uint8Array): Diff;
  libraries(): Libraries;
  validate_layout(json: string): string;
  layout_example(cols: number, rows: number, w: number, h: number): string;
};
