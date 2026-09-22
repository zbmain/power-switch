import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

/** Share the desktop development origin and browser-test configuration. */
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Rust compilation emits HTML assets; these must not reload the development UI.
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_"],
  test: {
    // Release scripts use node:test and run separately through test:release.
    include: ["src/**/*.test.{ts,tsx}"],
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    restoreMocks: true,
  },
});
