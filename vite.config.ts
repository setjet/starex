import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Tauri serves the UI from a fixed port in development and from `dist/` in release builds.
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2022",
    minify: "esbuild",
    sourcemap: false,
    cssMinify: true,
    reportCompressedSize: false,
  },
});
