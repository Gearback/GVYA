import type { RuntimeBackend } from "./backend.js";
import type {
  InvocationProposal,
  GvyaAssetInfo,
  GvyaCapabilitiesInfo,
  GvyaCapabilityInfo,
  GvyaCapabilityResultRequest,
  GvyaCapabilityResultResult,
  GvyaOpenRequest,
  GvyaRuntimeOpenOptions,
  GvyaRuntimeInfo,
  GvyaTurnRequest,
  GvyaTurnResult,
} from "./contracts.js";

export class GvyaRuntime {
  readonly #backend: RuntimeBackend;
  private constructor(backend: RuntimeBackend) { this.#backend = backend; }

  static async open(artifact: Uint8Array, backend: RuntimeBackend, options: GvyaRuntimeOpenOptions): Promise<GvyaRuntime> {
    await backend.open(artifact, options);
    return new GvyaRuntime(backend);
  }

  info(): Promise<GvyaRuntimeInfo> { return this.#backend.info(); }
  capabilities(): Promise<GvyaCapabilitiesInfo> { return this.#backend.capabilities(); }
  capabilityInfo(id: string): Promise<GvyaCapabilityInfo> { return this.#backend.capabilityInfo(id); }
  turn(request: GvyaTurnRequest): Promise<GvyaTurnResult> { return this.#backend.turn(request); }
  confirmTurn(request: GvyaTurnRequest, proposal: InvocationProposal, confirmed: boolean, confirmationId: string): Promise<GvyaTurnResult> {
    const grant = { id: confirmationId, proposal_id: proposal.id, fingerprint: proposal.fingerprint, confirmed };
    return this.#backend.turn({ ...request, confirmations: [...(request.confirmations ?? []), grant] });
  }
  openConversation(request: GvyaOpenRequest): Promise<GvyaTurnResult> { return this.#backend.openConversation(request); }
  capabilityResult(request: GvyaCapabilityResultRequest): Promise<GvyaCapabilityResultResult> { return this.#backend.capabilityResult(request); }
  assetByPath(path: string): Promise<Uint8Array> { return this.#backend.assetByPath(path); }
  assetById(id: string): Promise<Uint8Array> { return this.#backend.assetById(id); }
  assetInfoById(id: string): Promise<GvyaAssetInfo> { return this.#backend.assetInfoById(id); }
  close(): Promise<void> { return this.#backend.close(); }
}
