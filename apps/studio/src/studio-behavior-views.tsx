import { useEffect, useMemo, useState } from "react";
import { auditWorkspace } from "./audit.js";
import { languageKey } from "./languages.js";
import { JsonValueInput, Subheading } from "./studio-authoring-fields.js";
import { IssueList } from "./studio-content-views.js";
import { EmptyState, Field, Modal, ProgressiveSection } from "./studio-ui.js";
import {
  behaviorDeleteImpacts, deleteBehaviorPair, deleteResponseAtomic, groupBehaviorRows,
  renameBehaviorAtomic, renameMeaningAtomic, renameResponseAtomic, responseDeleteImpacts,
  type BehaviorGroupMode,
} from "./human-authoring.js";
import type {
  AuditIssue, BehaviorDefinition, Contribution, FallbackBehaviorDefinition, MeaningDefinition,
  ResponseDefinition, StudioBrainWorkspace, StudioPackage, ValueRequirement,
} from "./types.js";
import {
  cloneBrainWorkspace, contribution, createBehavior, createFallbackBehavior, createMeaning,
  createResponse, fallbackContribution, humanize, selectedPackage, touch, uniqueId,
} from "./workspace.js";

type Mutate = (mutator: (draft: StudioBrainWorkspace) => void) => boolean;

export function AuthorView(props: { workspace: StudioBrainWorkspace; mutate: Mutate; replaceWorkspace: (workspace: StudioBrainWorkspace) => boolean; issues: AuditIssue[]; onTryUtterance: (text: string) => void }) {
  const pkg = selectedPackage(props.workspace);
  return pkg.manifest.kind === "fallback"
    ? <FallbackAuthorView workspace={props.workspace} replaceWorkspace={props.replaceWorkspace} />
    : <StandardAuthorView {...props} />;
}

