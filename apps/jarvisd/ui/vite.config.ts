import { defineConfig } from "vite";

export default defineConfig({
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  build: {
    outDir: "../src/hud/generated", emptyOutDir: true, minify: true,
    lib: { entry: "src/catalog.tsx", name: "JarvisControls", formats: ["iife"], fileName: () => "controls.js" }
  }
});
