# e120 web app

Dev: `pnpm install`, `scripts/build-wasm.sh` (optional; without it Builder and Library report the module is not built), then `pnpm dev` (API proxied to a daemon on 127.0.0.1:7120; `VITE_E120_MOCK=1 pnpm dev` uses canned cards and a simulated job).
Build: `pnpm build` runs `svelte-check` and writes `dist/`, which `e120-server` embeds on its next `cargo build`.
Contract: `../docs/ui.md`.
