import { defineConfig } from "vite";

export default defineConfig({
  root: "ui",
  publicDir: false,
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
    reportCompressedSize: false,
    rollupOptions: {
      input: {
        index: "index.html",
        hud: "hud.html",
      },
    },
  },
});
