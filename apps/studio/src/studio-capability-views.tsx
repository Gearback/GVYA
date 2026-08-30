import { useEffect, useState } from "react";
import { auditWorkspace } from "./audit.js";
import { csv, JsonObjectEditor, JsonValueInput, Subheading } from "./studio-authoring-fields.js";
import { IssueList } from "./studio-content-views.js";
import { capabilityDeleteImpacts, deleteCapabilityAtomic, renameCapabilityIdentityAtomic } from "./human-authoring.js";
import { EmptyState, Field, Modal, ProgressiveSection } from "./studio-ui.js";
import type { AuditIssue, CapabilityBinding, CapabilityDefinition, CapabilityPolicy, Contribution, StudioBrainWorkspace, StudioPackage } from "./types.js";
import { cloneBrainWorkspace, contribution, createBinding, createCapability, createPolicy, humanize, selectedPackage, touch, uniqueId } from "./workspace.js";

type Mutate = (mutator: (draft: StudioBrainWorkspace) => void) => boolean;

export function CapabilitiesView(props: { workspace: StudioBrainWorkspace; mutate: Mutate; replaceWorkspace: (workspace: StudioBrainWorkspace) => boolean; issues: AuditIssue[] }) {
  const pkg = selectedPackage(props.workspace);
  const [draftWorkspace, setDraftWorkspace] = useState<StudioBrainWorkspace | null>(null);
  const [draftCapabilityId, setDraftCapabilityId] = useState("");
  const [draftOriginalCapabilityId, setDraftOriginalCapabilityId] = useState("");
  const [draftMode, setDraftMode] = useState<"create" | "edit" | null>(null);
  const [query, setQuery] = useState("");
  useEffect(() => { setDraftWorkspace(null); setDraftCapabilityId(""); setDraftOriginalCapabilityId(""); setDraftMode(null); }, [pkg.manifest.id]);
  const filtered = pkg.contents.capabilities.filter((row) => {
    const cap = row.value.contract;
    const haystack = `${cap.id} ${cap.version} ${cap.title} ${cap.description} ${cap.effect_class} ${cap.confirmation_hint}`.toLowerCase();
    return haystack.includes(query.trim().toLowerCase());
  });
  const openDraft = (id: string) => { setDraftWorkspace(cloneBrainWorkspace(props.workspace)); setDraftCapabilityId(id); setDraftOriginalCapabilityId(id); setDraftMode("edit"); };
  const add = () => {
    const draft = cloneBrainWorkspace(props.workspace);
    const current = selectedPackage(draft);
    const capability = createCapability("");
    capability.contract.version = "";
    capability.contract.title = "";
    current.contents.capabilities.push(contribution("", capability));
    setDraftWorkspace(draft); setDraftCapabilityId(""); setDraftOriginalCapabilityId(""); setDraftMode("create");
  };
  const mutateDraft: Mutate = (mutator) => { setDraftWorkspace((previous) => { if (!previous) return previous; const next = cloneBrainWorkspace(previous); mutator(next); return touch(next); }); return true; };
  const replaceDraft = (next: StudioBrainWorkspace): boolean => { setDraftWorkspace(next); return true; };
  const discardDraft = () => { setDraftWorkspace(null); setDraftCapabilityId(""); setDraftOriginalCapabilityId(""); setDraftMode(null); };
  const draftPackage = draftWorkspace ? selectedPackage(draftWorkspace) : null;
  const selected = draftPackage ? draftPackage.contents.capabilities.find((row) => row.value.contract.id === draftCapabilityId) ?? null : null;
  const draftIssues = draftWorkspace ? auditWorkspace(draftWorkspace) : [];
  const bindings = selected && draftPackage ? draftPackage.contents.capability_bindings.filter((row) => row.value.capability === selected.value.contract.id) : [];
  const policies = selected && draftPackage ? draftPackage.contents.capability_policies.filter((row) => row.value.capability === selected.value.contract.id) : [];
  const localIssues = selected && draftPackage ? draftIssues.filter((row) => row.packageId === draftPackage.manifest.id && [selected.value.contract.id, ...bindings.map((row) => row.value.id), ...policies.map((row) => row.value.id)].includes(row.objectId)) : [];
  const blocked = localIssues.some((row) => row.severity === "error");
  const save = () => { if (draftWorkspace && selected && !blocked && props.replaceWorkspace(draftWorkspace)) discardDraft(); };
  const deleteImpacts = draftMode === "edit" && draftOriginalCapabilityId ? capabilityDeleteImpacts(props.workspace, pkg.manifest.id, draftOriginalCapabilityId) : [];
  const remove = () => {
    if (draftMode !== "edit" || !draftOriginalCapabilityId || deleteImpacts.length) return;
    try { if (props.replaceWorkspace(deleteCapabilityAtomic(props.workspace, pkg.manifest.id, draftOriginalCapabilityId))) discardDraft(); }
    catch { /* canonical dependency state changed while confirmation was open */ }
  };
  return <div className="page-stack">
    <section className="panel object-list-page">
      <div className="object-page-heading"><div><h2>Capabilities</h2><p>Keep the capability catalog scannable; open a contract in a large editor only when needed.</p></div><div className="object-page-actions"><button className="primary compact" onClick={add}>New capability</button></div></div>
      <div className="filters-row capability-page-filters object-page-filters"><input aria-label="Search capabilities" placeholder="Search capabilities…" value={query} onChange={(e) => setQuery(e.target.value)} /></div>
      <div className="object-page-list capability-page-list">{filtered.map((row) => <button key={row.id} className={`capability-list-card ${draftMode === "edit" && row.value.contract.id === draftCapabilityId ? "selected" : ""}`} onClick={() => openDraft(row.value.contract.id)}><div className="card-title-row"><strong>{row.value.contract.title || humanize(row.value.contract.id)}</strong><span className="chip">v{row.value.contract.version}</span></div><div className="mono small">{row.value.contract.id}</div>{row.value.contract.description && <div className="list-description muted small">{row.value.contract.description}</div>}<div className="chip-row"><span className="chip">{row.value.contract.effect_class}</span><span className="chip">confirmation {row.value.contract.confirmation_hint}</span></div></button>)}{filtered.length === 0 && <EmptyState title="No capabilities match" text="Change the search or create a new capability." />}</div>
    </section>
    {draftWorkspace && draftPackage && selected && draftMode && <Modal title={draftMode === "create" ? "New capability" : `Capability · ${selected.value.contract.id}@${selected.value.contract.version}`} size="workspace" onClose={discardDraft}><CapabilityEditor workspace={draftWorkspace} pkg={draftPackage} row={selected} mutate={mutateDraft} replaceWorkspace={replaceDraft} onIdentityChanged={setDraftCapabilityId} issues={draftIssues} mode={draftMode} onSave={save} onCancel={discardDraft} onRemove={remove} deleteImpacts={deleteImpacts} saveBlocked={blocked} /></Modal>}
  </div>;
}
function CapabilityEditor(props: { workspace: StudioBrainWorkspace; pkg: StudioPackage; row: Contribution<CapabilityDefinition>; mutate: Mutate; replaceWorkspace: (workspace: StudioBrainWorkspace) => boolean; onIdentityChanged: (id: string) => void; issues: AuditIssue[]; mode: "create" | "edit"; onSave: () => void; onCancel: () => void; onRemove: () => void; deleteImpacts: ReturnType<typeof capabilityDeleteImpacts>; saveBlocked: boolean }) {
  const cap = props.row.value;
  const id = cap.contract.id;
  const version = cap.contract.version;
  const [idDraft, setIdDraft] = useState(id);
  const [versionDraft, setVersionDraft] = useState(version);
  const [identityError, setIdentityError] = useState("");
  const [removeOpen, setRemoveOpen] = useState(false);
  useEffect(() => { setIdDraft(id); setVersionDraft(version); setIdentityError(""); }, [id, version]);
  const update = (fn: (capability: CapabilityDefinition) => void) => props.mutate((draft) => { const p = draft.packages.find((x) => x.manifest.id === props.pkg.manifest.id); const row = p?.contents.capabilities.find((x) => x.id === props.row.id); if (row) fn(row.value); });
  const identityDirty = idDraft.trim() !== id || versionDraft.trim() !== version;
  const commitIdentity = () => {
    const nextId = idDraft.trim();
    const nextVersion = versionDraft.trim();
    try {
      props.replaceWorkspace(renameCapabilityIdentityAtomic(props.workspace, props.pkg.manifest.id, id, version, nextId, nextVersion));
      setIdDraft(nextId); setVersionDraft(nextVersion); setIdentityError("");
      props.onIdentityChanged(nextId);
    } catch (error) { setIdentityError(String(error)); setIdDraft(id); setVersionDraft(version); }
  };
  const resetIdentity = () => { setIdDraft(id); setVersionDraft(version); setIdentityError(""); };
  const bindings = props.pkg.contents.capability_bindings.filter((row) => row.value.capability === id);
  const policies = props.pkg.contents.capability_policies.filter((row) => row.value.capability === id);
  const localIssues = props.issues.filter((row) => row.packageId === props.pkg.manifest.id && [id, ...bindings.map((x) => x.value.id), ...policies.map((x) => x.value.id)].includes(row.objectId));
  return <div className="editor-layout"><div className="editor-scroll"><div className="editor-header"><div><div className="eyebrow">Capability contract</div><h2>{cap.contract.title || humanize(id)}</h2><div className="mono muted">{id}@{version}</div></div><div className="chip-row"><span className="chip">{cap.contract.effect_class}</span><span className="chip accent">confirmation {cap.contract.confirmation_hint}</span></div></div>
    <section className="identity-editor"><div className="form-grid two"><Field label="Capability ID"><input value={idDraft} placeholder="host.calendar.create" onChange={(e) => setIdDraft(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") commitIdentity(); if (e.key === "Escape") resetIdentity(); }} /></Field><Field label="Version"><input value={versionDraft} placeholder="1" onChange={(e) => setVersionDraft(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") commitIdentity(); if (e.key === "Escape") resetIdentity(); }} /></Field></div>{identityError && <span className="field-error">{identityError}</span>}<div className="inline-actions identity-actions"><button disabled={!identityDirty} onClick={resetIdentity}>Reset identity</button><button className="primary compact" disabled={!identityDirty || !idDraft.trim() || !versionDraft.trim()} onClick={commitIdentity}>Apply identity</button></div><p className="muted tiny">ID/version changes are committed atomically with bindings, policies, result handlers, tests, scenarios, host contexts, and specialization references.</p></section>
    <div className="form-grid two"><Field label="Title"><input value={cap.contract.title} placeholder="Calendar create" onChange={(e) => update((c) => { c.contract.title = e.target.value; })} /></Field><Field label="Effect class"><select value={cap.contract.effect_class} onChange={(e) => update((c) => { c.contract.effect_class = e.target.value as typeof c.contract.effect_class; })}><option>pure</option><option>reversible</option><option>irreversible</option><option>external</option></select></Field><Field label="Confirmation hint"><select value={cap.contract.confirmation_hint} onChange={(e) => update((c) => { c.contract.confirmation_hint = e.target.value as typeof c.contract.confirmation_hint; })}><option>never</option><option>conditional</option><option>always</option></select></Field><Field label="Reference kinds"><input value={cap.contract.reference_kinds.join(", ")} onChange={(e) => update((c) => { c.contract.reference_kinds = csv(e.target.value); })} placeholder="device, room" /></Field></div>
    <Field label="Description"><textarea rows={3} value={cap.contract.description} onChange={(e) => update((c) => { c.contract.description = e.target.value; })} /></Field>
    <ProgressiveSection title="Input / output schema" configured summary="JSON Schema · compiler validated"><JsonObjectEditor label="Input schema" value={cap.contract.input_schema} onChange={(value) => update((c) => { if (value !== null) c.contract.input_schema = value; })} /><JsonObjectEditor label="Output schema" nullable value={cap.contract.output_schema} onChange={(value) => update((c) => { c.contract.output_schema = value; })} /></ProgressiveSection>
    <ProgressiveSection title="Host effects" configured={cap.host_effects.length > 0} summary={cap.host_effects.length ? `${cap.host_effects.length} declared effect${cap.host_effects.length === 1 ? "" : "s"}` : "No declared host effects"}><div className="item-stack">{cap.host_effects.map((effect, index) => <div className="inline-form-row" key={index}><input value={effect.resource} onChange={(e) => update((c) => { c.host_effects[index]!.resource = e.target.value; })} placeholder="resource" /><select value={effect.kind} onChange={(e) => update((c) => { c.host_effects[index]!.kind = e.target.value as typeof c.host_effects[number]["kind"]; })}><option>read</option><option>update</option><option>create</option><option>delete</option><option>external</option></select><input value={effect.summary} onChange={(e) => update((c) => { c.host_effects[index]!.summary = e.target.value; })} placeholder="human summary" /><button className="icon-button" onClick={() => update((c) => { c.host_effects.splice(index, 1); })}>×</button></div>)}<button onClick={() => update((c) => { c.host_effects.push({ resource: "", kind: "read", summary: "" }); })}>Add host effect</button></div></ProgressiveSection>
    <BindingPolicyEditor pkg={props.pkg} capabilityId={id} bindings={bindings} policies={policies} mutate={props.mutate} />
    <section className="editor-section proximity-panel"><div className="section-heading compact-heading"><div><h3>Checks near this capability</h3></div></div>{localIssues.length ? <IssueList issues={localIssues} compact /> : <div className="success-row">No Studio authoring issues for this capability.</div>}</section>
  </div><div className="sticky-editor-footer">{props.mode === "edit" && <button className="danger" onClick={() => setRemoveOpen(true)}>Remove capability</button>}<span className="footer-spacer" aria-hidden="true" /><button onClick={props.onCancel}>Cancel</button><button className="primary" disabled={props.saveBlocked || Boolean(identityError) || !idDraft.trim() || !versionDraft.trim()} onClick={props.onSave}>{props.mode === "create" ? "Create capability" : "Save changes"}</button></div>
    {props.mode === "edit" && removeOpen && <Modal title={`Remove ${id}?`} onClose={() => setRemoveOpen(false)}>{props.deleteImpacts.length > 0 ? <><div className="error-banner">Removal is blocked until dependent references are removed or redirected.</div><div className="dependency-list">{props.deleteImpacts.map((impact, index) => <div className="dependency-row" key={`${impact.kind}-${impact.packageId}-${impact.objectId}-${index}`}><span className="code-chip">{impact.kind}</span><div><strong>{impact.objectId}</strong><div className="tiny muted">{impact.packageId}</div><p>{impact.detail}</p></div></div>)}</div><div className="modal-actions"><button onClick={() => setRemoveOpen(false)}>Close</button></div></> : <><p>This removes the Capability and its local bindings and policies atomically.</p><div className="modal-actions"><button onClick={() => setRemoveOpen(false)}>Cancel</button><button className="danger" onClick={props.onRemove}>Remove capability</button></div></>}</Modal>}
  </div>;
}