function StandardAuthorView(props: { workspace: StudioBrainWorkspace; mutate: Mutate; replaceWorkspace: (workspace: StudioBrainWorkspace) => boolean; issues: AuditIssue[]; onTryUtterance: (text: string) => void }) {
  const pkg = selectedPackage(props.workspace);
  const [query, setQuery] = useState("");
  const [draftWorkspace, setDraftWorkspace] = useState<StudioBrainWorkspace | null>(null);
  const [draftBehaviorId, setDraftBehaviorId] = useState("");
  const [draftMode, setDraftMode] = useState<"create" | "edit" | null>(null);
  const [draftOriginalBehaviorId, setDraftOriginalBehaviorId] = useState("");
  const [topicFilter, setTopicFilter] = useState("");
  const [groupMode, setGroupMode] = useState<BehaviorGroupMode>("flat");

  useEffect(() => { setDraftWorkspace(null); setDraftBehaviorId(""); setDraftOriginalBehaviorId(""); setDraftMode(null); }, [pkg.manifest.id]);

  const topics = [...new Set(pkg.contents.behaviors.map((row) => row.value.topic).filter(Boolean))].sort();
  const filtered = pkg.contents.behaviors.filter((row) => {
    const behavior = row.value;
    const meaning = pkg.contents.meanings.find((candidate) => candidate.value.id === behavior.meaning)?.value;
    const sample = meaning?.samples[0]?.text ?? "";
    const response = behavior.responses[0]?.texts[0]?.variants[0] ?? "";
    const haystack = `${behavior.id} ${behavior.meaning} ${sample} ${response} ${behavior.topic}`.toLowerCase();
    return (!topicFilter || behavior.topic === topicFilter) && haystack.includes(query.trim().toLowerCase());
  });
  const grouped = groupBehaviorRows(filtered, groupMode);

  const openBehaviorDraft = (behaviorId: string) => {
    setDraftWorkspace(cloneBrainWorkspace(props.workspace));
    setDraftBehaviorId(behaviorId);
    setDraftOriginalBehaviorId(behaviorId);
    setDraftMode("edit");
  };
  const addBehavior = () => {
    const draft = cloneBrainWorkspace(props.workspace);
    const current = selectedPackage(draft);
    const meaningId = uniqueId(current.contents.meanings.map((row) => row.value.id), "meaning.new");
    const behaviorId = uniqueId(current.contents.behaviors.map((row) => row.value.id), `${meaningId}.behavior`);
    const language = draft.authoring_language;
    current.contents.meanings.push(contribution(meaningId, createMeaning(meaningId, language)));
    current.contents.behaviors.push(contribution(behaviorId, createBehavior(behaviorId, meaningId, language)));
    setDraftWorkspace(draft);
    setDraftBehaviorId(behaviorId);
    setDraftOriginalBehaviorId("");
    setDraftMode("create");
  };
  const mutateDraft: Mutate = (mutator) => { setDraftWorkspace((previous) => {
    if (!previous) return previous;
    const next = cloneBrainWorkspace(previous);
    mutator(next);
    return touch(next);
  }); return true; };
  const replaceDraft = (next: StudioBrainWorkspace): boolean => { setDraftWorkspace(next); return true; };
  const discardDraft = () => { setDraftWorkspace(null); setDraftBehaviorId(""); setDraftOriginalBehaviorId(""); setDraftMode(null); };

  const draftPackage = draftWorkspace ? selectedPackage(draftWorkspace) : null;
  const draftBehaviorRow = draftPackage ? draftPackage.contents.behaviors.find((row) => row.value.id === draftBehaviorId) ?? null : null;
  const draftMeaningRow = draftPackage && draftBehaviorRow ? draftPackage.contents.meanings.find((row) => row.value.id === draftBehaviorRow.value.meaning) ?? null : null;
  const draftIssues = draftWorkspace ? auditWorkspace(draftWorkspace) : [];
  const localDraftIssues = draftPackage && draftBehaviorRow && draftMeaningRow
    ? draftIssues.filter((row) => row.packageId === draftPackage.manifest.id && [draftBehaviorRow.value.id, draftMeaningRow.value.id, ...draftBehaviorRow.value.responses.map((response) => response.id)].includes(row.objectId))
    : [];
  const draftBlocked = localDraftIssues.some((row) => row.severity === "error");
  const saveDraft = () => {
    if (!draftWorkspace || !draftBehaviorRow || draftBlocked) return;
    if (props.replaceWorkspace(draftWorkspace)) discardDraft();
  };
  const deleteDraft = (_nextWorkspace: StudioBrainWorkspace) => {
    if (draftMode !== "edit" || !draftOriginalBehaviorId) return;
    try {
      const next = deleteBehaviorPair(props.workspace, pkg.manifest.id, draftOriginalBehaviorId);
      if (props.replaceWorkspace(next)) discardDraft();
    } catch { /* canonical dependency state changed while modal was open; keep the draft open */ }
  };

  return <div className="page-stack">
    <section className="panel object-list-page">
      <div className="object-page-heading"><div><h2>Behaviors</h2><p>Scan the package as a list. Open a behavior only when you want the full authoring surface.</p></div><div className="object-page-actions"><button className="primary compact" onClick={addBehavior}>New behavior</button></div></div>
      <div className="filters-row author-filters object-page-filters"><input aria-label="Search behaviors" placeholder="Search behaviors…" value={query} onChange={(e) => setQuery(e.target.value)} /><select aria-label="Filter by topic" value={topicFilter} onChange={(e) => setTopicFilter(e.target.value)}><option value="">All topics</option>{topics.map((topic) => <option key={topic} value={topic}>{topic}</option>)}</select><select aria-label="Group behaviors" value={groupMode} onChange={(e) => setGroupMode(e.target.value as BehaviorGroupMode)}><option value="flat">Flat list</option><option value="topic">Group by topic</option><option value="followup">Group by follow-up</option></select></div>
      <div className="object-page-list behavior-page-list">
        {grouped.map((group) => <div className="behavior-group" key={group.key}>{groupMode !== "flat" && <div className="behavior-group-heading"><strong>{group.label}</strong><span>{group.rows.length}</span></div>}{group.rows.map((row) => <BehaviorListCard key={row.id} row={row} pkg={pkg} selected={draftMode === "edit" && row.value.id === draftBehaviorId} onSelect={() => openBehaviorDraft(row.value.id)} />)}</div>)}
        {filtered.length === 0 && <EmptyState title="No behaviors match" text="Change the filters or create a new behavior." />}
      </div>
    </section>
    {draftWorkspace && draftPackage && draftBehaviorRow && draftMeaningRow && draftMode && <Modal title={draftMode === "create" ? "New behavior" : `Behavior · ${draftBehaviorRow.value.id}`} size="workspace" onClose={discardDraft}>
      <BehaviorEditor workspace={draftWorkspace} pkg={draftPackage} behaviorRow={draftBehaviorRow} meaningRow={draftMeaningRow} mutate={mutateDraft} replaceWorkspace={replaceDraft} issues={draftIssues} onRenamed={setDraftBehaviorId} onDeleted={deleteDraft} mode={draftMode} onSave={saveDraft} onCancel={discardDraft} saveBlocked={draftBlocked} />
    </Modal>}
  </div>;
}
function FallbackAuthorView(props: { workspace: StudioBrainWorkspace; replaceWorkspace: (workspace: StudioBrainWorkspace) => boolean }) {
  const pkg = selectedPackage(props.workspace);
  const [query, setQuery] = useState("");
  const [draftWorkspace, setDraftWorkspace] = useState<StudioBrainWorkspace | null>(null);
  const [draftId, setDraftId] = useState("");
  const [mode, setMode] = useState<"create" | "edit" | null>(null);
  useEffect(() => { setDraftWorkspace(null); setDraftId(""); setMode(null); }, [pkg.manifest.id]);
  const filtered = pkg.contents.fallback_behaviors.filter((row) => {
    const response = row.value.responses.flatMap((candidate) => candidate.texts.flatMap((texts) => texts.variants)).find(Boolean) ?? "";
    return `${row.value.id} ${row.value.trigger} ${row.value.priority} ${response}`.toLowerCase().includes(query.trim().toLowerCase());
  });
  const open = (id: string) => { setDraftWorkspace(cloneBrainWorkspace(props.workspace)); setDraftId(id); setMode("edit"); };
  const add = () => {
    const next = cloneBrainWorkspace(props.workspace); const current = selectedPackage(next);
    const id = uniqueId(current.contents.fallback_behaviors.map((row) => row.value.id), "fallback.new");
    const behavior = createFallbackBehavior(id, next.authoring_language); behavior.responses[0]!.kind = "fallback";
    current.contents.fallback_behaviors.push(fallbackContribution(id, behavior));
    setDraftWorkspace(next); setDraftId(id); setMode("create");
  };
  const mutateDraft = (fn: (behavior: FallbackBehaviorDefinition) => void) => setDraftWorkspace((previous) => {
    if (!previous) return previous; const next = cloneBrainWorkspace(previous); const current = selectedPackage(next);
    const row = current.contents.fallback_behaviors.find((candidate) => candidate.value.id === draftId); if (!row) return previous;
    const before = row.value.id; fn(row.value); row.id = row.value.id; if (row.value.id !== before) setDraftId(row.value.id); return touch(next);
  });
  const discard = () => { setDraftWorkspace(null); setDraftId(""); setMode(null); };
  const draftPackage = draftWorkspace ? selectedPackage(draftWorkspace) : null;
  const row = draftPackage?.contents.fallback_behaviors.find((candidate) => candidate.value.id === draftId) ?? null;
  const issues = draftWorkspace ? auditWorkspace(draftWorkspace) : [];
  const localIssues = row && draftPackage ? issues.filter((issue) => issue.packageId === draftPackage.manifest.id && [row.value.id, ...row.value.responses.map((response) => response.id), draftPackage.manifest.id].includes(issue.objectId)) : [];
  const idDuplicate = row && draftPackage ? draftPackage.contents.fallback_behaviors.some((candidate) => candidate !== row && candidate.value.id === row.value.id) : false;
  const blocked = idDuplicate || localIssues.some((issue) => issue.severity === "error");
  const save = () => { if (draftWorkspace && row && !blocked && props.replaceWorkspace(draftWorkspace)) discard(); };
  const remove = () => { if (!row || mode !== "edit") return; const next = cloneBrainWorkspace(props.workspace); const current = selectedPackage(next); current.contents.fallback_behaviors = current.contents.fallback_behaviors.filter((candidate) => candidate.value.id !== row.value.id); if (props.replaceWorkspace(touch(next))) discard(); };
  const renameResponse = (previousId: string, nextId: string) => {
    const normalized = nextId.trim(); if (!normalized) throw new Error("Response ID is required.");
    if (!row) throw new Error("Fallback Behavior draft is unavailable.");
    if (row.value.responses.some((response) => response.id === normalized && response.id !== previousId)) throw new Error(`Response ${normalized} already exists in this Fallback Behavior.`);
    mutateDraft((behavior) => { const response = behavior.responses.find((candidate) => candidate.id === previousId); if (!response) throw new Error(`Response ${previousId} no longer exists.`); response.id = normalized; });
  };
  return <div className="page-stack"><section className="panel object-list-page">
    <div className="object-page-heading"><div><h2>Fallback Behaviors</h2><p>These behaviors never semantic-match user text. They run only for unresolved or repeated turns, after state conditions are evaluated.</p></div><div className="object-page-actions"><button className="primary compact" onClick={add}>New fallback behavior</button></div></div>
    <div className="filters-row object-page-filters"><input aria-label="Search fallback behaviors" placeholder="Search fallback behaviors…" value={query} onChange={(e)=>setQuery(e.target.value)} /></div>
    <div className="object-page-list behavior-page-list">{filtered.map((candidate) => <button className={`behavior-list-card ${mode === "edit" && candidate.value.id === draftId ? "selected" : ""}`} key={candidate.id} onClick={() => open(candidate.value.id)}><div className="card-title-row"><span className="mono">{candidate.value.id}</span><span className="priority-chip">P{candidate.value.priority}</span></div><div className="behavior-card-summary"><span>{candidate.value.trigger}</span><span>{candidate.value.conditions.length} conditions</span><span>{candidate.value.responses.length} responses</span></div><div className="behavior-card-response">{candidate.value.responses.flatMap((response) => response.texts.flatMap((texts) => texts.variants)).find(Boolean) ?? "No response"}</div></button>)}{filtered.length === 0 && <EmptyState title="No fallback behaviors match" text="Change the search or create a Fallback Behavior." />}</div>
  </section>
  {draftWorkspace && draftPackage && row && mode && <Modal title={mode === "create" ? "New fallback behavior" : `Fallback Behavior · ${row.value.id}`} size="workspace" onClose={discard}><div className="editor-layout">
    <div className="editor-scroll"><section className="editor-section primary-section"><div className="section-heading compact-heading"><div><h3>Selection</h3><p>Highest-priority eligible behavior wins. Conditions may use author, conversation, context, or system state.</p></div></div>
      <div className="form-grid three"><Field label="Fallback Behavior ID"><input className="mono" value={row.value.id} onChange={(e)=>mutateDraft((behavior)=>{behavior.id=e.target.value;})} /></Field><Field label="Trigger"><select value={row.value.trigger} onChange={(e)=>mutateDraft((behavior)=>{behavior.trigger=e.target.value as typeof behavior.trigger;})}><option value="unresolved">Unresolved</option><option value="repeat">Repeat</option></select></Field><Field label="Priority"><input type="number" step="1" value={row.value.priority} onChange={(e)=>mutateDraft((behavior)=>{behavior.priority=Number(e.target.value);})} /></Field></div>{idDuplicate && <div className="field-error">Fallback Behavior ID must be unique in this Package.</div>}
    </section>
    <section className="editor-section"><div className="section-heading compact-heading"><div><h3>Conditions</h3><p>Use state to keep fallback tone/personality aligned with the current NPC or assistant state.</p></div><button onClick={()=>mutateDraft((behavior)=>behavior.conditions.push({namespace:"author",path:"",op:"exists",value:null,hasValue:false}))}>Add condition</button></div><div className="item-stack">{row.value.conditions.map((condition,index)=><div className="condition-row" key={index}><select value={condition.namespace} onChange={(e)=>mutateDraft((behavior)=>{behavior.conditions[index]!.namespace=e.target.value as typeof behavior.conditions[number]["namespace"];})}><option>author</option><option>conversation</option><option>context</option><option>system</option></select><input value={condition.path} onChange={(e)=>mutateDraft((behavior)=>{behavior.conditions[index]!.path=e.target.value;})} placeholder="path"/><select value={condition.op} onChange={(e)=>mutateDraft((behavior)=>{behavior.conditions[index]!.op=e.target.value as typeof behavior.conditions[number]["op"];behavior.conditions[index]!.hasValue=!["exists","missing"].includes(e.target.value);})}><option>exists</option><option>missing</option><option>equal</option><option>not_equal</option><option>greater</option><option>greater_or_equal</option><option>less</option><option>less_or_equal</option></select>{condition.hasValue&&<JsonValueInput value={condition.value} onChange={(value)=>mutateDraft((behavior)=>{behavior.conditions[index]!.value=value;})}/>}<button className="icon-button danger" onClick={()=>mutateDraft((behavior)=>{behavior.conditions.splice(index,1);})}>×</button></div>)}</div></section>
    <section className="editor-section primary-section responses-section"><div className="section-heading compact-heading"><div><h3>Responses</h3><p>Responses keep the normal condition, effect, follow-up, localized text, asset, extra-message, and link surface.</p></div><button onClick={()=>mutateDraft((behavior)=>{const id=uniqueId(behavior.responses.map((response)=>response.id),`${behavior.id}.response`);const response=createResponse(id,props.workspace.authoring_language);response.kind="fallback";behavior.responses.push(response);})}>Add response</button></div>{row.value.responses.map((response,index)=><ResponseCard key={`${response.id}-${index}`} response={response} languages={props.workspace.languages} authoringLanguage={props.workspace.authoring_language} index={index} update={(fn)=>mutateDraft((behavior)=>{const current=behavior.responses[index];if(current)fn(current);})} rename={renameResponse} deleteImpacts={[]} remove={()=>mutateDraft((behavior)=>{behavior.responses.splice(index,1);})}/>)}</section>
    <section className="editor-section proximity-panel"><div className="section-heading compact-heading"><div><h3>Checks near this fallback</h3></div></div>{localIssues.length===0?<div className="success-row">No Studio authoring issues for this Fallback Behavior.</div>:<IssueList issues={localIssues} compact/>}</section></div>
    <div className="sticky-editor-footer">{mode==="edit"&&<button className="danger" onClick={remove}>Delete fallback behavior</button>}<span className="footer-spacer" aria-hidden="true" /><button onClick={discard}>Cancel</button><button className="primary" disabled={blocked} onClick={save}>{mode==="create"?"Create fallback behavior":"Save changes"}</button></div>
  </div></Modal>}
  </div>;
}

