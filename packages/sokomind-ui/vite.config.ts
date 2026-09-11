import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  base: "/SokoSolver/",
  build: {
    outDir: "dist",
    sourcemap: true,
  },
  worker: {
    format: "es",
  },
});
