// Desktop build: there is no engine worker (TauriTransport talks to the native engine).

export function createEngineWorker(): Worker {
  throw new Error("Desktop-Build ohne Engine-Worker");
}
