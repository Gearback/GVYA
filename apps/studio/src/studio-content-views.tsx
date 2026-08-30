import { useState } from "react";
import { coverageSummary, languageCoverage } from "./audit.js";
import { safeAssetSource, STUDIO_ASSET_MAX_BYTES } from "./asset-files.js";
import { packageIcon } from "./studio-navigation.js";
import { EmptyState, Field, Metric, Modal, useSubmitValidation, ValidationErrors } from "./studio-ui.js";
import type { AssetDefinition, AuditIssue, Contribution, StudioAssetFile, StudioBrainWorkspace, StudioPackage, StudioRoute, StudioWorkspace } from "./types.js";
import { contribution, createAsset, selectedPackage, uniqueId } from "./workspace.js";

type Mutate = (mutator: (draft: StudioBrainWorkspace) => void) => boolean;

export function AssetsView(props: { workspace: StudioBrainWorkspace; mutate: Mutate; assetOwner: string; assetFiles: readonly StudioAssetFile[]; saveAssetFile: (ownerKey: string, packageId: string, previousSource: string | null, asset: AssetDefinition, replacement: File | null) => void; removeAssetFile: (ownerKey: string, source: string) => void }) {
  const pkg = selectedPackage(props.workspace);
  const [editing, setEditing] = useState<{ rowId: string | null; previousSource: string | null; value: AssetDefinition; file: File | null } | null>(null);
  const openNew = () => { const id = uniqueId(pkg.contents.assets.map((row) => row.value.id), "asset.new"); setEditing({ rowId: null, previousSource: null, value: createAsset(id), file: null }); };
  const openExisting = (row: Contribution<AssetDefinition>) => setEditing({ rowId: row.id, previousSource: row.value.source, value: structuredClone(row.value), file: null });
  const existingFile = editing?.previousSource ? props.assetFiles.find((file) => file.owner_key === props.assetOwner && file.source === editing.previousSource) ?? null : null;
  const errors = editing ? validateAssetDraft(pkg, editing.rowId, editing.value, editing.file, existingFile) : [];
  const { attempted, guardSave } = useSubmitValidation(errors, editing);
  const save = () => {
    if (!editing || errors.length) return;
    const draft = structuredClone(editing.value);
    const committed = props.mutate((workspace) => {
      const target = workspace.packages.find((row) => row.manifest.id === pkg.manifest.id);
      if (!target) return;
      if (editing.rowId === null) target.contents.assets.push(contribution(draft.id, draft));
      else {
        const row = target.contents.assets.find((item) => item.id === editing.rowId);
        if (!row) return;
        const previous = row.value.id;
        row.id = draft.id; row.value = draft;
        if (previous !== draft.id) for (const behavior of target.contents.behaviors) for (const response of behavior.value.responses) for (const ref of response.assets) if (ref.asset_id === previous) ref.asset_id = draft.id;
      }
    });
    if (!committed) return;
    props.saveAssetFile(props.assetOwner, pkg.manifest.id, editing.previousSource, draft, editing.file);
    setEditing(null);
  };
  return <div className="page-stack"><section className="panel object-list-page">
    <div className="object-page-heading"><div><h2>Assets</h2><p>Package-owned runtime content referenced by stable IDs.</p></div><button className="primary compact" onClick={openNew}>Add asset</button></div>
    <div className="object-page-list">{pkg.contents.assets.length === 0 ? <EmptyState title="No assets" text="Add package-owned assets when responses or capabilities need runtime content." /> : pkg.contents.assets.map((row) => <button className="capability-list-card" key={row.id} onClick={() => openExisting(row)}><div className="card-title-row"><strong>{row.value.id}</strong><span className="chip">{row.value.media_type}</span></div><div className="mono small">{row.value.logical_path}</div><div className="muted small">{row.value.source}</div></button>)}</div>
  </section>{editing && <Modal title={editing.rowId === null ? "New asset" : `Asset · ${editing.value.id}`} size="workspace" onClose={() => setEditing(null)}><div className="editor-layout"><div className="editor-scroll"><ValidationErrors errors={errors} show={attempted} /><AssetDraftForm value={editing.value} file={editing.file} existingBytes={existingFile?.blob.size ?? null} onChange={(value) => setEditing((current) => current ? {...current, value} : current)} onFile={(file) => setEditing((current) => current ? {...current, file, value: {...current.value, media_type: file?.type || current.value.media_type}} : current)} /></div><div className="sticky-editor-footer">{editing.rowId !== null && <button className="danger" onClick={() => { const removed = props.mutate((workspace) => { const target=workspace.packages.find((row)=>row.manifest.id===pkg.manifest.id); if(target) target.contents.assets=target.contents.assets.filter((row)=>row.id!==editing.rowId); }); if (removed && editing.previousSource) props.removeAssetFile(props.assetOwner, editing.previousSource); if (removed) setEditing(null); }}>Remove asset</button>}<span className="footer-spacer" aria-hidden="true" /><button onClick={() => setEditing(null)}>Cancel</button><button className="primary" onClick={guardSave(save)}>{editing.rowId === null ? "Create asset" : "Save changes"}</button></div></div></Modal>}</div>;
}

