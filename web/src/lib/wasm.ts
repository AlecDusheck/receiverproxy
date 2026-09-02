// The WASM module, loaded lazily. `web/scripts/build-wasm.sh` writes
// src/wasm/rcvbp_wasm.js; when it is absent the stub below stands in and the
// functions that need rcvbp throw. Only api/ops.ts calls this.
import { app } from "./state.svelte";
import type { Diff, Entry, Format, Generated, Imported, Inspection, Libraries } from "../api/types";
import { validateJs, example } from "./layout";

export const NOT_BUILT = "wasm module not built: run web/scripts/build-wasm.sh";

/** The generated glue's exports, with the shapes docs/ui.md section 3 gives them. */
export type WasmModule = {
  generate(spec_toml: string, format: string): Generated;
  import(bytes: Uint8Array, format?: string): Imported;
  gallery(): Entry[];
  formats(): Format[];
  inspect(rcvbp: Uint8Array): Inspection;
  diff(a: Uint8Array, b: Uint8Array): Diff;
  libraries(): Libraries;
  validate_layout(json: string): string;
  layout_example(cols: number, rows: number, w: number, h: number): string;
};

const missing = (name: string) => () => {
  throw new Error(`wasm module has no ${name}(): rebuild with web/scripts/build-wasm.sh`);
};

const stub: WasmModule = {
  generate: () => {
    throw new Error(NOT_BUILT);
  },
  import: () => {
    throw new Error(NOT_BUILT);
  },
  gallery: () => [],
  formats: () => [],
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

// wasm-bindgen exports `import` as `_import`: a JavaScript function cannot be named `import`.
type Glue = Partial<Omit<WasmModule, "import">> & { _import?: WasmModule["import"]; default: () => Promise<unknown> };
const found = import.meta.glob("../wasm/rcvbp_wasm.js") as Record<string, () => Promise<Glue>>;

// A build older than the surface above throws a rebuild message for what it lacks.
function complete(mod: Glue): WasmModule {
  const out = {} as Record<keyof WasmModule, unknown>;
  for (const k of Object.keys(stub) as (keyof WasmModule)[]) out[k] = (k === "import" ? mod._import : mod[k]) ?? missing(k);
  return out as WasmModule;
}

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
    loaded = complete(mod);
    return loaded;
  } catch (e) {
    app.wasm = "failed";
    app.wasmError = e instanceof Error ? e.message : String(e);
    loaded = stub;
    return stub;
  }
})();

// The module once loaded, for synchronous callers; null before that.
export const current = (): WasmModule | null => loaded;
