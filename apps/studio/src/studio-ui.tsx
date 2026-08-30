import { useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { humanize } from "./workspace.js";

export function Modal(props: { title: string; onClose: () => void; children: ReactNode; size?: "default" | "workspace" }) {
  const cardRef = useRef<HTMLElement | null>(null);
  const closeRef = useRef(props.onClose); closeRef.current = props.onClose;
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const dragRef = useRef<{ pointerId: number; x: number; y: number; baseX: number; baseY: number } | null>(null);
  useEffect(() => {
    const card = cardRef.current; if (!card) return;
    const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const inerted: HTMLElement[] = []; let activeBranch: HTMLElement | null = card.parentElement;
    while (activeBranch && activeBranch !== document.body) { const parent = activeBranch.parentElement; if (!parent) break; for (const sibling of Array.from(parent.children)) { if (!(sibling instanceof HTMLElement) || sibling === activeBranch || sibling.hasAttribute("inert") || sibling.hasAttribute("data-studio-transient-layer")) continue; sibling.setAttribute("inert", ""); inerted.push(sibling); } activeBranch = parent; }
    const focusables = () => Array.from(card.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])')).filter((element) => element.getAttribute("aria-hidden") !== "true");
    (focusables()[0] ?? card).focus();
    const keydown = (event: KeyboardEvent) => { if (event.key === "Escape") { event.preventDefault(); closeRef.current(); return; } if (event.key !== "Tab") return; const rows = focusables(); if (rows.length === 0) { event.preventDefault(); card.focus(); return; } const first=rows[0]!, last=rows[rows.length-1]!; if (event.shiftKey && (document.activeElement===first || !card.contains(document.activeElement))) { event.preventDefault(); last.focus(); } else if (!event.shiftKey && (document.activeElement===last || !card.contains(document.activeElement))) { event.preventDefault(); first.focus(); } };
    document.addEventListener("keydown", keydown, true);
    return () => { document.removeEventListener("keydown", keydown, true); for (const element of inerted) element.removeAttribute("inert"); if (previouslyFocused?.isConnected) previouslyFocused.focus(); };
  }, []);
  const startDrag = (event: PointerEvent<HTMLDivElement>) => { if ((event.target as HTMLElement).closest("button")) return; dragRef.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, baseX: offset.x, baseY: offset.y }; try { event.currentTarget.setPointerCapture?.(event.pointerId); } catch {} };
  const drag = (event: PointerEvent<HTMLDivElement>) => { const state=dragRef.current; if (!state || state.pointerId!==event.pointerId) return; setOffset({ x: state.baseX + event.clientX-state.x, y: state.baseY + event.clientY-state.y }); };
  const stopDrag = (event: PointerEvent<HTMLDivElement>) => { if (dragRef.current?.pointerId===event.pointerId) dragRef.current=null; try { if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture?.(event.pointerId); } catch {} };
  return <div className="modal-backdrop" role="presentation"><section ref={cardRef} tabIndex={-1} className={`modal-card ${props.size === "workspace" ? "modal-card-workspace" : ""}`} style={{ transform: `translate(${offset.x}px, ${offset.y}px)` }} role="dialog" aria-modal="true" aria-label={props.title}><div className="modal-head" onPointerDown={startDrag} onPointerMove={drag} onPointerUp={stopDrag} onPointerCancel={stopDrag}><h2>{props.title}</h2><button className="modal-close" aria-label="Close modal" onClick={() => closeRef.current()}>×</button></div><div className="modal-body">{props.children}</div></section></div>;
}

export function ValidationErrors(props: { errors: string[]; show?: boolean }) {
  const bannerRef = useRef<HTMLDivElement | null>(null);
  const visible = props.show !== false && props.errors.length > 0;
  useEffect(() => { if (visible) bannerRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" }); }, [visible]);
  return visible ? <div ref={bannerRef} className="error-banner"><strong>Fix before saving:</strong><ul>{props.errors.map((error) => <li key={error}>{error}</li>)}</ul></div> : null;
}

/**
 * Blocking validation stays hidden until the author actually attempts Save, then tracks the draft live.
 * `draftKey` identifies the open draft: when the caller keeps one editor mounted across drafts, a new
 * identity clears the previous attempt so a fresh draft never opens already showing errors.
 */
export function useSubmitValidation(errors: string[], draftKey?: unknown) {
  const [attempted, setAttempted] = useState(false);
  const draftRef = useRef(draftKey);
  if (draftRef.current !== draftKey) { draftRef.current = draftKey; if (attempted) setAttempted(false); }
  return {
    attempted,
    guardSave: (commit: () => void) => () => { setAttempted(true); if (errors.length === 0) commit(); },
  };
}

export function Field(props: { label: string; children: ReactNode }) { return <label className="field"><span>{props.label}</span>{props.children}</label>; }
export function Metric(props: { label: string; value: string | number; tone?: string }) { return <div className={`metric-card ${props.tone ?? ""}`}><strong>{props.value}</strong><span>{props.label}</span></div>; }
export function EmptyState(props: { title: string; text: string }) { return <div className="empty-state"><strong>{props.title}</strong><p>{props.text}</p></div>; }
export function NumberSettings(props: { value: Record<string, number>; onChange: (key: string, value: number) => void }) {
  return <div className="form-grid two">{Object.entries(props.value).map(([key, value]) => <Field key={key} label={humanize(key)}><input type="number" step="any" value={value} onChange={(e) => props.onChange(key, Number(e.target.value))} /></Field>)}</div>;
}

export function ProgressiveSection(props: { title: string; configured?: boolean; startOpen?: boolean; summary?: string; children: ReactNode }) {
  const [open, setOpen] = useState(Boolean(props.configured || props.startOpen));
  useEffect(() => { if (props.configured || props.startOpen) setOpen(true); }, [props.configured, props.startOpen]);
  return <section className={`progressive-section ${open ? "open" : ""}`}><button type="button" className="progressive-toggle" aria-expanded={open} onClick={() => setOpen((value) => !value)}><span><strong>{props.title}</strong>{props.summary && <small>{props.summary}</small>}</span><span className="chevron">⌄</span></button>{open && <div className="progressive-body">{props.children}</div>}</section>;
}
