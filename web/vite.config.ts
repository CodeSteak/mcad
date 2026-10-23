import { lezer } from "@lezer/generator/rollup";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react(), lezer()],
  // smlib.wasm and cad_wasm_bg.wasm come only from src/wasm-pkg (imported with `?url` in the
  // worker, hashed assets). public/smlib is the input of scripts/build-wasm.sh and must not be
  // copied into the build a second time.
  publicDir: false,
  base: "./",
  worker: { format: "es" },
  server: { fs: { allow: [".."] } },
  build: { target: "es2022", chunkSizeWarningLimit: 2000 },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