function BindingPolicyEditor(props: { pkg: StudioPackage; capabilityId: string; bindings: Contribution<CapabilityBinding>[]; policies: Contribution<CapabilityPolicy>[]; mutate: Mutate }) {
  const addBinding = () => props.mutate((draft) => { const p = draft.packages.find((x) => x.manifest.id === props.pkg.manifest.id)!; const id = uniqueId(p.contents.capability_bindings.map((r) => r.value.id), `${props.capabilityId}.binding`); p.contents.capability_bindings.push(contribution(id, createBinding(id, props.capabilityId))); });
  const addPolicy = () => props.mutate((draft) => { const p = draft.packages.find((x) => x.manifest.id === props.pkg.manifest.id)!; const id = uniqueId(p.contents.capability_policies.map((r) => r.value.id), `${props.capabilityId}.policy`); p.contents.capability_policies.push(contribution(id, createPolicy(id, props.capabilityId))); });
  return <><section className="editor-section"><div className="section-heading compact-heading"><div><h3>Bindings</h3><p>Map Meaning, behavior, or response events to typed capability arguments.</p></div><button onClick={addBinding}>Add binding</button></div>{props.bindings.length === 0 ? <div className="muted empty-inline">No bindings yet.</div> : props.bindings.map((row) => <BindingCard key={row.id} pkgId={props.pkg.manifest.id} row={row} mutate={props.mutate} />)}</section><section className="editor-section"><div className="section-heading compact-heading"><div><h3>Admission policies</h3><p>Human-readable allow, confirmation, and deny rules.</p></div><button onClick={addPolicy}>Add policy</button></div>{props.policies.length === 0 ? <div className="muted empty-inline">No local policy rules yet.</div> : props.policies.map((row) => <PolicyCard key={row.id} pkgId={props.pkg.manifest.id} row={row} mutate={props.mutate} />)}</section></>;
}

