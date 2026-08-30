# Security Policy

GVYA is currently pre-1.0. Security reports are welcome, especially for issues that could violate deterministic execution, artifact integrity, resource bounds, capability isolation, or the host/runtime trust boundary.

## Supported versions

Until a stable release policy exists, security fixes target the current public development line and the latest tagged public-preview release, if one exists. Older development snapshots are not maintained as separate supported branches.

## Reporting a vulnerability

Do **not** publish exploit details in a normal public GitHub issue.

Use GitHub's private vulnerability reporting for this repository when it is available. If private reporting is not available, open a minimal public issue stating that you need a private channel for a security report, without including exploit steps, secrets, proof-of-concept payloads, or other details that would make the vulnerability easier to abuse.

A useful private report includes:

- the affected GVYA revision or release;
- the affected component;
- a concise description of the security impact;
- reproduction steps or a minimal proof of concept;
- whether the issue is known to be exploitable through untrusted bot source, `.gvya` artifacts, runtime requests, host capability results, Studio content, or build/release inputs;
- any suggested mitigation, if known.

## Security-sensitive boundaries

Reports are particularly useful when they concern:

- malformed or adversarial `.gvya` artifact handling;
- resource-budget or parser-bound bypasses;
- compiler/runtime divergence that could invalidate authored policy;
- capability proposal, confirmation, binding, or result-policy bypasses;
- unexpected host effect execution;
- integrity/signing boundary failures;
- unsafe FFI/WASM boundary behavior;
- source or package inputs that escape their intended filesystem or trust boundary;
- dependency or release-pipeline vulnerabilities that can affect distributed artifacts.

## Disclosure

Please allow time for investigation and remediation before public disclosure. GVYA does not currently promise a fixed response SLA, but valid reports will be handled as project-security work rather than normal feature requests.

The existence of a security report does not by itself imply a supported production guarantee; release/freeze claims remain governed by `docs/RELEASE.md`.
