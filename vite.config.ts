import { defineConfig } from "vite";

// Tauri expects a fixed port and no screen clearing, so its own logs stay visible.
export default defineConfig({
  clearScreen: false,
  server: { port: 1430, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "es2021", minify: "esbuild", sourcemap: false },
});