function BindingCard(props: { pkgId: string; row: Contribution<CapabilityBinding>; mutate: Mutate }) {
  const b = props.row.value;
  const update = (fn: (row: CapabilityBinding) => void) => props.mutate((draft) => { const target = draft.packages.find((p) => p.manifest.id === props.pkgId)?.contents.capability_bindings.find((x) => x.id === props.row.id); if (target) fn(target.value); });
  return <ProgressiveSection title={b.id} configured summary={bindingSummary(b)}><div className="form-grid three"><Field label="Meaning trigger"><input value={b.trigger.meaning} onChange={(e) => update((row) => { row.trigger.meaning = e.target.value; })} /></Field><Field label="Behavior trigger"><input value={b.trigger.behavior} onChange={(e) => update((row) => { row.trigger.behavior = e.target.value; })} /></Field><Field label="Response trigger"><input value={b.trigger.response} onChange={(e) => update((row) => { row.trigger.response = e.target.value; })} /></Field></div><Subheading title="Arguments" description="Bind named capability inputs without embedding host execution." /><div className="item-stack">{b.arguments.map((arg, index) => <div className="binding-arg-row" key={index}><input value={arg.target} onChange={(e) => update((row) => { row.arguments[index]!.target = e.target.value; })} placeholder="target.path" /><select value={arg.source.type} onChange={(e) => update((row) => { row.arguments[index]!.source.type = e.target.value as typeof row.arguments[number]["source"]["type"]; })}><option value="meaning_slot">Meaning slot</option><option value="meaning_reference">Meaning reference</option><option value="focus_reference">Focus reference</option><option value="context_path">Context path</option><option value="author_state_path">Author state path</option><option value="literal">Literal</option></select>{arg.source.type === "meaning_slot" && <input value={arg.source.name} onChange={(e) => update((row) => { row.arguments[index]!.source.name = e.target.value; })} placeholder="slot name" />}{["meaning_reference", "focus_reference"].includes(arg.source.type) && <><input value={arg.source.kind} onChange={(e) => update((row) => { row.arguments[index]!.source.kind = e.target.value; })} placeholder="reference kind" /><select value={arg.source.projection} onChange={(e) => update((row) => { row.arguments[index]!.source.projection = e.target.value as typeof row.arguments[number]["source"]["projection"]; })}><option value="id">ID</option><option value="object">Object</option></select></>}{["context_path", "author_state_path"].includes(arg.source.type) && <input value={arg.source.path} onChange={(e) => update((row) => { row.arguments[index]!.source.path = e.target.value; })} placeholder="source path" />}{arg.source.type === "literal" && <JsonValueInput value={arg.source.value} onChange={(value) => update((row) => { row.arguments[index]!.source.value = value; })} />}<button className="icon-button" onClick={() => update((row) => { row.arguments.splice(index, 1); })}>×</button></div>)}<button onClick={() => update((row) => { row.arguments.push({ target: "", source: { type: "meaning_slot", name: "", kind: "", projection: "id", path: "", value: null } }); })}>Add argument</button></div></ProgressiveSection>;
}

