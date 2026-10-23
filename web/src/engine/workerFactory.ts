// Creates the engine worker (browser). Kept apart from transport.ts so the desktop build
// (`vite build --mode tauri`) can replace it with workerFactory.stub.ts: the desktop app runs
// the engine natively and needs neither the worker nor the Wasm modules.

export function createEngineWorker(): Worker {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
}
