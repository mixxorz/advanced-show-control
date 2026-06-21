import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const debugEntry = process.env.VITE_DEBUG_ENTRY === "1";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: debugEntry ? 1421 : 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: debugEntry
    ? {
        rollupOptions: {
          input: resolve(__dirname, "debug.html"),
        },
      }
    : undefined,
});
