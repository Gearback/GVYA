import { useEffect, useMemo, useState } from "react";
import type { Contribution, JsonObject, PackageContents, StudioBrainWorkspace } from "./types.js";
import { selectedPackage } from "./workspace.js";

const SOURCE_NAMESPACES = [
  "capability_result_behaviors",
  "openings",
  "style_lexicons",
  "capability_configs",
  "types",
] as const satisfies readonly (keyof PackageContents)[];

type SourceNamespace = (typeof SOURCE_NAMESPACES)[number];
type Mutate = (mutator: (draft: StudioBrainWorkspace) => void) => boolean;

const LABELS: Record<SourceNamespace, string> = {
  capability_result_behaviors: "Capability-result behaviors",
  openings: "Openings",
  style_lexicons: "Style lexicons",
  capability_configs: "Capability configs",
  types: "Types",
};

/**
 * Source-complete human authoring surface for canonical namespaces that do not warrant a dedicated
 * domain editor yet. This is deliberately a first-class editor, not an import-only escape hatch:
 * every value written here is persisted through the same Package contribution model.
 */
export function PackageSourceView(props: { workspace: StudioBrainWorkspace; mutate: Mutate }) {
  const pkg = selectedPackage(props.workspace);
  const [namespace, setNamespace] = useState<SourceNamespace>("capability_result_behaviors");
  const current = pkg.contents[namespace];
  const serialized = useMemo(() => JSON.stringify(current, null, 2), [current]);
  const [text, setText] = useState(serialized);
  const [error, setError] = useState("");

  useEffect(() => {
    setText(serialized);
    setError("");
  }, [namespace, serialized]);

  const save = () => {
    try {
      const parsed = JSON.parse(text) as unknown;
      validateContributionArray(parsed, namespace);
      const accepted = props.mutate((draft) => {
        const target = selectedPackage(draft);
        // These namespaces share the canonical Contribution<T> envelope. Domain-specific value
        // validation remains compiler authority and is also enforced on source import/compile.
        const contents = target.contents as unknown as Record<SourceNamespace, Contribution<JsonObject>[]>;
        contents[namespace] = structuredClone(parsed);
      });
      if (accepted) setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  return <div className="page-stack">
    <section className="page-heading"><div><h1>Source</h1><p>Human authoring for canonical Package namespaces without a specialized visual editor.</p></div></section>
    <section className="panel">
      <div className="section-heading"><div><h2>{LABELS[namespace]}</h2><p>Edit the complete contribution array. IDs, export visibility, replacement mode, and values are preserved exactly.</p></div></div>
      <div className="form-grid">
        <label className="field"><span>Namespace</span><select value={namespace} onChange={(event) => setNamespace(event.target.value as SourceNamespace)}>{SOURCE_NAMESPACES.map((row) => <option key={row} value={row}>{LABELS[row]}</option>)}</select></label>
        <label className="field"><span>Contributions (JSON)</span><textarea className="code-textarea" rows={24} spellCheck={false} value={text} onChange={(event) => setText(event.target.value)} /></label>
      </div>
      {error ? <div className="error-banner">{error}</div> : null}
      <div className="modal-actions"><button onClick={() => setText(serialized)}>Reset</button><button className="primary" onClick={save}>Save source</button></div>
    </section>
  </div>;
}

function validateContributionArray(value: unknown, namespace: SourceNamespace): asserts value is Contribution<JsonObject>[] {
  if (!Array.isArray(value)) throw new Error(`${namespace}: expected an array of contributions.`);
  const ids = new Set<string>();
  value.forEach((raw, index) => {
    if (typeof raw !== "object" || raw === null || Array.isArray(raw)) throw new Error(`${namespace}[${index}]: expected an object.`);
    const row = raw as Record<string, unknown>;
    if (typeof row.id !== "string" || row.id.trim() === "") throw new Error(`${namespace}[${index}].id: expected a non-empty string.`);
    if (ids.has(row.id)) throw new Error(`${namespace}: duplicate contribution id ${row.id}.`);
    ids.add(row.id);
    if (typeof row.exported !== "boolean") throw new Error(`${namespace}[${index}].exported: expected boolean.`);
    const mode = row.mode;
    if (mode !== "add") {
      if (typeof mode !== "object" || mode === null || Array.isArray(mode)) throw new Error(`${namespace}[${index}].mode: expected \"add\" or a replace object.`);
      const replace = mode as Record<string, unknown>;
      if (replace.type !== "replace" || typeof replace.target_package !== "string" || typeof replace.target_id !== "string") {
        throw new Error(`${namespace}[${index}].mode: invalid replacement target.`);
      }
    }
    if (!("value" in row) || typeof row.value !== "object" || row.value === null || Array.isArray(row.value)) throw new Error(`${namespace}[${index}].value: expected an object.`);
  });
}