function BehaviorListCard(props: { row: Contribution<BehaviorDefinition>; pkg: StudioPackage; selected: boolean; onSelect: () => void }) {
  const behavior = props.row.value;
  const meaning = props.pkg.contents.meanings.find((row) => row.value.id === behavior.meaning)?.value;
  const sample = meaning?.samples.find((row)=>row.text.trim())?.text ?? meaning?.patterns.find((row)=>row.text.trim())?.text ?? "No matching evidence";
  const response = behavior.responses[0]?.texts.flatMap((row) => row.variants).find(Boolean) ?? "No response";
  const bound = props.pkg.contents.capability_bindings.filter((row) => row.value.trigger.behavior === behavior.id || row.value.trigger.meaning === behavior.meaning || behavior.responses.some((r) => r.id === row.value.trigger.response)).length;
  const conditionCount = behavior.requires_values.length + behavior.forbidden_values.length + behavior.responses.reduce((sum, row) => sum + row.conditions.length, 0);
  const effectCount = behavior.responses.reduce((sum, row) => sum + row.effects.length, 0);
  const extraCount = behavior.responses.reduce((sum, row) => sum + row.extra_messages.length + row.assets.length + row.links.length, 0);
  const opensFollowup = behavior.responses.find((row) => row.opens_followup)?.opens_followup?.id ?? "";
  return (
    <button className={`behavior-list-card ${props.selected ? "selected" : ""}`} onClick={props.onSelect}>
      <div className="card-title-row"><span className="mono">{behavior.id}</span>{meaning && <span className="priority-chip">P{meaning.priority}</span>}</div>
      <div className="preview-pair"><span>{sample}</span><span className="preview-arrow">→</span><span>{response}</span></div>
      <div className="chip-row">
        {behavior.topic && <span className="chip">topic {behavior.topic}</span>}
        {behavior.topic_scoped && <span className="chip">topic scoped</span>}
        {behavior.activates_topic && <span className="chip">sets topic</span>}
        {(behavior.followup_scope || opensFollowup) && <span className="chip">follow-up {behavior.followup_scope || opensFollowup}</span>}
        <span className="chip">{behavior.responses.length} response{behavior.responses.length === 1 ? "" : "s"}</span>
        {meaning && meaning.patterns.length > 0 && <span className="chip accent">{meaning.patterns.length} pattern{meaning.patterns.length === 1 ? "" : "s"}</span>}{meaning && meaning.negative_samples.length > 0 && <span className="chip">{meaning.negative_samples.length} negatives</span>}
        {meaning && meaning.slots.length > 0 && <span className="chip">{meaning.slots.length} slots</span>}
        {conditionCount > 0 && <span className="chip">{conditionCount} conditions</span>}
        {effectCount > 0 && <span className="chip">{effectCount} effects</span>}
        {extraCount > 0 && <span className="chip">{extraCount} extra</span>}
        {bound > 0 && <span className="chip accent">{bound} capability</span>}
      </div>
    </button>
  );
}

