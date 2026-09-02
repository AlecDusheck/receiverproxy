// The WASM module, loaded lazily. `web/scripts/build-wasm.sh` writes
// src/wasm/e120_wasm.js; when it is absent the stub below stands in and the
// functions that need e120-rcvbp throw.
import { app } from "./state.svelte";
import type { WasmModule } from "./types";
import { validateJs, example } from "./layout";

export const NOT_BUILT = "wasm module not built: run web/scripts/build-wasm.sh";

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
const found = import.meta.glob("../wasm/e120_wasm.js") as Record<string, () => Promise<Glue>>;

let loaded: WasmModule | null = null;

export const ready: Promise<WasmModule> = (async () => {
  const load = found["../wasm/e120_wasm.js"];
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

export const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));