function AssetDraftForm(props:{value:AssetDefinition;file:File|null;existingBytes:number|null;onChange:(value:AssetDefinition)=>void;onFile:(file:File|null)=>void}) { const set=(fn:(value:AssetDefinition)=>void)=>{const next=structuredClone(props.value);fn(next);props.onChange(next);};return <div className="form-grid two"><Field label="Asset ID"><input autoFocus value={props.value.id} onChange={(e)=>set((value)=>{value.id=e.target.value;})}/></Field><Field label="Media type"><input value={props.value.media_type} onChange={(e)=>set((value)=>{value.media_type=e.target.value;})} placeholder="image/png"/></Field><Field label="Logical path"><input value={props.value.logical_path} onChange={(e)=>set((value)=>{value.logical_path=e.target.value;})} placeholder="assets/avatar.png"/></Field><Field label="Source path"><input value={props.value.source} onChange={(e)=>set((value)=>{value.source=e.target.value;})} placeholder="assets/avatar.png"/></Field><Field label="Source bytes"><input type="file" onChange={(event)=>props.onFile(event.currentTarget.files?.[0]??null)}/><span className="muted tiny">{props.file ? `${props.file.name} · ${props.file.size.toLocaleString()} bytes` : props.existingBytes !== null ? `Current file · ${props.existingBytes.toLocaleString()} bytes` : "Choose a file"}</span></Field></div>; }
function validateAssetDraft(pkg:StudioPackage,rowId:string|null,value:AssetDefinition,file:File|null,existing:StudioAssetFile|null):string[] { const errors:string[]=[];if(!value.id.trim())errors.push("Asset ID is required.");if(pkg.contents.assets.some((row)=>row.id!==rowId&&row.value.id===value.id.trim()))errors.push(`Asset ${value.id.trim()} already exists.`);if(!value.media_type.trim())errors.push("Media type is required.");if(!safeAssetSource(value.logical_path))errors.push("Logical path must be a safe path under assets/.");if(!safeAssetSource(value.source))errors.push("Source path must be a safe package-relative path under assets/.");if(!file&&!existing)errors.push("Source bytes are required.");if(file&&file.size>STUDIO_ASSET_MAX_BYTES)errors.push("Source file exceeds the 32 MiB Studio asset limit.");return errors; }