function BehaviorEditor(props: { workspace: StudioBrainWorkspace; pkg: StudioPackage; behaviorRow: Contribution<BehaviorDefinition>; meaningRow: Contribution<MeaningDefinition>; mutate: Mutate; replaceWorkspace: (workspace: StudioBrainWorkspace) => boolean; issues: AuditIssue[]; onRenamed: (id: string) => void; onDeleted: (workspace: StudioBrainWorkspace) => void; mode: "edit" | "create"; onSave: () => void; onCancel: () => void; saveBlocked: boolean }) {
  const behavior = props.behaviorRow.value;
  const meaning = props.meaningRow.value;
  const packageId = props.pkg.manifest.id;
  const [behaviorIdDraft, setBehaviorIdDraft] = useState(behavior.id);
  const [meaningIdDraft, setMeaningIdDraft] = useState(meaning.id);
  const [behaviorIdError, setBehaviorIdError] = useState("");
  const [meaningIdError, setMeaningIdError] = useState("");
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [revealedBehaviorSections, setRevealedBehaviorSections] = useState<string[]>([]);
  const topicConfigured = Boolean(behavior.topic || behavior.topic_scoped || behavior.activates_topic || behavior.topic_ttl !== null);
  const flowConfigured = Boolean(behavior.followup_scope);
  const repeatConfigured = behavior.repeat_same_input_after !== null || behavior.repeat_same_meaning_after !== null;
  const eligibilityConfigured = behavior.requires_values.length > 0 || behavior.forbidden_values.length > 0;
  const advancedConfigured = meaning.negative_samples.length > 0 || meaning.retrieval_terms.length > 0 || meaning.slots.length > 0 || meaning.references.length > 0 || meaning.positive_assumption;
  const revealBehaviorSection = (key: string) => setRevealedBehaviorSections((rows) => rows.includes(key) ? rows : [...rows, key]);
  useEffect(() => { setBehaviorIdDraft(behavior.id); setBehaviorIdError(""); }, [behavior.id]);
  useEffect(() => { setMeaningIdDraft(meaning.id); setMeaningIdError(""); }, [meaning.id]);
  const update = (fn: (behavior: BehaviorDefinition, meaning: MeaningDefinition) => void) => props.mutate((draft) => {
    const pkg = draft.packages.find((p) => p.manifest.id === packageId);
    const b = pkg?.contents.behaviors.find((row) => row.id === props.behaviorRow.id)?.value;
    const m = pkg?.contents.meanings.find((row) => row.id === props.meaningRow.id)?.value;
    if (b && m) fn(b, m);
  });
  const commitBehaviorId = () => {
    const nextId = behaviorIdDraft.trim();
    try {
      props.replaceWorkspace(renameBehaviorAtomic(props.workspace, packageId, behavior.id, nextId));
      setBehaviorIdDraft(nextId);
      setBehaviorIdError("");
      props.onRenamed(nextId);
    } catch (error) { setBehaviorIdError(String(error)); setBehaviorIdDraft(behavior.id); }
  };
  const commitMeaningId = () => {
    try { props.replaceWorkspace(renameMeaningAtomic(props.workspace, packageId, meaning.id, meaningIdDraft.trim())); setMeaningIdError(""); }
    catch (error) { setMeaningIdError(String(error)); setMeaningIdDraft(meaning.id); }
  };
  const deleteImpacts = useMemo(() => behaviorDeleteImpacts(props.workspace, packageId, behavior.id), [props.workspace, packageId, behavior.id]);
  const deleteBehavior = () => {
    if (deleteImpacts.length || props.mode !== "edit") return;
    try { const next = deleteBehaviorPair(props.workspace, packageId, behavior.id); setDeleteOpen(false); props.onDeleted(next); }
    catch { /* dependency state changed between review and commit; modal will re-render with blockers */ }
  };
  const localIssues = props.issues.filter((row) => row.packageId === packageId && ([behavior.id, meaning.id, ...behavior.responses.map((r) => r.id)].includes(row.objectId)));
  const capabilityBindings = props.pkg.contents.capability_bindings.filter((row) => row.value.trigger.meaning === meaning.id || row.value.trigger.behavior === behavior.id || behavior.responses.some((r) => r.id === row.value.trigger.response));

  return (
    <div className="editor-layout">
      <div className="editor-scroll">
        <div className="editor-header"><div><div className="eyebrow">Behavior</div><h2>{humanize(behavior.id)}</h2><div className="mono muted">{behavior.id}</div></div><div className="chip-row"><span className="priority-chip">P{meaning.priority}</span><span className="chip">{meaning.class}</span>{capabilityBindings.length > 0 && <span className="chip accent">{capabilityBindings.length} capability binding{capabilityBindings.length === 1 ? "" : "s"}</span>}</div></div>
        <div className="form-grid two">
          <Field label="Behavior ID"><input value={behaviorIdDraft} onChange={(e) => setBehaviorIdDraft(e.target.value)} onBlur={commitBehaviorId} onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") { setBehaviorIdDraft(behavior.id); setBehaviorIdError(""); e.currentTarget.blur(); } }} />{behaviorIdError && <span className="field-error">{behaviorIdError}</span>}</Field>
          <Field label="Meaning ID"><input value={meaningIdDraft} onChange={(e) => setMeaningIdDraft(e.target.value)} onBlur={commitMeaningId} onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") { setMeaningIdDraft(meaning.id); setMeaningIdError(""); e.currentTarget.blur(); } }} />{meaningIdError && <span className="field-error">{meaningIdError}</span>}</Field>
          <Field label="Meaning class"><select value={meaning.class} onChange={(e) => update((_b, m) => { m.class = e.target.value as typeof m.class; })}><option value="general">General</option><option value="social">Social</option><option value="clarification">Clarification</option></select></Field>
          <Field label="Priority"><input type="number" min="-100" max="100" value={meaning.priority} onChange={(e) => update((_b, m) => { m.priority = Number(e.target.value); })} /></Field>
        </div>

        <div className="response-shortcuts behavior-shortcuts"><span>Add optional section:</span>{!flowConfigured && !revealedBehaviorSections.includes("flow") && <Shortcut label="Follow-up / continuation" onClick={() => revealBehaviorSection("flow")} />}{!repeatConfigured && !revealedBehaviorSections.includes("repeat") && <Shortcut label="Repeat thresholds" onClick={() => revealBehaviorSection("repeat")} />}{!eligibilityConfigured && !revealedBehaviorSections.includes("eligibility") && <Shortcut label="Eligibility values" onClick={() => revealBehaviorSection("eligibility")} />}{!topicConfigured && !revealedBehaviorSections.includes("topic") && <Shortcut label="Topic behavior" onClick={() => revealBehaviorSection("topic")} />}{!advancedConfigured && !revealedBehaviorSections.includes("advanced") && <Shortcut label="Advanced matching" onClick={() => revealBehaviorSection("advanced")} />}</div>
        {(flowConfigured || revealedBehaviorSections.includes("flow")) && <ProgressiveSection title="Follow-up & continuation" configured={flowConfigured} startOpen={revealedBehaviorSections.includes("flow")} summary={behavior.followup_scope ? `Requires follow-up ${behavior.followup_scope}` : "Optional conversation flow"}>
          <Field label="Follow-up scope"><input value={behavior.followup_scope} onChange={(e) => update((b) => { b.followup_scope = e.target.value; })} placeholder="confirm_help" /></Field>
          
        </ProgressiveSection>}

        {(repeatConfigured || revealedBehaviorSections.includes("repeat")) && <ProgressiveSection title="Repeat thresholds" configured={repeatConfigured} startOpen={revealedBehaviorSections.includes("repeat")} summary={repeatConfigured ? `same input ${behavior.repeat_same_input_after ?? "global"} · same meaning ${behavior.repeat_same_meaning_after ?? "global"}` : "Use global repeat thresholds"}>
          <div className="form-grid two"><Field label="Same input after"><input type="number" min="2" max="20" value={behavior.repeat_same_input_after ?? ""} onChange={(e) => update((b) => { b.repeat_same_input_after = e.target.value === "" ? null : Math.max(2, Math.min(20, Number(e.target.value))); })} placeholder="Global" /></Field><Field label="Same Meaning after"><input type="number" min="2" max="20" value={behavior.repeat_same_meaning_after ?? ""} onChange={(e) => update((b) => { b.repeat_same_meaning_after = e.target.value === "" ? null : Math.max(2, Math.min(20, Number(e.target.value))); })} placeholder="Global" /></Field></div>
          <p className="muted tiny">Leave blank to use the Bot/global repeat policy. Behavior-specific thresholds remain bounded to 2–20.</p>
        </ProgressiveSection>}

        {(eligibilityConfigured || revealedBehaviorSections.includes("eligibility")) && <ProgressiveSection title="Behavior eligibility" configured={eligibilityConfigured} startOpen={revealedBehaviorSections.includes("eligibility")} summary={eligibilityConfigured ? `${behavior.requires_values.length} required · ${behavior.forbidden_values.length} forbidden` : "Optional value gates"}>
          <Subheading title="Requires values" description="Every value must match before this Behavior is eligible." />
          <RequirementList rows={behavior.requires_values} onChange={(rows) => update((b) => { b.requires_values = rows; })} addLabel="Add required value" />
          <Subheading title="Forbidden values" description="A matching forbidden value makes this Behavior ineligible." />
          <RequirementList rows={behavior.forbidden_values} onChange={(rows) => update((b) => { b.forbidden_values = rows; })} addLabel="Add forbidden value" />
        </ProgressiveSection>}

        {(topicConfigured || revealedBehaviorSections.includes("topic")) && <ProgressiveSection title="Topic behavior" configured={topicConfigured} startOpen={revealedBehaviorSections.includes("topic")} summary={topicSummary(behavior)}>
          <div className="form-grid two"><Field label="Topic"><input value={behavior.topic} onChange={(e) => update((b) => { b.topic = e.target.value; })} placeholder="support.billing" /></Field><Field label="Topic TTL"><input type="number" min="1" value={behavior.topic_ttl ?? ""} onChange={(e) => update((b) => { b.topic_ttl = e.target.value === "" ? null : Number(e.target.value); })} /></Field></div>
          <label className="check-row"><input type="checkbox" checked={behavior.topic_scoped} onChange={(e) => update((b) => { b.topic_scoped = e.target.checked; })} /> Only eligible inside this topic</label>
          <label className="check-row"><input type="checkbox" checked={behavior.activates_topic} onChange={(e) => update((b) => { b.activates_topic = e.target.checked; })} /> Activate this topic after selection</label>
        </ProgressiveSection>}

        <section className="editor-section primary-section">
          <div className="section-heading compact-heading"><div><h3>Structural patterns</h3><p>Explicit whole-utterance rules run before semantic samples. Use <code>*</code> for one-or-more words, <code>^</code> for zero-or-more, <code>*&#123;slot&#125;</code> / <code>^&#123;slot&#125;</code> for declared String-slot capture, <code>&lt;set:name&gt;&#123;slot&#125;</code> for Matcher Profile sets, and <code>&lt;set:entity.kind&gt;&#123;slot&#125;</code> for a custom Entity slot of that kind. Every rule needs at least one literal or set anchor; wildcard-only catch-alls belong in Fallback authoring instead.</p></div></div>
          <LocalizedPatternList rows={meaning.patterns} languages={props.workspace.languages} onChange={(rows) => update((_b, m) => { m.patterns = rows; })} />
        </section>

        <section className="editor-section primary-section">
          <div className="section-heading compact-heading"><div><h3>Samples</h3><p>Semantic examples generalize beyond exact wording. Patterns are authoritative rules; samples are fuzzy evidence used only when no structural pattern matches.</p></div></div>
          <LocalizedSampleList rows={meaning.samples} languages={props.workspace.languages} onChange={(rows) => update((_b, m) => { m.samples = rows; })} />
        </section>

        {(advancedConfigured || revealedBehaviorSections.includes("advanced")) && <ProgressiveSection title="Advanced matching" configured={advancedConfigured} startOpen={revealedBehaviorSections.includes("advanced")} summary={advancedMeaningSummary(meaning)}>
          <Subheading title="Negative samples" description="Examples that should not resolve to this Meaning." />
          <LocalizedSampleList rows={meaning.negative_samples} languages={props.workspace.languages} onChange={(rows) => update((_b, m) => { m.negative_samples = rows; })} />
          <Subheading title="Retrieval terms" description="Optional author hints; runtime scoring remains canonical." />
          <LocalizedSampleList rows={meaning.retrieval_terms} languages={props.workspace.languages} onChange={(rows) => update((_b, m) => { m.retrieval_terms = rows; })} />
          <Subheading title="Typed slots" description="Values extracted from the utterance or resolved as references." />
          <div className="item-stack">{meaning.slots.map((slot, index) => <section className="item-stack" key={`${slot.name}-${index}`}><div className="inline-form-row"><input aria-label="Slot name" value={slot.name} onChange={(e) => update((_b, m) => { m.slots[index]!.name = e.target.value; })} placeholder="slot name" /><select value={slot.type} onChange={(e) => update((_b, m) => { m.slots[index]!.type = e.target.value as typeof m.slots[number]["type"]; })}><option>string</option><option>number</option><option>boolean</option><option>entity</option><option>reference</option></select>{slot.type === "entity" && <input value={slot.entity_kind} onChange={(e) => update((_b, m) => { m.slots[index]!.entity_kind = e.target.value; })} placeholder="entity kind" />}{slot.type === "reference" && <input value={slot.reference_kind} onChange={(e) => update((_b, m) => { m.slots[index]!.reference_kind = e.target.value; })} placeholder="reference kind" />}<label className="check-inline"><input type="checkbox" checked={slot.required} onChange={(e) => update((_b, m) => { const current = m.slots[index]!; current.required = e.target.checked; if (e.target.checked && current.elicitation.length === 0) current.elicitation.push({ language: props.workspace.authoring_language, text: "" }); })} /> required</label><button className="icon-button danger" aria-label="Remove slot" onClick={() => update((_b, m) => { m.slots.splice(index, 1); })}>×</button></div>{(slot.required || slot.elicitation.length > 0) && <div><Subheading title={`Ask for ${slot.name || "this slot"}`} description="Localized prompt shown while this required value is missing." /><LocalizedElicitationList rows={slot.elicitation} languages={props.workspace.languages} onChange={(rows) => update((_b, m) => { m.slots[index]!.elicitation = rows; })} /></div>}</section>)}<button onClick={() => update((_b, m) => { m.slots.push({ name: "", type: "string", entity_kind: "", reference_kind: "", required: false, elicitation: [] }); })}>Add slot</button></div>
          <Subheading title="References" description="Declare host-visible reference kinds required or optionally usable by this Meaning." />
          <div className="item-stack">{meaning.references.map((reference, index) => <section className="item-stack" key={`${reference.kind}-${index}`}><div className="inline-form-row"><input value={reference.kind} onChange={(e) => update((_b, m) => { m.references[index]!.kind = e.target.value; })} placeholder="reference kind" /><label className="check-inline"><input type="checkbox" checked={reference.required} onChange={(e) => update((_b, m) => { const current = m.references[index]!; current.required = e.target.checked; if (e.target.checked && current.elicitation.length === 0) current.elicitation.push({ language: props.workspace.authoring_language, text: "" }); })} /> required</label><button className="icon-button danger" aria-label="Remove reference" onClick={() => update((_b, m) => { m.references.splice(index, 1); })}>×</button></div>{(reference.required || reference.elicitation.length > 0) && <div><Subheading title={`Ask for ${reference.kind || "this reference"}`} description="Localized prompt shown while this required host reference is missing." /><LocalizedElicitationList rows={reference.elicitation} languages={props.workspace.languages} onChange={(rows) => update((_b, m) => { m.references[index]!.elicitation = rows; })} /></div>}</section>)}<button onClick={() => update((_b, m) => { m.references.push({ kind: "", required: false, elicitation: [] }); })}>Add reference</button></div>
          <label className="check-row"><input type="checkbox" checked={meaning.positive_assumption} onChange={(e) => update((_b, m) => { m.positive_assumption = e.target.checked; })} /> Allow positive-assumption evidence for this Meaning</label>
        </ProgressiveSection>}

        <section className="editor-section primary-section responses-section">
          <div className="section-heading compact-heading"><div><h3>Responses</h3><p>Response text is grouped by Project language so authors cannot accidentally relabel an existing language block.</p></div><button onClick={() => update((b) => { const id = uniqueId(b.responses.map((r) => r.id), `${b.id}.response`); b.responses.push(createResponse(id,props.workspace.authoring_language)); })}>Add response</button></div>
          {behavior.responses.map((response, index) => <ResponseCard key={response.id + index} response={response} languages={props.workspace.languages} authoringLanguage={props.workspace.authoring_language} index={index} update={(fn) => update((b) => { const current = b.responses[index]; if (current) fn(current); })} rename={(previousId, nextId) => props.replaceWorkspace(renameResponseAtomic(props.workspace, packageId, behavior.id, previousId, nextId))} deleteImpacts={responseDeleteImpacts(props.workspace, packageId, behavior.id, response.id)} remove={() => props.replaceWorkspace(deleteResponseAtomic(props.workspace, packageId, behavior.id, response.id))} />)}
        </section>

        <section className="editor-section proximity-panel">
          <div className="section-heading compact-heading"><div><h3>Checks near this behavior</h3><p>Audit feedback stays close to the authored object.</p></div></div>
          {localIssues.length === 0 ? <div className="success-row">No Studio authoring issues for this behavior.</div> : <IssueList issues={localIssues} compact />}
        </section>
      </div>
      <div className="sticky-editor-footer">{props.mode === "edit" && <button className="danger" onClick={() => setDeleteOpen(true)}>Delete behavior</button>}<span className="footer-spacer" aria-hidden="true" /><button onClick={props.onCancel}>Cancel</button><button className="primary" disabled={props.saveBlocked || Boolean(behaviorIdError) || Boolean(meaningIdError)} onClick={props.onSave}>{props.mode === "create" ? "Create behavior" : "Save changes"}</button></div>
      {props.mode === "edit" && deleteOpen && <Modal title={`Delete ${behavior.id}?`} onClose={() => setDeleteOpen(false)}>{deleteImpacts.length > 0 ? <><div className="error-banner">Deletion is blocked until dependent references are removed or redirected.</div><div className="dependency-list">{deleteImpacts.map((impact, index) => <div className="dependency-row" key={`${impact.kind}-${impact.packageId}-${impact.objectId}-${index}`}><span className="code-chip">{impact.kind}</span><div><strong>{impact.objectId}</strong><div className="tiny muted">{impact.packageId}</div><p>{impact.detail}</p></div></div>)}</div><div className="modal-actions"><button onClick={() => setDeleteOpen(false)}>Close</button></div></> : <><p>This removes the behavior and its paired meaning when no other behavior uses that meaning. The operation is atomic.</p><div className="modal-actions"><button onClick={() => setDeleteOpen(false)}>Cancel</button><button className="danger" onClick={deleteBehavior}>Delete behavior</button></div></>}</Modal>}
    </div>
  );
}

