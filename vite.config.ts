import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed port and doesn't want the screen cleared.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/target/**"] } },
  build: { target: "es2022", chunkSizeWarningLimit: 2500 },
});
