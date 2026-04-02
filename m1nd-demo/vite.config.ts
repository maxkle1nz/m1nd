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
    emptyOutDir: true
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