function ResponseCard(props: { response: ResponseDefinition; languages: string[]; authoringLanguage: string; index: number; update: (fn: (response: ResponseDefinition) => void) => void; rename: (previousId: string, nextId: string) => void; deleteImpacts: ReturnType<typeof responseDeleteImpacts>; remove: () => void }) {
  const r = props.response;
  const [idDraft, setIdDraft] = useState(r.id);
  const [idError, setIdError] = useState("");
  const [deleteOpen, setDeleteOpen] = useState(false);
  useEffect(() => { setIdDraft(r.id); setIdError(""); }, [r.id]);
  const commitId = () => { try { props.rename(r.id, idDraft); setIdError(""); } catch (error) { setIdError(String(error)); setIdDraft(r.id); } };
  const configuredConditions = r.conditions.length > 0;
  const configuredBehavior = r.kind !== "normal" || r.hint_level !== null || r.repeat_stage !== "";
  const configuredFollowup = r.opens_followup !== null;
  const configuredEffects = r.effects.length > 0;
  const configuredExtra = r.extra_messages.length > 0 || r.assets.length > 0 || r.links.length > 0;
  const [revealed, setRevealed] = useState<string[]>([]);
  const reveal = (key: string) => setRevealed((rows) => rows.includes(key) ? rows : [...rows, key]);
  const isRevealed = (key: string) => revealed.includes(key);
  return (
    <article className="response-card">
      <div className="response-card-head"><div><span className="response-number">Response {props.index + 1}</span><input className="mono response-id-input" value={idDraft} onChange={(e) => setIdDraft(e.target.value)} onBlur={commitId} onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") { setIdDraft(r.id); setIdError(""); e.currentTarget.blur(); } }} />{idError && <span className="field-error">{idError}</span>}</div><button className="icon-button danger" aria-label="Remove response" onClick={() => setDeleteOpen(true)}>×</button></div>
      <div className="localized-text-block">
        <LocalizedTextList rows={r.texts} languages={props.languages} placeholder="Response text" onChange={(texts) => props.update((row) => { row.texts = texts; })} />
      </div>
      <div className="response-shortcuts"><span>Add optional section:</span>{!configuredConditions && !isRevealed("conditions") && <Shortcut label="Conditions" onClick={() => reveal("conditions")} />}{!configuredBehavior && !isRevealed("behavior") && <Shortcut label="Behavior" onClick={() => reveal("behavior")} />}{!configuredFollowup && !isRevealed("followup") && <Shortcut label="Follow-up" onClick={() => reveal("followup")} />}{!configuredEffects && !isRevealed("effects") && <Shortcut label="Effects" onClick={() => reveal("effects")} />}{!configuredExtra && !isRevealed("extra") && <Shortcut label="Extra content" onClick={() => reveal("extra")} />}</div>
      {(configuredConditions || isRevealed("conditions")) && <ProgressiveSection title="Conditions" configured={configuredConditions} startOpen={isRevealed("conditions")} summary={configuredConditions ? `${r.conditions.length} condition${r.conditions.length === 1 ? "" : "s"}` : "Optional eligibility"}>
        <div className="item-stack">{r.conditions.map((condition, index) => <div className="condition-row" key={index}><select value={condition.namespace} onChange={(e) => props.update((row) => { row.conditions[index]!.namespace = e.target.value as typeof row.conditions[number]["namespace"]; })}><option>author</option><option>conversation</option><option>context</option><option>meaning</option><option>system</option></select><input value={condition.path} onChange={(e) => props.update((row) => { row.conditions[index]!.path = e.target.value; })} placeholder="path" /><select value={condition.op} onChange={(e) => props.update((row) => { row.conditions[index]!.op = e.target.value as typeof row.conditions[number]["op"]; row.conditions[index]!.hasValue = !["exists", "missing"].includes(e.target.value); })}><option>exists</option><option>missing</option><option>equal</option><option>not_equal</option><option>greater</option><option>greater_or_equal</option><option>less</option><option>less_or_equal</option></select>{condition.hasValue && <JsonValueInput value={condition.value} onChange={(value) => props.update((row) => { row.conditions[index]!.value = value; })} />}<button className="icon-button" onClick={() => props.update((row) => { row.conditions.splice(index, 1); })}>×</button></div>)}<button onClick={() => props.update((row) => { row.conditions.push({ namespace: "author", path: "", op: "exists", value: null, hasValue: false }); })}>Add condition</button></div>
      </ProgressiveSection>}
      {(configuredBehavior || isRevealed("behavior")) && <ProgressiveSection title="Behavior" configured={configuredBehavior} startOpen={isRevealed("behavior")} summary={responseBehaviorSummary(r)}>
        <div className="form-grid three"><Field label="Response kind"><select value={r.kind} onChange={(e) => props.update((row) => { row.kind = e.target.value as typeof row.kind; })}><option value="normal">Normal</option><option value="hint">Hint</option><option value="repeat">Repeat</option><option value="annoyed_repeat">Annoyed repeat</option><option value="final_repeat">Final repeat</option><option value="fallback">Fallback</option><option value="opening">Opening</option></select></Field><Field label="Hint level"><input type="number" min="0" value={r.hint_level ?? ""} onChange={(e) => props.update((row) => { row.hint_level = e.target.value === "" ? null : Number(e.target.value); })} /></Field><Field label="Repeat stage"><select value={r.repeat_stage} onChange={(e) => props.update((row) => { row.repeat_stage = e.target.value as typeof row.repeat_stage; })}><option value="">None</option><option value="repeat">Repeat</option><option value="annoyed">Annoyed</option><option value="final">Final</option></select></Field></div>
      </ProgressiveSection>}
      {(configuredFollowup || isRevealed("followup")) && <ProgressiveSection title="Follow-up" configured={configuredFollowup} startOpen={isRevealed("followup")} summary={r.opens_followup ? `${r.opens_followup.id} · ttl ${r.opens_followup.ttl}` : "No follow-up"}>
        {r.opens_followup ? <div className="inline-form-row"><input value={r.opens_followup.id} onChange={(e) => props.update((row) => { if (row.opens_followup) row.opens_followup.id = e.target.value; })} placeholder="follow-up id" /><input type="number" min="1" value={r.opens_followup.ttl} onChange={(e) => props.update((row) => { if (row.opens_followup) row.opens_followup.ttl = Number(e.target.value); })} /><button onClick={() => props.update((row) => { row.opens_followup = null; })}>Remove</button></div> : <button onClick={() => props.update((row) => { row.opens_followup = { id: "followup.new", ttl: 2, refresh_if_same: false }; })}>Open a follow-up</button>}
      </ProgressiveSection>}
      {(configuredEffects || isRevealed("effects")) && <ProgressiveSection title="Effects after response" configured={configuredEffects} startOpen={isRevealed("effects")} summary={configuredEffects ? `${r.effects.length} author-state effect${r.effects.length === 1 ? "" : "s"}` : "No state changes"}>
        <div className="item-stack">{r.effects.map((effect, index) => <div className="inline-form-row" key={index}><select value={effect.type} onChange={(e) => props.update((row) => { row.effects[index]!.type = e.target.value as typeof row.effects[number]["type"]; })}><option value="assign">Assign</option><option value="increment">Increment</option></select><input value={effect.target.path} onChange={(e) => props.update((row) => { row.effects[index]!.target.path = e.target.value; })} placeholder="author state path" />{effect.type === "assign" ? <JsonValueInput value={effect.value} onChange={(value) => props.update((row) => { row.effects[index]!.value = value; })} /> : <input type="number" value={effect.delta} onChange={(e) => props.update((row) => { row.effects[index]!.delta = Number(e.target.value); })} />}<button className="icon-button" onClick={() => props.update((row) => { row.effects.splice(index, 1); })}>×</button></div>)}<button onClick={() => props.update((row) => { row.effects.push({ type: "assign", target: { namespace: "author", path: "" }, value: true, delta: 1 }); })}>Add effect</button></div>
      </ProgressiveSection>}
      {(configuredExtra || isRevealed("extra")) && <ProgressiveSection title="Extra content" configured={configuredExtra} startOpen={isRevealed("extra")} summary={extraSummary(r)}>
        <Subheading title="Assets" description="Reference package-owned assets by stable ID with optional accessible text." /><div className="item-stack">{r.assets.map((asset, index) => <div className="inline-form-row" key={`${asset.asset_id}-${index}`}><input value={asset.asset_id} onChange={(e) => props.update((row) => { row.assets[index]!.asset_id = e.target.value; })} placeholder="asset.id" /><input value={asset.alt_text} onChange={(e) => props.update((row) => { row.assets[index]!.alt_text = e.target.value; })} placeholder="alt text" /><button className="icon-button danger" aria-label="Remove response asset" onClick={() => props.update((row) => { row.assets.splice(index, 1); })}>×</button></div>)}<button onClick={() => props.update((row) => { row.assets.push({ asset_id: "", alt_text: "" }); })}>Add asset</button></div>
        <Subheading title="Extra messages" description="Optional additional localized messages selected by the canonical runtime." />
        <div className="item-stack">
          {r.extra_messages.map((message, index) => <ProgressiveSection key={index} title={`Extra message ${index + 1}`} configured summary={`chance ${message.chance}`}>
            <Field label="Chance"><input type="number" min="0" max="1" step="0.01" value={message.chance} onChange={(e) => props.update((row) => { row.extra_messages[index]!.chance = Number(e.target.value); })} /></Field>
            <LocalizedTextList rows={message.texts} languages={props.languages} placeholder="Extra message text" onChange={(texts) => props.update((row) => { row.extra_messages[index]!.texts = texts; })} />
            <button className="danger" onClick={() => props.update((row) => { row.extra_messages.splice(index, 1); })}>Remove extra message</button>
          </ProgressiveSection>)}
          <button onClick={() => props.update((row) => { row.extra_messages.push({ chance: 1, texts: [{ language: props.authoringLanguage, variants: [""] }] }); })}>Add extra message</button>
        </div>
        <Subheading title="Links" description="Optional response links." /><div className="item-stack">{r.links.map((link, index) => <div className="inline-form-row" key={index}><input value={link.label} onChange={(e) => props.update((row) => { row.links[index]!.label = e.target.value; })} placeholder="label" /><input value={link.url} onChange={(e) => props.update((row) => { row.links[index]!.url = e.target.value; })} placeholder="https://…" /><button className="icon-button" onClick={() => props.update((row) => { row.links.splice(index, 1); })}>×</button></div>)}<button onClick={() => props.update((row) => { row.links.push({ label: "", url: "" }); })}>Add link</button></div>
      </ProgressiveSection>}
      {deleteOpen && <Modal title={`Delete response ${r.id}?`} onClose={() => setDeleteOpen(false)}>{props.deleteImpacts.length > 0 ? <><div className="error-banner">Response deletion is blocked until these references are removed or redirected.</div><div className="dependency-list">{props.deleteImpacts.map((impact, index) => <div className="dependency-row" key={`${impact.kind}-${impact.packageId}-${impact.objectId}-${index}`}><span className="code-chip">{impact.kind}</span><div><strong>{impact.objectId}</strong><div className="tiny muted">{impact.packageId}</div><p>{impact.detail}</p></div></div>)}</div><div className="modal-actions"><button onClick={() => setDeleteOpen(false)}>Close</button></div></> : <><p>This removes the response atomically.</p><div className="modal-actions"><button onClick={() => setDeleteOpen(false)}>Cancel</button><button className="danger" onClick={() => { props.remove(); setDeleteOpen(false); }}>Delete response</button></div></>}</Modal>}
    </article>
  );
}

