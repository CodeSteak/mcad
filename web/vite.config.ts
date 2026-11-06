import { lezer } from "@lezer/generator/rollup";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vitest/config";

// `vite build --mode tauri` (pnpm build:tauri): the UI of the desktop app, embedded by
// crates/cad-app (web/dist-tauri). The engine runs natively there: the worker factory is
// replaced by a stub, so neither the worker nor the Wasm modules end up in the bundle.
function desktop(mode: string): Plugin[] {
  if (mode !== "tauri") return [];
  const stub = fileURLToPath(new URL("./src/engine/workerFactory.stub.ts", import.meta.url));
  return [
    {
      name: "cad-desktop-no-worker",
      enforce: "pre",
      resolveId(id) {
        return /(^|\/)workerFactory(\.ts)?$/.test(id) ? stub : null;
      },
    },
  ];
}

export default defineConfig(({ mode }) => ({
  plugins: [react(), lezer(), ...desktop(mode)],
  // cad_wasm_bg.wasm comes only from src/wasm-pkg (imported with `?url` in the worker, hashed
  // asset); nothing is copied from public/.
  publicDir: false,
  base: "./",
  worker: { format: "es" },
  server: { fs: { allow: [".."] } },
  build: { target: "es2022", chunkSizeWarningLimit: 2000, outDir: mode === "tauri" ? "dist-tauri" : "dist", emptyOutDir: true },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    setupFiles: ["src/i18n/test-setup.ts"],
  },
}));