export function PackageOverviewView(props: { studio: StudioWorkspace; workspace: StudioBrainWorkspace; issues: AuditIssue[]; missingMatcherLanguages: string[]; setRoute: (route: StudioRoute) => void; onDownloadArchive: () => void }) {
  const pkg = selectedPackage(props.workspace);
  const scopeLabel = props.studio.selectedPackageScope === "bot" ? "Bot Package" : props.studio.selectedPackageScope === "project" ? "Project Package" : "Shared Package";
  const contents = pkg.contents;
  const testCount = contents.regression_cases.length + contents.scenarios.length;
  const errorCount = props.issues.filter((row) => row.severity === "error").length;
  const disabled = props.missingMatcherLanguages.length > 0;
  return <div className="page-stack">
    <section className="page-heading"><div className="heading-identity"><span className="heading-icon" aria-hidden="true">{packageIcon(pkg.manifest.kind)}</span><div><h1>{pkg.manifest.id}</h1><p>{pkg.manifest.description || scopeLabel}</p></div>{disabled && <span className="chip">Disabled</span>}</div><button className="archive-download" onClick={props.onDownloadArchive}>Download package ZIP</button></section>
    {disabled && <section className="panel"><div className="error-banner"><strong>Package disabled.</strong> Missing Language/Matcher Profile pair{props.missingMatcherLanguages.length === 1 ? "" : "s"} for: {props.missingMatcherLanguages.join(", ")}. Restore both profile JSON files for those languages in this Package's owning scope before authoring, testing, or compiling it.</div></section>}
    <section className="panel"><div className="section-heading"><div><h2>Overview</h2><p>Identity and authored content for this Package. Each tab above edits one part of it.</p></div></div>
      <div className="metric-grid"><Metric label="Behaviors" value={contents.behaviors.length} /><Metric label="Capabilities" value={contents.capabilities.length} /><Metric label="Assets" value={contents.assets.length} /><Metric label="Tests" value={testCount} /><Metric label="Audit errors" value={errorCount} tone={errorCount ? "danger" : "good"} /></div>
      <div className="overview-facts"><div><span>Package ID</span><code>{pkg.manifest.id}</code></div><div><span>Scope</span><strong>{scopeLabel}</strong></div><div><span>Kind</span><strong>{pkg.manifest.kind === "fallback" ? "Fallback" : "Standard"}</strong></div><div><span>Meanings</span><strong>{contents.meanings.length}</strong></div><div><span>Bindings</span><strong>{contents.capability_bindings.length}</strong></div></div>
    </section>
    {!disabled && <section className="panel"><div className="section-heading"><div><h2>Work on this Package</h2><p>Open its authored content or run this Package in isolation.</p></div><div className="inline-actions"><button onClick={() => props.setRoute("author")}>Behaviors</button><button className="primary" onClick={() => props.setRoute("package-simulate")}>Simulate</button></div></div></section>}
  </div>;
}

export function AuditView(props: { workspace: StudioBrainWorkspace; issues: AuditIssue[]; setRoute: (route: StudioRoute) => void }) {
  const coverage = coverageSummary(props.workspace);
  const errors = props.issues.filter((row) => row.severity === "error");
  const warnings = props.issues.filter((row) => row.severity === "warning");
  const info = props.issues.filter((row) => row.severity === "info");
  const testedPercent = coverage.meanings === 0 ? 100 : Math.round((coverage.meaningsWithRegression / coverage.meanings) * 100);
  const languages=languageCoverage(props.workspace);
  return <div className="page-stack"><section className="panel"><div className="section-heading"><div><h2>Authoring audit</h2><p>Human-first checks for source integrity, coverage, references, and obvious collisions. Runtime semantic decisions are not reproduced here.</p></div></div><div className="metric-grid"><Metric label="Errors" value={errors.length} tone={errors.length ? "danger" : "good"} /><Metric label="Warnings" value={warnings.length} tone={warnings.length ? "warning" : "good"} /><Metric label="Meanings tested" value={`${testedPercent}%`} /><Metric label="Exact sample collisions" value={coverage.exactSampleCollisions} tone={coverage.exactSampleCollisions ? "warning" : "good"} /></div></section><section className="panel list-panel"><div className="section-heading"><div><h2>Language coverage</h2><p>Counts for every language defined by the Project Matcher Profiles.</p></div></div><div className="data-list-header project-package-columns"><span>Language</span><span>Meaning samples</span><span>Response variants</span><span>Test turns</span></div>{languages.map((row,index)=><div className="data-row project-package-columns" key={row.language}><div className="row-primary"><strong>{row.language}</strong><span className="muted tiny">{index===0?"Human authoring default":"Project language"}</span></div><span>{row.samples}</span><span>{row.responseVariants}</span><span>{row.regressionTurns}</span></div>)}</section><section className="panel"><AuditGroup title="Errors" issues={errors} /><AuditGroup title="Warnings" issues={warnings} /><AuditGroup title="Information" issues={info} /></section><section className="panel subtle-panel"><h3>Runtime truth stays canonical</h3><p className="muted">Studio’s exact-string collision and coverage checks are authoring aids only. Matching, ambiguity, response eligibility, capability admission, confirmation, and Why traces must come from the compiled artifact through the runtime/SDK layer runtime.</p><button onClick={() => props.setRoute("simulate")}>Open simulator</button></section></div>;
}

function AuditGroup(props: { title: string; issues: AuditIssue[] }) {
  return <div className="audit-group"><div className="audit-group-title"><h3>{props.title}</h3><span className="chip">{props.issues.length}</span></div>{props.issues.length ? <IssueList issues={props.issues} /> : <div className="success-row">None.</div>}</div>;
}

export function IssueList(props: { issues: AuditIssue[]; compact?: boolean }) {
  return <div className={`issue-list ${props.compact ? "compact" : ""}`}>{props.issues.map((row) => <div className={`issue-row ${row.severity}`} key={row.id}><span className="severity-dot" /><div><div className="issue-title">{row.title}<span className="code-chip">{row.code}</span></div><p>{row.detail}</p><div className="mono tiny muted">{row.packageId || "project"} · {row.objectType} · {row.objectId}</div></div></div>)}</div>;
}