function Shortcut({ label, onClick }: { label: string; onClick: () => void }) { return <button type="button" className="section-add-button" onClick={onClick}><span className="section-add-icon" aria-hidden="true">+</span><span>{label}</span></button>; }
function LocalizedPatternList(props:{rows:MeaningDefinition["patterns"];languages:string[];onChange:(rows:MeaningDefinition["patterns"])=>void}) {
  const languages = props.languages.length > 0 ? props.languages : ["en-US"];
  return <div className="item-stack">{props.rows.map((row,index)=><div className="inline-form-row" key={`${row.language}-${index}`}><select aria-label="Pattern language" value={row.language} onChange={(e)=>{const next=structuredClone(props.rows);next[index]!.language=e.target.value;props.onChange(next);}}>{languages.map((language)=><option key={languageKey(language)} value={language}>{language}</option>)}</select><input aria-label="Structural pattern" className="mono" value={row.text} onChange={(e)=>{const next=structuredClone(props.rows);next[index]!.text=e.target.value;props.onChange(next);}} placeholder="^ CAPABILITY * RUNTIME" /><input aria-label="Pattern priority" type="number" min="-100" max="100" value={row.priority} onChange={(e)=>{const next=structuredClone(props.rows);next[index]!.priority=Number(e.target.value);props.onChange(next);}} /><button className="icon-button danger" aria-label="Remove pattern" onClick={()=>props.onChange(props.rows.filter((_row,i)=>i!==index))}>×</button></div>)}<button onClick={()=>props.onChange([...props.rows,{language:languages[0]!,text:"",priority:0}])}>Add structural pattern</button></div>;
}

