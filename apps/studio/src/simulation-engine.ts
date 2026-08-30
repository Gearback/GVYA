import type { GvyaRuntimeInfo } from "../../../packages/runtime-sdk/dist/contracts.js";
import { compilerSourceEntries, sourceFingerprint, WasmCompilerBackend } from "./compiler-wasm.js";
import { loadBundledEngineAssets, STUDIO_ENGINE_VERSION } from "./engine-assets.js";
import { StudioRuntimeSession } from "./runtime-simulator.js";
import type { StudioAssetFile, StudioBrainWorkspace } from "./types.js";

export interface SimulationReady {
  engine: typeof STUDIO_ENGINE_VERSION;
  sourceFingerprint: string;
  info: GvyaRuntimeInfo;
}

export class StudioSimulationEngine {
  readonly session = new StudioRuntimeSession();
  #compiler: WasmCompilerBackend | null = null;
  #engineModule: WebAssembly.Module | null = null;
  #fingerprint = "";
  #ready: SimulationReady | null = null;
  #prepareQueue: Promise<void> = Promise.resolve();

  get ready(): SimulationReady | null { return this.#ready; }

  prepare(workspace: StudioBrainWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<SimulationReady> {
    const run = this.#prepareQueue.then(async () => {
      const entries = await compilerSourceEntries(workspace, assetFiles);
      const fingerprint = await sourceFingerprint(entries);
      if (this.#ready && this.#fingerprint === fingerprint) return this.#ready;
      const assets = await loadBundledEngineAssets();
      this.#compiler ??= await WasmCompilerBackend.instantiate(assets.engineModule);
      this.#engineModule ??= assets.engineModule;
      const artifact = this.#compiler.compile(entries);
      const info = await this.session.open(this.#engineModule, artifact);
      this.#fingerprint = fingerprint;
      this.#ready = { engine: STUDIO_ENGINE_VERSION, sourceFingerprint: fingerprint, info };
      return this.#ready;
    });
    this.#prepareQueue = run.then(() => undefined, () => undefined);
    return run;
  }

  resetConversation(): void { this.session.resetState(); }

  async close(): Promise<void> {
    const run = this.#prepareQueue.then(async () => {
      await this.session.close();
      this.#ready = null;
      this.#fingerprint = "";
    });
    this.#prepareQueue = run.then(() => undefined, () => undefined);
    await run;
  }
}
