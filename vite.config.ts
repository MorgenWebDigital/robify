import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// tauri expects a fixed port and must not fall back to another one on error
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // the tests run in a rebuilt document.
  //
  // the calculating parts under `lib/` would get by without one, the
  // components would not: the last bugs lay exactly there, and all three could
  // only have shown in a document. the focus jumping out of the field after
  // every keystroke, the tile drawn as `inline`, the menu extending the
  // scrolling area.
  //
  // `globals` saves importing `describe` and `expect` in every file, and
  // `setup` brings the extra matchers and cleans up between the tests
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/aufbau.ts"],
  },

  // tauri uses chromium on windows and android, webkit on macos and linux
  build: {
    target:
      process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari15",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
