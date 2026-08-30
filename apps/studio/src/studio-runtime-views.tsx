import { useEffect, useRef, useState } from "react";
import { compilerSourceEntries, WasmCompilerBackend } from "./compiler-wasm.js";
import { loadBundledEngineAssets, STUDIO_ENGINE_VERSION } from "./engine-assets.js";
import { parseBoundedSimulatorJson } from "./runtime-simulator.js";
import { RUNTIME_EXPORTERS, type StudioRuntimeExporter } from "./runtime-exporters.js";
import { StudioSimulationEngine, type SimulationReady } from "./simulation-engine.js";
import { Field, Metric, ProgressiveSection } from "./studio-ui.js";
import type { GvyaCapabilityResultResult, GvyaTurnResult, InvocationProposal } from "../../../packages/runtime-sdk/dist/contracts.js";
import type { JsonValue, StudioAssetFile, StudioBrainWorkspace } from "./types.js";
import { humanize } from "./workspace.js";

type Mutate = (mutator: (draft: StudioBrainWorkspace) => void) => boolean;

export function SimulateView(props: { workspace: StudioBrainWorkspace; assetFiles: readonly StudioAssetFile[]; initialInput: string; target: "bot" | "package" }) {
  const engineRef = useRef(new StudioSimulationEngine());
  const packagePreview = props.target === "package";
  const [ready, setReady] = useState<SimulationReady | null>(null);
  const [input, setInput] = useState(props.initialInput.trim());
  const [messages, setMessages] = useState<Array<{ role: "user" | "bot"; text: string }>>([]);
  const [contextText, setContextText] = useState("{}");
  const [capabilitiesText, setCapabilitiesText] = useState("[]");
  const [referencesText, setReferencesText] = useState("[]");
  const [result, setResult] = useState<GvyaTurnResult | null>(null);
  const [lastProposal, setLastProposal] = useState<InvocationProposal | null>(null);
  const [confirmationProposal, setConfirmationProposal] = useState<InvocationProposal | null>(null);
  const [capabilityResult, setCapabilityResult] = useState<GvyaCapabilityResultResult | null>(null);
  const [capabilitySucceeded, setCapabilitySucceeded] = useState(true);
  const [capabilityOutputText, setCapabilityOutputText] = useState("{}");
  const [capabilityErrorCode, setCapabilityErrorCode] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const prepareSerial = useRef(0);
  useEffect(() => { if (props.initialInput.trim()) setInput(props.initialInput); }, [props.initialInput]);
  useEffect(() => {
    const serial = ++prepareSerial.current;
    setBusy(true); setError("");
    void engineRef.current.prepare(props.workspace, props.assetFiles).then((value) => {
      if (serial !== prepareSerial.current) return;
      setReady(value); setResult(null); setLastProposal(null); setConfirmationProposal(null); setCapabilityResult(null);
    }).catch((e) => {
      if (serial !== prepareSerial.current) return;
      setReady(null); setError(String(e));
    }).finally(() => { if (serial === prepareSerial.current) setBusy(false); });
  }, [props.workspace, props.assetFiles]);
  useEffect(() => () => { prepareSerial.current += 1; void engineRef.current.close(); }, []);
  const context = () => {
    const values = parseBoundedSimulatorJson(contextText, "Context values") as Record<string, JsonValue>;
    const availableCapabilities = parseBoundedSimulatorJson(capabilitiesText, "Available capabilities") as Array<{ id: string; version: string }>;
    const visibleReferences = parseBoundedSimulatorJson(referencesText, "Visible references") as Array<{ kind: string; id: string }>;
    if (!Array.isArray(availableCapabilities) || !Array.isArray(visibleReferences)) throw new Error("Capability and reference context must be JSON arrays.");
    return { values, availableCapabilities, visibleReferences };
  };
  const run = async () => {
    setBusy(true); setError("");
    try {
      const prepared = await engineRef.current.prepare(props.workspace, props.assetFiles); setReady(prepared);
      const turn = await engineRef.current.session.turn(input, context(), 1);
      const sent = input;
      const reply = extractResponseText(turn.response) || "No visible text response.";
      setMessages((rows) => [...rows, { role: "user", text: sent }, { role: "bot", text: reply }]);
      setInput("");
      setResult(turn); setCapabilityResult(null); setLastProposal(firstAdmittedProposal(turn.capabilities)); setConfirmationProposal(firstConfirmationProposal(turn.capabilities));
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const resolveConfirmation = async (confirmed: boolean) => {
    if (!confirmationProposal) return;
    setBusy(true); setError("");
    try {
      const confirmedTurn = await engineRef.current.session.confirmProposal(confirmationProposal, confirmed, context());
      const reply = extractResponseText(confirmedTurn.response);
      if (reply) setMessages((rows) => [...rows, { role: "bot", text: reply }]);
      setResult(confirmedTurn); setCapabilityResult(null); setLastProposal(firstAdmittedProposal(confirmedTurn.capabilities)); setConfirmationProposal(firstConfirmationProposal(confirmedTurn.capabilities));
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const completeCapability = async () => {
    if (!lastProposal) return;
    setBusy(true); setError("");
    try {
      const current = context();
      const output = capabilityOutputText.trim() === "" ? undefined : parseBoundedSimulatorJson(capabilityOutputText, "Capability output") as JsonValue;
      const completed = await engineRef.current.session.capabilityResult(lastProposal, capabilitySucceeded, output, capabilitySucceeded ? null : capabilityErrorCode.trim() || "host_error", current, 2);
      setCapabilityResult(completed);
      if (completed.interaction) { const reply = extractResponseText(completed.interaction.response); if (reply) setMessages((rows) => [...rows, { role: "bot", text: reply }]); setResult(completed.interaction); setLastProposal(firstAdmittedProposal(completed.interaction.capabilities)); setConfirmationProposal(firstConfirmationProposal(completed.interaction.capabilities)); }
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const reset = () => { engineRef.current.resetConversation(); setMessages([]); setResult(null); setLastProposal(null); setConfirmationProposal(null); setCapabilityResult(null); };
  return <div className="page-stack">
    <section className="panel simulation-ready-panel"><div className="section-heading"><div><h2>{packagePreview ? `Simulate Package · ${props.workspace.selectedPackageId}` : `Simulate ${props.workspace.brain_id}`}</h2><p>{packagePreview ? "Studio compiles this Package with only its required dependencies and global or Project defaults. No Bot composition or Bot settings are included." : "Studio compiles the current Bot transiently with its bundled canonical Engine and runs the artifact immediately. No runtime files or intermediate artifacts are required."}</p></div><span className={`chip ${ready ? "success-chip" : ""}`}>{busy && !ready ? "Preparing…" : ready ? `Ready · Engine ${ready.engine}` : "Not ready"}</span></div>{ready && <div className="runtime-info"><span className="chip">{ready.info.project_id}</span><span className="chip">{ready.info.brain_id}</span><span className="mono tiny">source {ready.sourceFingerprint.slice(0, 12)}</span></div>}{error && <div className="error-banner">{error}</div>}</section>
    <section className="panel"><div className="section-heading"><div><h2>Conversation</h2><p>{packagePreview ? "Chat against this isolated Package-rooted source graph. Required dependency content remains present so replacements and references are tested honestly." : "Chat against the exact Bot source currently open in Studio. Why remains attached to each canonical runtime turn."}</p></div><button onClick={reset}>Reset conversation</button></div>{messages.length > 0 && <div className="simulation-transcript">{messages.map((message, index) => <div key={`${index}-${message.role}`} className={`simulation-message ${message.role}`}><span>{message.role === "user" ? "You" : "Bot"}</span><div>{message.text}</div></div>)}</div>}<div className="form-grid"><Field label="Message"><input value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && input.trim() && !busy) { e.preventDefault(); void run(); } }} placeholder="Type a message…" /></Field></div><ProgressiveSection title="Host interaction context" configured={contextText !== "{}" || capabilitiesText !== "[]" || referencesText !== "[]"} summary="Values, available capabilities, and visible references"><Field label="Context values (JSON object)"><textarea className="code-textarea" rows={4} value={contextText} onChange={(e) => setContextText(e.target.value)} /></Field><Field label="Available capabilities (JSON array)"><textarea className="code-textarea" rows={4} value={capabilitiesText} onChange={(e) => setCapabilitiesText(e.target.value)} placeholder='[{"id":"host.weather.read","version":"1"}]' /></Field><Field label="Visible references (JSON array)"><textarea className="code-textarea" rows={4} value={referencesText} onChange={(e) => setReferencesText(e.target.value)} placeholder='[{"kind":"device","id":"thermostat-1"}]' /></Field></ProgressiveSection><button className="primary" disabled={busy || !input.trim()} onClick={() => void run()}>{busy ? "Working…" : "Send"}</button>{result && <RuntimeResult result={result} />}{confirmationProposal && <section className="capability-roundtrip"><div className="section-heading"><div><h3>Capability confirmation</h3><p>Review the exact proposal <code>{String(confirmationProposal.capability)}@{String(confirmationProposal.capability_version)}</code>. Confirmation replays the same canonical turn while revalidating current host context.</p></div></div><div className="button-row"><button className="primary" disabled={busy} onClick={() => void resolveConfirmation(true)}>Confirm exact proposal</button><button disabled={busy} onClick={() => void resolveConfirmation(false)}>Decline</button></div></section>}{lastProposal && <section className="capability-roundtrip"><div className="section-heading"><div><h3>Host capability round-trip</h3><p>GVYA proposed <code>{String(lastProposal.capability)}@{String(lastProposal.capability_version)}</code>. Simulate the host result and feed it back through the canonical capability-result interaction.</p></div></div><label className="check-row"><input type="checkbox" checked={capabilitySucceeded} onChange={(e) => setCapabilitySucceeded(e.target.checked)} /> Host execution succeeded</label><Field label="Host output JSON"><textarea className="code-textarea" rows={4} value={capabilityOutputText} onChange={(e) => setCapabilityOutputText(e.target.value)} /></Field>{!capabilitySucceeded && <Field label="Error code"><input value={capabilityErrorCode} onChange={(e) => setCapabilityErrorCode(e.target.value)} /></Field>}<button disabled={busy} onClick={() => void completeCapability()}>Return host result to GVYA</button>{capabilityResult && <details open><summary>Capability-result validation + continuation</summary><pre>{JSON.stringify(capabilityResult, null, 2)}</pre></details>}</section>}</section>
  </div>;
}

function RuntimeResult({ result }: { result: GvyaTurnResult }) {
  const responseText = extractResponseText(result.response);
  return <div className="runtime-result"><div className="result-summary"><div><span className="small-label">Mode</span><strong>{String(result.mode ?? "—")}</strong></div><div><span className="small-label">Behavior</span><strong className="mono">{String(result.behavior ?? "—")}</strong></div><div><span className="small-label">Meaning</span><strong className="mono">{extractMeaningId(result.meaning)}</strong></div></div><div className="response-bubble">{responseText || "No visible text in response plan"}</div><ProgressiveSection title="Why this result" configured summary={whySummary(result.why)}><WhyTable value={result.why} /><details><summary>Raw runtime result</summary><pre>{JSON.stringify(result, null, 2)}</pre></details></ProgressiveSection></div>;
}

function WhyTable({ value }: { value: unknown }) {
  if (!value || typeof value !== "object") return <div className="muted">No structured Why payload.</div>;
  const rows = Object.entries(value as Record<string, unknown>).slice(0, 18);
  return <div className="why-table">{rows.map(([key, val]) => <div key={key}><span>{humanize(key)}</span><code>{renderCompact(val)}</code></div>)}</div>;
}

export function BuildView(props: { workspace: StudioBrainWorkspace; assetFiles: readonly StudioAssetFile[]; mutate: Mutate; setStatus: (status: string) => void }) {
  const packageRows = props.workspace.packages.map((pkg) => ({
    pkg,
    behaviorCount: pkg.contents.behaviors.length + pkg.contents.capability_result_behaviors.length + pkg.contents.fallback_behaviors.length,
  }));
  const totalBehaviors = packageRows.reduce((sum, row) => sum + row.behaviorCount, 0);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const busy = busyAction !== null;
  const buildArtifact = async () => {
    setBusyAction("build");
    try {
      const assets = await loadBundledEngineAssets();
      const compiler = await WasmCompilerBackend.instantiate(assets.engineModule);
      const artifact = compiler.compile(await compilerSourceEntries(props.workspace, props.assetFiles));
      downloadBytes(`${props.workspace.brain_id}.gvya`, artifact, "application/octet-stream");
      props.setStatus(`Built ${props.workspace.brain_id}.gvya with Engine ${STUDIO_ENGINE_VERSION}`);
    } catch (error) { props.setStatus(`Build failed: ${String(error)}`); }
    finally { setBusyAction(null); }
  };
  const exportRuntime = async (exporter: StudioRuntimeExporter) => {
    setBusyAction(exporter.id);
    try {
      const bundle = await exporter.build(props.workspace, props.assetFiles);
      downloadBytes(bundle.filename, bundle.bytes, bundle.mediaType);
      props.setStatus(`Exported ${exporter.label} with Engine ${STUDIO_ENGINE_VERSION}`);
    } catch (error) { props.setStatus(`Runtime export failed: ${String(error)}`); }
    finally { setBusyAction(null); }
  };
  return <div className="page-stack">
    <section className="panel"><div className="section-heading"><div><h2>Build Bot</h2><p>Compile the selected Bot into its portable canonical artifact.</p></div><div className="inline-actions"><button className="primary" disabled={busy} onClick={() => void buildArtifact()}>{busyAction === "build" ? "Building…" : "Build .gvya"}</button></div></div><div className="metric-grid"><Metric label="Project" value={props.workspace.project_id} /><Metric label="Bot / Brain" value={props.workspace.brain_id} /><Metric label="Packages" value={packageRows.length} /><Metric label="Behaviors" value={totalBehaviors} /><Metric label="Engine" value={STUDIO_ENGINE_VERSION} /></div></section>
    <section className="panel"><div className="section-heading"><div><h2>Export runtimes</h2><p>Package the built Bot with the canonical Engine and integration files for a supported target.</p></div><div className="inline-actions">{RUNTIME_EXPORTERS.map((exporter) => <button key={exporter.id} disabled={busy} onClick={() => void exportRuntime(exporter)}>{busyAction === exporter.id ? "Exporting…" : `Export ${exporter.label}`}</button>)}</div></div></section>
    <section className="panel list-panel"><div className="list-toolbar"><div><h2>Included Packages</h2><p className="muted small">{packageRows.length} resolved package{packageRows.length === 1 ? "" : "s"} · {totalBehaviors} total behavior{totalBehaviors === 1 ? "" : "s"}</p></div></div><div className="data-list-header build-package-columns"><span>Package</span><span>Kind</span><span>Behaviors</span></div>{packageRows.map(({ pkg, behaviorCount }) => <div className="data-row build-package-columns" key={pkg.manifest.id}><div className="row-primary"><strong>{pkg.manifest.id}</strong><span className="muted tiny">{pkg.manifest.description || "No description"}</span></div><span>{pkg.manifest.kind === "fallback" ? "Fallback" : "Standard"}</span><strong>{behaviorCount}</strong></div>)}</section>
  </div>;
}


function downloadBytes(filename: string, bytes: Uint8Array, type: string): void {
  const blob = new Blob([bytes], { type });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a"); anchor.href = url; anchor.download = filename; anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

function firstConfirmationProposal(capabilities: JsonValue): InvocationProposal | null { return firstProposal(capabilities, "needs_confirmation"); }
function firstAdmittedProposal(capabilities: JsonValue): InvocationProposal | null { return firstProposal(capabilities, "admitted"); }

function firstProposal(capabilities: JsonValue, outcomeType: string): InvocationProposal | null {
  const root = jsonRecord(capabilities);
  if (!root || !Array.isArray(root.decisions)) return null;
  for (const decisionValue of root.decisions) {
    const decision = jsonRecord(decisionValue); const outcome = jsonRecord(decision?.outcome); const proposal = jsonRecord(decision?.proposal);
    if (outcome?.type === outcomeType && proposal && invocationProposal(proposal)) return proposal;
  }
  return null;
}

function invocationProposal(value: Record<string, JsonValue>): value is Record<string, JsonValue> & InvocationProposal {
  return typeof value.id === "string" && typeof value.capability === "string" && typeof value.capability_version === "string"
    && jsonRecord(value.arguments) !== null && typeof value.fingerprint === "string" && typeof value.trace_id === "string";
}

function whySummary(value: unknown): string {
  if (!value || typeof value !== "object") return "Runtime explanation";
  const keys = Object.keys(value as Record<string, unknown>);
  return `${keys.length} trace field${keys.length === 1 ? "" : "s"}`;
}
function renderCompact(value: unknown): string {
  if (value == null) return "—";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  const text = JSON.stringify(value);
  return text.length > 180 ? `${text.slice(0, 177)}…` : text;
}
function extractMeaningId(value: unknown): string {
  if (!value || typeof value !== "object") return "—";
  const row = value as Record<string, unknown>;
  return String(row.id ?? row.meaning ?? "—");
}
function extractResponseText(value: JsonValue): string {
  const queue: JsonValue[] = [value];
  for (let visited = 0; queue.length && visited < 128; visited += 1) {
    const current = queue.shift()!;
    const row = jsonRecord(current);
    if (row) {
      if (typeof row.selected_text === "string") return row.selected_text;
      if (typeof row.text === "string") return row.text;
      queue.push(...Object.values(row));
    } else if (Array.isArray(current)) queue.push(...current);
  }
  return "";
}

function jsonRecord(value: unknown): Record<string, JsonValue> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, JsonValue> : null;
}
