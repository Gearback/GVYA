import { useEffect, useState } from "react";
import { Field } from "./studio-ui.js";
import type { JsonObject, JsonValue } from "./types.js";

export function Subheading(props: { title: string; description?: string }) { return <div className="subheading"><strong>{props.title}</strong>{props.description && <span>{props.description}</span>}</div>; }


export function JsonValueInput(props: { value: JsonValue; onChange: (value: JsonValue) => void }) {
  const [text, setText] = useState(() => JSON.stringify(props.value));
  useEffect(() => setText(JSON.stringify(props.value)), [JSON.stringify(props.value)]);
  const commit = () => { try { props.onChange(JSON.parse(text) as JsonValue); } catch { /* keep text for correction */ } };
  return <input className="mono" value={text} onChange={(e) => setText(e.target.value)} onBlur={commit} aria-label="JSON value" />;
}

export function JsonObjectEditor(props: { label: string; value: JsonObject | null; nullable?: boolean; onChange: (value: JsonObject | null) => void }) {
  const [text, setText] = useState(() => props.value === null ? "" : JSON.stringify(props.value, null, 2));
  const [error, setError] = useState("");
  useEffect(() => setText(props.value === null ? "" : JSON.stringify(props.value, null, 2)), [JSON.stringify(props.value)]);
  const commit = () => { if (props.nullable && text.trim() === "") { props.onChange(null); setError(""); return; } try { const parsed = JSON.parse(text); if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") throw new Error("Schema must be a JSON object"); props.onChange(parsed as JsonObject); setError(""); } catch (e) { setError(String(e)); } };
  return <Field label={props.label}><textarea className="code-textarea" rows={8} value={text} onChange={(e) => setText(e.target.value)} onBlur={commit} />{error && <span className="field-error">{error}</span>}</Field>;
}


export function csv(value: string): string[] { return value.split(",").map((row) => row.trim()).filter(Boolean); }

