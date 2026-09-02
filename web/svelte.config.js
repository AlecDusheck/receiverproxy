// Two builds from one app. `pnpm build` (adapter-cloudflare) is the site
// wrangler deploys to receiverproxy.com; `pnpm build:embed` sets
// ADAPTER=static and writes build-static/, the copy crates/daemon embeds and
// serves at http://127.0.0.1:7120. The routes that need WASM or the daemon
// (/builder, /wall, /control) are not prerendered; the static build serves
// them from fallback.html; index.html stays the prerendered home page.
import cloudflare from "@sveltejs/adapter-cloudflare";
import stat from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

const adapter = process.env.ADAPTER === "static" ? stat({ pages: "build-static", assets: "build-static", fallback: "fallback.html" }) : cloudflare();

export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter,
    alias: { $api: "src/api", $parts: "src/parts" },
    prerender: { origin: "https://receiverproxy.com" },
  },
};
