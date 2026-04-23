import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

const port = Number(process.env.PORT ?? "5175");
const basePath = process.env.BASE_PATH ?? "/";

export default defineConfig({
  base: basePath,
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
