import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";
import { readFileSync } from "fs";

const port = Number(process.env.PORT ?? "5175");
const basePath = process.env.BASE_PATH ?? "/";

// Single source of truth for the product version shown on the site:
// the repo root package.json (kept in lockstep with the m1nd-mcp crate).
const rootPkg = JSON.parse(
  readFileSync(path.resolve(import.meta.dirname, "../package.json"), "utf-8")
);

export default defineConfig({
  base: basePath,
  define: {
    __M1ND_VERSION__: JSON.stringify(rootPkg.version as string)
  },
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "src")
    },
    dedupe: ["react", "react-dom", "three", "@react-three/fiber", "@react-three/drei"]
  },
  build: {
    outDir: path.resolve(import.meta.dirname, "dist"),
    emptyOutDir: true,
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) {
            return;
          }
          if (id.includes("three") || id.includes("@react-three")) {
            return "three-vendor";
          }
          if (id.includes("@radix-ui")) {
            return "radix-vendor";
          }
          if (id.includes("framer-motion")) {
            return "motion-vendor";
          }
          return "vendor";
        }
      }
    }
  },
  server: {
    port,
    host: "0.0.0.0",
    allowedHosts: true
  },
  preview: {
    port,
    host: "0.0.0.0",
    allowedHosts: true
  }
});
