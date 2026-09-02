// The WASM module, loaded lazily. `web/scripts/build-wasm.sh` writes
// src/wasm/rcvbp_wasm.js; when it is absent the stub below stands in and the
// functions that need rcvbp throw. Only api/ops.ts calls this.
import { app } from "./state.svelte";
import type { Diff, Generated, Inspection, Libraries } from "../api/types";
import { validateJs, example } from "./layout";

export const NOT_BUILT = "wasm module not built: run web/scripts/build-wasm.sh";

/** The generated glue's exports, with the shapes docs/ui.md section 3 gives them. */
export type WasmModule = {
  generate(spec_toml: string): Generated;
  inspect(rcvbp: Uint8Array): Inspection;
  diff(a: Uint8Array, b: Uint8Array): Diff;
  libraries(): Libraries;
  validate_layout(json: string): string;
  layout_example(cols: number, rows: number, w: number, h: number): string;
};

const stub: WasmModule = {
  generate: () => {
    throw new Error(NOT_BUILT);
  },
  inspect: () => {
    throw new Error(NOT_BUILT);
  },
  diff: () => {
    throw new Error(NOT_BUILT);
  },
  libraries: () => ({ chips: [], panels: [] }),
  validate_layout: (json) => validateJs(JSON.parse(json)),
  layout_example: (cols, rows, w, h) => JSON.stringify(example(cols, rows, w, h), null, 2),
};

type Glue = WasmModule & { default: () => Promise<unknown> };
const found = import.meta.glob("../wasm/rcvbp_wasm.js") as Record<string, () => Promise<Glue>>;

let loaded: WasmModule | null = null;

export const ready: Promise<WasmModule> = (async () => {
  const load = found["../wasm/rcvbp_wasm.js"];
  if (!load) {
    app.wasm = "failed";
    app.wasmError = NOT_BUILT;
    loaded = stub;
    return stub;
  }
  try {
    const mod = await load();
    await mod.default();
    app.wasm = "ready";
    loaded = mod;
    return mod;
  } catch (e) {
    app.wasm = "failed";
    app.wasmError = e instanceof Error ? e.message : String(e);
    loaded = stub;
    return stub;
  }
})();

// The module once loaded, for synchronous callers; null before that.
export const current = (): WasmModule | null => loaded;
