import { defineConfig } from "vite";

export default defineConfig({
  root: "web/app",
  base: "./",
  build: {
    outDir: "../../dist-web",
    emptyOutDir: true,
    target: "es2022",
  },
  server: {
    port: 5183,
    strictPort: true,
  },
  worker: {
    format: "es",
  },
  optimizeDeps: {
    exclude: ["@huggingface/transformers"],
  },
});
