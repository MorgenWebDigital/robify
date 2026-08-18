import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri erwartet einen festen Port und darf bei Fehlern nicht ausweichen.
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
  /*
   * Tests laufen in einem nachgebauten Dokument.
   *
   * Die Rechenteile unter `lib/` kämen ohne aus, die Bauteile nicht: Genau
   * dort lagen die letzten Fehler, und alle drei hätten sich nur im Dokument
   * zeigen können. Der Fokus, der nach jedem Tastendruck aus dem Feld sprang;
   * die Kachel, die als `inline` gezeichnet wurde; das Menü, das die
   * Bildlauffläche verlängerte.
   *
   * `globals` spart das Einbinden von `describe` und `expect` in jeder Datei,
   * `setup` bringt die zusätzlichen Vergleiche und räumt zwischen den Tests
   * auf.
   */
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/aufbau.ts"],
  },

  // Tauri nutzt Chromium (Windows/Android) bzw. WebKit (macOS/Linux).
  build: {
    target:
      process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari15",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
