import { defineConfig } from "vite";

// `npm run build` bundles the catalog as one offline IIFE that the daemon embeds (no Node needed by installed users).
// `npm run dev` serves index.html: a playground that mounts the same components on the real HUD stylesheet with sample data.
export default defineConfig({
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  server: { port: 5199, strictPort: true, fs: { allow: [".."] } },
  build: {
    outDir: "../src/hud/generated", emptyOutDir: true, minify: true, target: "es2022", reportCompressedSize: false,
    lib: { entry: "src/catalog.tsx", name: "JarvisControls", formats: ["iife"], fileName: () => "controls.js" }
  }
});
