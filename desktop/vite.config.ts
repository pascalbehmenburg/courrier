import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  // For Tauri dev: fixed port + strict.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      // In dev, the Vite server proxies /api to the Rust backend so the
      // browser sees a single origin and CORS doesn't get in the way.
      "/api": {
        target: "http://127.0.0.1:3000",
        changeOrigin: true,
      },
    },
  },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
  },
});
