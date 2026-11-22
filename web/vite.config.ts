import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [svelte()],
  server: {
    host: true,
    port: 5173,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8789",
        changeOrigin: true,
      },
      "/ws": {
        target: "ws://127.0.0.1:8789",
        ws: true,
      },
    },
  },
});
