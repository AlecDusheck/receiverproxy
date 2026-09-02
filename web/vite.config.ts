import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  base: "./",
  server: {
    proxy: { "/api": "http://127.0.0.1:7120" },
  },
  build: { target: "es2022" },
});
