import type { Canvas, Health, Job, Settings, State } from "../api/types";

export const single = (w: number, h: number): Canvas => ({
  width: w,
  height: h,
  receivers: [{ index: 0, x: 0, y: 0, width: w, height: h }],
  panels: [{ receiver: 0, receiver_x: 0, receiver_y: 0, x: 0, y: 0, width: w, height: h, rotation: "none", flip_x: false, flip_y: false, max_brightness: 255 }],
  max_brightness: 255,
});

function storedWall(): Canvas {
  try {
    const s = localStorage.getItem("rxp.wall");
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
  // The job last started from a screen; its lines arrive over SSE and show where the action was.
  job: null as Job | null,
  // What the daemon says is on the panel: one subscription in +layout
  // (`GET /state/events`), null while no daemon answers.
  live: null as State | null,
  // The receiver index the Control pages act on.
  card: 0,
  // Loaded on first use (lib/wasm.ts); "unloaded" until a route asks for it.
  wasm: "unloaded" as "unloaded" | "loading" | "ready" | "failed",
  wasmError: "",
  // The install banner under the top bar; dismissed for the session (sessionStorage).
  install: false,
});

// Hand a spec to the Builder or the Control provision form: both read this key at load.
export function handSpec(toml: string) {
  try {
    localStorage.setItem("rxp.builder.toml", toml);
  } catch {
    /* no storage */
  }
}
