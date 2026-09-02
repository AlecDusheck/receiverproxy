import type { Canvas, Health, Job, Settings } from "../api/types";

export type Status = { kind: "idle" | "busy" | "error"; text: string };

export const single = (w: number, h: number): Canvas => ({
  width: w,
  height: h,
  receivers: [{ index: 0, x: 0, y: 0, width: w, height: h }],
  panels: [{ receiver: 0, receiver_x: 0, receiver_y: 0, x: 0, y: 0, width: w, height: h, rotation: "none", flip_x: false, flip_y: false }],
});

function storedWall(): Canvas {
  try {
    const s = localStorage.getItem("e120.wall");
    if (s) return JSON.parse(s) as Canvas;
  } catch {
    /* no storage */
  }
  return single(128, 64);
}

// One shared store; every screen imports it.
export const app = $state({
  daemon: "probing" as "probing" | "absent" | "locked" | "present",
  tokenError: "",
  // The full body: `probe` keeps only a health that carried `iface` and `cards`.
  health: null as Required<Health> | null,
  settings: null as Settings | null,
  wall: storedWall(),
  job: null as Job | null,
  status: { kind: "idle", text: "" } as Status,
  wasm: "loading" as "loading" | "ready" | "failed",
  wasmError: "",
  // The first-visit install line under the title row; dismissed once, kept in localStorage.
  install: false,
});

export function setStatus(kind: Status["kind"], text = "") {
  app.status = { kind, text };
}

// Hand a spec to the Builder or the Cards provision form: both read this key at load.
export function handSpec(toml: string) {
  try {
    localStorage.setItem("e120.builder.toml", toml);
  } catch {
    /* no storage */
  }
}