function LocalizedSampleList(props:{rows:MeaningDefinition["samples"];languages:string[];onChange:(rows:MeaningDefinition["samples"])=>void}) {
  const activeLanguages = authoredLanguages(props.languages, props.rows);
  const missingLanguages = availableLanguages(props.languages, props.rows);
  const replaceLanguage = (language:string,texts:string[]) => {
    const key=authoredLanguageKey(language);const first=props.rows.findIndex((row)=>authoredLanguageKey(row.language)===key);
    const next=props.rows.filter((row)=>authoredLanguageKey(row.language)!==key);
    if(texts.length){const at=first<0?next.length:Math.min(first,next.length);next.splice(at,0,...texts.map((text)=>({language,text})));}
    props.onChange(next);
  };
  return <div className="localized-language-editor">{activeLanguages.map((language)=>{const texts=props.rows.filter((row)=>authoredLanguageKey(row.language)===authoredLanguageKey(language)).map((row)=>row.text);return <section className="language-editor-section" key={authoredLanguageKey(language)}><div className="language-row"><strong>{language||"Missing language"}</strong><button className="danger compact" onClick={()=>replaceLanguage(language,[])}>Remove language</button></div><StringList rows={texts} onChange={(rows)=>replaceLanguage(language,rows)} placeholder="What might a user say?" /></section>;})}<LanguageAddActions languages={missingLanguages} onAdd={(language)=>props.onChange([...props.rows,{language,text:""}])}/></div>;
}