function PolicyCard(props: { pkgId: string; row: Contribution<CapabilityPolicy>; mutate: Mutate }) {
  const p = props.row.value;
  const update = (fn: (row: CapabilityPolicy) => void) => props.mutate((draft) => { const target = draft.packages.find((pkg) => pkg.manifest.id === props.pkgId)?.contents.capability_policies.find((x) => x.id === props.row.id); if (target) fn(target.value); });
  return <ProgressiveSection title={p.id} configured summary={`${p.effect.type.replaceAll("_", " ")} · priority ${p.priority}`}><div className="form-grid two"><Field label="Priority"><input type="number" value={p.priority} onChange={(e) => update((row) => { row.priority = Number(e.target.value); })} /></Field><Field label="Effect"><select value={p.effect.type} onChange={(e) => update((row) => { row.effect = e.target.value === "allow" ? { type: "allow", reason_code: "" } : e.target.value === "deny" ? { type: "deny", reason_code: "policy.denied" } : { type: "require_confirmation", reason_code: "policy.confirm" }; })}><option value="allow">Allow</option><option value="require_confirmation">Require confirmation</option><option value="deny">Deny</option></select></Field>{p.effect.type !== "allow" && <Field label="Reason code"><input value={p.effect.reason_code} onChange={(e) => update((row) => { if (row.effect.type !== "allow") row.effect.reason_code = e.target.value; })} /></Field>}</div><Subheading title="Conditions" description="All predicates must match for this rule." /><div className="item-stack">{p.conditions.map((condition, index) => <div className="condition-row" key={index}><select value={condition.namespace} onChange={(e) => update((row) => { row.conditions[index]!.namespace = e.target.value as typeof row.conditions[number]["namespace"]; })}><option>arguments</option><option>context</option><option>author</option><option>conversation</option><option>system</option></select><input value={condition.path} onChange={(e) => update((row) => { row.conditions[index]!.path = e.target.value; })} placeholder="path" /><select value={condition.op} onChange={(e) => update((row) => { row.conditions[index]!.op = e.target.value as typeof row.conditions[number]["op"]; row.conditions[index]!.hasValue = !["exists", "missing"].includes(e.target.value); })}><option>exists</option><option>missing</option><option>equal</option><option>not_equal</option><option>greater</option><option>greater_or_equal</option><option>less</option><option>less_or_equal</option></select>{condition.hasValue && <JsonValueInput value={condition.value} onChange={(value) => update((row) => { row.conditions[index]!.value = value; })} />}<button className="icon-button" onClick={() => update((row) => { row.conditions.splice(index, 1); })}>×</button></div>)}<button onClick={() => update((row) => { row.conditions.push({ namespace: "context", path: "", op: "exists", value: null, hasValue: false }); })}>Add condition</button></div></ProgressiveSection>;
}


function bindingSummary(binding: CapabilityBinding): string {
  const triggers = [binding.trigger.meaning, binding.trigger.behavior, binding.trigger.response].filter(Boolean);
  return `${triggers.join(" · ") || "No trigger"} → ${binding.capability} · ${binding.arguments.length} args`;
}