function LocalizedElicitationList(props:{rows:MeaningDefinition["samples"];languages:string[];onChange:(rows:MeaningDefinition["samples"])=>void}) {
  const activeLanguages = authoredLanguages(props.languages, props.rows);
  const missingLanguages = availableLanguages(props.languages, props.rows);
  const replaceLanguage = (language:string,texts:string[]) => {
    const key=authoredLanguageKey(language);const first=props.rows.findIndex((row)=>authoredLanguageKey(row.language)===key);
    const next=props.rows.filter((row)=>authoredLanguageKey(row.language)!==key);
    if(texts.length){const at=first<0?next.length:Math.min(first,next.length);next.splice(at,0,...texts.map((text)=>({language,text})));}
    props.onChange(next);
  };
  return <div className="localized-language-editor">{activeLanguages.map((language)=>{const texts=props.rows.filter((row)=>authoredLanguageKey(row.language)===authoredLanguageKey(language)).map((row)=>row.text);return <section className="language-editor-section" key={authoredLanguageKey(language)}><div className="language-row"><strong>{language||"Missing language"}</strong><button className="danger compact" onClick={()=>replaceLanguage(language,[])}>Remove language</button></div><StringList rows={texts} onChange={(rows)=>replaceLanguage(language,rows)} placeholder="What should the bot ask?" /></section>;})}<LanguageAddActions languages={missingLanguages} onAdd={(language)=>props.onChange([...props.rows,{language,text:""}])}/></div>;
}

function LocalizedTextList(props:{rows:ResponseDefinition["texts"];languages:string[];placeholder:string;onChange:(rows:ResponseDefinition["texts"])=>void}) {
  const activeLanguages=authoredLanguages(props.languages,props.rows);
  const missingLanguages=availableLanguages(props.languages,props.rows);
  const replaceLanguage=(language:string,variants:string[])=>{
    const key=authoredLanguageKey(language);const first=props.rows.findIndex((row)=>authoredLanguageKey(row.language)===key);
    const next=props.rows.filter((row)=>authoredLanguageKey(row.language)!==key);const at=first<0?next.length:Math.min(first,next.length);
    next.splice(at,0,{language,variants});props.onChange(next);
  };
  return <div className="localized-language-editor">{activeLanguages.map((language)=>{const variants=props.rows.filter((row)=>authoredLanguageKey(row.language)===authoredLanguageKey(language)).flatMap((row)=>row.variants);return <section className="language-editor-section" key={authoredLanguageKey(language)}><div className="language-row"><strong>{language||"Missing language"}</strong><button className="danger compact" onClick={()=>props.onChange(props.rows.filter((row)=>authoredLanguageKey(row.language)!==authoredLanguageKey(language)))}>Remove language</button></div><StringList rows={variants} onChange={(rows)=>replaceLanguage(language,rows)} placeholder={props.placeholder} /></section>;})}<LanguageAddActions languages={missingLanguages} onAdd={(language)=>props.onChange([...props.rows,{language,variants:[""]}])}/></div>;
}

function LanguageAddActions(props:{languages:string[];onAdd:(language:string)=>void}) {
  if(props.languages.length===0)return null;
  return <div className="language-add-actions"><span>Add language:</span>{props.languages.map((language)=><button type="button" className="language-add-button" key={languageKey(language)} onClick={()=>props.onAdd(language)}><span aria-hidden="true">+</span> Add {language}</button>)}</div>;
}

function authoredLanguages(languages:string[],rows:ReadonlyArray<{language:string}>):string[] {
  const result:string[]=[];const seen=new Set<string>();
  const add=(language:string)=>{const key=authoredLanguageKey(language);if(seen.has(key)||!rows.some((row)=>authoredLanguageKey(row.language)===key))return;seen.add(key);result.push(language);};
  languages.forEach(add);rows.forEach((row)=>add(row.language));return result;
}

function availableLanguages(languages:string[],rows:ReadonlyArray<{language:string}>):string[] {
  const used=new Set(rows.map((row)=>languageKey(row.language)));return languages.filter((language)=>!used.has(languageKey(language)));
}

function authoredLanguageKey(language:string):string { return languageKey(language)||"\u0000missing"; }

function StringList(props: { rows: string[]; onChange: (rows: string[]) => void; placeholder: string; addLabel?: string }) {
  const rows = props.rows;
  return <div className="string-list">{rows.map((row, index) => <div className="text-row" key={index}><input value={row} onChange={(e) => { const next = [...rows]; next[index] = e.target.value; props.onChange(next); }} placeholder={props.placeholder} /><button className="icon-button danger" aria-label="Remove row" onClick={() => props.onChange(rows.filter((_x, i) => i !== index))}>×</button></div>)}<button className="link-button" onClick={() => props.onChange([...rows, ""])}>{props.addLabel ?? "Add another"}</button></div>;
}

function RequirementList(props: { rows: ValueRequirement[]; onChange: (rows: ValueRequirement[]) => void; addLabel: string }) {
  return <div className="item-stack">{props.rows.map((row, index) => <div className="condition-row" key={index}><select value={row.namespace} onChange={(e) => { const next = structuredClone(props.rows); next[index]!.namespace = e.target.value as typeof next[number]["namespace"]; props.onChange(next); }}><option>author</option><option>conversation</option><option>context</option><option>meaning</option><option>system</option></select><input value={row.path} onChange={(e) => { const next = structuredClone(props.rows); next[index]!.path = e.target.value; props.onChange(next); }} placeholder="value path" /><JsonValueInput value={row.value} onChange={(value) => { const next = structuredClone(props.rows); next[index]!.value = value; props.onChange(next); }} /><button className="icon-button danger" aria-label="Remove value requirement" onClick={() => props.onChange(props.rows.filter((_row, i) => i !== index))}>×</button></div>)}<button onClick={() => props.onChange([...props.rows, { namespace: "author", path: "", value: true }])}>{props.addLabel}</button></div>;
}

function topicSummary(b: BehaviorDefinition): string { const bits = [b.topic && `topic ${b.topic}`, b.topic_scoped && "scoped", b.activates_topic && "activates", b.topic_ttl !== null && `ttl ${b.topic_ttl}`].filter(Boolean); return bits.length ? bits.join(" · ") : "No topic behavior"; }
function advancedMeaningSummary(m: MeaningDefinition): string { const bits = [m.negative_samples.length && `${m.negative_samples.length} negatives`, m.retrieval_terms.length && `${m.retrieval_terms.length} retrieval terms`, m.slots.length && `${m.slots.length} slots`, m.references.length && `${m.references.length} refs`, m.positive_assumption && "positive assumption"].filter(Boolean); return bits.length ? bits.join(" · ") : "Optional matching power"; }
function responseBehaviorSummary(r: ResponseDefinition): string { const bits = [r.kind !== "normal" && r.kind.replaceAll("_", " "), r.hint_level !== null && `hint ${r.hint_level}`, r.repeat_stage && `stage ${r.repeat_stage}`].filter(Boolean); return bits.length ? bits.join(" · ") : "Normal response"; }
function extraSummary(r: ResponseDefinition): string { const bits = [r.assets.length && `${r.assets.length} assets`, r.links.length && `${r.links.length} links`, r.extra_messages.length && `${r.extra_messages.length} extra messages`].filter(Boolean); return bits.length ? bits.join(" · ") : "No extra content"; }
